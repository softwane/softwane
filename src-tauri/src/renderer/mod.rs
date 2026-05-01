use std::sync::{
    Arc,
};

use tauri::AppHandle;
use nalgebra::Matrix5;
use tokio::sync::mpsc::{Sender, error::SendError};

use crate::{
    channels::{ChannelSwitchStates, ChannelType, ChannelValue, LogicFrame},
    events::RendererEvent,
};


#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use self::macos::*;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
#[allow(unused_imports)]
pub use self::windows::*;


pub trait Rendering {
    fn render(
        &mut self,
        logic_frame: Arc<LogicFrame>,
        app: &AppHandle,
    );

    // Used when exiting the app.
    fn try_shutdown(&mut self, app: &AppHandle);

    // Used by user to reset the renderer to the initial state.
    fn try_reset(&mut self, app: &AppHandle);

    // Used when the channel switch states change.
    fn switch_subrenderer_states(
        &mut self,
        channel_switch_states: ChannelSwitchStates,
        app: &AppHandle,
    );
}

trait RendererEventSending {
    fn send_event(sender: Sender<RendererEvent>, event: RendererEvent) -> Result<(), SendError<RendererEvent>>;
}

// TODO: add explanation for the matrix
type ColorTransformMatrix = Matrix5<f64>;

type RGB = (f64, f64, f64);

/// Tanner Helland algorithm: approximated blackbody RGB for a given temperature.
/// Returns channel values on a 0.0–1.0 scale.
/// Use 6500 K as the white point: the algorithm's R channel is already
/// pinned to 255 there (t=6500 ≤ 6600), while G and B are at their natural
/// peak values just below 255. Dividing by these values normalises every
/// channel so that 6500 K yields identity scaling (all coefficients = 1).
/// Valid range: 1000 K – 40000 K. See https://tannerhelland.com/2012/09/18/convert-temperature-rgb-algorithm-code.html
// TODO: update the fitting model, referring to "/refs/Black body temperature.nb".
// It's from https://github.com/rgatkinson/ParticleDmxNeopixel
fn kelvin_to_rgb(kelvin: u32) -> RGB {
    let t = kelvin.clamp(1000, 40000);
    const T_D65: u32 = match ChannelType::ColorTemp.neutral_value() {
        ChannelValue::ColorTempKelvin(k) => k,
        _ => unreachable!(),
    };

    fn r_function(t: u32) -> f64 {
        if t <= 6600 {
            255.0
        } else {
            (329.698727446 * (((t - 6000)/100) as f64).powf(-0.1332047592)).clamp(0.0, 255.0)
        }
    }
    let r = r_function(t);
    let r_d65: f64 = r_function(T_D65);

    fn g_function(t: u32) -> f64 {
        if t <= 6600 {
            (99.4708025861 * ((t/100) as f64).ln() - 161.1195681661).clamp(0.0, 255.0)
        } else {
            (288.1221695283 * (((t - 6000)/100) as f64).powf(-0.0755148492)).clamp(0.0, 255.0)
        }
    }
    let g = g_function(t);
    let g_d65: f64 = g_function(T_D65);

    fn b_function(t: u32) -> f64 {
        if t >= 6600 {
            255.0
        } else if t <= 1900 {
            0.0
        } else {
            (138.5177312231 * (((t - 1000)/100) as f64).ln() - 305.0447927307).clamp(0.0, 255.0)
        }
    }
    let b = b_function(t);
    let b_d65: f64 = b_function(T_D65);

    (
        (r / r_d65).clamp(0.0, 1.0),
        (g / g_d65).clamp(0.0, 1.0),
        (b / b_d65).clamp(0.0, 1.0),
    )
}


#[cfg(test)]
mod tests {
    use super::*;

    
    #[test]
    fn test_kelvin_to_rgb() {
        let (r, g, b) = kelvin_to_rgb(6500);
        let r_abs_diff = (r - 1.0).abs();
        let g_abs_diff = (g - 1.0).abs();
        let b_abs_diff = (b - 1.0).abs();
        assert!(r_abs_diff < 0.001);
        assert!(g_abs_diff < 0.001);
        assert!(b_abs_diff < 0.001);
        let (_, _, b) = kelvin_to_rgb(1900);
        let b_abs_diff = (b - 0.0).abs();
        assert!(b_abs_diff < 0.001);
    }
}