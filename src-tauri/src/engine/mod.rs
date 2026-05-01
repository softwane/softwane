//! Engine loop timing: `std::thread::sleep` has poor resolution on some OSes
//! (notably Windows ~15.6 ms by default), which can jitter a ~60 Hz tick.
//! If that becomes a problem, switch to [`spin_sleep`](https://crates.io/crates/spin_sleep)
//! (already a dependency) for frame pacing.

mod frame_events;
pub use frame_events::FrameEvents;

use std::{
    sync::Mutex,
    thread::JoinHandle,
    time::{Duration, Instant},
};

use tauri::AppHandle;
use tokio::sync::mpsc::{Receiver, Sender};

use crate::events::EngineEvent;

const TARGET_FRAME_INTERVAL: Duration = Duration::from_micros(16_667);

pub struct Engine {
    event_rx: Receiver<EngineEvent>,

    app: AppHandle,

    last_frame_at: Instant,
}

impl Engine {
    pub fn new(app: AppHandle, event_rx: Receiver<EngineEvent>) -> Self {
        Self {
            app,
            event_rx,
            last_frame_at: Instant::now(),
        }
    }

    pub fn run(mut self) {
        loop {
            let frame_started_at = Instant::now();
            let _dt_ms = frame_started_at
                .saturating_duration_since(self.last_frame_at)
                .as_millis() as u64;
            self.last_frame_at = frame_started_at;

            let frame_events = self.collect_events();
            if frame_events.shutdown_requested {
                break;
            }

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

    #[allow(dead_code)]
    fn app(&self) -> &AppHandle {
        &self.app
    }
}

pub struct EngineHandle {
    pub tx: Sender<EngineEvent>,
    pub join: Mutex<Option<JoinHandle<()>>>,
}
