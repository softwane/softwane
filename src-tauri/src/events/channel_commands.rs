use crate::channels::{ChannelType, ChannelValue, CurveParameters};


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
        progress_begin_ratio: f64,  // This one should be clamped to 0.0 ~ 1.0
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