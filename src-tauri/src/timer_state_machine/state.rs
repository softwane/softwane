#![allow(dead_code)]

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
