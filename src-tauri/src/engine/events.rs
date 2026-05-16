use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;

use crate::{
    channels::{ChannelType, commands::ChannelCommand},
    renderers::events::RendererEvent,
    timer_state_machine::{TimerState, commands::StateCommand},
};

// ── EngineEvent ───────────────────────────────────────────────────────

#[derive(Debug)]
pub enum EngineEvent {
    State(StateCommand),
    Channel(ChannelCommand),
    Renderer(RendererEvent),
    Progress(ProgressCommandInner),
    ForceReset,
    Shutdown,
    AbnormalShutdown,
}

pub enum ProgressCommandInner {
    RegisterChannel{ channel: Channel<ProgressPayload>, window: String },
    ClearChannel{ window: String },
}

impl std::fmt::Debug for ProgressCommandInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RegisterChannel{ channel, window } => write!(f, "RegisterChannel(Channel(id = {}) on {window} window)", channel.id()),
            Self::ClearChannel{ window} => write!(f, "ClearChannel on {window} window"),
        }
    }
}

// ── Progress ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ProgressPayload {
    pub timer_state: TimerState,
}

// ── FrameEvents ────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct FrameEvents {
    pub state_commands: Vec<StateCommand>,
    pub channel_commands: Vec<ChannelCommand>,
    pub shutdown_requested: bool,
    pub shutdown: bool,
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
            shutdown: false,
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

#[derive(Debug)]
pub struct NeedPersist {
    pub timer_state_machine: bool,
    pub channels_system: Option<Vec<ChannelType>>,
}