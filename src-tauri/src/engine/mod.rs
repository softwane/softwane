//! Engine loop timing: `std::thread::sleep` has poor resolution on some OSes
//! (notably Windows ~15.6 ms by default), which can jitter a ~60 Hz tick.
//! If that becomes a problem, switch to [`spin_sleep`](https://crates.io/crates/spin_sleep)
//! (already a dependency) for frame pacing.

mod events;
mod config;
pub mod shutdown;
pub mod commands;

pub use events::*;

use std::{
    sync::{Arc, Mutex},
    thread::JoinHandle,
    time::{Duration, Instant},
};

use tokio::sync::mpsc::{Receiver, Sender};
use tauri::{AppHandle, Wry};
use tauri_plugin_store::Store;

use crate::{
    channels::{SensoryChannelsSystem, load_channel_config_array},
    timer_state_machine::{TimerStateMachine, load_timer_config},
    renderers::{RendererDispatcher, events::RendererEvent},
    state::SharedTimerState,
    tray::{refresh_tray_menu, update_tray_progress},
};
use config::StoredConfig;

const PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(100); // 10 fps 给前端

const TARGET_FRAME_INTERVAL: Duration = Duration::from_micros(16_667);

#[derive(Debug)]
pub struct EngineHandle {
    pub tx: Sender<EngineEvent>,
    pub join: Mutex<Option<JoinHandle<Engine>>>,
}

pub struct Engine {
    timer: TimerStateMachine,
    channels: SensoryChannelsSystem,
    renderers: RendererDispatcher,

    app: AppHandle,
    store: Arc<Store<Wry>>,
    event_rx: Receiver<EngineEvent>,

    /// Mirror of `timer.state()` accessible from non-engine threads via
    /// `app.state::<SharedTimerState>()`.  Updated on every transition.
    shared_state: SharedTimerState,

    progress_channel: Option<tauri::ipc::Channel<ProgressPayload>>,
    last_progress_emit: Instant,
    
    last_frame_at: Instant,
    cleaned_up: bool,
}

impl Engine {
    pub fn new(
        app: AppHandle,
        event_rx: Receiver<EngineEvent>,
        event_tx: Sender<EngineEvent>,
        store: Arc<Store<Wry>>,
        shared_state: SharedTimerState,
    ) -> Self {
        let timer = TimerStateMachine::new(load_timer_config(&store));
        let channels = SensoryChannelsSystem::new(load_channel_config_array(&store));
        let renderers = RendererDispatcher::new(event_tx, channels.switch_states(), &app);
        Self {
            timer,
            channels,
            renderers,
            app,
            store,
            event_rx,
            shared_state,
            progress_channel: None,
            last_progress_emit: Instant::now(),
            last_frame_at: Instant::now(),
            cleaned_up: false,
        }
    }

    pub fn run(mut self) -> Self { loop {
        let frame_started_at = Instant::now();
        let dt_ms = frame_started_at
            .saturating_duration_since(self.last_frame_at)
            .as_millis() as u64;
        self.last_frame_at = frame_started_at;

        let mut frame_events = self.collect_events();

        if frame_events.shutdown_requested {
            self.shutdown();
            self.renderers.prepare_send();
            return self;
        }
        if frame_events.shutdown {
            self.renderers.prepare_send();
            return self;
        }

        // force_reset first
        if frame_events.force_reset {
            self.timer.reset();
            self.channels.reset();
            self.renderers.reset(self.channels.switch_states(), &self.app);
        }

        // State advance
        self.timer.handle_commands(&mut frame_events);
        self.timer.tick(dt_ms, &mut frame_events);

        // Channel calculation
        self.channels.handle_commands(&mut frame_events);
        self.channels.tick(self.timer.state(), &mut frame_events);
        let logic_frame = Arc::new(self.channels.logic_frame());

        // Persistence
        if frame_events.need_persist.timer_state_machine {
            crate::timer_state_machine::persist(&self.timer, &self.store);
        }
        if let Some(channel_types) = frame_events.need_persist.channels_system.take() {
            for ct in channel_types {
                crate::channels::persist_channel(&self.channels, ct, &self.store);
            }
        }

        // Render (side effects)
        if frame_events.switch_changed {
            self.renderers.switch_renderer(self.channels.switch_states(), &self.app);
        }
        self.renderers.dispatch(logic_frame, &self.app);

        // Update frontend state
        self.update_frontend_state(&frame_events);

        // Frame pacing
        let elapsed = frame_started_at.elapsed();
        if elapsed < self.recommended_tick_interval() {
            std::thread::sleep(self.recommended_tick_interval() - elapsed);
        }
    }}

    fn collect_events(&mut self) -> FrameEvents {
        let mut frame_events = FrameEvents::default();

        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                // TODO: use channel system to get suggested reverse / settling time based on the input max time
                EngineEvent::State(command) => frame_events.state_commands.push(command),
                EngineEvent::Channel(command) => frame_events.channel_commands.push(command),
                EngineEvent::Renderer(renderer_event) => log_renderer_event(renderer_event),
                EngineEvent::Progress(command) => {
                    match command {
                        ProgressCommandInner::RegisterChannel { channel, window: _ } => {
                            self.progress_channel = Some(channel);
                            self.last_progress_emit = Instant::now() - PROGRESS_EMIT_INTERVAL;
                        },
                        ProgressCommandInner::ClearChannel { window: _ } => self.progress_channel = None,
                    }
                },
                EngineEvent::ForceReset => frame_events.force_reset = true,
                EngineEvent::Shutdown => frame_events.shutdown_requested = true,
                EngineEvent::AbnormalShutdown => frame_events.shutdown = true,
            }
        }

        frame_events
    }

    fn recommended_tick_interval(&self) -> Duration {
        TARGET_FRAME_INTERVAL
    }

    fn update_frontend_state(&mut self, frame_events: &FrameEvents) {
        let now = Instant::now();
        
        // Update progress
        if now.duration_since(self.last_progress_emit) >= PROGRESS_EMIT_INTERVAL
            || frame_events.just_transited
        {
            let progress = ProgressPayload { timer_state: self.timer.state() };

            if let Some(ch) = &self.progress_channel {
                if let Err(e) = ch.send(progress.clone()) {
                    tracing::warn!("Update progress to the main window failed: {e:?}.");
                };
            }

            if let Err(err) = update_tray_progress(&self.app, progress) {
                tracing::error!("Failed to update tray progress: {err:?}.")
            }

            self.last_progress_emit = now;
        }

        // Update tray
        if frame_events.just_transited {
            let state = self.timer.state();
            tracing::debug!("Current state: {:?}", state);

            // Publish to SharedTimerState BEFORE rebuilding the menu so
            // that any concurrent reader (e.g. a tray refresh triggered
            // by another command on its own thread) sees the latest
            // value.
            self.shared_state.set(state);

            if let Err(err) = refresh_tray_menu(&self.app) {
                tracing::error!("Failed to update tray state: {err:?}.")
            }
        }
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

/// Translate a [`RendererEvent`] to a tracing record.
///
/// Severity choices:
/// - `RenderSuccessful` / `RenderUnappliedDueToUnchanged`: `debug!`
///   (high-frequency, suppressed by default in release builds)
/// - `RenderUnappliedDueToNotStartupped`: `warn!` (renderer is in a bad
///   transient state; can spam if startup hangs)
/// - `Startup/RenderFailed`: `error!`
/// - `StartupSuccessful` / `ShutdownCompleted`: `info!` (low-frequency
///   lifecycle markers)
fn log_renderer_event(event: RendererEvent) {
    use RendererEvent::*;
    match event {
        RenderSuccessful { renderer_name } => {
            tracing::debug!(target: "renderer", renderer_name, "render successful");
        }
        RenderUnappliedDueToUnchanged { renderer_name } => {
            tracing::debug!(target: "renderer", renderer_name, "render skipped: unchanged");
        }
        RenderUnappliedDueToNotStartupped { renderer_name } => {
            tracing::warn!(target: "renderer", renderer_name, "render skipped: not yet started");
        }
        RenderFailed { renderer_name, error } => {
            tracing::error!(target: "renderer", renderer_name, %error, "render failed");
        }
        StartupSuccessful { renderer_name } => {
            tracing::info!(target: "renderer", renderer_name, "startup successful");
        }
        StartupFailed { renderer_name, error } => {
            tracing::error!(target: "renderer", renderer_name, %error, "startup failed");
        }
        ShutdownCompleted { renderer_name } => {
            tracing::info!(
                target: "renderer",
                renderer_name,
                "shutdown completed (drained outside of normal shutdown path)",
            );
        }
    }
}
