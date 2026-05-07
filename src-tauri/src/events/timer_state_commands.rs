use tauri::{AppHandle, Runtime, State};
use tauri_plugin_store::StoreExt;

use crate::{engine::EngineHandle, events::forward_engine_nowait, shortcuts::setup_global_shortcuts};
use super::{EngineEvent, CommandError, forward_engine};

#[derive(Debug)]
pub enum StateCommand {
    StartSession {
        target_duration_ms: u64,
    },
    TakeBreakNow,
    StopSession,
    EnterPreview,
    ExitPreview,
    UpdatePreviewProgress {
        progress: f64,
    },
    UpdateSettlingDuration {
        duration_ms: u64,
    },
    UpdateReverseDuration {
        duration_ms: u64,
    },
}

#[tauri::command]
pub async fn start_session(engine_handle: State<'_, EngineHandle>, target_duration_ms: u64) -> Result<(), CommandError> {
    forward_engine(engine_handle.tx.clone(), EngineEvent::State(StateCommand::StartSession { target_duration_ms })).await
}

#[tauri::command]
pub async fn take_break_now(engine_handle: State<'_, EngineHandle>) -> Result<(), CommandError> {
    forward_engine(engine_handle.tx.clone(), EngineEvent::State(StateCommand::TakeBreakNow)).await
}

#[tauri::command]
pub async fn stop_session(engine_handle: State<'_, EngineHandle>) -> Result<(), CommandError> {
    forward_engine(engine_handle.tx.clone(), EngineEvent::State(StateCommand::StopSession)).await
}

#[tauri::command]
pub async fn enter_preview(engine_handle: State<'_, EngineHandle>) -> Result<(), CommandError> {
    forward_engine(engine_handle.tx.clone(), EngineEvent::State(StateCommand::EnterPreview)).await
}

#[tauri::command]
pub async fn exit_preview(engine_handle: State<'_, EngineHandle>) -> Result<(), CommandError> {
    forward_engine(engine_handle.tx.clone(), EngineEvent::State(StateCommand::ExitPreview)).await
}

#[tauri::command]
pub fn update_preview_progress(engine_handle: State<'_, EngineHandle>, progress: f64) -> Result<(), CommandError> {
    tracing::debug!("Updating preview progress: {progress}");
    forward_engine_nowait(engine_handle.tx.clone(), EngineEvent::State(StateCommand::UpdatePreviewProgress { progress }))
}

#[tauri::command]
pub async fn update_settling_duration(engine_handle: State<'_, EngineHandle>, duration_ms: u64) -> Result<(), CommandError> {
    forward_engine(engine_handle.tx.clone(), EngineEvent::State(StateCommand::UpdateSettlingDuration { duration_ms })).await
}

#[tauri::command]
pub async fn update_reverse_duration(engine_handle: State<'_, EngineHandle>, duration_ms: u64) -> Result<(), CommandError> {
    forward_engine(engine_handle.tx.clone(), EngineEvent::State(StateCommand::UpdateReverseDuration { duration_ms })).await
}

pub const STORE_KEY_PRESET_SESSION_DURATIONS: &str = "session_durations_ms";
pub const DEFAULT_DURATIONS: [u64; 3] = [25 * 60_000, 50 * 60_000, 90 * 60_000];

#[tauri::command]
pub fn get_preset_session_durations<R: Runtime>(app_handle: AppHandle<R>) -> [u64; 3] {
    let store = match app_handle.store("config.json") {
        Ok(s) => s,
        Err(err) => {
            tracing::error!("Trying to create or to load a store failed: {err:?}.");
            return DEFAULT_DURATIONS;
        }
    };
    let raw = store.get(STORE_KEY_PRESET_SESSION_DURATIONS);
    match raw.and_then(|v| serde_json::from_value::<[u64; 3]>(v).ok()) {
        Some(d) => d,
        _ => DEFAULT_DURATIONS,
    }
}

#[tauri::command]
pub fn update_preset_session_durations(app_handle: AppHandle, durations: Vec<u64>) -> Result<(), CommandError> {
    if durations.len() != 3 {
        return Err(CommandError::BadArguments(format!(
            "expected 3 durations, got {}",
            durations.len()
        )));
    }
    let store = app_handle.store("config.json")?;
    store.set(
        STORE_KEY_PRESET_SESSION_DURATIONS.to_string(),
        serde_json::to_value::<[u64; 3]>(durations.try_into().unwrap()).expect("Vec<u64> serialization is infallible"),
    );
    setup_global_shortcuts(&app_handle);
    Ok(())
}