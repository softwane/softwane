use serde::Deserialize;

use crate::channels::{ChannelType, ChannelValue, CurveParameters};

#[derive(Debug, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
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
                (*target_channel_value).into()
            }
        }
    }
}
