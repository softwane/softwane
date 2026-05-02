use crate::{channels::ChannelType, events::{ChannelCommand, StateCommand}};

pub struct FrameEvents {
    pub state_commands: Vec<StateCommand>,
    pub channel_commands: Vec<ChannelCommand>,
    pub shutdown_requested: bool,
    pub force_reset: bool,
    pub just_transited: bool,
    pub switch_changed: bool,
    pub need_persist: NeedPersist,
}

impl Default for FrameEvents {
    fn default() -> Self {
        Self {
            state_commands: Vec::new(),
            channel_commands: Vec::new(),
            shutdown_requested: false,
            force_reset: false,
            just_transited: false,
            switch_changed: false,
            need_persist: NeedPersist {
                timer_state_machine: false,
                channels_system: Option::None,
            }
        }
    }
}

pub struct NeedPersist {
    pub timer_state_machine: bool,
    pub channels_system: Option<Vec<ChannelType>>,
}