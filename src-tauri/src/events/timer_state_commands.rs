use tauri::{AppHandle, Runtime, State as TauriState};
use tauri_plugin_store::StoreExt;

use crate::{
    engine::EngineHandle,
    tray::refresh_tray_menu,
};
use super::{EngineEvent::State, CommandError, forward_engine_nowait, forward_engine};

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
use StateCommand::*;

// TODO: 也许，允许用户通过快捷键呼出一个窗口输入多少分钟的session
// 这个窗口可以是原生的，因为就是一个输入框，比launcher还简单
#[tauri::command]
pub async fn start_session(engine_handle: TauriState<'_, EngineHandle>, target_duration_ms: u64) -> Result<(), CommandError> {
    forward_engine(engine_handle.tx.clone(), State(StartSession { target_duration_ms })).await
}

#[tauri::command]
pub async fn take_break_now(engine_handle: TauriState<'_, EngineHandle>) -> Result<(), CommandError> {
    forward_engine(engine_handle.tx.clone(), State(TakeBreakNow)).await
}

#[tauri::command]
pub async fn stop_session(engine_handle: TauriState<'_, EngineHandle>) -> Result<(), CommandError> {
    forward_engine(engine_handle.tx.clone(), State(StopSession)).await
}

#[tauri::command]
pub async fn enter_preview(engine_handle: TauriState<'_, EngineHandle>) -> Result<(), CommandError> {
    forward_engine(engine_handle.tx.clone(), State(EnterPreview)).await
}

#[tauri::command]
pub async fn exit_preview(engine_handle: TauriState<'_, EngineHandle>) -> Result<(), CommandError> {
    forward_engine(engine_handle.tx.clone(), State(ExitPreview)).await
}

#[tauri::command]
pub fn update_preview_progress(engine_handle: TauriState<'_, EngineHandle>, progress: f64) -> Result<(), CommandError> {
    forward_engine_nowait(engine_handle.tx.clone(), State(UpdatePreviewProgress { progress }))
}

#[tauri::command]
pub async fn update_settling_duration(engine_handle: TauriState<'_, EngineHandle>, duration_ms: u64) -> Result<(), CommandError> {
    forward_engine(engine_handle.tx.clone(), State(UpdateSettlingDuration { duration_ms })).await
}

#[tauri::command]
pub async fn update_reverse_duration(engine_handle: TauriState<'_, EngineHandle>, duration_ms: u64) -> Result<(), CommandError> {
    forward_engine(engine_handle.tx.clone(), State(UpdateReverseDuration { duration_ms })).await
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
        serde_json::to_value::<[u64; 3]>(durations.try_into().unwrap()).expect("[u64; 3] serialization is infallible"),
    );

    // Shortcut callbacks read preset durations from the store at fire
    // time (see `shortcuts::start_preset_event`), so re-registering
    // the global shortcuts is unnecessary on duration change.

    // Preset duration labels are baked into the tray menu (see
    // `tray::build_menu`).  Reading the current TimerState from the
    // shared cache, we can rebuild the menu while preserving the
    // correct enable/disable flags for the active phase.
    if let Err(err) = refresh_tray_menu(&app_handle) {
        tracing::error!("Failed to refresh tray menu after updating preset durations: {err:?}.");
    }
    Ok(())
}