//! Sub-renderer for the Windows Magnification API.
//!
//! Maintains an `initialized: Arc<AtomicBool>` whose semantics are:
//!
//! | `initialized` | Meaning |
//! |---|---|
//! | `false` | Dispatcher has ordered shutdown (or never started). |
//! | `true`  | Dispatcher has ordered startup.  The init closure may |
//! |         | still be queued on the main thread, but FIFO ordering |
//! |         | guarantees it executes before any subsequent apply.   |
//!
//! Two safety nets prevent an apply closure from running when
//! `MagInitialize` has not succeeded:
//! 1. `render()` checks `initialized` on the engine thread and skips
//!    with `RenderUnappliedDueToNotInitialized` when false.
//! 2. `apply_matrix`'s main-thread closure re-checks `initialized`
//!    (defence against the init closure having rolled it back to false).
//!
//! Init and apply are **strictly separated** — there is no lazy-init
//! inside the apply path.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tauri::AppHandle;
use tokio::sync::mpsc::Sender;

use crate::channels::ChannelValue;
use crate::events::{EngineEvent, RendererEvent};
use crate::utils::Update;

use super::utils::{ColorTransformMatrix, kelvin_to_rgb};

#[derive(Debug, Clone)]
pub(super) struct WinMagAPIColorTransformer {
    name: &'static str,
    cached_matrix: Update<ColorTransformMatrix>,
    magnification_initialized: Arc<AtomicBool>,
}

impl Default for WinMagAPIColorTransformer {
    fn default() -> Self {
        Self {
            name: "Windows-MagnificationAPI-Color-Transformer",
            cached_matrix: Update::Changed(ColorTransformMatrix::identity()),
            magnification_initialized: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl WinMagAPIColorTransformer {
    pub(super) fn new() -> Self {
        Self::default()
    }
}

// ── Public API (called by the dispatcher) ────────────────────────────
impl WinMagAPIColorTransformer {
    pub(super) fn render(
        &mut self,
        saturation: Update<ChannelValue>,
        color_temperature: Update<ChannelValue>,
        brightness: Update<ChannelValue>,
        app: &AppHandle,
        tx: Sender<EngineEvent>,
    ) {

        self.update_cached_matrix(saturation, color_temperature, brightness);
        
        if !self.magnification_initialized.load(Ordering::Acquire) {
            let _ = tx.try_send(EngineEvent::Renderer(
                RendererEvent::RenderUnappliedDueToNotStartupped {
                    renderer_name: self.name,
                },
            ));
            return;
        }
        match self.cached_matrix {
            Update::Changed(_) => self.apply_matrix(app, tx),
            Update::Unchanged(_) => {
                let _ = tx.try_send(EngineEvent::Renderer(
                    RendererEvent::RenderUnappliedDueToUnchanged {
                        renderer_name: self.name,
                    },
                ));
            }
        }
    }

    pub(super) fn startup(&mut self, app: &AppHandle, tx: Sender<EngineEvent>) {
        if self.magnification_initialized.load(Ordering::Acquire) {
            return;
        }

        // Reset cached matrix so the next render forces recomputation + dispatch.
        self.cached_matrix = Update::Changed(ColorTransformMatrix::identity());

        // Optimistically set — init closure will roll back on failure.
        self.magnification_initialized.store(true, Ordering::Release);

        self.init_api(app, tx);
    }

    /// Shutdown sequence (on the engine thread):
    /// 1. Apply identity matrix (returns screen to neutral).
    /// 2. Queue `MagUninitialize` on the main thread.
    /// 3. Optimistically clear `initialized`.
    ///
    /// FIFO ordering guarantees the apply closure runs before the uninit
    /// closure.  The uninit closure always emits [`ShutdownCompleted`].
    pub(super) fn shutdown(&mut self, app: &AppHandle, tx: Sender<EngineEvent>) {
        if !self.magnification_initialized.load(Ordering::Acquire) {
            let _ = tx.try_send(EngineEvent::Renderer(
                RendererEvent::ShutdownCompleted { renderer_name: self.name },
            ));
            return;
        }

        self.cached_matrix = Update::Changed(ColorTransformMatrix::identity());
        self.apply_matrix(app, tx.clone());

        self.uninit_api(app, tx);
        self.magnification_initialized.store(false, Ordering::Release);
    }
}

// ── Main-thread dispatch helpers ─────────────────────────────────────

impl WinMagAPIColorTransformer {
    /// Queue a `MagInitialize` call on the main thread.
    ///
    /// On failure, rolls back `initialized` to `false` and emits
    /// [`StartupFailed`].  On success, emits [`StartupCompleted`].
    fn init_api(&self, app: &AppHandle, tx: Sender<EngineEvent>) {
        let name = self.name;
        let initialized = Arc::clone(&self.magnification_initialized);
        let dispatch = app.run_on_main_thread(move || {
            if unsafe { MagInitialize() } == 0 {
                initialized.store(false, Ordering::Release);
                let _ = tx.try_send(EngineEvent::Renderer(
                    RendererEvent::StartupFailed {
                        renderer_name: name,
                        error: "MagInitialize failed".into(),
                    },
                ));
            } else {
                let _ = tx.try_send(EngineEvent::Renderer(
                    RendererEvent::StartupSuccessful { renderer_name: name },
                ));
            }
        });
        if let Err(err) = dispatch {
            tracing::error!(
                "{} when trying to initialize Magnification API on main thread",
                err.to_string(),
            );
            // EventLoopClosed: engine thread cannot process events either.
            self.magnification_initialized.store(false, Ordering::Release);
        }
    }

    /// Queue a `MagSetFullscreenColorEffect` call on the main thread.
    ///
    /// The closure checks `initialized` as a defence-in-depth measure:
    /// if the init closure failed and rolled back `initialized`, any
    /// already-queued apply closure skips with
    /// [`RenderUnappliedDueToNotInitialized`].
    fn apply_matrix(&self, app: &AppHandle, tx: Sender<EngineEvent>) {
        let name = self.name;
        let initialized = Arc::clone(&self.magnification_initialized);
        let matrix_f32 = self.cached_matrix.get_value().cast().into();

        let dispatch = app.run_on_main_thread(move || {
            if !initialized.load(Ordering::Acquire) {
                let _ = tx.try_send(EngineEvent::Renderer(
                    RendererEvent::RenderUnappliedDueToNotStartupped {
                        renderer_name: name,
                    },
                ));
                return;
            }
            let effect = MAGCOLOREFFECT { transform: matrix_f32 };
            let ok = unsafe {
                MagSetFullscreenColorEffect(&effect as *const MAGCOLOREFFECT)
            } != 0;
            let event = if ok {
                RendererEvent::RenderSuccessful { renderer_name: name }
            } else {
                RendererEvent::RenderFailed {
                    renderer_name: name,
                    error: "MagSetFullscreenColorEffect failed".into(),
                }
            };
            let _ = tx.try_send(EngineEvent::Renderer(event));
        });
        if let Err(err) = &dispatch {
            tracing::error!(
                "{} when trying to apply color transform matrix using Magnification API on main thread",
                err.to_string(),
            );
            // EventLoopClosed: the main thread has already exited;
            // this is only reachable during process teardown.  Panic
            // propagates to the engine thread, which B6 will catch.
            dispatch.unwrap();
        }
    }

    /// Queue a `MagUninitialize` call on the main thread.
    ///
    /// Always emits [`ShutdownCompleted`] — even on EventLoopClosed
    /// (fallback path so the engine never hangs waiting for an ack).
    fn uninit_api(&self, app: &AppHandle, tx: Sender<EngineEvent>) {
        let name = self.name;
        let initialized = Arc::clone(&self.magnification_initialized);
        let tx_for_closure = tx.clone();
        let dispatch = app.run_on_main_thread(move || {
            unsafe { MagUninitialize() };   // 返回值忽略，无可恢复路径
            initialized.store(false, Ordering::Release);
            let _ = tx_for_closure.try_send(EngineEvent::Renderer(
                RendererEvent::ShutdownCompleted { renderer_name: name },
            ));
        });
        if let Err(err) = dispatch {
            tracing::error!(
                "{} when trying to uninitialize Magnification API on main thread",
                err.to_string(),
            );
            // Fallback: main thread gone, ack anyway to prevent hang.
            let _ = tx.try_send(EngineEvent::Renderer(
                RendererEvent::ShutdownCompleted { renderer_name: name },
            ));
        }
    }
}

// ── Private helpers ──────────────────────────────────────────────────

impl WinMagAPIColorTransformer {
    fn update_cached_matrix(
        &mut self,
        saturation: Update<ChannelValue>,
        color_temperature: Update<ChannelValue>,
        brightness: Update<ChannelValue>,
    ) {
        if !saturation.is_changed()
            && !color_temperature.is_changed()
            && !brightness.is_changed()
        {
            self.cached_matrix =
                Update::Unchanged(*self.cached_matrix.get_value());
            return;
        }

        let s = match saturation.get_value() {
            ChannelValue::Saturation(s) => *s,
            _ => panic!("Invalid ChannelValue when update_cached_matrix. Expect Saturation, but get: {saturation:?}."),
        };
        let ct_kelvin = match color_temperature.get_value() {
            ChannelValue::ColorTempKelvin(t) => *t,
            _ => panic!("Invalid ChannelValue when update_cached_matrix. Expect ColorTempKelvin, but get: {color_temperature:?}."),
        };
        let br = match brightness.get_value() {
            ChannelValue::Brightness(b) => *b,
            _ => panic!("Invalid ChannelValue when update_cached_matrix. Expect Brightness, but get: {brightness:?}."),
        };

        let saturation_matrix = Self::saturation_to_matrix(s);

        let (r, g, b) = kelvin_to_rgb(ct_kelvin);
        let rgb_brightness_matrix = ColorTransformMatrix::new(
            r * br, 0.0,    0.0,    0.0,    0.0,
            0.0,    g * br, 0.0,    0.0,    0.0,
            0.0,    0.0,    b * br, 0.0,    0.0,
            0.0,    0.0,    0.0,    1.0,    0.0,
            0.0,    0.0,    0.0,    0.0,    1.0,
        );

        let result = rgb_brightness_matrix * saturation_matrix;
        self.cached_matrix = Update::Changed(result);
    }

    // ── Matrix helpers ────────────────────────────────────────────────

    fn saturation_to_matrix(s: f64) -> ColorTransformMatrix {
        const LR: f64 = 0.2126;
        const LG: f64 = 0.7152;
        const LB: f64 = 0.0722;

        const GRAYSCALE: ColorTransformMatrix = ColorTransformMatrix::new(
            LR,  LG,  LB,  0.0, 0.0,
            LR,  LG,  LB,  0.0, 0.0,
            LR,  LG,  LB,  0.0, 0.0,
            0.0, 0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 0.0, 1.0,
        );

        GRAYSCALE * (1.0 - s) + ColorTransformMatrix::identity() * s
    }
}

// ---------------------------------------------------------------------------
// Windows Magnification API – FFI
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::ChannelType;

    #[test]
    fn saturation_matrix_at_one_is_identity() {
        let m = WinMagAPIColorTransformer::saturation_to_matrix(1.0);
        let diff = (m - ColorTransformMatrix::identity()).abs();
        assert!(diff.max() < 1e-10);
    }

    #[test]
    fn saturation_matrix_at_zero_all_rgb_rows_equal() {
        let m = WinMagAPIColorTransformer::saturation_to_matrix(0.0);
        for col in 0..3 {
            assert!((m[(0, col)] - m[(1, col)]).abs() < 1e-10);
            assert!((m[(1, col)] - m[(2, col)]).abs() < 1e-10);
        }
        assert!((m[(3, 3)] - 1.0).abs() < 1e-10);
        assert!((m[(4, 4)] - 1.0).abs() < 1e-10);
    }

    #[test]
    fn all_unchanged_preserves_and_untags_cached() {
        let mut t = WinMagAPIColorTransformer::default();
        t.update_cached_matrix(
            Update::Changed(ChannelValue::Saturation(0.5)),
            Update::Changed(ChannelValue::ColorTempKelvin(3000)),
            Update::Changed(ChannelValue::Brightness(0.8)),
        );
        let cached_before = *t.cached_matrix.get_value();

        t.update_cached_matrix(
            Update::Unchanged(ChannelValue::Saturation(0.5)),
            Update::Unchanged(ChannelValue::ColorTempKelvin(3000)),
            Update::Unchanged(ChannelValue::Brightness(0.8)),
        );

        assert!(!t.cached_matrix.is_changed());
        let diff = (*t.cached_matrix.get_value() - cached_before).abs();
        assert!(diff.max() < 1e-10);
    }

    #[test]
    fn neutral_inputs_produce_near_identity() {
        let mut t = WinMagAPIColorTransformer::default();
        t.update_cached_matrix(
            Update::Changed(ChannelType::Saturation.neutral_value()),
            Update::Changed(ChannelType::ColorTemp.neutral_value()),
            Update::Changed(ChannelType::Brightness.neutral_value()),
        );
        let m = *t.cached_matrix.get_value();
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
