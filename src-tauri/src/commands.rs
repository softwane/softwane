use serde::Serialize;
use tauri::State;

use crate::{
    config::SessionConfig,
    engine::{calculate_snapshot, EffectSnapshot},
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
    applier: State<'_, ManagedDisplayEffectApplier>,
) -> ApplyResult {
    let snapshot = EffectSnapshot {
        phase: parse_phase(&phase),
        saturation,
        warmth_kelvin,
    };

    apply_preview(applier, &snapshot, CueStyle::from_id(&cue_style))
}

#[tauri::command]
pub fn preview_effect(
    session_duration_minutes: f32,
    remaining_minutes: f32,
    cue_style: String,
    applier: State<'_, ManagedDisplayEffectApplier>,
) -> PreviewPayload {
    let snapshot = calculate_snapshot(
        &SessionConfig::default(),
        session_duration_minutes,
        remaining_minutes,
    );
    let apply_result = apply_preview(applier, &snapshot, CueStyle::from_id(&cue_style));

    PreviewPayload {
        snapshot,
        apply_result,
    }
}

fn parse_phase(value: &str) -> crate::engine::Phase {
    match value.trim().to_ascii_lowercase().as_str() {
        "jnd" => crate::engine::Phase::Jnd,
        "evolution" => crate::engine::Phase::Evolution,
        "statue" => crate::engine::Phase::Statue,
        _ => crate::engine::Phase::Stable,
    }
}
