mod definition;
mod state_params;
mod channel_implementation;
mod system;
mod curves;

pub use state_params::*;
pub use channel_implementation::*;
pub use system::*;
pub use curves::*;
pub use definition::*;

use serde::{Deserialize, Serialize};

use crate::utils::Update;

define_channels!(
    Saturation(default_on=true,  persist_key="saturation")
        => Saturation(f64, neutral=1.0f64, default_target=0.2f64);
    ColorTemp(default_on=true,   persist_key="color_temp")
        => ColorTempKelvin(u32, neutral=6500u32, default_target=2500u32);
    Brightness(default_on=false, persist_key="brightness")
        => Brightness(f64, neutral=1.0f64, default_target=0.6f64);
);


// ---------------------------------------------------------------------------
// ChannelType helpers
// ---------------------------------------------------------------------------

impl ChannelType {
    /// Returns whether this channel is supported on the current platform.
    /// The frontend calls this once at startup to decide which settings
    /// panels to render.
    ///
    /// macOS does not support Saturation currently (the gamma table cannot
    /// de-saturate across channels).
    pub const fn is_available_on_this_platform(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            !matches!(self, ChannelType::Saturation)
        }
        #[cfg(target_os = "windows")]
        {
            true
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    // TODO: Add tests for the channels
    // TODO: TOO MANY clamps! We need to test the math.
}
