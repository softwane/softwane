use crate::configs::AppConfig;
use crate::events::StateCommand;
use crate::engine::FrameEvents;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimerState {
    Idle,
    /// Frontend-driven preview: `progress` is in 0.0–1.0, controlled by the config slider.
    Preview {
        progress: f64,
    },
    Progress {
        elapsed_ms: u64,
        target_duration_ms: u64,
    },
    Settling {
        elapsed_ms: u64,
        target_duration_ms: u64,
    },
    Sabi,
    Reverse {
        elapsed_ms: u64,
        target_duration_ms: u64,
    },
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

pub struct TimerStateMachine {
    state: TimerState,
}

impl TimerStateMachine {
    pub fn new() -> Self {
        Self {
            state: TimerState::Idle,
        }
    }

    pub fn reset(&mut self) {
        self.state = TimerState::Idle;
    }

    fn transit(&mut self, new_state: TimerState) {
        self.state = new_state;
    }

    fn apply(&mut self, command: &StateCommand, config: &AppConfig, just_transited_flag: &mut bool) {
        match command {
            StateCommand::StartSession { target_duration_ms } => {
                match self.state {
                    TimerState::Idle => {
                        self.transit(TimerState::Progress {
                            elapsed_ms: 0,
                            target_duration_ms: *target_duration_ms,
                        });
                        *just_transited_flag = true;
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
                            target_duration_ms: config.settling_duration_ms,
                        });
                        *just_transited_flag = true;
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
                            target_duration_ms: config.reverse_duration_ms,
                        });
                        *just_transited_flag = true;
                    }
                    _ => {
                        eprintln!(
                            "[TimerStateMachine] ignoring StopSession: expected Progress/Settling/Sabi, got {}",
                            self.state.label()
                        );
                    }
                }
            }
            StateCommand::EnterPreview => {
                match self.state {
                    TimerState::Idle => {
                        self.transit(TimerState::Preview { progress: 0.0 });
                        *just_transited_flag = true;
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
                        *just_transited_flag = true;
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
                            self.transit(TimerState::Preview { 
                                progress: 0.0
                            });
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

    pub fn handle_commands(
        &mut self,
        commands: &mut Vec<StateCommand>,
        config: &AppConfig,
        just_transited_flag: &mut bool,
    ) {
        for command in commands.drain(..) {
            self.apply(&command, config, just_transited_flag);
        }
    }

    pub fn tick(&mut self, dt_ms: u64, frame_events: &mut FrameEvents) {
        if frame_events.just_transited {
            return;
        }
        match self.state {
            TimerState::Idle | TimerState::Sabi => { /* quiescent */ }
            TimerState::Preview { .. } => { /* driven by frontend slider, not by elapsed time */ }
            TimerState::Progress {
                elapsed_ms,
                target_duration_ms,
            } => {
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
            TimerState::Settling {
                elapsed_ms,
                target_duration_ms,
            } => {
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
            TimerState::Reverse {
                elapsed_ms,
                target_duration_ms,
            } => {
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

    fn config() -> AppConfig {
        AppConfig {
            settling_duration_ms: 5000,
            reverse_duration_ms: 3000,
        }
    }

    fn empty_fe() -> FrameEvents {
        FrameEvents::default()
    }

    fn start_command(duration_ms: u64) -> StateCommand {
        StateCommand::StartSession {
            target_duration_ms: duration_ms,
        }
    }

    fn drain_and_handle(
        t: &mut TimerStateMachine,
        fe: &mut FrameEvents,
        cfg: &AppConfig,
    ) {
        t.handle_commands(&mut fe.state_commands, cfg, &mut fe.just_transited);
    }

    #[test]
    fn idle_rejects_non_start_commands() {
        let mut t = TimerStateMachine::new();
        let cfg = config();

        t.tick(0, &mut empty_fe());
        assert_eq!(t.state(), TimerState::Idle);

        // TakeBreakNow ignored at Idle
        let mut fe = empty_fe();
        fe.state_commands.push(StateCommand::TakeBreakNow);
        drain_and_handle(&mut t, &mut fe, &cfg);
        t.tick(0, &mut fe);
        assert_eq!(t.state(), TimerState::Idle);

        // StopSession ignored at Idle
        let mut fe = empty_fe();
        fe.state_commands.push(StateCommand::StopSession);
        drain_and_handle(&mut t, &mut fe, &cfg);
        t.tick(0, &mut fe);
        assert_eq!(t.state(), TimerState::Idle);
    }

    #[test]
    fn idle_to_progress_to_sabi_to_reverse_to_idle() {
        let mut t = TimerStateMachine::new();
        let cfg = config();

        // Start
        let mut fe = empty_fe();
        fe.state_commands.push(start_command(100));
        drain_and_handle(&mut t, &mut fe, &cfg);
        t.tick(0, &mut fe);
        assert!(matches!(t.state(), TimerState::Progress { .. }));

        // Advance past duration → Sabi
        let mut fe = empty_fe();
        t.tick(150, &mut fe);
        assert_eq!(t.state(), TimerState::Sabi);

        // Stop → Reverse
        let mut fe = empty_fe();
        fe.state_commands.push(StateCommand::StopSession);
        drain_and_handle(&mut t, &mut fe, &cfg);
        t.tick(0, &mut fe);
        assert!(matches!(t.state(), TimerState::Reverse { .. }));

        // Advance past reverse → Idle
        let mut fe = empty_fe();
        t.tick(4000, &mut fe);
        assert_eq!(t.state(), TimerState::Idle);
    }

    #[test]
    fn progress_take_break_now_to_settling_to_sabi() {
        let mut t = TimerStateMachine::new();
        let cfg = config();

        // Start
        let mut fe = empty_fe();
        fe.state_commands.push(start_command(60_000));
        drain_and_handle(&mut t, &mut fe, &cfg);
        t.tick(0, &mut fe);
        assert!(matches!(t.state(), TimerState::Progress { .. }));

        // TakeBreakNow before natural end
        let mut fe = empty_fe();
        fe.state_commands.push(StateCommand::TakeBreakNow);
        drain_and_handle(&mut t, &mut fe, &cfg);
        t.tick(0, &mut fe);
        assert!(matches!(t.state(), TimerState::Settling { .. }));

        // Advance to end of settling → Sabi
        let mut fe = empty_fe();
        t.tick(6000, &mut fe);
        assert_eq!(t.state(), TimerState::Sabi);
    }

    #[test]
    fn preview_enter_exit() {
        let mut t = TimerStateMachine::new();
        let cfg = config();

        // Enter preview from Idle
        let mut fe = empty_fe();
        fe.state_commands.push(StateCommand::EnterPreview);
        drain_and_handle(&mut t, &mut fe, &cfg);
        t.tick(0, &mut fe);
        assert!(matches!(t.state(), TimerState::Preview { .. }));

        // Update progress
        let mut fe = empty_fe();
        fe.state_commands
            .push(StateCommand::UpdatePreviewProgress { progress: 0.5 });
        drain_and_handle(&mut t, &mut fe, &cfg);
        t.tick(0, &mut fe);
        match t.state() {
            TimerState::Preview { progress } => {
                assert!((progress - 0.5).abs() < 0.001);
            }
            other => panic!("expected Preview, got {:?}", other),
        }

        // Exit preview → Idle
        let mut fe = empty_fe();
        fe.state_commands.push(StateCommand::ExitPreview);
        drain_and_handle(&mut t, &mut fe, &cfg);
        t.tick(0, &mut fe);
        assert_eq!(t.state(), TimerState::Idle);
    }

    #[test]
    fn preview_rejects_start_session() {
        let mut t = TimerStateMachine::new();
        let cfg = config();

        // Enter preview
        let mut fe = empty_fe();
        fe.state_commands.push(StateCommand::EnterPreview);
        drain_and_handle(&mut t, &mut fe, &cfg);
        t.tick(0, &mut fe);

        // Try to start session → ignored
        let mut fe = empty_fe();
        fe.state_commands.push(start_command(100));
        drain_and_handle(&mut t, &mut fe, &cfg);
        t.tick(0, &mut fe);
        assert!(matches!(t.state(), TimerState::Preview { .. }));
    }

    #[test]
    fn preview_noop_on_time_advance() {
        let mut t = TimerStateMachine::new();
        let cfg = config();

        // Enter preview
        let mut fe = empty_fe();
        fe.state_commands.push(StateCommand::EnterPreview);
        drain_and_handle(&mut t, &mut fe, &cfg);
        t.tick(0, &mut fe);

        // Time passes, preview progress unchanged
        let mut fe = empty_fe();
        t.tick(10_000, &mut fe);
        match t.state() {
            TimerState::Preview { progress } => {
                assert!((progress - 0.0).abs() < 0.001);
            }
            other => panic!("expected Preview, got {:?}", other),
        }
    }

    #[test]
    fn reset_from_any_state_to_idle() {
        let mut t = TimerStateMachine::new();
        let cfg = config();

        // Go to Progress
        let mut fe = empty_fe();
        fe.state_commands.push(start_command(100));
        drain_and_handle(&mut t, &mut fe, &cfg);
        t.tick(0, &mut fe);
        assert!(matches!(t.state(), TimerState::Progress { .. }));

        // Reset
        t.reset();
        assert_eq!(t.state(), TimerState::Idle);
    }

    #[test]
    fn preview_reset_to_idle() {
        let mut t = TimerStateMachine::new();
        let cfg = config();

        // Enter preview
        let mut fe = empty_fe();
        fe.state_commands.push(StateCommand::EnterPreview);
        drain_and_handle(&mut t, &mut fe, &cfg);
        t.tick(0, &mut fe);

        t.reset();
        assert_eq!(t.state(), TimerState::Idle);
    }

    #[test]
    fn update_preview_progress_clamped() {
        let mut t = TimerStateMachine::new();
        let cfg = config();

        let mut fe = empty_fe();
        fe.state_commands.push(StateCommand::EnterPreview);
        drain_and_handle(&mut t, &mut fe, &cfg);
        t.tick(0, &mut fe);

        // Out of range values are clamped
        let mut fe = empty_fe();
        fe.state_commands
            .push(StateCommand::UpdatePreviewProgress { progress: 1.5 });
        drain_and_handle(&mut t, &mut fe, &cfg);
        t.tick(0, &mut fe);
        match t.state() {
            TimerState::Preview { progress } => assert!((progress - 1.0).abs() < 0.001),
            other => panic!("expected Preview, got {:?}", other),
        }

        let mut fe = empty_fe();
        fe.state_commands
            .push(StateCommand::UpdatePreviewProgress { progress: -0.3 });
        drain_and_handle(&mut t, &mut fe, &cfg);
        t.tick(0, &mut fe);
        match t.state() {
            TimerState::Preview { progress } => assert!((progress - 0.0).abs() < 0.001),
            other => panic!("expected Preview, got {:?}", other),
        }
    }

    #[test]
    fn just_transited_skips_tick() {
        let mut t = TimerStateMachine::new();
        let cfg = config();

        // Start session
        let mut fe = empty_fe();
        fe.state_commands.push(start_command(100));
        drain_and_handle(&mut t, &mut fe, &cfg);
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
