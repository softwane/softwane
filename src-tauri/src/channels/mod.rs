mod definition;
mod config;
mod channel_implementation;
mod system;
mod curves;
pub mod commands;

pub use config::*;
pub use channel_implementation::*;
pub use system::*;
pub use curves::*;
pub use definition::*;

use serde::{Deserialize, Serialize};

use crate::utils::Update;

define_channels!(
    Saturation(default_on=false, persist_key="saturation")
        => Saturation(f64, neutral=1.0, default_target=0.2);
    ColorTemp(default_on=true,   persist_key="color_temp")
        => ColorTempKelvin(u32, neutral=6500, default_target=2500);
    Brightness(default_on=false, persist_key="brightness")
        => Brightness(f64, neutral=1.0, default_target=0.6);
);
const DEFAULT_PROGRESS_BEGIN_RATIO: f64 = 0.9;

impl ChannelType {
    /// Returns whether this channel is supported on the current platform.
    /// The frontend calls this once at startup to decide which settings
    /// panels to render.
    ///
    pub const fn is_available_on_this_platform(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            true
        }
        #[cfg(target_os = "windows")]
        {
            true
        }
    }

    pub const fn conflicts(&self) -> &'static [ChannelType] {
        #[cfg(target_os = "windows")]
        return &[];
        #[cfg(target_os = "macos")]
        match self {
            Self::Brightness => &[Self::Saturation],
            Self::ColorTemp => &[Self::Saturation],
            Self::Saturation => &[Self::Brightness, Self::ColorTemp],
        }
    }
}

#[cfg(test)]
mod tests {
    // TODO: Add tests for the channels
    // TOO MANY clamps! We need to test the math.
}
