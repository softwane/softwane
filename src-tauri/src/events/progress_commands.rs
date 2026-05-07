use serde::{Deserialize, Serialize};
use tauri::{ipc::Channel, State};

use crate::engine::EngineHandle;
use crate::timer_state_machine::TimerState;
use super::{EngineEvent, CommandError, forward_engine};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ProgressPayload {
    pub timer_state: TimerState,
}

pub enum ProgressCommand {
    RegisterChannel(Channel<ProgressPayload>),
}

impl std::fmt::Debug for ProgressCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RegisterChannel(ch) => write!(f, "RegisterChannel(Channel(id = {}))", ch.id()),
        }
    }
}

#[tauri::command]
pub async fn register_progress_channel(engine_handle: State<'_, EngineHandle>, channel: Channel<ProgressPayload>) -> Result<(), CommandError> {
    forward_engine(engine_handle.tx.clone(), EngineEvent::Progress(ProgressCommand::RegisterChannel(channel))).await
}
