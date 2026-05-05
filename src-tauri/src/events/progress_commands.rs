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

impl std::fmt::Debug for ProgressCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RegisterChannel(ch) => write!(f, "RegisterChannel(Channel(id = {}))", ch.id()),
            Self::ClearChannel => f.debug_tuple("ClearChannel").finish(),
        }
    }
}
