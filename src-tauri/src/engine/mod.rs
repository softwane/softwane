//! Engine loop timing: `std::thread::sleep` has poor resolution on some OSes
//! (notably Windows ~15.6 ms by default), which can jitter a ~60 Hz tick.
//! If that becomes a problem, switch to [`spin_sleep`](https://crates.io/crates/spin_sleep)
//! (already a dependency) for frame pacing.

mod frame_events;
pub use frame_events::FrameEvents;

use std::{
    sync::{Arc, Mutex},
    thread::JoinHandle,
    time::{Duration, Instant},
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

pub struct Engine {
    timer: TimerStateMachine,
    channels: SensoryChannelsSystem,
    renderers: RendererDispatcher,

    app: AppHandle,
    store: Arc<Store<Wry>>,
    event_rx: Receiver<EngineEvent>,

    last_frame_at: Instant,
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
            app,
            event_rx,
            timer,
            channels,
            renderers,
            store,
            last_frame_at: Instant::now(),
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
                break;
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
                    // B1 only wires the bus; renderer events will be logged/handled later.
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

    fn recommended_tick_interval(&self) -> Duration {
        TARGET_FRAME_INTERVAL
    }
}
