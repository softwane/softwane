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

use nalgebra::Matrix5;
use tauri::AppHandle;
use tokio::sync::mpsc::Sender;

use crate::{
    channels::ChannelValue,
    engine::EngineEvent,
    utils::Update,
};
use super::{
    events::RendererEvent,
    utils::{tsb_to_ct_matrix3},
};

#[derive(Debug, Clone)]
pub(super) struct WinMagAPIColorTransformer {
    name: &'static str,
    magnification_initialized: Arc<AtomicBool>,
}

impl Default for WinMagAPIColorTransformer {
    fn default() -> Self {
        Self {
            name: "Windows-MagnificationAPI-Color-Transformer",
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
        color_temperature: Update<ChannelValue>,
        saturation: Update<ChannelValue>,
        brightness: Update<ChannelValue>,
        app: &AppHandle,
        tx: Sender<EngineEvent>,
    ) {
        if !self.magnification_initialized.load(Ordering::Acquire) {
            let _ = tx.try_send(EngineEvent::Renderer(
                RendererEvent::RenderUnappliedDueToNotStartupped {
                    renderer_name: self.name,
                },
            ));
            return;
        }
        
        if !color_temperature.is_changed()
            && !saturation.is_changed()
            && !brightness.is_changed()
        {
            let _ = tx.try_send(EngineEvent::Renderer(
                RendererEvent::RenderUnappliedDueToUnchanged {
                    renderer_name: self.name,
                },
            ));
            return;
        }

        let matrix = self.calculate_matrix(
            *color_temperature.get_value(),
            *saturation.get_value(),
            *brightness.get_value(),
        );

        self.apply_matrix(app, tx, &matrix);
    }

    pub(super) fn startup(&mut self, app: &AppHandle, tx: Sender<EngineEvent>) {
        if self.magnification_initialized.load(Ordering::Acquire) {
            return;
        }

        self.init_api(app, tx.clone());

        let identity = Matrix5::identity();
        self.apply_matrix(app, tx, &identity);
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

        let identity = Matrix5::identity();
        self.apply_matrix(app, tx.clone(), &identity);

        self.uninit_api(app, tx);
    }

    pub(super) fn shutdown_on_main_thread(&mut self) {
        if !self.magnification_initialized.load(Ordering::Acquire) { return; }

        let identity = Matrix5::identity().into();

        let effect = MAGCOLOREFFECT { transform: identity };
        let ok = unsafe {
            MagSetFullscreenColorEffect(&effect as *const MAGCOLOREFFECT)
        } != 0;

        if !ok {
            tracing::error!("Unable to reset color effect when shutdown_on_main_thread");
        }

        if unsafe { MagUninitialize() == 0 } {
            tracing::error!("Unable to uninitialize the Windows Magnification API when shutdown_on_main_thread");
        };

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
                let _ = tx.try_send(EngineEvent::Renderer(
                    RendererEvent::StartupFailed {
                        renderer_name: name,
                        error: "MagInitialize failed".into(),
                    },
                ));
            } else {
                initialized.store(true, Ordering::Release);
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
        }
    }

    /// Queue a `MagSetFullscreenColorEffect` call on the main thread.
    ///
    /// The closure checks `initialized` as a defence-in-depth measure:
    /// if the init closure failed and rolled back `initialized`, any
    /// already-queued apply closure skips with
    /// [`RenderUnappliedDueToNotInitialized`].
    fn apply_matrix(&self, app: &AppHandle, tx: Sender<EngineEvent>, matrix: &Matrix5<f64>) {
        let name = self.name;
        let initialized = Arc::clone(&self.magnification_initialized);
        let matrix_f32 = matrix.cast().into();

        let dispatch = app.run_on_main_thread(move || {
            tracing::trace!("applying color transform matrix: {}", initialized.load(Ordering::Acquire));
            if !initialized.load(Ordering::Acquire) {
                let _ = tx.try_send(EngineEvent::Renderer(
                    RendererEvent::RenderUnappliedDueToNotStartupped {
                        renderer_name: name,
                    },
                ));
                tracing::trace!("cancel applying color transform matrix: {}", initialized.load(Ordering::Acquire));
                return;
            }
            let effect = MAGCOLOREFFECT { transform: matrix_f32 };
            let ok = unsafe {
                MagSetFullscreenColorEffect(&effect as *const MAGCOLOREFFECT)
            } != 0;
            tracing::trace!("applied color transform matrix: {}", initialized.load(Ordering::Acquire));
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
            if unsafe { MagUninitialize() == 0 } {
                tracing::error!("Unable to uninitialize the Windows Magnification API when run_on_main_thread");
            } else {
                tracing::info!("Magnification API uninitialized successfully");
                initialized.store(false, Ordering::Release);
            }
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

// ── Calculation ──────────────────────────────────────────────────

impl WinMagAPIColorTransformer {
    /// Compute a 5×5 colour-transform matrix from channel values.
    ///
    /// Calls `tsb_to_ct_matrix3` in [`utils`](super::utils) for the
    /// 3×3 colour portion, then embeds it into the top-left corner of a
    /// 5×5 identity matrix (rows 3–4 are pass-through for alpha / unused
    /// channels required by the Magnification API).
    fn calculate_matrix(&self, color_temperature: ChannelValue, saturation: ChannelValue, brightness: ChannelValue) -> Matrix5<f64> {
        let ct_kelvin = match color_temperature {
            ChannelValue::ColorTempKelvin(t) => t,
            _ => panic!("Invalid ChannelValue when calculate_matrix. Expect Color Temperature, but get: {color_temperature:?}."),
        };
        let s = match saturation {
            ChannelValue::Saturation(s) => s,
            _ => panic!("Invalid ChannelValue when calculate_matrix. Expect Saturation, but get: {saturation:?}."),
        };
        let br = match brightness {
            ChannelValue::Brightness(b) => b,
            _ => panic!("Invalid ChannelValue when calculate_matrix. Expect Brightness, but get: {brightness:?}."),
        };
        
        let m3 = tsb_to_ct_matrix3(ct_kelvin, s, br);
        let mut m5 = Matrix5::identity();
        m5.fixed_view_mut::<3, 3>(0, 0).copy_from(&m3);
        m5
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
    fn calculate_matrix_neutral_produces_identity() {
        let t = WinMagAPIColorTransformer::default();
        let m = t.calculate_matrix(
            ChannelType::ColorTemp.neutral_value(),
            ChannelType::Saturation.neutral_value(),
            ChannelType::Brightness.neutral_value(),
        );
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
        // rows 3-4 should remain identity
        for i in 3..5 {
            assert!((m[(i, i)] - 1.0).abs() < 1e-6);
        }
    }
}
