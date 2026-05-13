use serde::{Deserialize, Serialize};
use tauri::{State, ipc::Channel};
use tokio::sync::mpsc::{Sender, error::{SendError, TrySendError}};

use crate::{
    engine::EngineHandle,
    timer_state_machine::{TimerState, commands::StateCommand},
    channels::commands::ChannelCommand,
    renderers::events::RendererEvent,
    commands::CommandError,
};

// ── EngineEvent ───────────────────────────────────────────────────────

#[derive(Debug)]
pub enum EngineEvent {
    State(StateCommand),
    Channel(ChannelCommand),
    Renderer(RendererEvent),
    Progress(ProgressCommand),
    ForceReset,
    Shutdown,
}

// ── Forward helpers ──────────────────────────────────────────────────

pub async fn forward_engine(
    tx: Sender<EngineEvent>,
    event: EngineEvent,
) -> Result<(), CommandError> {
    tracing::debug!("forward_engine: Sending event: {event:?}");
    tx.send(event).await?;
    Ok(())
}

pub fn forward_engine_sync(
    tx: Sender<EngineEvent>,
    event: EngineEvent,
) {
    tauri::async_runtime::spawn(async move {
        if let Err(err) = forward_engine(tx, event).await {
            tracing::error!("Failed to forward engine event: {err:?}.");
        }
    });
}

pub fn forward_engine_nowait(
    tx: Sender<EngineEvent>,
    event: EngineEvent,
) -> Result<(), CommandError> {
    match tx.try_send(event) {
        Ok(_) => Ok(()),
        Err(TrySendError::Closed(event)) => {
            Err(CommandError::EngineClosed(SendError(event)))
        },
        Err(err) => {
            Err(CommandError::EngineChannelFull(err))
        }
    }
}

// ── Progress ──────────────────────────────────────────────────────────

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
    forward_engine(engine_handle.tx.clone(), EngineEvent::Progress(RegisterChannel(channel))).await
}

#[tauri::command]
pub fn clear_progress_channel(engine_handle: State<'_, EngineHandle>) {
    forward_engine_sync(engine_handle.tx.clone(), EngineEvent::Progress(ClearChannel))
}
