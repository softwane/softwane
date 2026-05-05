use tauri::State;

use crate::engine::EngineHandle;
use super::{EngineEvent, CommandError, foward};

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
    foward(&engine_handle, EngineEvent::State(StateCommand::StartSession { target_duration_ms })).await
}

#[tauri::command]
pub async fn take_break_now(engine_handle: State<'_, EngineHandle>) -> Result<(), CommandError> {
    foward(&engine_handle, EngineEvent::State(StateCommand::TakeBreakNow)).await
}

#[tauri::command]
pub async fn stop_session(engine_handle: State<'_, EngineHandle>) -> Result<(), CommandError> {
    foward(&engine_handle, EngineEvent::State(StateCommand::StopSession)).await
}

#[tauri::command]
pub async fn enter_preview(engine_handle: State<'_, EngineHandle>) -> Result<(), CommandError> {
    foward(&engine_handle, EngineEvent::State(StateCommand::EnterPreview)).await
}

#[tauri::command]
pub async fn exit_preview(engine_handle: State<'_, EngineHandle>) -> Result<(), CommandError> {
    foward(&engine_handle, EngineEvent::State(StateCommand::ExitPreview)).await
}

#[tauri::command]
pub async fn update_preview_progress(engine_handle: State<'_, EngineHandle>, progress: f64) -> Result<(), CommandError> {
    foward(&engine_handle, EngineEvent::State(StateCommand::UpdatePreviewProgress { progress })).await
}

#[tauri::command]
pub async fn update_settling_duration(engine_handle: State<'_, EngineHandle>, duration_ms: u64) -> Result<(), CommandError> {
    foward(&engine_handle, EngineEvent::State(StateCommand::UpdateSettlingDuration { duration_ms })).await
}

#[tauri::command]
pub async fn update_reverse_duration(engine_handle: State<'_, EngineHandle>, duration_ms: u64) -> Result<(), CommandError> {
    foward(&engine_handle, EngineEvent::State(StateCommand::UpdateReverseDuration { duration_ms })).await
}
