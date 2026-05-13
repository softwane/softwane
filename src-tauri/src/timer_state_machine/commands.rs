use tauri::State as TauriState;

use crate::{
    engine::EngineHandle,
    engine::commands::{EngineEvent, forward_engine, forward_engine_nowait},
    commands::CommandError,
};

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

#[tauri::command]
pub async fn start_session(engine_handle: TauriState<'_, EngineHandle>, target_duration_ms: u64) -> Result<(), CommandError> {
    forward_engine(engine_handle.tx.clone(), EngineEvent::State(StartSession { target_duration_ms })).await
}

#[tauri::command]
pub async fn take_break_now(engine_handle: TauriState<'_, EngineHandle>) -> Result<(), CommandError> {
    forward_engine(engine_handle.tx.clone(), EngineEvent::State(TakeBreakNow)).await
}

#[tauri::command]
pub async fn stop_session(engine_handle: TauriState<'_, EngineHandle>) -> Result<(), CommandError> {
    forward_engine(engine_handle.tx.clone(), EngineEvent::State(StopSession)).await
}

#[tauri::command]
pub async fn enter_preview(engine_handle: TauriState<'_, EngineHandle>) -> Result<(), CommandError> {
    forward_engine(engine_handle.tx.clone(), EngineEvent::State(EnterPreview)).await
}

#[tauri::command]
pub async fn exit_preview(engine_handle: TauriState<'_, EngineHandle>) -> Result<(), CommandError> {
    forward_engine(engine_handle.tx.clone(), EngineEvent::State(ExitPreview)).await
}

#[tauri::command]
pub fn update_preview_progress(engine_handle: TauriState<'_, EngineHandle>, progress: f64) -> Result<(), CommandError> {
    forward_engine_nowait(engine_handle.tx.clone(), EngineEvent::State(UpdatePreviewProgress { progress }))
}

#[tauri::command]
pub async fn update_settling_duration(engine_handle: TauriState<'_, EngineHandle>, duration_ms: u64) -> Result<(), CommandError> {
    forward_engine(engine_handle.tx.clone(), EngineEvent::State(UpdateSettlingDuration { duration_ms })).await
}

#[tauri::command]
pub async fn update_reverse_duration(engine_handle: TauriState<'_, EngineHandle>, duration_ms: u64) -> Result<(), CommandError> {
    forward_engine(engine_handle.tx.clone(), EngineEvent::State(UpdateReverseDuration { duration_ms })).await
}
