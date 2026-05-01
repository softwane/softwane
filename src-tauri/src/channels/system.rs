//! B3: `SensoryChannelsSystem` — the wrapper that owns the full channel array.
//!
//! TODO: `SensoryChannelsSystem::load(&Store)` – load persisted config on startup.
//! TODO: `persist_channel(...)` – wire up `store.set(...)` per-channel after each
//!       command that modifies config.  Deferred to the persistence batch.

use crate::engine::FrameEvents;
use crate::events::ChannelCommand;
use crate::timer_state_machine::TimerState;
use super::*;

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

pub struct SensoryChannelsSystem {
    array: SensoryChannelArray,
}

impl SensoryChannelsSystem {
    pub fn new(configs: ChannelConfigArray) -> Self {
        Self {
            array: std::array::from_fn(|i| SensoryChannel::new(configs[i])),
        }
    }

    /// Drain `frame_events.channel_commands`, route each to the correct
    /// channel, and set `frame_events.switch_changed` whenever a
    /// `ToggleSwitch` is processed.
    fn handle_commands(&mut self, frame_events: &mut FrameEvents) {
        for command in frame_events.channel_commands.drain(..) {
            if matches!(command, ChannelCommand::ToggleSwitch { .. }) {
                frame_events.switch_changed = true;
            }
            let target_type = command.channel_type();
            self.array[target_type].apply(command);
            // TODO(persistence batch): call self.persist_channel(target_type, store)
        }
    }

    /// Tick every channel.
    pub fn tick(&mut self, state: TimerState, frame_events: &mut FrameEvents) {
        for channel in self.array.iter_mut() {
            channel.tick(state, frame_events);
        }
    }
    
    /// Force-reset: snap every channel's `current` to neutral.
    pub fn reset(&mut self) {
        for channel in self.array.iter_mut() {
            channel.reset_current_to_neutral();
        }
    }

    pub fn logic_frame(&self) -> LogicFrame {
        std::array::from_fn(|i| self.array[i].current())
    }

    pub fn switch_states(&self) -> ChannelSwitchStates {
        std::array::from_fn(|i| {
            // Access switch_on through persist() to avoid exposing the field.
            self.array[i].switch_on()
        })
    }
}
