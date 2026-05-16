use serde::Deserialize;
use tauri::{AppHandle, Manager, Webview, ipc::JavaScriptChannelId};
use tauri_plugin_store::StoreExt;
use tokio::sync::mpsc::error::{SendError, TrySendError};

use crate::{
    channels::{SENSORY_CHANNEL_TYPES, commands::ChannelCommand, load_channel_config},
    timer_state_machine::{commands::StateCommand, load_timer_config},
    commands::CommandError,
};
use super::{EngineHandle, EngineEvent, ProgressCommandInner, StoredConfig};

#[derive(Debug, Deserialize)]
#[serde(tag = "category", content = "content", rename_all = "snake_case")]
pub enum EngineCommand {
    State(StateCommand),
    Channel(ChannelCommand),
    Progress(ProgressCommand),
    ForceReset,
    Shutdown,
}

#[derive(Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum ProgressCommand {
    RegisterChannel{ channel: JavaScriptChannelId },
    ClearChannel,
}

impl std::fmt::Debug for ProgressCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RegisterChannel{ channel: _ } => write!(f, "RegisterChannel"),
            Self::ClearChannel => write!(f, "ClearChannel"),
        }
    }
}

// ── Forward commands ──────────────────────────────────────────────────

#[tauri::command]
pub async fn command_engine(
    webview: Webview,
    command: EngineCommand,
) -> Result<(), CommandError> {
    let app = webview.app_handle().clone();
    let event = match command {
        EngineCommand::State(cmd) => EngineEvent::State(cmd),
        EngineCommand::Channel(cmd) => EngineEvent::Channel(cmd),
        EngineCommand::ForceReset => EngineEvent::ForceReset,
        EngineCommand::Shutdown => EngineEvent::Shutdown,
        EngineCommand::Progress(cmd) => {
            let window = webview.label().to_string();
            let inner_command = match cmd {
                ProgressCommand::RegisterChannel { channel} => {
                    let initialized_channel = channel.channel_on(webview);
                    ProgressCommandInner::RegisterChannel{ channel: initialized_channel, window}
                },
                ProgressCommand::ClearChannel => ProgressCommandInner::ClearChannel{ window },
            };
            EngineEvent::Progress(inner_command)
        },
    };
    tracing::debug!("command_engine: Sending command: {event:?}");
    app.state::<EngineHandle>().tx.send(event).await?;
    Ok(())
}

#[tauri::command]
pub fn command_engine_nowait(
    webview: Webview,
    command: EngineCommand,
) -> Result<(), CommandError> {
    let app = webview.app_handle().clone();
    let event = match command {
        EngineCommand::State(cmd) => EngineEvent::State(cmd),
        EngineCommand::Channel(cmd) => EngineEvent::Channel(cmd),
        EngineCommand::ForceReset => EngineEvent::ForceReset,
        EngineCommand::Shutdown => EngineEvent::Shutdown,
        EngineCommand::Progress(cmd) => {
            let window = webview.label().to_string();
            let inner_command = match cmd {
                ProgressCommand::RegisterChannel { channel, .. } => {
                    let initialized_channel = channel.channel_on(webview);
                    ProgressCommandInner::RegisterChannel{ channel: initialized_channel, window}
                },
                ProgressCommand::ClearChannel { .. } => ProgressCommandInner::ClearChannel{ window },
            };
            EngineEvent::Progress(inner_command)
        },
    };
    tracing::debug!("command_engine_nowait: Sending command: {event:?}");
    match app.state::<EngineHandle>().tx.try_send(event) {
        Ok(_) => Ok(()),
        Err(TrySendError::Closed(event)) => {
            Err(CommandError::EngineClosed(SendError(event)))
        },
        Err(err) => {
            Err(CommandError::EngineChannelFull(err))
        }
    }
}

#[tauri::command]
pub fn get_available_stored_config(app_handle: AppHandle) -> Result<StoredConfig, CommandError> {
    let store = app_handle.store("config.json")?;
    let channel_configs = SENSORY_CHANNEL_TYPES.into_iter()
        .filter(|c| c.is_available_on_this_platform())
        .map(|c| (c, load_channel_config(&store, c)))
        .collect();
    let timer_config = load_timer_config(&store);
    Ok(StoredConfig{ channel_configs, timer_config })
}
