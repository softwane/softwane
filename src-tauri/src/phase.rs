use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SessionPhase {
    Idle,
    Forward {
        elapsed_ms: u64,
        target_duration_ms: u64,
    },
    Settling {
        elapsed_ms: u64,
    },
    Sabi,
    Reverse {
        elapsed_ms: u64,
        // max_duration_ms should be a config
        max_duration_ms: u64,
    },
}

impl SessionPhase {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Forward { .. } => "Forward",
            Self::Settling { .. } => "Settling",
            Self::Sabi => "Sabi",
            Self::Reverse { .. } => "Reverse",
        }
    }

    pub fn is_idle(&self) -> bool {
        matches!(self, Self::Idle)
    }

    pub fn is_active(&self) -> bool {
        !matches!(self, Self::Idle)
    }

    /// Returns the forward progress ratio (0.0..1.0) if in Forward phase.
    pub fn forward_progress(&self) -> Option<f32> {
        match self {
            Self::Forward {
                elapsed_ms,
                target_duration_ms,
            } => {
                let target = (*target_duration_ms).max(1) as f32;
                Some((*elapsed_ms as f32 / target).clamp(0.0, 1.0))
            }
            _ => None,
        }
    }
}
