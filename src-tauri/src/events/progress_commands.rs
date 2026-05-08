use serde::{Deserialize, Serialize};
use tauri::{State, ipc::Channel};

use crate::{engine::EngineHandle, events::forward_engine_sync, timer_state_machine::TimerState};
use super::{EngineEvent::Progress, CommandError, forward_engine};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ProgressPayload {
    pub timer_state: TimerState,
}

pub enum ProgressCommand {
    RegisterChannel(Channel<ProgressPayload>),
    ClearChannel,
}
use ProgressCommand::*;

impl std::fmt::Debug for ProgressCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RegisterChannel(ch) => write!(f, "RegisterChannel(Channel(id = {}))", ch.id()),
            Self::ClearChannel => f.debug_tuple("ClearChannel").finish(),
        }
    }
}

#[tauri::command]
pub async fn register_progress_channel(engine_handle: State<'_, EngineHandle>, channel: Channel<ProgressPayload>) -> Result<(), CommandError> {
    forward_engine(engine_handle.tx.clone(), Progress(RegisterChannel(channel))).await
}

#[tauri::command]
pub fn clear_progress_channel(engine_handle: State<'_, EngineHandle>) {
    forward_engine_sync(engine_handle.tx.clone(), Progress(ClearChannel))
}