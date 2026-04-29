mod timer_state_commands;
pub use self::timer_state_commands::*;

mod channel_commands;
pub use self::channel_commands::*;

mod renderer_events;
pub use self::renderer_events::*;

mod progress_commands;
pub use self::progress_commands::*;

pub enum EngineEvent {
    State(StateCommand),
    Channel(ChannelCommand),
    Renderer(RendererEvent),
    Progress(ProgressCommand),
    ForceReset,
    Shutdown,
}