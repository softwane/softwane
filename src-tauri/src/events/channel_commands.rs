use tauri::State;

use crate::channels::{ChannelType, ChannelValue, CurveParameters};
use crate::engine::EngineHandle;
use super::{EngineEvent, CommandError, foward};

#[derive(Debug)]
pub enum ChannelCommand {
    ToggleSwitch {
        channel_type: ChannelType,
        switch_on: bool,
    },
    UpdateTargetChannelValue {
        target_channel_value: ChannelValue,
    },
    UpdateProgressBeginRatio {
        channel_type: ChannelType,
        progress_begin_ratio: f64,
    },
    UpdateProgressCurveParas {
        channel_type: ChannelType,
        curve_parameters: CurveParameters,
    },
    UpdateSettlingCurveParas {
        channel_type: ChannelType,
        curve_parameters: CurveParameters,
    },
    UpdateReverseCurveParas {
        channel_type: ChannelType,
        curve_parameters: CurveParameters,
    },
}

impl ChannelCommand {
    pub fn channel_type(&self) -> ChannelType {
        match self {
            Self::ToggleSwitch { channel_type, .. }
            | Self::UpdateProgressBeginRatio { channel_type, .. }
            | Self::UpdateProgressCurveParas { channel_type, .. }
            | Self::UpdateSettlingCurveParas { channel_type, .. }
            | Self::UpdateReverseCurveParas { channel_type, .. } => *channel_type,
            Self::UpdateTargetChannelValue { target_channel_value } => {
                ChannelType::from(*target_channel_value)
            }
        }
    }
}

#[tauri::command]
pub async fn toggle_channel_switch(engine_handle: State<'_, EngineHandle>, channel_type: ChannelType, switch_on: bool) -> Result<(), CommandError> {
    foward(&engine_handle, EngineEvent::Channel(ChannelCommand::ToggleSwitch { channel_type, switch_on })).await
}

#[tauri::command]
pub async fn update_target_channel_value(engine_handle: State<'_, EngineHandle>, target_channel_value: ChannelValue) -> Result<(), CommandError> {
    foward(&engine_handle, EngineEvent::Channel(ChannelCommand::UpdateTargetChannelValue { target_channel_value })).await
}

#[tauri::command]
pub async fn update_progress_begin_ratio(engine_handle: State<'_, EngineHandle>, channel_type: ChannelType, progress_begin_ratio: f64) -> Result<(), CommandError> {
    foward(&engine_handle, EngineEvent::Channel(ChannelCommand::UpdateProgressBeginRatio { channel_type, progress_begin_ratio })).await
}

#[tauri::command]
pub async fn update_progress_curve_params(engine_handle: State<'_, EngineHandle>, channel_type: ChannelType, curve_parameters: CurveParameters) -> Result<(), CommandError> {
    foward(&engine_handle, EngineEvent::Channel(ChannelCommand::UpdateProgressCurveParas { channel_type, curve_parameters })).await
}

#[tauri::command]
pub async fn update_settling_curve_params(engine_handle: State<'_, EngineHandle>, channel_type: ChannelType, curve_parameters: CurveParameters) -> Result<(), CommandError> {
    foward(&engine_handle, EngineEvent::Channel(ChannelCommand::UpdateSettlingCurveParas { channel_type, curve_parameters })).await
}

#[tauri::command]
pub async fn update_reverse_curve_params(engine_handle: State<'_, EngineHandle>, channel_type: ChannelType, curve_parameters: CurveParameters) -> Result<(), CommandError> {
    foward(&engine_handle, EngineEvent::Channel(ChannelCommand::UpdateReverseCurveParas { channel_type, curve_parameters })).await
}
