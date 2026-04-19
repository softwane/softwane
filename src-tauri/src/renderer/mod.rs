use std::sync::{
    Arc,
};

use tauri::AppHandle;
use nalgebra::Matrix5;
use tokio::sync::mpsc::{Sender, error::SendError};

use crate::{
    channels::{ChannelType, ChannelValue, LogicFrame},
    events::RendererEvent,
};


#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use self::macos::*;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use self::windows::*;


pub trait Rendering {
    fn render(
        &mut self,
        logic_frame: Arc<LogicFrame>,
        app: &AppHandle,
    );
}

trait RendererEventSending {
    fn send_event(sender: Sender<RendererEvent>, event: RendererEvent) -> Result<(), SendError<RendererEvent>>;
}

// TODO: add explanation for the matrix
type ColorTransformMatrix = Matrix5<f64>;

type R = f64;
type G = f64;
type B = f64;

/// Tanner Helland algorithm: approximated blackbody RGB for a given temperature.
/// Returns channel values on a 0.0–1.0 scale.
/// Use 6500 K as the white point: the algorithm's R channel is already
/// pinned to 255 there (t=6500 ≤ 6600), while G and B are at their natural
/// peak values just below 255. Dividing by these values normalises every
/// channel so that 6500 K yields identity scaling (all coefficients = 1).
/// Valid range: 1000 K – 40000 K. See https://tannerhelland.com/2012/09/18/convert-temperature-rgb-algorithm-code.html
// TODO: update the fitting model, referring to "/refs/Black body temperature.nb".
// It's from https://github.com/rgatkinson/ParticleDmxNeopixel
fn kelvin_to_rgb(kelvin: u32) -> (R, G, B) {
    let t = kelvin.clamp(1000, 40000);
    const T_D65: u32 = match ChannelType::ColorTemperature.neutral_value() {
        ChannelValue::ColorKelvin(k) => k,
        _ => unreachable!(),
    };

    const R_FUNCTION: fn(u32) -> R = |t: u32| -> R {
        if t <= 6600 {
            255.0
        } else {
            (329.698727446 * (((t - 6000)/100) as f64).powf(-0.1332047592)).clamp(0.0, 255.0)
        }
    };
    let r = R_FUNCTION(t);
    const R_D65: R = R_FUNCTION(T_D65);

    const G_FUNCTION: fn(u32) -> G = |t: u32| -> G {
        if t <= 6600 {
            (99.4708025861 * (t/100 as f64).ln() - 161.1195681661).clamp(0.0, 255.0)
        } else {
            (288.1221695283 * (((t - 6000)/100) as f64).powf(-0.0755148492)).clamp(0.0, 255.0)
        }
    };
    let g = G_FUNCTION(t);
    const G_D65: G = G_FUNCTION(T_D65);

    const B_FUNCTION: fn(u32) -> B = |t: u32| -> B {
        if t >= 6600 {
            255.0
        } else if t <= 1900 {
            0.0
        } else {
            (138.5177312231 * (((t - 1000)/100) as f64).ln() - 305.0447927307).clamp(0.0, 255.0)
        }
    };
    let b = B_FUNCTION(t);
    const B_D65: B = B_FUNCTION(T_D65);

    (
        (r / R_D65).clamp(0.0, 1.0),
        (g / G_D65).clamp(0.0, 1.0),
        (b / B_D65).clamp(0.0, 1.0),
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