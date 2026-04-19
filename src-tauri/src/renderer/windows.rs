use std::sync::atomic::{AtomicBool, Ordering};

use crate::utils::*;
use super::*;


#[derive(Debug)]
pub struct Renderer {
    sender: Sender<RendererEvent>,
    color_transformer: WindowsColorTransformer,
}

impl Renderer {
    pub fn new(sender: Sender<RendererEvent>) -> Self {
        Self {
            sender,
            color_transformer: WindowsColorTransformer::default(),
        }
    }
}

impl RendererEventSending for Renderer {
    fn send_event(
        sender: Sender<RendererEvent>,
        event: RendererEvent,
    ) -> Result<(), SendError<RendererEvent>> {
        // blocking_send is safe here because the main loop runs on a synchronous
        // thread, not inside a tokio async runtime context.
        // TODO: switch to try_send if the main loop is ever moved to a tokio task.
        sender.blocking_send(event)
    }
}

impl Rendering for Renderer {
    fn render(&mut self, logic_frame: Arc<LogicFrame>, app: &AppHandle) {
        let saturation       = logic_frame[ChannelType::Saturation];
        let color_temperature = logic_frame[ChannelType::ColorTemperature];
        let brightness       = logic_frame[ChannelType::Brightness];
        self.color_transformer.transform_color(
            saturation,
            color_temperature,
            brightness,
            app,
            self.sender.clone(),
        );

        // Future render logic will be added here
    }
}

// ---------------------------------------------------------------------------
// WindowsColorTransformer
// ---------------------------------------------------------------------------

/// Tracks per-frame colour transformation state for the Windows Magnification
/// API back-end.
#[derive(Debug, Clone)]
struct WindowsColorTransformer {
    name: &'static str,
    cached_color_transform_matrix: Update<ColorTransformMatrix>,
    /// Shared with the main-thread closure that calls `MagInitialize`.
    /// `Arc<AtomicBool>` is required because the closure must be
    /// `FnOnce + Send + 'static` and therefore cannot borrow `&mut self`.
    magnification_initialized: Arc<AtomicBool>,
}

impl Default for WindowsColorTransformer {
    fn default() -> Self {
        Self {
            name: "windows-color-transformer-MagnificationAPI",
            cached_color_transform_matrix: Update::Changed(ColorTransformMatrix::identity()),
            magnification_initialized: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl WindowsColorTransformer {
    /// Computes a colour transformation matrix from the three channel values and
    /// dispatches it to the main thread via the Windows Magnification API.
    /// Returns immediately; success or failure is reported asynchronously
    /// through `sender`.
    fn transform_color(
        &mut self,
        saturation: Update<ChannelValue>,
        color_temperature: Update<ChannelValue>,
        brightness: Update<ChannelValue>,
        app: &AppHandle,
        sender: Sender<RendererEvent>,
    ) {
        self.update_color_transformation_matrix(saturation, color_temperature, brightness);
        match self.cached_color_transform_matrix {
            Update::Changed(color_transform_matrix) => {
                self.apply_color_transformation_matrix(color_transform_matrix, app, sender);
            }
            Update::Unchanged(_) => {
                let _ = Renderer::send_event(
                    sender,
                    RendererEvent::RenderUnappliedDueToUnchanged {
                        sub_renderer_name: self.name,
                    },
                );
            }
        }
    }

    /// Recomputes `cached_color_transform_matrix` from the three channel values.
    ///
    /// If all three inputs are `Unchanged` the cached matrix is preserved and
    /// re-tagged `Unchanged` without any arithmetic.  Otherwise the matrix is
    /// recomputed and tagged `Changed`.
    fn update_color_transformation_matrix(
        &mut self,
        saturation: Update<ChannelValue>,
        color_temperature: Update<ChannelValue>,
        brightness: Update<ChannelValue>,
    ) {
        if !saturation.is_changed()
            && !color_temperature.is_changed()
            && !brightness.is_changed()
        {
            self.cached_color_transform_matrix =
                Update::Unchanged(*self.cached_color_transform_matrix.get_value());
            return;
        }

        let s = match saturation.get_value() {
            ChannelValue::Saturation(s) => *s,
            _ => unreachable!(),
        };
        let c_kelvin = match color_temperature.get_value() {
            ChannelValue::ColorKelvin(t) => *t,
            _ => unreachable!(),
        };
        let b = match brightness.get_value() {
            ChannelValue::Brightness(b) => *b,
            _ => unreachable!(),
        };

        let saturation_matrix = Self::saturation_to_matrix(s);

        // `kelvin_to_rgb` returns normalised [0.0, 1.0] coefficients relative to
        // the D65 (6500 K) white point.  Multiplying each coefficient by the
        // brightness scalar ensures brightness only affects the R/G/B diagonal
        // entries; m44 (alpha) and m55 (homogeneous coordinate) remain 1.0.
        let (r, g, b_kelvin) = kelvin_to_rgb(c_kelvin);
        let rgb_brightness_matrix = ColorTransformMatrix::new(
            r * b,      0.0,        0.0,          0.0, 0.0,
            0.0,        g * b,      0.0,          0.0, 0.0,
            0.0,        0.0,        b_kelvin * b, 0.0, 0.0,
            0.0,        0.0,        0.0,          1.0, 0.0,
            0.0,        0.0,        0.0,          0.0, 1.0,
        );

        // Apply saturation first (may reduce to grayscale), then re-colour with
        // the colour-temperature coefficients and scale by brightness.
        let result = rgb_brightness_matrix * saturation_matrix;

        self.cached_color_transform_matrix = Update::Changed(result);
    }

    /// Dispatches `color_transform_matrix` to the Windows Magnification API on
    /// the main thread.  Returns immediately; the outcome is reported through
    /// `sender`.
    fn apply_color_transformation_matrix(
        &mut self,
        color_transform_matrix: ColorTransformMatrix,
        app: &AppHandle,
        sender: Sender<RendererEvent>,
    ) {
        // TODO: Two consecutive ticks may yield Matrix5<f64> values that differ
        // only by floating-point noise yet are identical after narrowing to f32.
        // No tolerance short-circuit is applied here (smooth-at-all-costs policy).
        // If a future optimisation is needed: compare `matrix_f32` with the last
        // f32 matrix actually written and send `RenderUnappliedDueToUnchanged`
        // when they are element-wise equal.
        let matrix_f32 = Self::to_row_major_f32(&color_transform_matrix);

        let name        = self.name;
        let initialized = Arc::clone(&self.magnification_initialized);
        let sender_for_closure = sender.clone();

        let dispatch_result = app.run_on_main_thread(move || {
            // ── runs on the main thread ────────────────────────────────────
            if !initialized.load(Ordering::Acquire) {
                if unsafe { MagInitialize() } == 0 {
                    let _ = Renderer::send_event(
                        sender_for_closure,
                        RendererEvent::RenderFailed {
                            sub_renderer_name: name,
                            error: "MagInitialize failed".into(),
                        },
                    );
                    return;
                }
                initialized.store(true, Ordering::Release);
            }

            let effect = MAGCOLOREFFECT { transform: matrix_f32 };
            let ok = unsafe {
                MagSetFullscreenColorEffect(&effect as *const MAGCOLOREFFECT)
            } != 0;

            let event = if ok {
                RendererEvent::RenderSuccessful { sub_renderer_name: name }
            } else {
                RendererEvent::RenderFailed {
                    sub_renderer_name: name,
                    error: "MagSetFullscreenColorEffect failed".into(),
                }
            };
            let _ = Renderer::send_event(sender_for_closure, event);
        });

        // `run_on_main_thread` itself fails only in extreme cases
        // (e.g. main thread has already exited).
        if let Err(err) = dispatch_result {
            let _ = Renderer::send_event(
                sender,
                RendererEvent::RenderFailed {
                    sub_renderer_name: self.name,
                    error: format!("run_on_main_thread failed: {err}"),
                },
            );
        }
    }

    
    // ---------------------------------------------------------------------------
    // Matrix helpers
    // ---------------------------------------------------------------------------

    /// Lerps between a fully desaturated (grayscale) matrix at `s = 0` and the
    /// identity matrix at `s = 1`, using Rec. 709 luminance weights.
    ///
    /// The matrix is in standard **M × v** column-vector convention.
    /// Call [`to_row_major_f32`] to convert it to the GDI+ **v × M** row-vector
    /// convention required by `MAGCOLOREFFECT`.
    fn saturation_to_matrix(s: f64) -> ColorTransformMatrix {
        const LR: f64 = 0.2126; // Rec. 709 luminance weights
        const LG: f64 = 0.7152;
        const LB: f64 = 0.0722;

        // Fully desaturated matrix: every output channel becomes the perceived
        // luminance of the input.
        // nalgebra::Matrix5::new() takes arguments in row-major order (m_row_col).
        const GRAYSCALE: ColorTransformMatrix = ColorTransformMatrix::new(
            LR,  LG,  LB,  0.0, 0.0,
            LR,  LG,  LB,  0.0, 0.0,
            LR,  LG,  LB,  0.0, 0.0,
            0.0, 0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 0.0, 1.0,
        );

        GRAYSCALE * (1.0 - s) + ColorTransformMatrix::identity() * s
    }

    /// Converts a nalgebra `Matrix5<f64>` expressed in the standard **M × v**
    /// column-vector convention to a row-major `[[f32; 5]; 5]` in the GDI+
    /// **v × M** row-vector convention expected by `MAGCOLOREFFECT`.
    ///
    /// GDI+ applies a colour effect as `output_row = input_row × M`, so
    /// `out[i][j]` is the weight applied to input channel *i* when computing
    /// output channel *j*. This is the transpose of the M × v representation
    /// where `m[(i, j)]` is the weight applied to input channel *j* for output
    /// channel *i*.
    fn to_row_major_f32(m: &ColorTransformMatrix) -> [[f32; 5]; 5] {
        let mut out = [[0.0f32; 5]; 5];
        for i in 0..5 {
            for j in 0..5 {
                // Transpose: GDI+ out[i][j] = M×v m[(j, i)]
                out[i][j] = m[(j, i)] as f32;
            }
        }
        out
    }
}


// ---------------------------------------------------------------------------
// Windows Magnification API – FFI bindings
// ---------------------------------------------------------------------------

#[repr(C)]
struct MAGCOLOREFFECT {
    transform: [[f32; 5]; 5],
}

#[link(name = "Magnification")]
unsafe extern "system" {
    fn MagInitialize() -> i32;
    fn MagUninitialize() -> i32;
    fn MagSetFullscreenColorEffect(pEffect: *const MAGCOLOREFFECT) -> i32;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saturation_matrix_at_one_is_identity() {
        let m = WindowsColorTransformer::saturation_to_matrix(1.0);
        let diff = (m - ColorTransformMatrix::identity()).abs();
        assert!(diff.max() < 1e-10);
    }

    #[test]
    fn saturation_matrix_at_zero_all_rgb_rows_equal() {
        let m = WindowsColorTransformer::saturation_to_matrix(0.0);
        // In M×v convention each row is one output channel.
        // All three RGB output rows must carry the same luminance weights.
        for col in 0..3 {
            assert!((m[(0, col)] - m[(1, col)]).abs() < 1e-10);
            assert!((m[(1, col)] - m[(2, col)]).abs() < 1e-10);
        }
        // Alpha and homogeneous rows/cols are untouched.
        assert!((m[(3, 3)] - 1.0).abs() < 1e-10);
        assert!((m[(4, 4)] - 1.0).abs() < 1e-10);
    }

    #[test]
    fn to_row_major_f32_transposes_correctly() {
        // Use an asymmetric matrix so the transpose direction is observable.
        let mut m = ColorTransformMatrix::zeros();
        m[(0, 1)] = 3.0; // row 0, col 1
        m[(1, 0)] = 7.0; // row 1, col 0
        let out = WindowsColorTransformer::to_row_major_f32(&m);
        // out[i][j] = m[(j, i)], so:
        assert!((out[0][1] - 7.0f32).abs() < 1e-6, "out[0][1] should equal m[(1,0)]=7");
        assert!((out[1][0] - 3.0f32).abs() < 1e-6, "out[1][0] should equal m[(0,1)]=3");
    }

    #[test]
    fn to_row_major_f32_identity_maps_to_identity() {
        let out = WindowsColorTransformer::to_row_major_f32(&ColorTransformMatrix::identity());
        for row in 0..5 {
            for col in 0..5 {
                let expected = if row == col { 1.0f32 } else { 0.0f32 };
                assert!((out[row][col] - expected).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn all_unchanged_preserves_and_untags_cached() {
        let mut t = WindowsColorTransformer::default();
        // Stamp in a non-trivial Changed matrix.
        t.update_color_transformation_matrix(
            Update::Changed(ChannelValue::Saturation(0.5)),
            Update::Changed(ChannelValue::ColorKelvin(3000)),
            Update::Changed(ChannelValue::Brightness(0.8)),
        );
        let cached_before = *t.cached_color_transform_matrix.get_value();

        // All Unchanged: must re-tag Unchanged without recomputing.
        t.update_color_transformation_matrix(
            Update::Unchanged(ChannelValue::Saturation(0.5)),
            Update::Unchanged(ChannelValue::ColorKelvin(3000)),
            Update::Unchanged(ChannelValue::Brightness(0.8)),
        );

        assert!(!t.cached_color_transform_matrix.is_changed());
        let diff = (*t.cached_color_transform_matrix.get_value() - cached_before).abs();
        assert!(diff.max() < 1e-10);
    }

    #[test]
    fn neutral_inputs_produce_near_identity() {
        let mut t = WindowsColorTransformer::default();
        t.update_color_transformation_matrix(
            Update::Changed(ChannelType::Saturation.neutral_value()),
            Update::Changed(ChannelType::ColorTemperature.neutral_value()),
            Update::Changed(ChannelType::Brightness.neutral_value()),
        );
        let m = *t.cached_color_transform_matrix.get_value();
        for i in 0..3 {
            for j in 0..3 {
                let expected = if i == j { 1.0 } else { 0.0 };
                assert!(
                    (m[(i, j)] - expected).abs() < 1e-6,
                    "m[{i},{j}] = {} (expected ≈ {expected})", m[(i, j)]
                );
            }
        }
    }
}
