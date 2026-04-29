use serde::{Deserialize, Serialize};

use crate::{
    events::ChannelCommand,
    engine::FrameEvents,
    timer_state_machine::*,
    utils::*,
};

pub use curve_functions::*;


#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChannelFuncParams {
    curve_parameters: CurveParameters,
    curve_begin_ratio: f64, // 0.0 ~ 1.0, 0 for at the beginning, 1 for next to the end, 0.9 for 90% of the way to the end
    curve_begin_value: ChannelValue,
    target_value: ChannelValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChannelFuncParamsMatrix([ChannelFuncParams; 3]);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PersistentChannelFuncParamsMatrix {
    progress_curve_parameters: CurveParameters,
    settling_curve_parameters: CurveParameters,
    reverse_curve_parameters: CurveParameters,
    progress_begin_ratio: f64,
    target_channel_value: ChannelValue,
}

impl ChannelFuncParamsMatrix {
    pub fn new(persistent_params_mat: &PersistentChannelFuncParamsMatrix) -> Self {
        let channel_type: ChannelType = persistent_params_mat.target_channel_value.into();
        let progress_params = ChannelFuncParams {
            curve_parameters: persistent_params_mat.progress_curve_parameters,
            curve_begin_ratio: persistent_params_mat.progress_begin_ratio,
            curve_begin_value: channel_type.neutral_value(),
            target_value: persistent_params_mat.target_channel_value,
        };
        let settling_params = ChannelFuncParams {
            curve_parameters: persistent_params_mat.settling_curve_parameters,
            curve_begin_ratio: 0.0,
            curve_begin_value: channel_type.neutral_value(),
            target_value: persistent_params_mat.target_channel_value,
        };
        let reverse_params = ChannelFuncParams {
            curve_parameters: persistent_params_mat.reverse_curve_parameters,
            curve_begin_ratio: 0.0,
            curve_begin_value: persistent_params_mat.target_channel_value,
            target_value: channel_type.neutral_value(),
        };
        Self([progress_params, settling_params, reverse_params])
    }

    pub fn persist(&self) -> PersistentChannelFuncParamsMatrix {
        let progress_params = self[PROGRESS_STATE].clone();
        let settling_params = self[SETTLING_STATE].clone();
        let reverse_params = self[REVERSE_STATE].clone();
        PersistentChannelFuncParamsMatrix {
            progress_curve_parameters: progress_params.curve_parameters,
            settling_curve_parameters: settling_params.curve_parameters,
            reverse_curve_parameters: reverse_params.curve_parameters,
            progress_begin_ratio: progress_params.curve_begin_ratio,
            target_channel_value: progress_params.target_value,
        }
    }
}

impl std::ops::Index<TimerState> for ChannelFuncParamsMatrix {
    type Output = ChannelFuncParams;
    fn index(&self, index: TimerState) -> &Self::Output {
        match index {
            TimerState::Progress { .. } => &self.0[0],
            TimerState::Settling { .. } => &self.0[1],
            TimerState::Reverse { .. } => &self.0[2],
            _ => unreachable!("Invalid timer state: {:?}", index.label()),
        }
    }
}

impl std::ops::IndexMut<TimerState> for ChannelFuncParamsMatrix {
    fn index_mut(&mut self, index: TimerState) -> &mut Self::Output {
        match index {
            TimerState::Progress { .. } => &mut self.0[0],
            TimerState::Settling { .. } => &mut self.0[1],
            TimerState::Reverse { .. } => &mut self.0[2],
            _ => unreachable!("Invalid timer state: {:?}", index.label()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SensoryChannel {
    channel_type: ChannelType,
    switch_on: bool,
    function_params_matrix: ChannelFuncParamsMatrix,
    current_value: Update<ChannelValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChannelConfigs {
    switch_on: bool,
    persistent_params_mat: PersistentChannelFuncParamsMatrix,
}

impl SensoryChannel {
    pub fn new(config: ChannelConfigs) -> Self {
        let channel_type: ChannelType = config.persistent_params_mat.target_channel_value.into();
        Self {
            channel_type: channel_type,
            switch_on: config.switch_on,
            function_params_matrix: ChannelFuncParamsMatrix::new(&config.persistent_params_mat),
            // If app crashed, we need to neatralize the channel value while initializing the channel
            current_value: Update::Changed(channel_type.neutral_value()),
        }
    }

    pub fn persist(&self) -> ChannelConfigs {
        ChannelConfigs {
            switch_on: self.switch_on,
            persistent_params_mat: self.function_params_matrix.persist(),
        }
    }

    pub fn handle_command(&mut self, command: ChannelCommand) {
        match command {
            ChannelCommand::ToggleSwitch { channel_type: _, switch_on } => {
                self.switch_on = switch_on;
            }
            ChannelCommand::UpdateTargetChannelValue { target_channel_value } => {
                self.function_params_matrix[PROGRESS_STATE].target_value = target_channel_value;
                self.function_params_matrix[SETTLING_STATE].target_value = target_channel_value;
            }
            ChannelCommand::UpdateProgressBeginRatio { channel_type: _, progress_begin_ratio } => {
                self.function_params_matrix[PROGRESS_STATE].curve_begin_ratio = progress_begin_ratio;
            }
            ChannelCommand::UpdateProgressCurveParas { channel_type: _, curve_parameters } => {
                self.function_params_matrix[PROGRESS_STATE].curve_parameters = curve_parameters;
            }
            ChannelCommand::UpdateSettlingCurveParas { channel_type: _, curve_parameters } => {
                self.function_params_matrix[SETTLING_STATE].curve_parameters = curve_parameters;
            }
            ChannelCommand::UpdateReverseCurveParas { channel_type: _, curve_parameters } => {
                self.function_params_matrix[REVERSE_STATE].curve_parameters = curve_parameters;
            }
        }
    }

    fn calculate_this_value(&self, state: TimerState) -> ChannelValue {
        let func_params = &self.function_params_matrix[state];
        let elapsed_ms = state.elapsed_ms();
        let target_duration_ms = state.target_duration_ms();
        let curve_begin_time_ms = (func_params.curve_begin_ratio * 10000.0) as u64 * target_duration_ms / 10000;
        if elapsed_ms < curve_begin_time_ms {
            return func_params.curve_begin_value;
        }
        if elapsed_ms > target_duration_ms {
            return func_params.target_value;
        }
        let progress = (elapsed_ms - curve_begin_time_ms) as f64 / (target_duration_ms - curve_begin_time_ms) as f64;
        let curve_intensity = match func_params.curve_parameters {
            CurveParameters::NormalizedSigmoid { steepness } => {
                normalized_sigmoid(progress, steepness)
            }
        };
        // begin + (target - begin) * curve_intensity, but in order to avoid overflow, we use the following formula
        // begin * (1 - curve_intensity) + target * curve_intensity
        func_params.curve_begin_value * (1.0 - curve_intensity) + func_params.target_value * curve_intensity
    }

    pub fn tick(&mut self, state: TimerState, frame_flags: &FrameEvents) {
        let this_value: ChannelValue;
        if !self.switch_on {
            this_value = self.channel_type.neutral_value();
        } else {
            this_value = match state {
                TimerState::Idle => self.channel_type.neutral_value(),
                TimerState::Sabi => {
                    self.function_params_matrix[SETTLING_STATE].target_value
                }
                // TODO(B3): Preview should compute channel value at the given progress.
                TimerState::Preview { .. } => self.channel_type.neutral_value(),
                TimerState::Progress { .. } => {
                    self.calculate_this_value(state)
                }
                TimerState::Settling { .. }
                | TimerState::Reverse { .. } => {
                    if frame_flags.just_transited {
                        self.function_params_matrix[state].curve_begin_value = *self.current_value.get_value();
                    }
                    self.calculate_this_value(state)
                }
            }
        }
        
        if this_value == *self.current_value.get_value() {
            self.current_value = Update::Unchanged(this_value);
        } else {
            self.current_value = Update::Changed(this_value);
        }
    }

    pub fn channel_type(&self) -> ChannelType {
        self.channel_type
    }

    pub fn current_value(&self) -> Update<ChannelValue> {
        self.current_value
    }
}

macro_rules! define_channels {
    (
        $(
            $type_var:ident(default_on=$switch_on:literal) => $val_var:ident($data_type:ty, neutral=$val_neutral:literal, default_target=$default_target:literal)
        );* $(;)? // 允许末尾带分号
    ) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum ChannelType {
            $( $type_var, )*
        }

        pub const SENSORY_CHANNELS_COUNT: usize = [$(ChannelType::$type_var,)*].len();
        pub const SENSORY_CHANNEL_TYPES: [ChannelType; SENSORY_CHANNELS_COUNT] = [$(ChannelType::$type_var,)*];

        impl ChannelType {
            pub const fn neutral_value(&self) -> ChannelValue {
                match self {
                    $( ChannelType::$type_var => ChannelValue::$val_var($val_neutral), )*
                }
            }
        }

        #[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum ChannelValue {
            $( $val_var($data_type), )*
        }

        impl From<ChannelValue> for ChannelType {
            fn from(value: ChannelValue) -> Self {
                match value {
                    $( ChannelValue::$val_var(_) => ChannelType::$type_var, )*
                }
            }
        }

        macro_rules! impl_channel_array_type_index {
            ($array_type:ty, $output_type:ty) => {
                impl std::ops::Index<ChannelType> for $array_type {
                    type Output = $output_type;
                    fn index(&self, index: ChannelType) -> &Self::Output {
                        &self[index as usize]
                    }
                }
                impl std::ops::IndexMut<ChannelType> for $array_type {
                    fn index_mut(&mut self, index: ChannelType) -> &mut Self::Output {
                        &mut self[index as usize]
                    }
                }
            }
        }

        pub type AllChannelsConfigs = [ChannelConfigs; SENSORY_CHANNELS_COUNT];
        pub type SensoryChannels = [SensoryChannel; SENSORY_CHANNELS_COUNT];
        pub type LogicFrame = [Update<ChannelValue>; SENSORY_CHANNELS_COUNT];
        pub type ChannelSwitchStates = [bool; SENSORY_CHANNELS_COUNT];

        impl_channel_array_type_index!(AllChannelsConfigs, ChannelConfigs);
        impl_channel_array_type_index!(SensoryChannels, SensoryChannel);
        impl_channel_array_type_index!(LogicFrame, Update<ChannelValue>);
        impl_channel_array_type_index!(ChannelSwitchStates, bool);
        
        /// Macro to implement binary operations for [`ChannelValue`].
        macro_rules! impl_channel_binop {
            ($binop_trait:ident, $binop_method:ident, $op:tt) => {
                impl std::ops::$binop_trait for ChannelValue {
                    type Output = Self;
                    fn $binop_method(self, other: ChannelValue) -> Self {
                        match (self, other) {
                            $( (Self::$val_var(a), Self::$val_var(b)) => Self::$val_var(a $op b), )*
                            _ => panic!(
                                "Cannot {} between different channel values: {:?} and {:?}",
                                stringify!($binop_method), self, other
                            ),
                        }
                    }
                }
            };
        }
        impl_channel_binop!(Add, add, +);
        impl_channel_binop!(Sub, sub, -);
        impl_channel_binop!(Mul, mul, *);

        impl std::ops::Mul<f64> for ChannelValue {
            type Output = Self;
            #[allow(clippy::unnecessary_cast)]  // Suppress unnecessary cast warning
            fn mul(self, other: f64) -> Self {
                match self {
                    $( 
                        // Convert to f64 for multiplication, then convert back to target type ($data_type)
                        // If there are non-primitive types, we need to change this implementation
                        Self::$val_var(a) => Self::$val_var((a as f64 * other) as $data_type), 
                    )*
                }
            }
        }

        pub const DEFAULT_ALL_CHANNELS_CONFIGS: AllChannelsConfigs = [
            $( 
                ChannelConfigs {
                    switch_on: $switch_on,
                    persistent_params_mat: PersistentChannelFuncParamsMatrix {
                        progress_curve_parameters: CurveParameters::NormalizedSigmoid { steepness: DEFAULT_SIGMOID_STEEPNESS },
                        settling_curve_parameters: CurveParameters::NormalizedSigmoid { steepness: DEFAULT_SIGMOID_STEEPNESS },
                        reverse_curve_parameters: CurveParameters::NormalizedSigmoid { steepness: DEFAULT_SIGMOID_STEEPNESS },
                        progress_begin_ratio: DEFAULT_PROGRESS_BEGIN_RATIO,
                        target_channel_value: ChannelValue::$val_var($default_target),
                    },
                },
            )*
        ];
    }
}

const DEFAULT_PROGRESS_BEGIN_RATIO: f64 = 0.9;

#[cfg(target_os = "windows")]
define_channels!(
    Saturation(default_on=true)       => Saturation(f64, neutral=1.0f64, default_target=0.2f64);
    ColorTemperature(default_on=true) => ColorKelvin(u32, neutral=6500u32, default_target=2500u32);
    Brightness(default_on=false)      => Brightness(f64, neutral=1.0f64, default_target=0.6f64);
);

#[cfg(target_os = "macos")]
define_channels!(
    ColorTemperature(default_on=true) => ColorKelvin(u32, neutral=6500u32, default_target=2500u32);
    Brightness(default_on=false)      => Brightness(f64, neutral=1.0f64, default_target=0.6f64);
);

// TODO: 用容差实现ParitialEq for ChannelValue
impl Eq for ChannelValue {

}

mod curve_functions {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Copy, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum CurveParameters {
        NormalizedSigmoid {
            steepness: f64
        },
    }

    pub(super) const DEFAULT_SIGMOID_STEEPNESS: f64 = 10.0;
    fn sigmoid(x: f64, steepness: f64) -> f64 {
        1.0 / (1.0 + f64::exp(-steepness * (x - 0.5)))
    }
    pub(super) fn normalized_sigmoid(x: f64, steepness: f64) -> f64 {
        debug_assert!(0.0 <= x && x <= 1.0, "x must be in [0, 1], but got {}", x);

        let low = sigmoid(0.0, steepness);
        let high = sigmoid(1.0, steepness);
        let raw = sigmoid(x, steepness);

        // equivalent to (raw - low) / (high - low),
        // but in order to reduce the error, we use the following formula
        // ((raw^2 - low^2) * (high + low)) / ((raw + low) * (high^2 - low^2))
        let result = ((raw.powi(2) - low.powi(2)) * (high + low))
                        / ((raw + low) * (high.powi(2) - low.powi(2)));
        result.clamp(0.0, 1.0)
    }
}


#[cfg(test)]
mod tests {
    // TODO: Add tests for the channels
    // TODO: TOO MANY clamps! We need to test the math.
}