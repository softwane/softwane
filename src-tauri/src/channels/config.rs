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
            _ => panic!(
                "Invalid timer state index for StateParamsTable: {:?}. The table is: {:?}.",
                index.label(),
                self,
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
            _ => panic!(
                "Invalid timer state index for StateParamsTable: {:?}. The table is: {:?}.",
                index.label(),
                self,
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

pub fn load_channel_config(store: &Store<Wry>, channel_type: ChannelType) -> ChannelConfig {
    let key = channel_type.store_key();
    store.get(&key)
        .and_then(|v| Some(serde_json::from_value(v)
            .inspect_err(|e| {
                tracing::warn!(?channel_type, ?e,"stored config schema mismatch, using default");
                let value = serde_json::to_value(DEFAULT_CHANNEL_CONFIG_ARRAY[channel_type])
                    .expect("ChannelConfig serialization is infallible");
                store.set(&key, value);
            })
            .unwrap_or(DEFAULT_CHANNEL_CONFIG_ARRAY[channel_type])
        ))
        .expect("Defaults are set when setting up.")
}

pub fn load_channel_config_array(store: &Store<Wry>) -> ChannelConfigArray {
    let mut configs = std::array::from_fn(|i| {
        let channel_type = SENSORY_CHANNEL_TYPES[i];
        load_channel_config(store, channel_type)
    });
    normalize_channel_switch_conflicts(&mut configs, store);
    configs
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

fn normalize_channel_switch_conflicts(configs: &mut ChannelConfigArray, store: &Store<Wry>) {
    for channel_type in SENSORY_CHANNEL_TYPES {
        if !configs[channel_type].switch_on {
            continue;
        }

        for conflict in channel_type.conflicts() {
            if configs[*conflict].switch_on {
                configs[*conflict].switch_on = false;
                let value = serde_json::to_value(configs[*conflict])
                    .expect("ChannelConfig serialization is infallible");
                store.set(conflict.store_key(), value);
            }
        }
    }
}
