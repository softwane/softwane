use serde::Serialize;
use serde_json::json;
use tauri::State;

use crate::{
    config::SessionConfig,
    engine::{calculate_snapshot, EffectSnapshot},
    observability,
    platform::{apply_preview, ApplyResult, CueStyle, ManagedDisplayEffectApplier},
};

#[derive(Debug, Serialize)]
pub struct PreviewPayload {
    pub snapshot: EffectSnapshot,
    pub apply_result: ApplyResult,
}

#[tauri::command]
pub fn apply_effect_snapshot(
    phase: String,
    saturation: f32,
    warmth_kelvin: u32,
    cue_style: String,
    app_handle: tauri::AppHandle,
    applier: State<'_, ManagedDisplayEffectApplier>,
) -> ApplyResult {
    let snapshot = EffectSnapshot {
        phase: parse_phase(&phase),
        saturation,
        warmth_kelvin,
    };

    let cue_style = CueStyle::from_id(&cue_style);
    let result = apply_preview(applier, &snapshot, cue_style);
    log_platform_apply_result(
        &app_handle,
        "apply_effect_snapshot",
        &snapshot,
        cue_style,
        &result,
    );
    result
}

#[tauri::command]
pub fn preview_effect(
    session_duration_minutes: f32,
    remaining_minutes: f32,
    cue_style: String,
    app_handle: tauri::AppHandle,
    applier: State<'_, ManagedDisplayEffectApplier>,
) -> PreviewPayload {
    let snapshot = calculate_snapshot(
        &SessionConfig::default(),
        session_duration_minutes,
        remaining_minutes,
    );
    let cue_style = CueStyle::from_id(&cue_style);
    let apply_result = apply_preview(applier, &snapshot, cue_style);
    log_platform_apply_result(
        &app_handle,
        "preview_effect",
        &snapshot,
        cue_style,
        &apply_result,
    );

    PreviewPayload {
        snapshot,
        apply_result,
    }
}

fn log_platform_apply_result(
    app_handle: &tauri::AppHandle,
    source: &'static str,
    snapshot: &EffectSnapshot,
    cue_style: CueStyle,
    result: &ApplyResult,
) {
    if result.error.is_none() && !result.recovery_attempted {
        return;
    }

    observability::log_event(
        app_handle,
        "platform_apply",
        json!({
            "source": source,
            "backend": result.backend,
            "cueStyle": cue_style.as_id(),
            "snapshot": snapshot,
            "applied": result.applied,
            "error": result.error,
            "recoveryAttempted": result.recovery_attempted,
            "recoverySucceeded": result.recovery_succeeded,
            "recoveryError": result.recovery_error,
        }),
    );
}

fn parse_phase(value: &str) -> crate::engine::Phase {
    match value.trim().to_ascii_lowercase().as_str() {
        "jnd" => crate::engine::Phase::Jnd,
        "evolution" => crate::engine::Phase::Evolution,
        "statue" => crate::engine::Phase::Statue,
        _ => crate::engine::Phase::Stable,
    }
}
