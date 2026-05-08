use tauri::State;

use crate::{
    channels::{ChannelType, ChannelValue, CurveParameters},
    engine::EngineHandle,
};
use super::{EngineEvent::Channel, CommandError, forward_engine, forward_engine_nowait};

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
use ChannelCommand::*;

#[tauri::command]
pub async fn toggle_channel_switch(engine_handle: State<'_, EngineHandle>, channel_type: ChannelType, switch_on: bool) -> Result<(), CommandError> {
    forward_engine(engine_handle.tx.clone(), Channel(ToggleSwitch { channel_type, switch_on })).await
}

#[tauri::command]
pub fn update_target_channel_value(engine_handle: State<'_, EngineHandle>, target_channel_value: ChannelValue) -> Result<(), CommandError> {
    forward_engine_nowait(engine_handle.tx.clone(), Channel(UpdateTargetChannelValue { target_channel_value }))
}

#[tauri::command]
pub fn update_progress_begin_ratio(engine_handle: State<'_, EngineHandle>, channel_type: ChannelType, progress_begin_ratio: f64) -> Result<(), CommandError> {
    forward_engine_nowait(engine_handle.tx.clone(), Channel(UpdateProgressBeginRatio { channel_type, progress_begin_ratio }))
}

#[tauri::command]
pub fn update_progress_curve_params(engine_handle: State<'_, EngineHandle>, channel_type: ChannelType, curve_parameters: CurveParameters) -> Result<(), CommandError> {
    forward_engine_nowait(engine_handle.tx.clone(), Channel(UpdateProgressCurveParas { channel_type, curve_parameters }))
}

#[tauri::command]
pub fn update_settling_curve_params(engine_handle: State<'_, EngineHandle>, channel_type: ChannelType, curve_parameters: CurveParameters) -> Result<(), CommandError> {
    forward_engine_nowait(engine_handle.tx.clone(), Channel(UpdateSettlingCurveParas { channel_type, curve_parameters }))
}

#[tauri::command]
pub fn update_reverse_curve_params(engine_handle: State<'_, EngineHandle>, channel_type: ChannelType, curve_parameters: CurveParameters) -> Result<(), CommandError> {
    forward_engine_nowait(engine_handle.tx.clone(), Channel(UpdateReverseCurveParas { channel_type, curve_parameters }))
}
