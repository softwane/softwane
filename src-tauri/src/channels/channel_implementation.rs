use serde::{Deserialize, Serialize};

use crate::{
    engine::FrameEvents,
    timer_state_machine::*,
    utils::*,
};
use super::*;
use super::commands::ChannelCommand;



#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SensoryChannel {
    channel_type: ChannelType,
    switch_on: bool,
    state_params_table: StateParamsTable,
    /// Current channel value with change-tracking. Initialised to
    /// `Changed(neutral)` so that the first tick always produces a
    /// defined output regardless of platform renderer state.
    current: Update<ChannelValue>,
}

impl SensoryChannel {
    pub fn new(config: ChannelConfig) -> Self {
        let channel_type = config.persistent_state_params_table.target_channel_value.into();
        Self {
            channel_type,
            switch_on: config.switch_on,
            state_params_table: StateParamsTable::new(
                &config.persistent_state_params_table,
            ),
            current: Update::Unchanged(channel_type.neutral_value()),
        }
    }

    pub fn persist(&self) -> ChannelConfig {
        ChannelConfig {
            switch_on: self.switch_on,
            persistent_state_params_table: self.state_params_table.persist(),
        }
    }

    /// Apply a single command to this channel.
    ///
    /// The caller ([`SensoryChannelsSystem`](super::SensoryChannelsSystem))
    /// guarantees that the command targets this channel.  A mis-routed
    /// command is logged with [`tracing::warn!`] and silently rejected
    /// (defence-in-depth).
    pub fn apply(&mut self, command: ChannelCommand) {
        if command.channel_type() != self.channel_type {
            tracing::warn!(
                cmd_channel = ?command.channel_type(),
                self_channel = ?self.channel_type,
                "command routed to wrong channel; ignored"
            );
            return;
        }
        match command {
            ChannelCommand::ToggleSwitch { switch_on, .. } => {
                self.switch_on = switch_on;
            }
            ChannelCommand::UpdateTargetChannelValue {
                target_channel_value,
            } => {
                self.state_params_table[PROGRESS_STATE].target_value =
                    target_channel_value;
                self.state_params_table[SETTLING_STATE].target_value =
                    target_channel_value;
            }
            ChannelCommand::UpdateProgressBeginRatio {
                progress_begin_ratio,
                ..
            } => {
                self.state_params_table[PROGRESS_STATE].curve_begin_ratio =
                    progress_begin_ratio;
            }
            ChannelCommand::UpdateProgressCurveParas {
                curve_parameters, ..
            } => {
                self.state_params_table[PROGRESS_STATE].curve_parameters =
                    curve_parameters;
            }
            ChannelCommand::UpdateSettlingCurveParas {
                curve_parameters, ..
            } => {
                self.state_params_table[SETTLING_STATE].curve_parameters =
                    curve_parameters;
            }
            ChannelCommand::UpdateReverseCurveParas {
                curve_parameters, ..
            } => {
                self.state_params_table[REVERSE_STATE].curve_parameters =
                    curve_parameters;
            }
        }
    }

    pub fn tick(&mut self, state: TimerState, frame_events: &FrameEvents) {
        let this_value: ChannelValue = if !self.switch_on {
            self.channel_type.neutral_value()
        } else {
            match state {
                TimerState::Idle => self.channel_type.neutral_value(),
                TimerState::Rest => {
                    self.state_params_table[SETTLING_STATE].target_value
                }
                TimerState::Preview { progress } => {
                    Self::calculate_at_progress(
                        &self.state_params_table[state],
                        progress,
                    )
                }
                TimerState::Progress { .. } => self.calculate_at_state(state),
                TimerState::Settling { .. }
                | TimerState::Reverse { .. } => {
                    if frame_events.just_transited {
                        self.state_params_table[state].curve_begin_value =
                            *self.current.get_value();
                    }
                    self.calculate_at_state(state)
                }
            }
        };

        if this_value == *self.current.get_value() {
            self.current = Update::Unchanged(this_value);
        } else {
            self.current = Update::Changed(this_value);
        }
    }
    
    /// Compute the channel value at a given progress (0.0–1.0).
    /// Used directly by [`TimerState::Preview`] and indirectly by
    /// [`calculate_at_state`] for time-driven states.
    fn calculate_at_progress(
        params: &StateParams,
        progress: f64,
    ) -> ChannelValue {
        if progress < params.curve_begin_ratio {
            return params.curve_begin_value;
        }
        if progress >= 1.0 {
            return params.target_value;
        }
        let normalized = (progress - params.curve_begin_ratio)
            / (1.0 - params.curve_begin_ratio);
        let curve_intensity = match params.curve_parameters {
            CurveParameters::NormalizedSigmoid { steepness } => {
                normalized_sigmoid(normalized, steepness)
            }
        };
        // begin * (1 - intensity) + target * intensity
        params.curve_begin_value * (1.0 - curve_intensity)
            + params.target_value * curve_intensity
    }

    fn calculate_at_state(&self, state: TimerState) -> ChannelValue {
        let params = &self.state_params_table[state];
        let elapsed_ms = state.elapsed_ms();
        let target_duration_ms = state.target_duration_ms();
        let progress = elapsed_ms as f64 / target_duration_ms as f64;
        Self::calculate_at_progress(params, progress)
    }


    pub fn channel_type(&self) -> ChannelType {
        self.channel_type
    }

    pub fn current(&self) -> Update<ChannelValue> {
        self.current
    }

    pub(super) fn switch_on(&self) -> bool {
        self.switch_on
    }

    /// Force-reset: snap `current` to the neutral value without modifying
    /// `switch_on` or `state_params_table`.  The next tick will recalculate
    /// normally.
    pub(super) fn reset_current_to_neutral(&mut self) {
        self.current = Update::Changed(self.channel_type.neutral_value());
    }
}
