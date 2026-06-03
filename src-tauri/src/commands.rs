use serde::Serialize;
use tauri::{AppHandle, Error as TauriError, Runtime};
use tauri_plugin_autostart::{Error as AutostartError, ManagerExt as AutostartManagerExt};
use tauri_plugin_store::{Error as StoreError, StoreExt};
use thiserror::Error;
use tokio::sync::mpsc::error::{SendError, TrySendError};

use crate::{
    engine::EngineEvent,
    tray::refresh_tray_menu,
};

// ── Error ─────────────────────────────────────────────────────────────

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

// ── Store keys ────────────────────────────────────────────────────────

pub const STORE_KEY_LAST_CRASH: &str = "program_last_crash";
pub const STORE_KEY_PRESET_SESSION_DURATIONS: &str = "session_durations_ms";
pub const STORE_KEY_LAUNCH_SESSION_ON_START: &str = "launch_session_on_start";
pub const STORE_KEY_AUTO_START_NEXT_SESSION: &str = "auto_start_next_session";
pub const DEFAULT_DURATIONS: [u64; 3] = [25 * 60_000, 50 * 60_000, 90 * 60_000];

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

// ── Preset session durations ──────────────────────────────────────────

#[tauri::command]
pub fn get_preset_session_durations<R: Runtime>(app_handle: AppHandle<R>) -> [u64; 3] {
    let store = match app_handle.store("config.json") {
        Ok(s) => s,
        Err(err) => {
            tracing::error!("Trying to create or to load a store failed: {err:?}.");
            return DEFAULT_DURATIONS;
        }
    };
    store.get(STORE_KEY_PRESET_SESSION_DURATIONS)
        .and_then(|v| Some(serde_json::from_value(v)
            .inspect_err(|e| {
                tracing::warn!(?e, "stored preset session durations are failed to deserialized, using default");
                let value = serde_json::to_value(DEFAULT_DURATIONS).expect("DURATIONS serialization is infallible");
                store.set(STORE_KEY_PRESET_SESSION_DURATIONS, value);
            })
            .unwrap_or(DEFAULT_DURATIONS)
        ))
        .expect("Defaults are set when setting up")
}

#[tauri::command]
pub fn update_preset_session_durations(app_handle: AppHandle, durations: [u64; 3]) -> Result<(), CommandError> {
    let store = app_handle.store("config.json")?;
    store.set(
        STORE_KEY_PRESET_SESSION_DURATIONS.to_string(),
        serde_json::to_value::<[u64; 3]>(durations.try_into().unwrap()).expect("[u64; 3] serialization is infallible"),
    );

    if let Err(err) = refresh_tray_menu(&app_handle) {
        tracing::error!("Failed to refresh tray menu after updating preset durations: {err:?}.");
    }
    Ok(())
}

fn get_bool_setting<R: Runtime>(app_handle: AppHandle<R>, key: &str, default: bool) -> Result<bool, CommandError> {
    let store = app_handle.store("config.json")?;
    Ok(store
        .get(key)
        .and_then(|v| v.as_bool().or_else(|| {
            tracing::warn!("stored config for {key} is not a boolean (it is {v}), using default: {default}");
            store.set(key, default);
            Some(default)
        }))
        .expect("Defaults are set."))
}

fn set_bool_setting<R: Runtime>(app_handle: AppHandle<R>, key: &str, enabled: bool) -> Result<(), CommandError> {
    let store = app_handle.store("config.json")?;
    store.set(key.to_string(), serde_json::Value::Bool(enabled));
    Ok(())
}

#[tauri::command]
pub fn get_launch_session_on_start<R: Runtime>(app_handle: AppHandle<R>) -> Result<bool, CommandError> {
    get_bool_setting(app_handle, STORE_KEY_LAUNCH_SESSION_ON_START, false)
}

#[tauri::command]
pub fn set_launch_session_on_start<R: Runtime>(app_handle: AppHandle<R>, enabled: bool) -> Result<(), CommandError> {
    set_bool_setting(app_handle, STORE_KEY_LAUNCH_SESSION_ON_START, enabled)
}

#[tauri::command]
pub fn get_auto_start_next_session<R: Runtime>(app_handle: AppHandle<R>) -> Result<bool, CommandError> {
    get_bool_setting(app_handle, STORE_KEY_AUTO_START_NEXT_SESSION, false)
}

#[tauri::command]
pub fn set_auto_start_next_session<R: Runtime>(app_handle: AppHandle<R>, enabled: bool) -> Result<(), CommandError> {
    set_bool_setting(app_handle, STORE_KEY_AUTO_START_NEXT_SESSION, enabled)
}
