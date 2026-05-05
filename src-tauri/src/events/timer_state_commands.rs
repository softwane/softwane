#[derive(Debug)]
pub enum StateCommand {
    StartSession {
        target_duration_ms: u64,
    },
    TakeBreakNow,
    StopSession,
    UpdateSettlingDuration {
        duration_ms: u64,
    },
    UpdateReverseDuration {
        duration_ms: u64,
    },
    EnterPreview,
    ExitPreview,
    UpdatePreviewProgress {
        progress: f64,
    },
}
