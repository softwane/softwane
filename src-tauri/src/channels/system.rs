//! B5: `SensoryChannelsSystem` — the wrapper that owns the full channel array,
//! now with store-backed persistence.

use tauri::Wry;
use tauri_plugin_store::Store;

use crate::engine::FrameEvents;
use crate::events::ChannelCommand;
use crate::timer_state_machine::TimerState;
use super::*;

pub struct SensoryChannelsSystem {
    sensory_channels: SensoryChannelArray,
}

impl SensoryChannelsSystem {
    pub fn new(configs: ChannelConfigArray) -> Self {
        Self {
            sensory_channels: std::array::from_fn(|i| SensoryChannel::new(configs[i])),
        }
    }

    pub fn load_from_store(store: &Store<Wry>) -> Self {
        let sensory_channels: SensoryChannelArray = std::array::from_fn(|i| {
            let channel_type = SENSORY_CHANNEL_TYPES[i];
            let config = load_channel_config(store, channel_type);
            SensoryChannel::new(config)
        });
        Self { sensory_channels }
    }

    /// Drain `frame_events.channel_commands`, route each to the correct
    /// channel. Sets `frame_events.switch_changed` whenever a
    /// `ToggleSwitch` is processed, and accumulates channels that need
    /// persistence into `frame_events.need_persist.channels_system`.
    pub fn handle_commands(&mut self, frame_events: &mut FrameEvents) {
        for command in frame_events.channel_commands.drain(..) {
            if matches!(command, ChannelCommand::ToggleSwitch { .. }) {
                frame_events.switch_changed = true;
            }
            let target_type = command.channel_type();
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
    
    /// Persist a single channel's config. Called by the Engine after
    /// `handle_commands` has marked it as dirty.
    pub fn persist_channel(&self, channel_type: ChannelType, store: &Store<Wry>) {
        let key = channel_type.store_key();
        let value = serde_json::to_value(self.sensory_channels[channel_type].persist())
            .expect("ChannelConfig serialization is infallible");
        store.set(key, value);
    }
}
