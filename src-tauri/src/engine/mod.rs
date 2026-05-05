//! Engine loop timing: `std::thread::sleep` has poor resolution on some OSes
//! (notably Windows ~15.6 ms by default), which can jitter a ~60 Hz tick.
//! If that becomes a problem, switch to [`spin_sleep`](https://crates.io/crates/spin_sleep)
//! (already a dependency) for frame pacing.

mod frame_events;
pub use frame_events::FrameEvents;

use std::{
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{Arc, Mutex},
    thread::JoinHandle,
    time::{Duration, Instant},
    io::Write,
};

use tokio::sync::mpsc::{Receiver, Sender, error::TryRecvError};
use tauri::AppHandle;
use tauri::Wry;
use tauri_plugin_store::Store;

use crate::events::{EngineEvent, RendererEvent};
use crate::timer_state_machine::TimerStateMachine;
use crate::channels::SensoryChannelsSystem;
use crate::renderers::RendererDispatcher;

const TARGET_FRAME_INTERVAL: Duration = Duration::from_micros(16_667);

// ── Shutdown ──────────────────────────────────────────────────────────

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(3);
const SHUTDOWN_POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Debug)]
pub struct EngineHandle {
    pub tx: Sender<EngineEvent>,
    pub join: Mutex<Option<JoinHandle<()>>>,
}

pub struct Engine {
    timer: TimerStateMachine,
    channels: SensoryChannelsSystem,
    renderers: RendererDispatcher,

    app: AppHandle,
    store: Arc<Store<Wry>>,
    event_rx: Receiver<EngineEvent>,

    last_frame_at: Instant,
    cleaned_up: bool,
}

impl Engine {
    pub fn new(
        app: AppHandle,
        event_rx: Receiver<EngineEvent>,
        event_tx: Sender<EngineEvent>,
        store: Arc<Store<Wry>>,
    ) -> Self {
        let timer = TimerStateMachine::load_from_store(&store);
        let channels = SensoryChannelsSystem::load_from_store(&store);
        let renderers = RendererDispatcher::new(event_tx);
        Self {
            timer,
            channels,
            renderers,
            app,
            store,
            event_rx,
            last_frame_at: Instant::now(),
            cleaned_up: false,
        }
    }

    pub fn run(mut self) {
        loop {
            let frame_started_at = Instant::now();
            let dt_ms = frame_started_at
                .saturating_duration_since(self.last_frame_at)
                .as_millis() as u64;
            self.last_frame_at = frame_started_at;

            let mut frame_events = self.collect_events();

            if frame_events.shutdown_requested {
                self.shutdown();
                return;
            }

            // ── force_reset first (emergency brake) ────────────
            if frame_events.force_reset {
                self.timer.reset();
                self.channels.reset();
                self.renderers
                    .reset(self.channels.switch_states(), &self.app);
            }

            // State advance
            self.timer.handle_commands(&mut frame_events);
            self.timer.tick(dt_ms, &mut frame_events);

            // Channel calculation
            self.channels
                .handle_commands(&mut frame_events);
            self.channels
                .tick(self.timer.state(), &mut frame_events);
            let logic_frame = Arc::new(self.channels.logic_frame());

            // ── Persistence ──────────────────────────────────
            if frame_events.need_persist.timer_state_machine {
                self.timer.persist(&self.store);
            }
            if let Some(ref channel_types) = frame_events.need_persist.channels_system {
                for ct in channel_types {
                    self.channels.persist_channel(*ct, &self.store);
                }
            }

            // ── Render side effects ────────────────────────────
            if frame_events.switch_changed {
                self.renderers
                    .switch_renderer(self.channels.switch_states(), &self.app);
            }
            self.renderers.dispatch(logic_frame, &self.app);

            // ── Frame pacing ───────────────────────────────────
            let elapsed = frame_started_at.elapsed();
            if elapsed < self.recommended_tick_interval() {
                std::thread::sleep(self.recommended_tick_interval() - elapsed);
            }
        }
    }

    fn collect_events(&mut self) -> FrameEvents {
        let mut frame_events = FrameEvents::default();

        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                EngineEvent::State(command) => frame_events.state_commands.push(command),
                EngineEvent::Channel(command) => frame_events.channel_commands.push(command),
                EngineEvent::Renderer(_event) => {
                    // Renderer events are informational during normal operation;
                    // they are drained and counted during shutdown (see shutdown).
                }
                EngineEvent::Progress(_command) => {
                    // Progress channel ownership is added when frontend commands are wired.
                }
                EngineEvent::ForceReset => frame_events.force_reset = true,
                EngineEvent::Shutdown => frame_events.shutdown_requested = true,
            }
        }

        frame_events
    }

    fn shutdown(&mut self) {
        tracing::info!("engine shutdown begin");

        // 1. Dispatch shutdown closures (non-blocking).
        self.renderers.shutdown(&self.app);
        let store_for_closure = self.store.clone();
        let save_handle = std::thread::spawn( move || {
            if let Err(e) = store_for_closure.save() {
                tracing::error!("failed to save store during shutdown: {e}");
            }
        });

        // 2. Drain events within the timeout, waiting for ShutdownCompleted.
        let deadline = Instant::now() + SHUTDOWN_TIMEOUT;
        let mut acked = false;

        while !acked && Instant::now() < deadline {
            match self.event_rx.try_recv() {
                Ok(EngineEvent::Renderer(RendererEvent::ShutdownCompleted { renderer_name })) => {
                    tracing::info!(renderer_name, "renderer shutdown acked");
                    acked = true;
                }
                Ok(EngineEvent::Renderer(other)) => {
                    tracing::trace!(?other, "drained during shutdown");
                }
                Ok(_) => {
                    // Inbound commands during shutdown are discarded.
                }
                Err(TryRecvError::Empty) => std::thread::sleep(SHUTDOWN_POLL_INTERVAL),
                Err(TryRecvError::Disconnected) => break,
            }
        }

        if !acked {
            tracing::warn!(
                "renderer shutdown timed out after {:?}",
                SHUTDOWN_TIMEOUT
            );
        }

        // 3. Wait for persisting store to disk (synchronous, blocks until write completes).
        if let Err(e) = save_handle.join() {
            tracing::error!("failed to save store during shutdown: {:?}", e);
        }

        self.cleaned_up = true;
        tracing::info!("engine shutdown complete");
    }

    fn recommended_tick_interval(&self) -> Duration {
        TARGET_FRAME_INTERVAL
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        if self.cleaned_up {
            return;
        }

        // Abnormal path: engine was dropped without shutdown (panic unwinding).
        // catch_unwind wraps all cleanup to prevent double-panic → abort.
        if let Err(err) = catch_unwind(AssertUnwindSafe(|| {
            // Avoid potential new panic info overlaying the old one.
            // See the hook in lib.rs
            let _prev = std::panic::take_hook();

            self.renderers.shutdown(&self.app);
            let deadline = Instant::now() + SHUTDOWN_TIMEOUT;
            let mut acked = false;
            while !acked && Instant::now() < deadline {
                match self.event_rx.try_recv() {
                    Ok(EngineEvent::Renderer(RendererEvent::ShutdownCompleted { renderer_name })) => {
                        tracing::info!(renderer_name, "renderer shutdown acked");
                        acked = true;
                    }
                    Ok(EngineEvent::Renderer(other)) => {
                        tracing::trace!(?other, "drained during shutdown");
                    }
                    Ok(_) => {
                        // Inbound commands during shutdown are discarded.
                    }
                    Err(TryRecvError::Empty) => std::thread::sleep(SHUTDOWN_POLL_INTERVAL),
                    Err(TryRecvError::Disconnected) => break,
                }
            }

            if !acked {
                tracing::warn!(
                    "renderer shutdown timed out after {:?}",
                    SHUTDOWN_TIMEOUT
                );
            }
            let _ = self.store.save();
            // tracing may not work in such cases
            let _ = std::io::stderr().write_fmt(
                format_args!("[Engine::drop] panic recovery cleanup done\n"),
            );
        })) {
            tracing::error!(
                "[Engine::drop] panic recovery shutting down fails:\n{:#?}\n",
                err
            );
            // tracing may not work in such cases
            let _ = std::io::stderr().write_fmt(
            format_args!(
                    "[Engine::drop] panic recovery shutting down fails:\n{:#?}\n",
                    err
                )
            );
        };
    }
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine")
         .field("timer", &self.timer)
         .field("channels", &self.channels)
         .field("renderers", &self.renderers)
         .field("app", &self.app)
         .field("store", &self.store.entries())
         .field("event_rx", &self.event_rx)
         .field("last_frame_at", &self.last_frame_at)
         .field("cleaned_up", &self.cleaned_up)
         .finish()
    }
}