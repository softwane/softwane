use nalgebra::Matrix3;

use crate::channels::{ChannelType, ChannelValue};

// TODO: add documentation explaining what color transform matrix is 
// for this type
pub(super) type ColorTransformMatrix = Matrix3<f64>;

pub(super) type RGB = (f64, f64, f64);

/// Tanner Helland algorithm: approximated blackbody RGB for a given temperature.
/// Returns channel values on a 0.0–1.0 scale.
/// Use 6500 K as the white point: the algorithm's R channel is already
/// pinned to 255 there (t=6500 ≤ 6600), while G and B are at their natural
/// peak values just below 255. Dividing by these values normalises every
/// channel so that 6500 K yields identity scaling (all coefficients = 1).
/// Valid range: 1000 K – 40000 K. See https://tannerhelland.com/2012/09/18/convert-temperature-rgb-algorithm-code.html
// TODO: update the fitting model, referring to "https://github.com/softwane/kelvin-to-rgb-algo/tree/main/refs".
// It's from https://github.com/rgatkinson/ParticleDmxNeopixel
pub(super) fn color_temperature_to_rgb(kelvin: u32) -> RGB {
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

/// Build a 3×3 Rec.709-luma saturation matrix.
///
/// `s = 1.0` → identity (full saturation).
/// `s = 0.0` → all rows equal luma weights (full grayscale).
pub(super) fn saturation_to_ct_matrix3(s: f64) -> ColorTransformMatrix {
    const LR: f64 = 0.2126;
    const LG: f64 = 0.7152;
    const LB: f64 = 0.0722;

    const GRAYSCALE: ColorTransformMatrix = ColorTransformMatrix::new(
        LR, LG, LB,
        LR, LG, LB,
        LR, LG, LB,
    );

    GRAYSCALE * (1.0 - s) + ColorTransformMatrix::identity() * s
}

/// Combine saturation, colour temperature, and brightness into a single
/// 3×3 colour-transform matrix.
///
/// The returned matrix is the product:
///
/// ```text
/// diag(r·br, g·br, b·br) × saturation_matrix(s)
/// ```
///
/// where `(r, g, b) = kelvin_to_rgb(ct)`.
pub(super) fn tsb_to_ct_matrix3(ct_kelvin: u32, s: f64, br: f64) -> ColorTransformMatrix {
    let (r, g, b) = color_temperature_to_rgb(ct_kelvin);
    let rgb_brightness = ColorTransformMatrix::new(
        r * br, 0.0,    0.0,
        0.0,    g * br, 0.0,
        0.0,    0.0,    b * br,
    );
    rgb_brightness * saturation_to_ct_matrix3(s)
}

// ── macOS: shared display enumeration ─────────────────────────────────

#[cfg(target_os = "macos")]
pub(super) fn active_display_ids() -> Result<Vec<u32>, String> {
    type CGDirectDisplayID = u32;
    type CGDisplayCount = u32;
    type CGError = i32;

    const CG_ERROR_SUCCESS: CGError = 0;
    const MAX_ACTIVE_DISPLAYS: usize = 32;

    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn CGGetActiveDisplayList(
            max_displays: CGDisplayCount,
            active_displays: *mut CGDirectDisplayID,
            display_count: *mut CGDisplayCount,
        ) -> CGError;
    }

    let mut display_ids = vec![0u32; MAX_ACTIVE_DISPLAYS];
    let mut display_count: CGDisplayCount = 0;

    let result = unsafe {
        CGGetActiveDisplayList(
            MAX_ACTIVE_DISPLAYS as CGDisplayCount,
            display_ids.as_mut_ptr(),
            &mut display_count,
        )
    };

    if result != CG_ERROR_SUCCESS {
        return Err(format!("CGGetActiveDisplayList failed: CGError {result}"));
    }

    display_ids.truncate(display_count as usize);
    Ok(display_ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_color_temperature_to_rgb() {
        let (r, g, b) = color_temperature_to_rgb(6500);
        assert!((r - 1.0).abs() < 0.001);
        assert!((g - 1.0).abs() < 0.001);
        assert!((b - 1.0).abs() < 0.001);
        let (_, _, b) = color_temperature_to_rgb(1900);
        assert!((b - 0.0).abs() < 0.001);
    }

    #[test]
    fn saturation_matrix_at_one_is_identity() {
        let m = saturation_to_ct_matrix3(1.0);
        let diff = (m - ColorTransformMatrix::identity()).abs();
        assert!(diff.max() < 1e-10);
    }

    #[test]
    fn saturation_matrix_at_zero_all_rows_equal() {
        let m = saturation_to_ct_matrix3(0.0);
        for col in 0..3 {
            assert!((m[(0, col)] - m[(1, col)]).abs() < 1e-10);
            assert!((m[(1, col)] - m[(2, col)]).abs() < 1e-10);
        }
    }

    #[test]
    fn tsb_neutral_inputs_produce_near_identity() {
        let m = tsb_to_ct_matrix3(6500, 1.0, 1.0);
        for i in 0..3 {
            for j in 0..3 {
                let expected = if i == j { 1.0 } else { 0.0 };
                assert!(
                    (m[(i, j)] - expected).abs() < 1e-6,
                    "m[{i},{j}] = {} (expected ≈ {expected})",
                    m[(i, j)]
                );
            }
        }
    }
}