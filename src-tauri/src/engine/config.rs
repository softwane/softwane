use serde::Serialize;

use crate::{
    channels::{ChannelConfig, ChannelType},
    timer_state_machine::TimerConfig,
};

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub struct StoredConfig {
    pub channel_configs: Vec<(ChannelType, ChannelConfig)>,
    pub timer_config: TimerConfig,
}