//! Per-state curve parameters and their 3-state table (Progress / Settling / Reverse).
//!
//! TODO: schema version / migration code for PersistentStateParamsTable.

use serde::{Deserialize, Serialize};

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
                "Invalid timer state for StateParamsTable: {:?}",
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
                "Invalid timer state for StateParamsTable: {:?}",
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
