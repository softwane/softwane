//! The `define_channels!` macro and the standalone `impl_channel_array_type_index!` helper.
//!
//! This macro generates `ChannelType`, `ChannelValue`, type aliases, arithmetic ops,
//! and the default `ChannelConfigArray`.  It is invoked **once** in `mod.rs` with the
//! full channel list (cross-platform), relying on
//! [`ChannelType::is_available_on_this_platform`] for per-platform filtering at
//! runtime rather than splitting the definition with `#[cfg]`.

use super::*;

macro_rules! define_channels {
    (
        $(
            $type_var:ident(default_on=$switch_on:literal, persist_key=$persist_key:literal)
                => $val_var:ident($data_type:ty, neutral=$val_neutral:literal, default_target=$default_target:literal)
        );* $(;)?
    ) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum ChannelType {
            $( $type_var, )*
        }

        pub const SENSORY_CHANNEL_COUNT: usize = [$(ChannelType::$type_var,)*].len();
        pub const SENSORY_CHANNEL_TYPES: [ChannelType; SENSORY_CHANNEL_COUNT] =
            [$(ChannelType::$type_var,)*];

        impl ChannelType {
            pub const fn neutral_value(&self) -> ChannelValue {
                match self {
                    $( ChannelType::$type_var => ChannelValue::$val_var($val_neutral), )*
                }
            }

            /// Stable key used for persistence.  Even if the enum variant name
            /// changes in a future refactor, this value stays the same so that
            /// previously-stored config continues to deserialise correctly.
            const fn persist_key(&self) -> &'static str {
                match self {
                    $( ChannelType::$type_var => $persist_key, )*
                }
            }

            pub fn store_key(&self) -> String {
                format!("channels.{}", self.persist_key())
            }
        }

        // TODO: custom PartialEq with f64 tolerance to absorb jitter.
        //       When that happens, introduce a ChannelValueEq helper trait,
        //       impl for f64 (with tolerance) / for u32 (==), and replace
        //       #[derive(PartialEq)] with a manual impl.
        #[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
        #[serde(tag = "type", content = "data", rename_all = "snake_case")]
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

        macro_rules! impl_channel_value_binop {
            ($binop_trait:ident, $binop_method:ident, $op:tt) => {
                impl std::ops::$binop_trait for ChannelValue {
                    type Output = Self;
                    fn $binop_method(self, other: ChannelValue) -> Self {
                        match (self, other) {
                            $( (Self::$val_var(a), Self::$val_var(b)) => Self::$val_var(a $op b), )*
                            _ => panic!(
                                "Cannot {} between different channel values: {:?} and {:?}.",
                                stringify!($binop_method), self, other
                            ),
                        }
                    }
                }
            };
        }
        pub(self) use impl_channel_value_binop;

        impl std::ops::Mul<f64> for ChannelValue {
            type Output = Self;
            #[allow(clippy::unnecessary_cast)]
            fn mul(self, other: f64) -> Self {
                match self {
                    $( Self::$val_var(a) => Self::$val_var((a as f64 * other) as $data_type), )*
                }
            }
        }

        pub const DEFAULT_CHANNEL_CONFIG_ARRAY: ChannelConfigArray = [
            $(
                ChannelConfig {
                    switch_on: $switch_on,
                    persistent_state_params_table: PersistentStateParamsTable {
                        progress_curve_parameters: DEFAULT_NORMALIZED_SIGMOID_PARAMETERS,
                        settling_curve_parameters: DEFAULT_NORMALIZED_SIGMOID_PARAMETERS,
                        reverse_curve_parameters: DEFAULT_NORMALIZED_SIGMOID_PARAMETERS,
                        progress_begin_ratio: DEFAULT_PROGRESS_BEGIN_RATIO,
                        target_channel_value: ChannelValue::$val_var($default_target),
                    },
                },
            )*
        ];
    };
}

pub(super) use define_channels;

impl_channel_value_binop!(Add, add, +);
impl_channel_value_binop!(Sub, sub, -);
impl_channel_value_binop!(Mul, mul, *);

pub type ChannelConfigArray = [ChannelConfig; SENSORY_CHANNEL_COUNT];
pub type SensoryChannelArray = [SensoryChannel; SENSORY_CHANNEL_COUNT];
pub type LogicFrame = [Update<ChannelValue>; SENSORY_CHANNEL_COUNT];
pub type ChannelSwitchStates = [bool; SENSORY_CHANNEL_COUNT];

/// Standalone helper macro — no longer nested inside `define_channels!`.
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
    };
}

impl_channel_array_type_index!(ChannelConfigArray, ChannelConfig);
impl_channel_array_type_index!(SensoryChannelArray, SensoryChannel);
impl_channel_array_type_index!(LogicFrame, Update<ChannelValue>);
impl_channel_array_type_index!(ChannelSwitchStates, bool);

impl ChannelType {
    pub const fn conflicts(&self) -> &'static [ChannelType] {
        #[cfg(target_os = "windows")]
        return &[];
        #[cfg(target_os = "macos")]
        match self {
            Self::Brightness => &[Self::Saturation],
            Self::ColorTemp => &[Self::Saturation],
            Self::Saturation => &[Self::Brightness, Self::ColorTemp],
        }
    }
}
