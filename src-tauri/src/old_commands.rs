use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, State};

use crate::{
    channels::{
        ChannelType,
        ChannelValue,
        CurveParameters,
    },
    compositor::CompositeFrame,
    observability,
    platform::{
        ApplyResult,
        ManagedPlatformAdapter,
        apply_frame
    },
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewPayload {
    pub frame: CompositeFrame,
    pub apply_result: ApplyResult,
}

#[tauri::command]
pub fn preview_frame(
    saturation: f32,
    color_kelvin: u32,
    brightness: f32,
    app_handle: AppHandle,
    applier: State<'_, ManagedPlatformAdapter>,
) -> PreviewPayload {
    let frame = CompositeFrame {
        saturation: saturation.clamp(0.0, 1.0),
        warmth_kelvin: color_kelvin.clamp(2000, 6500),
        brightness: brightness.clamp(0.0, 1.0),
    };

    let apply_result = apply_frame(applier, &frame);

    if apply_result.error.is_some() || apply_result.recovery_attempted {
        observability::log_event(
            &app_handle,
            "platform_apply",
            json!({
                "source": "preview_frame",
                "backend": apply_result.backend,
                "frame": frame,
                "applied": apply_result.applied,
                "error": apply_result.error,
                "recoveryAttempted": apply_result.recovery_attempted,
            }),
        );
    }

    PreviewPayload {
        frame,
        apply_result,
    }
}

#[tauri::command]
pub fn reset_display(
    applier: State<'_, ManagedPlatformAdapter>,
) -> ApplyResult {
    apply_frame(applier, &CompositeFrame::neutral())
}

