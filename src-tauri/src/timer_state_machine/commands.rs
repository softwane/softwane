use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum StateCommand {
    StartSession {
        target_duration_ms: u64,
    },
    TakeBreakNow,
    StopSession,
    EnterPreview,
    ExitPreview,
    UpdatePreviewProgress {
        progress: f64,
    },
    UpdateSettlingDuration {
        duration_ms: u64,
    },
    UpdateReverseDuration {
        duration_ms: u64,
    },
}
