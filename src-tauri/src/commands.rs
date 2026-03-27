use serde::Serialize;
use tauri::State;

use crate::{
    config::SessionConfig,
    engine::{calculate_snapshot, EffectSnapshot},
    platform::{apply_preview, ApplyResult, MockDisplayEffectApplier},
};

#[derive(Debug, Serialize)]
pub struct PreviewPayload {
    pub snapshot: EffectSnapshot,
    pub apply_result: ApplyResult,
}

#[tauri::command]
pub fn preview_effect(
    session_duration_minutes: f32,
    remaining_minutes: f32,
    applier: State<'_, MockDisplayEffectApplier>,
) -> PreviewPayload {
    let snapshot = calculate_snapshot(
        &SessionConfig::default(),
        session_duration_minutes,
        remaining_minutes,
    );
    let apply_result = apply_preview(applier, &snapshot);

    PreviewPayload {
        snapshot,
        apply_result,
    }
}
