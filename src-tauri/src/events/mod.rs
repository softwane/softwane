mod timer_state_commands;
mod channel_commands;
mod renderer_events;
mod window_commands;

use serde::Serialize;
use tauri::{AppHandle, Error as TauriError, State};
use tauri_plugin_autostart::{Error as AutostartError, ManagerExt as AutostartManagerExt};
use tauri_plugin_store::{Error as StoreError, StoreExt};
use thiserror::Error;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::{Sender, error::SendError};

use crate::channels::{SENSORY_CHANNEL_TYPES, load_channel_config};
use crate::engine::{EngineHandle, StoredConfig};
use crate::timer_state_machine::load_timer_config;

pub use self::timer_state_commands::*;
pub use self::channel_commands::*;
pub use self::renderer_events::*;
pub use self::window_commands::*;

pub const STORE_KEY_LAST_CRASH: &str = "program_last_crash";

#[derive(Debug)]
pub enum EngineEvent {
    State(StateCommand),
    Channel(ChannelCommand),
    Renderer(RendererEvent),
    Progress(ProgressCommand),
    ForceReset,
    Shutdown,
}

#[derive(Debug, Error)]
pub enum CommandError {
    #[error("Engine event channel closed when: {0}")]
    EngineClosed(#[from] SendError<EngineEvent>),
    #[error("Engine event channel was full when: {0}")]
    EngineChannelFull(#[source] TrySendError<EngineEvent>),
    #[error("Trying to create or to load a store failed: {0}")]
    StoreError(#[from] StoreError),
    #[error("Creating window failed: {0}")]
    CreateWindowFailed(#[source] TauriError),
    #[error("Showing window failed: {0}")]
    ShowWindowFailed(#[source] TauriError),
    #[error("Closing window failed: {0}")]
    CloseWindowFailed(#[source] TauriError),
    #[error("Hiding window failed: {0}")]
    HideWindowFailed(#[source] TauriError),
    #[error("{0}")]
    OtherWindowError(#[source] TauriError),
    #[error("{0}")]
    BadArguments(String),
    #[error("{0}")]
    AutostartError(#[from] AutostartError),
}

impl Serialize for CommandError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string().as_ref())
    }
}

async fn forward_engine(
    tx: Sender<EngineEvent>,
    event: EngineEvent,
) -> Result<(), CommandError> {
    tracing::debug!("forward_engine: Sending event: {event:?}");
    tx.send(event).await?;
    Ok(())
}

pub(super) fn forward_engine_sync(
    tx: Sender<EngineEvent>,
    event: EngineEvent,
)  {
    tauri::async_runtime::spawn(async move {
        if let Err(err) = forward_engine(tx, event).await {
            tracing::error!("Failed to forward engine event from tray: {err:?}.");
        }
    });
}

fn forward_engine_nowait(
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

#[tauri::command]
pub fn get_available_stored_config(app_handle: AppHandle) -> Result<StoredConfig, CommandError> {
    let store = app_handle.store("config.json")?;
    let channel_configs = SENSORY_CHANNEL_TYPES.into_iter()
        .filter(|c| c.is_available_on_this_platform())
        .map(|c| (c, load_channel_config(&store, c)))
        .collect();
    let timer_config = load_timer_config(&store);
    Ok(StoredConfig { channel_configs, timer_config })
}

#[tauri::command]
pub async fn force_reset(engine_handle: State<'_, EngineHandle>) -> Result<(), CommandError> {
    forward_engine(engine_handle.tx.clone(), EngineEvent::ForceReset).await
}

// ── Crash ───────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_last_crash(app_handle: AppHandle) -> Result<Option<serde_json::Value>, CommandError> {
    let store = app_handle.store("config.json")?;
    Ok(store.get(STORE_KEY_LAST_CRASH))
}

#[tauri::command]
pub fn acknowledge_crash(app_handle: AppHandle) -> Result<(), CommandError> {
    let store = app_handle.store("config.json")?;
    store.delete(STORE_KEY_LAST_CRASH);
    Ok(())
}

// ── Autostart ────────────────────────────────────────────────────────

#[tauri::command]
pub fn set_autostart_enabled(app_handle: AppHandle, enabled: bool) -> Result<(), CommandError> {
    let m = app_handle.autolaunch();
    if enabled {
        m.enable()?;
        Ok(())
    } else {
        m.disable()?;
        Ok(())
    }
}

#[tauri::command]
pub fn is_autostart_enabled(app_handle: AppHandle) -> Result<bool, CommandError> {
    Ok(app_handle.autolaunch().is_enabled()?)
}
