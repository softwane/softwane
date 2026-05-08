use serde::{Deserialize, Serialize};
use tauri::{ipc::Channel, State};

use crate::timer_state_machine::TimerState;
use crate::channels::{ChannelType, ChannelValue, CurveParameters};
use crate::engine::EngineHandle;
use super::{EngineEvent, CommandError, forward_engine, forward_engine_nowait};

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
    forward_engine(engine_handle.tx.clone(), EngineEvent::Channel(ChannelCommand::ToggleSwitch { channel_type, switch_on })).await
}

#[tauri::command]
pub fn update_target_channel_value(engine_handle: State<'_, EngineHandle>, target_channel_value: ChannelValue) -> Result<(), CommandError> {
    forward_engine_nowait(engine_handle.tx.clone(), EngineEvent::Channel(ChannelCommand::UpdateTargetChannelValue { target_channel_value }))
}

#[tauri::command]
pub fn update_progress_begin_ratio(engine_handle: State<'_, EngineHandle>, channel_type: ChannelType, progress_begin_ratio: f64) -> Result<(), CommandError> {
    forward_engine_nowait(engine_handle.tx.clone(), EngineEvent::Channel(ChannelCommand::UpdateProgressBeginRatio { channel_type, progress_begin_ratio }))
}

#[tauri::command]
pub fn update_progress_curve_params(engine_handle: State<'_, EngineHandle>, channel_type: ChannelType, curve_parameters: CurveParameters) -> Result<(), CommandError> {
    forward_engine_nowait(engine_handle.tx.clone(), EngineEvent::Channel(ChannelCommand::UpdateProgressCurveParas { channel_type, curve_parameters }))
}

#[tauri::command]
pub fn update_settling_curve_params(engine_handle: State<'_, EngineHandle>, channel_type: ChannelType, curve_parameters: CurveParameters) -> Result<(), CommandError> {
    forward_engine_nowait(engine_handle.tx.clone(), EngineEvent::Channel(ChannelCommand::UpdateSettlingCurveParas { channel_type, curve_parameters }))
}

#[tauri::command]
pub fn update_reverse_curve_params(engine_handle: State<'_, EngineHandle>, channel_type: ChannelType, curve_parameters: CurveParameters) -> Result<(), CommandError> {
    forward_engine_nowait(engine_handle.tx.clone(), EngineEvent::Channel(ChannelCommand::UpdateReverseCurveParas { channel_type, curve_parameters }))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ProgressPayload {
    pub timer_state: TimerState,
}

pub enum ProgressCommand {
    RegisterChannel(Channel<ProgressPayload>),
}

impl std::fmt::Debug for ProgressCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RegisterChannel(ch) => write!(f, "RegisterChannel(Channel(id = {}))", ch.id()),
        }
    }
}

#[tauri::command]
pub async fn register_progress_channel(engine_handle: State<'_, EngineHandle>, channel: Channel<ProgressPayload>) -> Result<(), CommandError> {
    forward_engine(engine_handle.tx.clone(), EngineEvent::Progress(ProgressCommand::RegisterChannel(channel))).await
}
