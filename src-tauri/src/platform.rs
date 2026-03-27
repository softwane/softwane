use serde::Serialize;
use tauri::State;

use crate::engine::EffectSnapshot;

#[derive(Debug, Clone, Serialize)]
pub struct ApplyResult {
    pub applied: bool,
    pub backend: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub enum CueStyle {
    Dim,
    Warm,
    Full,
}

impl CueStyle {
    pub fn from_id(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "color" | "dim" => Self::Dim,
            "full" => Self::Full,
            _ => Self::Warm,
        }
    }
}

pub trait DisplayEffectApplier {
    fn apply(&self, snapshot: &EffectSnapshot, cue_style: CueStyle) -> ApplyResult;
}

pub type ManagedDisplayEffectApplier = PlatformDisplayEffectApplier;

#[cfg_attr(target_os = "macos", allow(dead_code))]
#[derive(Default)]
pub struct MockDisplayEffectApplier;

impl DisplayEffectApplier for MockDisplayEffectApplier {
    fn apply(&self, _snapshot: &EffectSnapshot, _cue_style: CueStyle) -> ApplyResult {
        ApplyResult {
            applied: false,
            backend: "mock",
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub type PlatformDisplayEffectApplier = MockDisplayEffectApplier;

#[cfg(target_os = "macos")]
pub type PlatformDisplayEffectApplier = MacDisplayEffectApplier;

#[cfg(target_os = "macos")]
mod macos {
    use std::sync::Mutex;

    use super::{ApplyResult, CueStyle, DisplayEffectApplier};
    use crate::engine::EffectSnapshot;

    type CGDirectDisplayID = u32;
    type CGDisplayCount = u32;
    type CGTableCount = u32;
    type CGGammaValue = f32;
    type CGError = i32;

    const CG_ERROR_SUCCESS: CGError = 0;
    const MAX_ACTIVE_DISPLAYS: usize = 32;
    const GAMMA_TABLE_CAPACITY: usize = 256;
    const NEUTRAL_WARMTH_KELVIN: f32 = 6500.0;
    const MIN_WARMTH_KELVIN: f32 = 2500.0;

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

    #[derive(Clone)]
    struct DisplayGammaTable {
        display_id: CGDirectDisplayID,
        red: Vec<CGGammaValue>,
        green: Vec<CGGammaValue>,
        blue: Vec<CGGammaValue>,
    }

    #[derive(Default)]
    struct MacDisplayState {
        baseline_tables: Option<Vec<DisplayGammaTable>>,
    }

    #[derive(Default)]
    pub struct MacDisplayEffectApplier {
        state: Mutex<MacDisplayState>,
    }

    impl DisplayEffectApplier for MacDisplayEffectApplier {
        fn apply(&self, snapshot: &EffectSnapshot, cue_style: CueStyle) -> ApplyResult {
            match self.apply_snapshot(snapshot, cue_style) {
                Ok(applied) => ApplyResult {
                    applied,
                    backend: "macos-core-graphics",
                },
                Err(_) => ApplyResult {
                    applied: false,
                    backend: "macos-core-graphics",
                },
            }
        }
    }

    impl Drop for MacDisplayEffectApplier {
        fn drop(&mut self) {
            unsafe {
                CGDisplayRestoreColorSyncSettings();
            }
        }
    }

    impl MacDisplayEffectApplier {
        fn apply_snapshot(
            &self,
            snapshot: &EffectSnapshot,
            cue_style: CueStyle,
        ) -> Result<bool, MacDisplayError> {
            let mut state = self
                .state
                .lock()
                .map_err(|_| MacDisplayError::StatePoisoned)?;

            if state.baseline_tables.is_none() {
                state.baseline_tables = Some(capture_display_baselines()?);
            }

            let baseline_tables = state
                .baseline_tables
                .as_ref()
                .ok_or(MacDisplayError::MissingBaselines)?;

            if is_neutral_snapshot(snapshot) {
                unsafe {
                    CGDisplayRestoreColorSyncSettings();
                }
                return Ok(true);
            }

            let modifiers = cue_style.modifiers();
            let warmth =
                (normalize_warmth(snapshot.warmth_kelvin) * modifiers.warmth).clamp(0.0, 1.0);
            let grayscale =
                (snapshot.grayscale.clamp(0.0, 1.0) * modifiers.grayscale).clamp(0.0, 1.0);
            let saturation =
                (snapshot.saturation.clamp(0.0, 1.0) * modifiers.saturation).clamp(0.0, 1.0);
            let chroma = (saturation * (1.0 - grayscale)).clamp(0.0, 1.0);
            let brightness = (1.0
                - warmth * modifiers.brightness_warmth
                - grayscale * modifiers.brightness_grayscale)
                .clamp(modifiers.min_brightness, 1.0);
            let red_scale = (brightness * (1.0 + warmth * modifiers.red_boost)).clamp(0.0, 1.25);
            let green_scale =
                (brightness * (1.0 + warmth * modifiers.green_boost)).clamp(0.0, 1.15);
            let blue_scale =
                (brightness * (1.0 - warmth * modifiers.blue_reduction)).clamp(0.0, 1.0);
            let gamma = (1.0
                + grayscale * modifiers.gamma_grayscale
                + (1.0 - chroma) * modifiers.gamma_chroma)
                .clamp(1.0, 2.2);

            for table in baseline_tables {
                let red = transform_channel(&table.red, red_scale, gamma);
                let green = transform_channel(&table.green, green_scale, gamma);
                let blue = transform_channel(&table.blue, blue_scale, gamma);

                let result = unsafe {
                    CGSetDisplayTransferByTable(
                        table.display_id,
                        red.len() as CGTableCount,
                        red.as_ptr(),
                        green.as_ptr(),
                        blue.as_ptr(),
                    )
                };

                if result != CG_ERROR_SUCCESS {
                    return Err(MacDisplayError::CgError(result));
                }
            }

            Ok(true)
        }
    }

    fn is_neutral_snapshot(snapshot: &EffectSnapshot) -> bool {
        snapshot.grayscale <= 0.001
            && (snapshot.saturation - 1.0).abs() <= 0.001
            && snapshot.warmth_kelvin >= NEUTRAL_WARMTH_KELVIN as u32
    }

    fn normalize_warmth(warmth_kelvin: u32) -> f32 {
        ((NEUTRAL_WARMTH_KELVIN - warmth_kelvin as f32)
            / (NEUTRAL_WARMTH_KELVIN - MIN_WARMTH_KELVIN))
            .clamp(0.0, 1.0)
    }

    fn transform_channel(baseline: &[CGGammaValue], scale: f32, gamma: f32) -> Vec<CGGammaValue> {
        baseline
            .iter()
            .map(|value| value.clamp(0.0, 1.0).powf(gamma) * scale)
            .map(|value| value.clamp(0.0, 1.0))
            .collect()
    }

    fn capture_display_baselines() -> Result<Vec<DisplayGammaTable>, MacDisplayError> {
        let display_ids = active_display_ids()?;
        let mut tables = Vec::with_capacity(display_ids.len());

        for display_id in display_ids {
            let mut red = vec![0.0; GAMMA_TABLE_CAPACITY];
            let mut green = vec![0.0; GAMMA_TABLE_CAPACITY];
            let mut blue = vec![0.0; GAMMA_TABLE_CAPACITY];
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
                return Err(MacDisplayError::CgError(result));
            }

            let sample_count = sample_count as usize;
            red.truncate(sample_count);
            green.truncate(sample_count);
            blue.truncate(sample_count);

            if sample_count == 0 {
                return Err(MacDisplayError::EmptyTransferTable(display_id));
            }

            tables.push(DisplayGammaTable {
                display_id,
                red,
                green,
                blue,
            });
        }

        Ok(tables)
    }

    fn active_display_ids() -> Result<Vec<CGDirectDisplayID>, MacDisplayError> {
        let mut display_ids = vec![0; MAX_ACTIVE_DISPLAYS];
        let mut display_count: CGDisplayCount = 0;
        let result = unsafe {
            CGGetActiveDisplayList(
                MAX_ACTIVE_DISPLAYS as CGDisplayCount,
                display_ids.as_mut_ptr(),
                &mut display_count,
            )
        };

        if result != CG_ERROR_SUCCESS {
            return Err(MacDisplayError::CgError(result));
        }

        display_ids.truncate(display_count as usize);

        if display_ids.is_empty() {
            return Err(MacDisplayError::NoDisplays);
        }

        Ok(display_ids)
    }

    #[derive(Debug, thiserror::Error)]
    enum MacDisplayError {
        #[error("core graphics error {0}")]
        CgError(CGError),
        #[error("display state lock was poisoned")]
        StatePoisoned,
        #[error("failed to capture baseline tables")]
        MissingBaselines,
        #[error("no active displays were found")]
        NoDisplays,
        #[error("display {0} returned an empty transfer table")]
        EmptyTransferTable(CGDirectDisplayID),
    }

    #[derive(Debug, Clone, Copy)]
    struct MacCueStyleModifiers {
        warmth: f32,
        grayscale: f32,
        saturation: f32,
        brightness_warmth: f32,
        brightness_grayscale: f32,
        min_brightness: f32,
        red_boost: f32,
        green_boost: f32,
        blue_reduction: f32,
        gamma_grayscale: f32,
        gamma_chroma: f32,
    }

    impl CueStyle {
        fn modifiers(self) -> MacCueStyleModifiers {
            match self {
                CueStyle::Dim => MacCueStyleModifiers {
                    warmth: 0.18,
                    grayscale: 1.0,
                    saturation: 0.74,
                    brightness_warmth: 0.025,
                    brightness_grayscale: 0.16,
                    min_brightness: 0.58,
                    red_boost: 0.05,
                    green_boost: 0.0,
                    blue_reduction: 0.12,
                    gamma_grayscale: 1.1,
                    gamma_chroma: 0.52,
                },
                CueStyle::Full => MacCueStyleModifiers {
                    warmth: 1.18,
                    grayscale: 1.0,
                    saturation: 0.88,
                    brightness_warmth: 0.075,
                    brightness_grayscale: 0.11,
                    min_brightness: 0.66,
                    red_boost: 0.19,
                    green_boost: 0.03,
                    blue_reduction: 0.28,
                    gamma_grayscale: 1.05,
                    gamma_chroma: 0.42,
                },
                CueStyle::Warm => MacCueStyleModifiers {
                    warmth: 0.9,
                    grayscale: 0.55,
                    saturation: 0.97,
                    brightness_warmth: 0.055,
                    brightness_grayscale: 0.065,
                    min_brightness: 0.74,
                    red_boost: 0.15,
                    green_boost: 0.02,
                    blue_reduction: 0.2,
                    gamma_grayscale: 0.72,
                    gamma_chroma: 0.24,
                },
            }
        }
    }

    pub use MacDisplayEffectApplier as PublicMacDisplayEffectApplier;
}

#[cfg(target_os = "macos")]
pub use macos::PublicMacDisplayEffectApplier as MacDisplayEffectApplier;

pub fn apply_preview(
    applier: State<'_, ManagedDisplayEffectApplier>,
    snapshot: &EffectSnapshot,
    cue_style: CueStyle,
) -> ApplyResult {
    applier.apply(snapshot, cue_style)
}
