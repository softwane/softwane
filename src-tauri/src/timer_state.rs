use std::mem::discriminant;
use crate::configs::AppConfig;
use crate::commands::StateCommand;
use crate::engine::FrameFlags;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerState {
    Idle,
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
            Self::Progress { elapsed_ms, .. } => *elapsed_ms,
            Self::Settling { elapsed_ms, .. } => *elapsed_ms,
            Self::Reverse { elapsed_ms, .. } =>*elapsed_ms,
            Self::Idle => 0,
            Self::Sabi => 0,
        }
    }

    pub fn target_duration_ms(&self) -> u64 {
        match self {
            Self::Progress { target_duration_ms, .. } => *target_duration_ms,
            Self::Settling { target_duration_ms, .. } => *target_duration_ms,
            Self::Reverse { target_duration_ms, .. } => *target_duration_ms,
            Self::Idle => u64::MAX,
            Self::Sabi => u64::MAX,
        }
    }
}

pub const IDLE_STATE: TimerState = TimerState::Idle;
pub const PROGRESS_STATE: TimerState = TimerState::Progress { elapsed_ms: 0, target_duration_ms: 0 };
pub const SETTLING_STATE: TimerState = TimerState::Settling { elapsed_ms: 0, target_duration_ms: 0 };
pub const SABI_STATE: TimerState = TimerState::Sabi;
pub const REVERSE_STATE: TimerState = TimerState::Reverse { elapsed_ms: 0, target_duration_ms: 0 };

impl TimerState {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Progress { .. } => "Progress",
            Self::Settling { .. } => "Settling",
            Self::Sabi => "Sabi",
            Self::Reverse { .. } => "Reverse",
        }
    }
}

pub struct TimerStateMachine {
    state: TimerState,
    cerrent_elapsed_ms: u64,
}

impl TimerStateMachine {
    pub fn new() -> Self {
        Self {
            state: TimerState::Idle,
            cerrent_elapsed_ms: 0,
        }
    }

    fn transit(&mut self, new_state: TimerState) {
        if discriminant(&self.state) != discriminant(&new_state) {
            self.cerrent_elapsed_ms = 0;
        }
        self.state = new_state;
    }

    fn handle_command(&mut self, command: &StateCommand, config: &AppConfig, frame_flags: &mut FrameFlags) {
        // TODO: Add logging for `_ => {}`
        match command {
            StateCommand::StartSession { target_duration_ms } => {
                match self.state {
                    TimerState::Idle => {
                        self.transit(TimerState::Progress {
                            elapsed_ms: 0,
                            target_duration_ms: *target_duration_ms,
                        });
                        frame_flags.just_transited = true;
                    }
                    _ => {}
                }
            },
            StateCommand::TakeBreakNow => {
                match self.state {
                    TimerState::Progress { .. } => {
                        self.transit(TimerState::Settling {
                            elapsed_ms: 0,
                            target_duration_ms: config.settling_duration_ms,
                        });
                        frame_flags.just_transited = true;
                    }
                    _ => {}
                }
            },
            StateCommand::StopSession => {
                match self.state {
                    TimerState::Progress { .. }
                    | TimerState::Settling { .. }
                    | TimerState::Sabi => {
                        self.transit(TimerState::Reverse {
                            elapsed_ms: 0,
                            target_duration_ms: config.reverse_duration_ms,
                        });
                        frame_flags.just_transited = true;
                    }
                    _ => {}
                }
            },
        }
    }

    fn update(&mut self, dt_ms: u64, frame_flags: &mut FrameFlags) {
        if !frame_flags.just_transited { 
            self.cerrent_elapsed_ms += dt_ms;
            match self.state {
                TimerState::Progress { elapsed_ms, target_duration_ms } => {
                    if elapsed_ms + dt_ms >= target_duration_ms {
                        self.transit(TimerState::Sabi);
                        frame_flags.just_transited = true;
                    } else {
                        self.transit(TimerState::Progress {
                            elapsed_ms: elapsed_ms + dt_ms,
                            target_duration_ms: target_duration_ms,
                        });
                    }
                },
                TimerState::Settling { elapsed_ms , target_duration_ms } => {
                    if elapsed_ms + dt_ms >= target_duration_ms {
                        self.transit(TimerState::Sabi);
                        frame_flags.just_transited = true;
                    } else {
                        self.transit(TimerState::Settling {
                            elapsed_ms: elapsed_ms + dt_ms,
                            target_duration_ms: target_duration_ms,
                        });
                    }
                },
                TimerState::Reverse { elapsed_ms , target_duration_ms } => {
                    if elapsed_ms + dt_ms >= target_duration_ms {
                        self.transit(TimerState::Idle);
                        frame_flags.just_transited = true;
                    } else {
                        self.transit(TimerState::Reverse {
                            elapsed_ms: elapsed_ms + dt_ms,
                            target_duration_ms: target_duration_ms,
                        });
                    }
                },
                _ => {}
            }
        }
    }

    pub fn tick(&mut self, dt_ms: u64, frame_flags: &mut FrameFlags, commands: &Vec<StateCommand>, config: &AppConfig,) {
        for command in commands {
            self.handle_command(command, config, frame_flags);
        }
        self.update(dt_ms, frame_flags);
    }

    pub fn current_elapsed_ms(&self) -> u64 {
        self.cerrent_elapsed_ms
    }

    pub fn state(&self) -> TimerState {
        self.state
    }
}
