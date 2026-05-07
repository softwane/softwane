//! Per-state curve parameters and their 3-state table (Progress / Settling / Reverse).
//!
//! TODO: schema version / migration code for PersistentStateParamsTable.

use serde::{Deserialize, Serialize};
use tauri::Wry;
use tauri_plugin_store::Store;

use crate::timer_state_machine::*;
use super::*;

/// Parameters for a single "active" timer state.
///
/// The name "state" here refers to the subset of [`TimerState`] variants that
/// can advance over time: Progress, Settling, and Reverse.  Preview reuses the
/// Progress slot (see [`StateParamsTable::index`]).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct StateParams {
    pub curve_parameters: CurveParameters,
    /// 0.0 = curve begins immediately; 1.0 = curve never begins.
    pub curve_begin_ratio: f64,
    pub curve_begin_value: ChannelValue,
    pub target_value: ChannelValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct StateParamsTable([StateParams; 3]);

impl StateParamsTable {
    pub fn new(persistent: &PersistentStateParamsTable) -> Self {
        let channel_type: ChannelType = persistent.target_channel_value.into();
        let progress_params = StateParams {
            curve_parameters: persistent.progress_curve_parameters,
            curve_begin_ratio: persistent.progress_begin_ratio,
            curve_begin_value: channel_type.neutral_value(),
            target_value: persistent.target_channel_value,
        };
        let settling_params = StateParams {
            curve_parameters: persistent.settling_curve_parameters,
            curve_begin_ratio: 0.0,
            curve_begin_value: channel_type.neutral_value(),
            target_value: persistent.target_channel_value,
        };
        let reverse_params = StateParams {
            curve_parameters: persistent.reverse_curve_parameters,
            curve_begin_ratio: 0.0,
            curve_begin_value: persistent.target_channel_value,
            target_value: channel_type.neutral_value(),
        };
        Self([progress_params, settling_params, reverse_params])
    }

    pub fn persist(&self) -> PersistentStateParamsTable {
        let progress_params = self[PROGRESS_STATE];
        let settling_params = self[SETTLING_STATE];
        let reverse_params = self[REVERSE_STATE];
        PersistentStateParamsTable {
            progress_curve_parameters: progress_params.curve_parameters,
            settling_curve_parameters: settling_params.curve_parameters,
            reverse_curve_parameters: reverse_params.curve_parameters,
            progress_begin_ratio: progress_params.curve_begin_ratio,
            target_channel_value: progress_params.target_value,
        }
    }
}

impl std::ops::Index<TimerState> for StateParamsTable {
    type Output = StateParams;
    fn index(&self, index: TimerState) -> &Self::Output {
        match index {
            TimerState::Progress { .. } | TimerState::Preview { .. } => &self.0[0],
            TimerState::Settling { .. } => &self.0[1],
            TimerState::Reverse { .. } => &self.0[2],
            _ => unreachable!(
                "Invalid timer state for StateParamsTable: {:?}.",
                index.label()
            ),
        }
    }
}

impl std::ops::IndexMut<TimerState> for StateParamsTable {
    fn index_mut(&mut self, index: TimerState) -> &mut Self::Output {
        match index {
            TimerState::Progress { .. } | TimerState::Preview { .. } => &mut self.0[0],
            TimerState::Settling { .. } => &mut self.0[1],
            TimerState::Reverse { .. } => &mut self.0[2],
            _ => unreachable!(
                "Invalid timer state for StateParamsTable: {:?}.",
                index.label()
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PersistentStateParamsTable {
    pub progress_curve_parameters: CurveParameters,
    pub settling_curve_parameters: CurveParameters,
    pub reverse_curve_parameters: CurveParameters,
    pub progress_begin_ratio: f64,
    pub target_channel_value: ChannelValue,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChannelConfig {
    pub switch_on: bool,
    pub persistent_state_params_table: PersistentStateParamsTable,
}

pub fn load_channel_config(store: &tauri_plugin_store::Store<tauri::Wry>, channel_type: ChannelType) -> ChannelConfig {
    let key = channel_type.store_key();
    let Some(raw) = store.get(&key) else {
        tracing::info!(?channel_type, "no stored config, using default");
        return DEFAULT_CHANNEL_CONFIG_ARRAY[channel_type];
    };
    serde_json::from_value::<ChannelConfig>(raw).unwrap_or_else(|e| {
        tracing::warn!(
            ?channel_type, ?e,
            "stored config schema mismatch, using default"
        );
        DEFAULT_CHANNEL_CONFIG_ARRAY[channel_type]
        
    })
}

pub fn load_channel_config_array(store: &Store<Wry>) -> ChannelConfigArray {
    std::array::from_fn(|i| {
        let channel_type = SENSORY_CHANNEL_TYPES[i];
        load_channel_config(store, channel_type)
    })
}

/// Persist a single channel's config. Called by the Engine after
/// `handle_commands` has marked it as dirty.
pub fn persist_channel(channels_system: &SensoryChannelsSystem, channel_type: ChannelType, store: &Store<Wry>) {
    let key = channel_type.store_key();
    let value = serde_json::to_value(channels_system.sensory_channels[channel_type].persist())
        .expect("ChannelConfig serialization is infallible");
    store.set(key, value);
}

pub fn store_defaults() -> Vec<(String, serde_json::Value)> {
    SENSORY_CHANNEL_TYPES
        .iter()
        .map(|ct| {
            (
                ct.store_key(),
                serde_json::to_value(DEFAULT_CHANNEL_CONFIG_ARRAY[*ct])
                    .expect("ChannelConfig serialization is infallible"),
            )
        })
        .collect()
}