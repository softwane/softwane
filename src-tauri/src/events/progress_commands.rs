use serde::Serialize;
use tauri::ipc::Channel;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProgressPayload {
    pub elapsed_ms: u64,
    pub target_duration_ms: u64,
}

pub enum ProgressCommand {
    RegisterChannel(Channel<ProgressPayload>),
    ClearChannel,
}
