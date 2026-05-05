mod timer_state_commands;
mod channel_commands;
mod renderer_events;
mod progress_commands;

use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_store::{Error as StoreError, StoreExt};
use thiserror::Error;
use tokio::sync::mpsc::error::SendError;

use crate::channels::{ChannelType, SENSORY_CHANNEL_TYPES, load_channel_config, ChannelConfig};
use crate::engine::EngineHandle;
use crate::timer_state_machine::{TimerConfig, TimerStateMachine};

pub use self::timer_state_commands::*;
pub use self::channel_commands::*;
pub use self::renderer_events::*;
pub use self::progress_commands::*;

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
    #[error("Got error when trying to create or to load a store: {0}")]
    StoreError(#[from] StoreError)
}

impl Serialize for CommandError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string().as_ref())
    }
}

async fn foward(
    engine_handle: &EngineHandle,
    event: EngineEvent,
) -> Result<(), CommandError> {
    engine_handle.tx.send(event).await?;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub struct StoredConfig {
    pub channel_configs: Vec<(ChannelType, ChannelConfig)>,
    pub timer_config: TimerConfig,
}

#[tauri::command]
pub fn get_stored_config(app_handle: AppHandle) -> Result<StoredConfig, CommandError> {
    let store = app_handle.store("config.json")?;
    let channel_configs = SENSORY_CHANNEL_TYPES.into_iter()
        .filter(|c| c.is_available_on_this_platform())
        .map(|c| (c, load_channel_config(&store, c)))
        .collect();
    let timer_config = TimerStateMachine::load_config_from_store(&store);
    Ok(StoredConfig { channel_configs, timer_config })
}

#[tauri::command]
pub async fn force_reset(engine_handle: State<'_, EngineHandle>) -> Result<(), CommandError> {
    foward(&engine_handle, EngineEvent::ForceReset).await
}
