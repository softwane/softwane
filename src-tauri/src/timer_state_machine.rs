use serde::{Deserialize, Serialize};

use tauri::Wry;
use tauri_plugin_store::Store;

use crate::events::StateCommand;
use crate::engine::FrameEvents;

// ── TimerState ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimerState {
    Idle,
    /// Frontend-driven preview: `progress` is in 0.0–1.0, controlled by the config slider.
    Preview { progress: f64 },
    Progress { elapsed_ms: u64, target_duration_ms: u64 },
    Settling { elapsed_ms: u64, target_duration_ms: u64 },
    Sabi,
    Reverse { elapsed_ms: u64, target_duration_ms: u64 },
}

impl TimerState {
    pub fn elapsed_ms(&self) -> u64 {
        match self {
            Self::Progress { elapsed_ms, .. }
            | Self::Settling { elapsed_ms, .. }
            | Self::Reverse { elapsed_ms, .. } => *elapsed_ms,
            Self::Idle | Self::Sabi | Self::Preview { .. } => 0,
        }
    }

    pub fn target_duration_ms(&self) -> u64 {
        match self {
            Self::Progress { target_duration_ms, .. }
            | Self::Settling { target_duration_ms, .. }
            | Self::Reverse { target_duration_ms, .. } => *target_duration_ms,
            Self::Idle | Self::Sabi | Self::Preview { .. } => u64::MAX,
        }
    }

    pub const fn label(&self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Preview { .. } => "Preview",
            Self::Progress { .. } => "Progress",
            Self::Settling { .. } => "Settling",
            Self::Sabi => "Sabi",
            Self::Reverse { .. } => "Reverse",
        }
    }
}

pub const IDLE_STATE: TimerState = TimerState::Idle;
pub const PREVIEW_STATE: TimerState = TimerState::Preview { progress: 0.0 };
pub const PROGRESS_STATE: TimerState = TimerState::Progress { elapsed_ms: 0, target_duration_ms: 0 };
pub const SETTLING_STATE: TimerState = TimerState::Settling { elapsed_ms: 0, target_duration_ms: 0 };
pub const SABI_STATE: TimerState = TimerState::Sabi;
pub const REVERSE_STATE: TimerState = TimerState::Reverse { elapsed_ms: 0, target_duration_ms: 0 };

// ── Constants ────────────────────────────────────────────────────────

pub const DEFAULT_SETTLING_DURATION_MS: u64 = 5_000;
pub const DEFAULT_REVERSE_DURATION_MS: u64 = 2_000;

// ── Persistent config ────────────────────────────────────────────────

pub const STORE_KEY_TIMER: &str = "timer";

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct PersistentTimerConfig {
    pub settling_duration_ms: u64,
    pub reverse_duration_ms: u64,
}

impl Default for PersistentTimerConfig {
    fn default() -> Self {
        Self {
            settling_duration_ms: DEFAULT_SETTLING_DURATION_MS,
            reverse_duration_ms: DEFAULT_REVERSE_DURATION_MS,
        }
    }
}

pub fn store_defaults() -> Vec<(String, serde_json::Value)> {
    vec![(
        STORE_KEY_TIMER.into(),
        serde_json::to_value(PersistentTimerConfig::default())
            .expect("PersistentTimerConfig serialization is infallible"),
    )]
}

// ── TimerStateMachine ────────────────────────────────────────────────

pub struct TimerStateMachine {
    state: TimerState,
    settling_duration_ms: u64,
    reverse_duration_ms: u64,
}

impl TimerStateMachine {
    pub fn new(config: PersistentTimerConfig) -> Self {
        Self {
            state: TimerState::Idle,
            settling_duration_ms: config.settling_duration_ms,
            reverse_duration_ms: config.reverse_duration_ms,
        }
    }

    pub fn load_from_store(store: &Store<Wry>) -> Self {
        let config = store
            .get(STORE_KEY_TIMER)
            .and_then(|v| serde_json::from_value::<PersistentTimerConfig>(v).ok())
            .unwrap();
        Self::new(config)
    }

    pub fn persist(&self, store: &Store<Wry>) {
        let config = PersistentTimerConfig {
            settling_duration_ms: self.settling_duration_ms,
            reverse_duration_ms: self.reverse_duration_ms,
        };
        store.set(STORE_KEY_TIMER, serde_json::to_value(config).unwrap());
    }

    pub fn reset(&mut self) {
        self.state = TimerState::Idle;
    }

    fn transit(&mut self, new_state: TimerState) {
        self.state = new_state;
    }

    fn apply(&mut self, command: &StateCommand, just_transited: &mut bool) {
        match command {
            StateCommand::StartSession { target_duration_ms } => {
                match self.state {
                    TimerState::Idle => {
                        self.transit(TimerState::Progress {
                            elapsed_ms: 0,
                            target_duration_ms: *target_duration_ms,
                        });
                        *just_transited = true;
                    }
                    TimerState::Preview { .. } => {
                        eprintln!(
                            "[TimerStateMachine] ignoring StartSession: currently in Preview; exit preview first"
                        );
                    }
                    _ => {
                        eprintln!(
                            "[TimerStateMachine] ignoring StartSession: expected Idle, got {}",
                            self.state.label()
                        );
                    }
                }
            }
            StateCommand::TakeBreakNow => {
                match self.state {
                    TimerState::Progress { .. } => {
                        self.transit(TimerState::Settling {
                            elapsed_ms: 0,
                            target_duration_ms: self.settling_duration_ms,
                        });
                        *just_transited = true;
                    }
                    _ => {
                        eprintln!(
                            "[TimerStateMachine] ignoring TakeBreakNow: expected Progress, got {}",
                            self.state.label()
                        );
                    }
                }
            }
            StateCommand::StopSession => {
                match self.state {
                    TimerState::Progress { .. }
                    | TimerState::Settling { .. }
                    | TimerState::Sabi => {
                        self.transit(TimerState::Reverse {
                            elapsed_ms: 0,
                            target_duration_ms: self.reverse_duration_ms,
                        });
                        *just_transited = true;
                    }
                    _ => {
                        eprintln!(
                            "[TimerStateMachine] ignoring StopSession: expected Progress/Settling/Sabi, got {}",
                            self.state.label()
                        );
                    }
                }
            }
            StateCommand::UpdateSettlingDuration { duration_ms } => {
                self.settling_duration_ms = *duration_ms;
                if let TimerState::Settling { elapsed_ms, .. } = self.state {
                    self.state = TimerState::Settling {
                        elapsed_ms,
                        target_duration_ms: *duration_ms,
                    };
                }
            }
            StateCommand::UpdateReverseDuration { duration_ms } => {
                self.reverse_duration_ms = *duration_ms;
                if let TimerState::Reverse { elapsed_ms, .. } = self.state {
                    self.state = TimerState::Reverse {
                        elapsed_ms,
                        target_duration_ms: *duration_ms,
                    };
                }
            }
            StateCommand::EnterPreview => {
                match self.state {
                    TimerState::Idle => {
                        self.transit(TimerState::Preview { progress: 0.0 });
                        *just_transited = true;
                    }
                    _ => {
                        eprintln!(
                            "[TimerStateMachine] ignoring EnterPreview: expected Idle, got {}",
                            self.state.label()
                        );
                    }
                }
            }
            StateCommand::ExitPreview => {
                match self.state {
                    TimerState::Preview { .. } => {
                        self.transit(TimerState::Idle);
                        *just_transited = true;
                    }
                    _ => {
                        eprintln!(
                            "[TimerStateMachine] ignoring ExitPreview: expected Preview, got {}",
                            self.state.label()
                        );
                    }
                }
            }
            StateCommand::UpdatePreviewProgress { progress } => {
                match self.state {
                    TimerState::Preview { .. } => {
                        if progress.is_nan() {
                            self.transit(TimerState::Preview { progress: 0.0 });
                        }
                        self.transit(TimerState::Preview {
                            progress: progress.clamp(0.0, 1.0),
                        });
                    }
                    _ => {
                        eprintln!(
                            "[TimerStateMachine] ignoring UpdatePreviewProgress: expected Preview, got {}",
                            self.state.label()
                        );
                    }
                }
            }
        }
    }

    /// Drain state commands and apply them. Sets
    /// `frame_events.need_persist.timer_state_machine = true` when a
    /// config-modifying command (`UpdateSettlingDuration` /
    /// `UpdateReverseDuration`) is processed.
    pub fn handle_commands(&mut self, frame_events: &mut FrameEvents) {
        for command in frame_events.state_commands.drain(..) {
            let needs_persist = matches!(
                command,
                StateCommand::UpdateSettlingDuration { .. }
                    | StateCommand::UpdateReverseDuration { .. }
            );
            self.apply(&command, &mut frame_events.just_transited);
            if needs_persist {
                frame_events.need_persist.timer_state_machine = true;
            }
        }
    }

    pub fn tick(&mut self, dt_ms: u64, frame_events: &mut FrameEvents) {
        if frame_events.just_transited {
            return;
        }
        match self.state {
            TimerState::Idle | TimerState::Sabi => { /* quiescent */ }
            TimerState::Preview { .. } => { /* driven by frontend slider */ }
            TimerState::Progress { elapsed_ms, target_duration_ms } => {
                if elapsed_ms + dt_ms >= target_duration_ms {
                    self.transit(TimerState::Sabi);
                    frame_events.just_transited = true;
                } else {
                    self.transit(TimerState::Progress {
                        elapsed_ms: elapsed_ms + dt_ms,
                        target_duration_ms,
                    });
                }
            }
            TimerState::Settling { elapsed_ms, target_duration_ms } => {
                if elapsed_ms + dt_ms >= target_duration_ms {
                    self.transit(TimerState::Sabi);
                    frame_events.just_transited = true;
                } else {
                    self.transit(TimerState::Settling {
                        elapsed_ms: elapsed_ms + dt_ms,
                        target_duration_ms,
                    });
                }
            }
            TimerState::Reverse { elapsed_ms, target_duration_ms } => {
                if elapsed_ms + dt_ms >= target_duration_ms {
                    self.transit(TimerState::Idle);
                    frame_events.just_transited = true;
                } else {
                    self.transit(TimerState::Reverse {
                        elapsed_ms: elapsed_ms + dt_ms,
                        target_duration_ms,
                    });
                }
            }
        }
    }

    pub fn state(&self) -> TimerState {
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> PersistentTimerConfig {
        PersistentTimerConfig {
            settling_duration_ms: 5000,
            reverse_duration_ms: 3000,
        }
    }

    fn empty_fe() -> FrameEvents {
        FrameEvents::default()
    }

    fn start_command(duration_ms: u64) -> StateCommand {
        StateCommand::StartSession { target_duration_ms: duration_ms }
    }

    fn settling_command(duration_ms: u64) -> StateCommand {
        StateCommand::UpdateSettlingDuration { duration_ms }
    }

    fn reverse_command(duration_ms: u64) -> StateCommand {
        StateCommand::UpdateReverseDuration { duration_ms }
    }

    #[test]
    fn update_settling_duration_while_settling() {
        let mut t = TimerStateMachine::new(config());

        let mut fe = empty_fe();
        fe.state_commands.push(start_command(60_000));
        t.handle_commands(&mut fe);
        t.tick(0, &mut fe);
        let mut fe = empty_fe();
        fe.state_commands.push(StateCommand::TakeBreakNow);
        t.handle_commands(&mut fe);
        t.tick(0, &mut fe);
        assert!(matches!(t.state(), TimerState::Settling { target_duration_ms: 5000, .. }));

        let mut fe = empty_fe();
        fe.state_commands.push(settling_command(2000));
        t.handle_commands(&mut fe);
        t.tick(0, &mut fe);

        assert_eq!(t.settling_duration_ms, 2000);
        assert!(matches!(t.state(), TimerState::Settling { target_duration_ms: 2000, .. }));
        assert!(fe.need_persist.timer_state_machine);
    }

    #[test]
    fn update_settling_duration_while_idle() {
        let mut t = TimerStateMachine::new(config());
        assert_eq!(t.settling_duration_ms, 5000);

        let mut fe = empty_fe();
        fe.state_commands.push(settling_command(8000));
        t.handle_commands(&mut fe);

        assert_eq!(t.settling_duration_ms, 8000);
        assert_eq!(t.state(), TimerState::Idle);
        assert!(fe.need_persist.timer_state_machine);
    }

    #[test]
    fn update_reverse_duration_while_reversing() {
        let mut t = TimerStateMachine::new(config());

        let mut fe = empty_fe();
        fe.state_commands.push(start_command(100));
        t.handle_commands(&mut fe);
        t.tick(0, &mut fe);
        t.tick(150, &mut empty_fe());
        assert_eq!(t.state(), TimerState::Sabi);
        let mut fe = empty_fe();
        fe.state_commands.push(StateCommand::StopSession);
        t.handle_commands(&mut fe);
        t.tick(0, &mut fe);
        assert!(matches!(t.state(), TimerState::Reverse { target_duration_ms: 3000, .. }));

        let mut fe = empty_fe();
        fe.state_commands.push(reverse_command(5000));
        t.handle_commands(&mut fe);
        t.tick(0, &mut fe);

        assert_eq!(t.reverse_duration_ms, 5000);
        assert!(matches!(t.state(), TimerState::Reverse { target_duration_ms: 5000, .. }));
        assert!(fe.need_persist.timer_state_machine);
    }

    #[test]
    fn update_reverse_duration_while_idle() {
        let mut t = TimerStateMachine::new(config());
        assert_eq!(t.reverse_duration_ms, 3000);

        let mut fe = empty_fe();
        fe.state_commands.push(reverse_command(1000));
        t.handle_commands(&mut fe);

        assert_eq!(t.reverse_duration_ms, 1000);
        assert_eq!(t.state(), TimerState::Idle);
        assert!(fe.need_persist.timer_state_machine);
    }

    #[test]
    fn start_session_does_not_trigger_persist() {
        let mut t = TimerStateMachine::new(config());
        let mut fe = empty_fe();
        fe.state_commands.push(start_command(100));
        t.handle_commands(&mut fe);
        assert!(!fe.need_persist.timer_state_machine);
    }

    #[test]
    fn idle_rejects_non_start_commands() {
        let mut t = TimerStateMachine::new(config());
        t.tick(0, &mut empty_fe());
        assert_eq!(t.state(), TimerState::Idle);

        // TakeBreakNow ignored at Idle
        let mut fe = empty_fe();
        fe.state_commands.push(StateCommand::TakeBreakNow);
        t.handle_commands(&mut fe);
        t.tick(0, &mut fe);
        assert_eq!(t.state(), TimerState::Idle);

        // StopSession ignored at Idle
        let mut fe = empty_fe();
        fe.state_commands.push(StateCommand::StopSession);
        t.handle_commands(&mut fe);
        t.tick(0, &mut fe);
        assert_eq!(t.state(), TimerState::Idle);
    }

    #[test]
    fn idle_to_progress_to_sabi_to_reverse_to_idle() {
        let mut t = TimerStateMachine::new(config());

        // Start
        let mut fe = empty_fe();
        fe.state_commands.push(start_command(100));
        t.handle_commands(&mut fe);
        t.tick(0, &mut fe);
        assert!(matches!(t.state(), TimerState::Progress { .. }));

        // Advance past duration → Sabi
        let mut fe = empty_fe();
        t.tick(150, &mut fe);
        assert_eq!(t.state(), TimerState::Sabi);

        // Stop → Reverse
        let mut fe = empty_fe();
        fe.state_commands.push(StateCommand::StopSession);
        t.handle_commands(&mut fe);
        t.tick(0, &mut fe);
        assert!(matches!(t.state(), TimerState::Reverse { .. }));

        // Advance past reverse → Idle
        let mut fe = empty_fe();
        t.tick(4000, &mut fe);
        assert_eq!(t.state(), TimerState::Idle);
    }

    #[test]
    fn progress_take_break_now_to_settling_to_sabi() {
        let mut t = TimerStateMachine::new(config());

        // Start
        let mut fe = empty_fe();
        fe.state_commands.push(start_command(60_000));
        t.handle_commands(&mut fe);
        t.tick(0, &mut fe);
        assert!(matches!(t.state(), TimerState::Progress { .. }));

        // TakeBreakNow before natural end
        let mut fe = empty_fe();
        fe.state_commands.push(StateCommand::TakeBreakNow);
        t.handle_commands(&mut fe);
        t.tick(0, &mut fe);
        assert!(matches!(t.state(), TimerState::Settling { .. }));

        // Advance to end of settling → Sabi
        let mut fe = empty_fe();
        t.tick(6000, &mut fe);
        assert_eq!(t.state(), TimerState::Sabi);
    }

    #[test]
    fn preview_enter_exit() {
        let mut t = TimerStateMachine::new(config());

        // Enter preview from Idle
        let mut fe = empty_fe();
        fe.state_commands.push(StateCommand::EnterPreview);
        t.handle_commands(&mut fe);
        t.tick(0, &mut fe);
        assert!(matches!(t.state(), TimerState::Preview { .. }));

        // Update progress
        let mut fe = empty_fe();
        fe.state_commands.push(StateCommand::UpdatePreviewProgress { progress: 0.5 });
        t.handle_commands(&mut fe);
        t.tick(0, &mut fe);
        match t.state() {
            TimerState::Preview { progress } => assert!((progress - 0.5).abs() < 0.001),
            other => panic!("expected Preview, got {:?}", other),
        }

        // Exit preview → Idle
        let mut fe = empty_fe();
        fe.state_commands.push(StateCommand::ExitPreview);
        t.handle_commands(&mut fe);
        t.tick(0, &mut fe);
        assert_eq!(t.state(), TimerState::Idle);
    }

    #[test]
    fn preview_rejects_start_session() {
        let mut t = TimerStateMachine::new(config());

        // Enter preview
        let mut fe = empty_fe();
        fe.state_commands.push(StateCommand::EnterPreview);
        t.handle_commands(&mut fe);
        t.tick(0, &mut fe);

        // Try to start session → ignored
        let mut fe = empty_fe();
        fe.state_commands.push(start_command(100));
        t.handle_commands(&mut fe);
        t.tick(0, &mut fe);
        assert!(matches!(t.state(), TimerState::Preview { .. }));
    }

    #[test]
    fn preview_noop_on_time_advance() {
        let mut t = TimerStateMachine::new(config());

        // Enter preview
        let mut fe = empty_fe();
        fe.state_commands.push(StateCommand::EnterPreview);
        t.handle_commands(&mut fe);
        t.tick(0, &mut fe);

        // Time passes, preview progress unchanged
        let mut fe = empty_fe();
        t.tick(10_000, &mut fe);
        match t.state() {
            TimerState::Preview { progress } => assert!((progress - 0.0).abs() < 0.001),
            other => panic!("expected Preview, got {:?}", other),
        }
    }

    #[test]
    fn reset_from_any_state_to_idle() {
        let mut t = TimerStateMachine::new(config());

        // Go to Progress
        let mut fe = empty_fe();
        fe.state_commands.push(start_command(100));
        t.handle_commands(&mut fe);
        t.tick(0, &mut fe);
        assert!(matches!(t.state(), TimerState::Progress { .. }));

        // Reset
        t.reset();
        assert_eq!(t.state(), TimerState::Idle);
    }

    #[test]
    fn preview_reset_to_idle() {
        let mut t = TimerStateMachine::new(config());

        // Enter preview
        let mut fe = empty_fe();
        fe.state_commands.push(StateCommand::EnterPreview);
        t.handle_commands(&mut fe);
        t.tick(0, &mut fe);

        t.reset();
        assert_eq!(t.state(), TimerState::Idle);
    }

    #[test]
    fn update_preview_progress_clamped() {
        let mut t = TimerStateMachine::new(config());

        let mut fe = empty_fe();
        fe.state_commands.push(StateCommand::EnterPreview);
        t.handle_commands(&mut fe);
        t.tick(0, &mut fe);

        // Out of range values are clamped
        let mut fe = empty_fe();
        fe.state_commands.push(StateCommand::UpdatePreviewProgress { progress: 1.5 });
        t.handle_commands(&mut fe);
        t.tick(0, &mut fe);
        match t.state() {
            TimerState::Preview { progress } => assert!((progress - 1.0).abs() < 0.001),
            other => panic!("expected Preview, got {:?}", other),
        }

        let mut fe = empty_fe();
        fe.state_commands.push(StateCommand::UpdatePreviewProgress { progress: -0.3 });
        t.handle_commands(&mut fe);
        t.tick(0, &mut fe);
        match t.state() {
            TimerState::Preview { progress } => assert!((progress - 0.0).abs() < 0.001),
            other => panic!("expected Preview, got {:?}", other),
        }
    }

    #[test]
    fn just_transited_skips_tick() {
        let mut t = TimerStateMachine::new(config());

        // Start session
        let mut fe = empty_fe();
        fe.state_commands.push(start_command(100));
        t.handle_commands(&mut fe);
        t.tick(0, &mut fe);
        assert!(fe.just_transited);

        // With just_transited still set, tick should be skipped
        let mut fe = empty_fe();
        fe.just_transited = true;
        let state_before = t.state();
        t.tick(999, &mut fe);
        assert_eq!(t.state(), state_before);
    }
}
