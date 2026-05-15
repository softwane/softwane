//! Sub-renderer for macOS Core Graphics gamma-table manipulation.
//!
//! Unlike Windows' Magnification API, Core Graphics gamma tables are
//! **stateless** — no init/uninit pairing is needed.  All FFI calls are
//! synchronous and run directly on the engine thread.
//!
//! # Lifecycle
//!
//! | Method     | Behaviour |
//! |------------|-----------|
//! | `startup`  | Captures current gamma tables for every active display as
//! |            | the baseline that subsequent frames transform from. |
//! | `render`   | 1) Detect display hotplug (remove gone, add new).   |
//! |            | 2) When `ct`/`br` unchanged → emit `Unchanged` + skip. |
//! |            | 3) Compute per-channel multipliers from kelvin→RGB ×
//! |            |    brightness and apply them to every baseline table entry. |
//! |            | 4) Write transformed tables via `CGSetDisplayTransferByTable`. |
//! | `shutdown` | Restore all displays via `CGDisplayRestoreColorSyncSettings`,|
//! |            | drop baselines, emit `ShutdownCompleted`.            |

use std::collections::HashMap;

use tauri::AppHandle;
use tokio::sync::mpsc::Sender;

use crate::{
    channels::ChannelValue,
    engine::EngineEvent,
    utils::Update,
};
use super::{
    events::RendererEvent,
    utils::color_temperature_to_rgb,
};

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct DisplayGammaTable {
    red: Vec<CGGammaValue>,
    green: Vec<CGGammaValue>,
    blue: Vec<CGGammaValue>,
}

#[derive(Debug)]
pub(super) struct CoreGraphicsGammaTweaker {
    name: &'static str,
    /// Map from display ID to its baseline gamma table (captured at startup
    /// or on hotplug).  We never re-read a table for a display we have
    /// already touched — otherwise we would capture our own modifications.
    baseline_tables: HashMap<CGDirectDisplayID, DisplayGammaTable>,
    switch_on: bool,
}

impl Default for CoreGraphicsGammaTweaker {
    fn default() -> Self {
        Self {
            name: "MacOS-ApplicationServices-CoreGraphics-Gamma-Tweaker",
            baseline_tables: HashMap::new(),
            switch_on: false,
        }
    }
}

impl CoreGraphicsGammaTweaker {
    pub(super) fn new() -> Self {
        Self::default()
    }
}

// ── Public API (called by the dispatcher) ────────────────────────────

impl CoreGraphicsGammaTweaker {
    /// Compute and apply transformed gamma tables.
    pub(super) fn render(
        &mut self,
        color_temperature: Update<ChannelValue>,
        brightness: Update<ChannelValue>,
        _app: &AppHandle,
        tx: Sender<EngineEvent>,
    ) {
        // ── 0. Guard: render called before startup ────────────────
        if !self.switch_on {
            let _ = tx.try_send(EngineEvent::Renderer(
                RendererEvent::RenderUnappliedDueToNotStartupped {
                    renderer_name: self.name,
                },
            ));
            return;
        }

        // ── 1. Hotplug detection ──────────────────────────────────
        if let Ok(current_ids) = active_display_ids().inspect_err(|e| {
            // TODO: add semi-seccessful
            // But it couple system api with engine. Engine should NOT care about whether
            // there are different rendering results for different displays.
            // So, maybe move `log_renderer_event` from engine to renderer dispatcher.
            tracing::warn!(
                renderer_name = self.name,
                error = %e,
                "hotplug: failed to capture active display ids; skipping",
            );
            // skip updating available displays
        }) {
            // Remove displays that have been disconnected.
            self.baseline_tables
                .retain(|id, _| current_ids.contains(id));

            // Add newly connected displays (capture before we touch them).
            for &id in &current_ids {
                if self.baseline_tables.contains_key(&id) {
                    continue;
                }

                let Ok(table) = capture_gamma_table(id).inspect_err(|e| {
                    tracing::warn!(
                        renderer_name = self.name,
                        display_id = id,
                        error = %e,
                        "hotplug: failed to capture gamma table for new display; skipping",
                    );
                }) else {
                    continue; // skip this display
                };
                self.baseline_tables.insert(id, table);
            }
        };

        // ── 2. Unchanged fast path ────────────────────────────────
        if !color_temperature.is_changed() && !brightness.is_changed() {
            let _ = tx.try_send(EngineEvent::Renderer(
                RendererEvent::RenderUnappliedDueToUnchanged {
                    renderer_name: self.name,
                },
            ));
            return;
        }

        let ct_kelvin = match color_temperature.get_value() {
            ChannelValue::ColorTempKelvin(t) => *t,
            _ => panic!("Invalid ChannelValue when render. Expect ColorTempKelvin, but get: {color_temperature:?}."),
        };
        let br = match brightness.get_value() {
            ChannelValue::Brightness(b) => *b,
            _ => panic!("Invalid ChannelValue when render. Expect Brightness, but get: {brightness:?}."),
        };

        // ── 4. Compute per-channel multipliers ────────────────────
        let (r, g, b) = color_temperature_to_rgb(ct_kelvin);
        let r_scale = (r * br) as CGGammaValue;
        let g_scale = (g * br) as CGGammaValue;
        let b_scale = (b * br) as CGGammaValue;

        // ── 5. Transform & write every display ────────────────────
        for (&display_id, baseline) in &self.baseline_tables {
            let new_red: Vec<CGGammaValue> = baseline
                .red
                .iter()
                .map(|v| (v * r_scale).clamp(0.0, 1.0))
                .collect();
            let new_green: Vec<CGGammaValue> = baseline
                .green
                .iter()
                .map(|v| (v * g_scale).clamp(0.0, 1.0))
                .collect();
            let new_blue: Vec<CGGammaValue> = baseline
                .blue
                .iter()
                .map(|v| (v * b_scale).clamp(0.0, 1.0))
                .collect();

            let result = unsafe {
                CGSetDisplayTransferByTable(
                    display_id,
                    new_red.len() as CGTableCount,
                    new_red.as_ptr(),
                    new_green.as_ptr(),
                    new_blue.as_ptr(),
                )
            };

            if result != CG_ERROR_SUCCESS {
                let _ = tx.try_send(EngineEvent::Renderer(
                    RendererEvent::RenderFailed {
                        renderer_name: self.name,
                        error: format!(
                            "CGSetDisplayTransferByTable failed for display {display_id}: CGError {result}",
                        ),
                    },
                ));
                return;
            }
        }

        let _ = tx.try_send(EngineEvent::Renderer(
            RendererEvent::RenderSuccessful {
                renderer_name: self.name,
            },
        ));
    }

    /// Capture the current gamma tables of every active display and store
    /// them as the baseline for subsequent renders.  Emits
    /// [`StartupSuccessful`] on success, [`StartupFailed`] if any display
    /// cannot be read.
    pub(super) fn startup(&mut self, _app: &AppHandle, tx: Sender<EngineEvent>) {
        if self.switch_on {
            let _ = tx.try_send(EngineEvent::Renderer(
                RendererEvent::ShutdownCompleted { renderer_name: self.name },
            ));
            return;
        }
        
        let Ok(ids) = active_display_ids().inspect_err(|e| {
            let _ = tx.try_send(EngineEvent::Renderer(
                RendererEvent::StartupFailed {
                    renderer_name: self.name,
                    error: e.clone(),
                },
            ));
        }) else {
            return;
        };

        for id in ids {
            let Ok(table) = capture_gamma_table(id).inspect_err(|e| {
                let _ = tx.try_send(EngineEvent::Renderer(
                    RendererEvent::StartupFailed {
                        renderer_name: self.name,
                        error: format!("failed to capture display {id}: {e}"),
                    },
                ));
            }) else {
                return;
            };
            self.baseline_tables.insert(id, table);
        }

        self.switch_on = true;

        let _ = tx.try_send(EngineEvent::Renderer(
            RendererEvent::ShutdownCompleted { renderer_name: self.name },
        ));
    }


    /// Restore all displays to their system ColorSync defaults, clear
    /// stored baselines, and emit [`ShutdownCompleted`].
    pub(super) fn shutdown(&mut self, _app: &AppHandle, tx: Sender<EngineEvent>) {
        if !self.switch_on {
            let _ = tx.try_send(EngineEvent::Renderer(
                RendererEvent::ShutdownCompleted { renderer_name: self.name },
            ));
            return;
        }

        self.baseline_tables.clear();

        unsafe {
            CGDisplayRestoreColorSyncSettings();
        }

        let _ = tx.try_send(EngineEvent::Renderer(
            RendererEvent::ShutdownCompleted {
                renderer_name: self.name,
            },
        ));

        self.switch_on = false;
    }
}

// ---------------------------------------------------------------------------
// FFI helpers
// ---------------------------------------------------------------------------

fn active_display_ids() -> Result<Vec<CGDirectDisplayID>, String> {
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

fn capture_gamma_table(
    display_id: CGDirectDisplayID,
) -> Result<DisplayGammaTable, String> {
    let mut red = vec![0.0f32; GAMMA_TABLE_CAPACITY];
    let mut green = vec![0.0f32; GAMMA_TABLE_CAPACITY];
    let mut blue = vec![0.0f32; GAMMA_TABLE_CAPACITY];
    let mut sample_count: CGTableCount = 0;

    let result = unsafe {
        CGGetDisplayTransferByTable(
            display_id,
            GAMMA_TABLE_CAPACITY as CGTableCount,
            red.as_mut_ptr(),
            green.as_mut_ptr(),
            blue.as_mut_ptr(),
            &mut sample_count,
        )
    };

    if result != CG_ERROR_SUCCESS {
        return Err(format!(
            "CGGetDisplayTransferByTable failed for display {display_id}: CGError {result}",
        ));
    }

    let sc = sample_count as usize;
    red.truncate(sc);
    green.truncate(sc);
    blue.truncate(sc);

    if sc == 0 {
        return Err(format!(
            "display {display_id} returned an empty transfer table",
        ));
    }

    Ok(DisplayGammaTable { red, green, blue })
}

// ---------------------------------------------------------------------------
// macOS Core Graphics – FFI
// ---------------------------------------------------------------------------

type CGDirectDisplayID = u32;
type CGDisplayCount = u32;
type CGTableCount = u32;
type CGGammaValue = f32;
type CGError = i32;

const CG_ERROR_SUCCESS: CGError = 0;
const MAX_ACTIVE_DISPLAYS: usize = 32;
const GAMMA_TABLE_CAPACITY: usize = 256;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn CGGetActiveDisplayList(
        max_displays: CGDisplayCount,
        active_displays: *mut CGDirectDisplayID,
        display_count: *mut CGDisplayCount,
    ) -> CGError;

    fn CGGetDisplayTransferByTable(
        display: CGDirectDisplayID,
        capacity: CGTableCount,
        red_table: *mut CGGammaValue,
        green_table: *mut CGGammaValue,
        blue_table: *mut CGGammaValue,
        sample_count: *mut CGTableCount,
    ) -> CGError;

    fn CGSetDisplayTransferByTable(
        display: CGDirectDisplayID,
        table_size: CGTableCount,
        red_table: *const CGGammaValue,
        green_table: *const CGGammaValue,
        blue_table: *const CGGammaValue,
    ) -> CGError;

    fn CGDisplayRestoreColorSyncSettings();
}
