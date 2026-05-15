//! B5: `SensoryChannelsSystem` — the wrapper that owns the full channel array,
//! now with store-backed persistence.

use crate::{
    engine::FrameEvents,
    timer_state_machine::TimerState,
};
use super::commands::ChannelCommand;
use super::*;

#[derive(Debug)]
pub struct SensoryChannelsSystem {
    pub(super) sensory_channels: SensoryChannelArray,
}

impl SensoryChannelsSystem {
    pub fn new(configs: ChannelConfigArray) -> Self {
        Self {
            sensory_channels: std::array::from_fn(|i| SensoryChannel::new(configs[i])),
        }
    }

    /// Drain `frame_events.channel_commands`, route each to the correct
    /// channel. Sets `frame_events.switch_changed` whenever a
    /// `ToggleSwitch` is processed, and accumulates channels that need
    /// persistence into `frame_events.need_persist.channels_system`.
    pub fn handle_commands(&mut self, frame_events: &mut FrameEvents) {
        for command in std::mem::take(&mut frame_events.channel_commands) {
            let target_type = command.channel_type();
            if let ChannelCommand::ToggleSwitch { switch_on, .. } = command {
                if switch_on {
                    self.disable_conflicting_channels(target_type, frame_events);
                }
                frame_events.switch_changed = true;
            }
            self.sensory_channels[target_type].apply(command);
            frame_events
                .need_persist
                .channels_system
                .get_or_insert_with(Vec::new)
                .push(target_type);
        }
    }

    /// Tick every channel.
    pub fn tick(&mut self, state: TimerState, frame_events: &mut FrameEvents) {
        for channel in self.sensory_channels.iter_mut() {
            channel.tick(state, frame_events);
        }
    }

    /// Force-reset: snap every channel's `current` to neutral.
    pub fn reset(&mut self) {
        for channel in self.sensory_channels.iter_mut() {
            channel.reset_current_to_neutral();
        }
    }

    pub fn logic_frame(&self) -> LogicFrame {
        std::array::from_fn(|i| self.sensory_channels[i].current())
    }

    pub fn switch_states(&self) -> ChannelSwitchStates {
        std::array::from_fn(|i| self.sensory_channels[i].switch_on())
    }

    fn disable_conflicting_channels(&mut self, channel_type: ChannelType, frame_events: &mut FrameEvents) {
        for conflict in channel_type.conflicts() {
            if self.sensory_channels[*conflict].switch_on() {
                self.sensory_channels[*conflict].apply(ChannelCommand::ToggleSwitch {
                    channel_type: *conflict,
                    switch_on: false,
                });
                frame_events
                    .need_persist
                    .channels_system
                    .get_or_insert_with(Vec::new)
                    .push(*conflict);
            }
        }
    }
}
