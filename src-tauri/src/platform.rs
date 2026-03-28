use serde::Serialize;
use tauri::State;

use crate::engine::{EffectSnapshot, Phase};

#[derive(Debug, Clone, Serialize)]
pub struct ApplyResult {
    pub applied: bool,
    pub backend: &'static str,
    pub error: Option<String>,
    pub recovery_attempted: bool,
    pub recovery_succeeded: bool,
    pub recovery_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

    pub fn as_id(self) -> &'static str {
        match self {
            Self::Dim => "dim",
            Self::Warm => "warm",
            Self::Full => "full",
        }
    }
}

pub trait DisplayEffectApplier {
    fn backend_name(&self) -> &'static str;
    fn try_apply(&self, snapshot: &EffectSnapshot, cue_style: CueStyle) -> Result<bool, String>;

    fn apply(&self, snapshot: &EffectSnapshot, cue_style: CueStyle) -> ApplyResult {
        match self.try_apply(snapshot, cue_style) {
            Ok(applied) => ApplyResult {
                applied,
                backend: self.backend_name(),
                error: None,
                recovery_attempted: false,
                recovery_succeeded: false,
                recovery_error: None,
            },
            Err(error) => {
                let recovery_attempted = !is_neutral_snapshot(snapshot);
                let (recovery_succeeded, recovery_error) = if recovery_attempted {
                    match self.try_apply(&neutral_snapshot(), cue_style) {
                        Ok(applied) => (applied, None),
                        Err(recovery_error) => (false, Some(recovery_error)),
                    }
                } else {
                    (false, None)
                };

                ApplyResult {
                    applied: false,
                    backend: self.backend_name(),
                    error: Some(error),
                    recovery_attempted,
                    recovery_succeeded,
                    recovery_error,
                }
            }
        }
    }
}

pub type ManagedDisplayEffectApplier = PlatformDisplayEffectApplier;

#[cfg_attr(target_os = "macos", allow(dead_code))]
#[derive(Default)]
pub struct MockDisplayEffectApplier;

impl DisplayEffectApplier for MockDisplayEffectApplier {
    fn backend_name(&self) -> &'static str {
        "mock"
    }

    fn try_apply(&self, _snapshot: &EffectSnapshot, _cue_style: CueStyle) -> Result<bool, String> {
        Ok(false)
    }
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
pub type PlatformDisplayEffectApplier = MockDisplayEffectApplier;

#[cfg(target_os = "macos")]
pub type PlatformDisplayEffectApplier = MacDisplayEffectApplier;

#[cfg(target_os = "windows")]
pub type PlatformDisplayEffectApplier = WindowsDisplayEffectApplier;

#[cfg(target_os = "macos")]
mod macos {
    use std::sync::Mutex;

    use super::{is_neutral_snapshot, CueStyle, DisplayEffectApplier};
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
        fn backend_name(&self) -> &'static str {
            "macos-core-graphics"
        }

        fn try_apply(
            &self,
            snapshot: &EffectSnapshot,
            cue_style: CueStyle,
        ) -> Result<bool, String> {
            self.apply_snapshot(snapshot, cue_style)
                .map_err(|error| error.to_string())
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
            let saturation =
                (snapshot.saturation.clamp(0.0, 1.0) * modifiers.saturation).clamp(0.0, 1.0);
            let chroma = saturation;
            let brightness =
                (1.0 - warmth * modifiers.brightness_warmth).clamp(modifiers.min_brightness, 1.0);
            let red_scale = (brightness * (1.0 + warmth * modifiers.red_boost)).clamp(0.0, 1.25);
            let green_scale =
                (brightness * (1.0 + warmth * modifiers.green_boost)).clamp(0.0, 1.15);
            let blue_scale =
                (brightness * (1.0 - warmth * modifiers.blue_reduction)).clamp(0.0, 1.0);
            let gamma = (1.0 + (1.0 - chroma) * modifiers.gamma_chroma).clamp(1.0, 2.2);

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
        saturation: f32,
        brightness_warmth: f32,
        min_brightness: f32,
        red_boost: f32,
        green_boost: f32,
        blue_reduction: f32,
        gamma_chroma: f32,
    }

    impl CueStyle {
        fn modifiers(self) -> MacCueStyleModifiers {
            match self {
                CueStyle::Dim => MacCueStyleModifiers {
                    warmth: 0.18,
                    saturation: 0.74,
                    brightness_warmth: 0.025,
                    min_brightness: 0.58,
                    red_boost: 0.05,
                    green_boost: 0.0,
                    blue_reduction: 0.12,
                    gamma_chroma: 0.52,
                },
                CueStyle::Full => MacCueStyleModifiers {
                    warmth: 1.18,
                    saturation: 0.88,
                    brightness_warmth: 0.075,
                    min_brightness: 0.66,
                    red_boost: 0.19,
                    green_boost: 0.03,
                    blue_reduction: 0.28,
                    gamma_chroma: 0.42,
                },
                CueStyle::Warm => MacCueStyleModifiers {
                    warmth: 0.9,
                    saturation: 0.97,
                    brightness_warmth: 0.055,
                    min_brightness: 0.74,
                    red_boost: 0.15,
                    green_boost: 0.02,
                    blue_reduction: 0.2,
                    gamma_chroma: 0.24,
                },
            }
        }
    }

    pub use MacDisplayEffectApplier as PublicMacDisplayEffectApplier;
}

#[cfg(target_os = "macos")]
pub use macos::PublicMacDisplayEffectApplier as MacDisplayEffectApplier;

#[cfg(target_os = "windows")]
mod windows {
    use std::sync::Mutex;

    use super::{CueStyle, DisplayEffectApplier};
    use crate::engine::EffectSnapshot;

    #[derive(Default)]
    struct WindowsState {
        initialized: bool,
        last_matrix: Option<[[f32; 5]; 5]>,
    }

    #[derive(Default)]
    pub struct WindowsDisplayEffectApplier {
        state: Mutex<WindowsState>,
    }

    impl DisplayEffectApplier for WindowsDisplayEffectApplier {
        fn backend_name(&self) -> &'static str {
            "windows-magnification"
        }

        fn try_apply(
            &self,
            snapshot: &EffectSnapshot,
            cue_style: CueStyle,
        ) -> Result<bool, String> {
            self.apply_snapshot(snapshot, cue_style)
                .map_err(|error| error.to_string())
        }
    }

    impl Drop for WindowsDisplayEffectApplier {
        fn drop(&mut self) {
            let _ = self.restore_identity();
            let _ = self.uninitialize();
        }
    }

    impl WindowsDisplayEffectApplier {
        fn apply_snapshot(
            &self,
            snapshot: &EffectSnapshot,
            cue_style: CueStyle,
        ) -> Result<bool, WindowsDisplayError> {
            if is_neutral_snapshot(snapshot) {
                self.restore_identity()?;
                return Ok(true);
            }

            self.ensure_initialized()?;

            let matrix = snapshot_to_matrix(snapshot, cue_style);
            self.set_fullscreen_color_effect(matrix)?;
            Ok(true)
        }

        fn ensure_initialized(&self) -> Result<(), WindowsDisplayError> {
            let mut state = self
                .state
                .lock()
                .map_err(|_| WindowsDisplayError::StatePoisoned)?;
            if state.initialized {
                return Ok(());
            }

            unsafe {
                if MagInitialize() == 0 {
                    return Err(WindowsDisplayError::ApiCallFailed("MagInitialize"));
                }
            }

            state.initialized = true;
            Ok(())
        }

        fn uninitialize(&self) -> Result<(), WindowsDisplayError> {
            let mut state = self
                .state
                .lock()
                .map_err(|_| WindowsDisplayError::StatePoisoned)?;
            if !state.initialized {
                return Ok(());
            }

            unsafe {
                if MagUninitialize() == 0 {
                    return Err(WindowsDisplayError::ApiCallFailed("MagUninitialize"));
                }
            }

            state.initialized = false;
            state.last_matrix = None;
            Ok(())
        }

        fn restore_identity(&self) -> Result<(), WindowsDisplayError> {
            self.ensure_initialized()?;
            self.set_fullscreen_color_effect(identity_matrix())
        }

        fn set_fullscreen_color_effect(
            &self,
            matrix: [[f32; 5]; 5],
        ) -> Result<(), WindowsDisplayError> {
            let mut state = self
                .state
                .lock()
                .map_err(|_| WindowsDisplayError::StatePoisoned)?;

            if let Some(prev) = state.last_matrix {
                if matrix_nearly_equal(&prev, &matrix, 0.0005) {
                    return Ok(());
                }
            }

            let effect = MAGCOLOREFFECT { transform: matrix };
            unsafe {
                if MagSetFullscreenColorEffect(&effect as *const MAGCOLOREFFECT) == 0 {
                    return Err(WindowsDisplayError::ApiCallFailed(
                        "MagSetFullscreenColorEffect",
                    ));
                }
            }

            state.last_matrix = Some(matrix);
            Ok(())
        }
    }

    fn identity_matrix() -> [[f32; 5]; 5] {
        [
            [1.0, 0.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 0.0, 1.0],
        ]
    }

    fn matrix_nearly_equal(a: &[[f32; 5]; 5], b: &[[f32; 5]; 5], eps: f32) -> bool {
        a.iter()
            .flatten()
            .zip(b.iter().flatten())
            .all(|(x, y)| (*x - *y).abs() <= eps || ((*x).is_nan() && (*y).is_nan()))
    }

    fn mul(a: [[f32; 5]; 5], b: [[f32; 5]; 5]) -> [[f32; 5]; 5] {
        let mut out = [[0.0f32; 5]; 5];
        for r in 0..5 {
            for c in 0..5 {
                let mut acc = 0.0;
                for k in 0..5 {
                    acc += a[r][k] * b[k][c];
                }
                out[r][c] = acc;
            }
        }
        out
    }

    // Matrices follow the GDI+ convention referenced by MAGCOLOREFFECT:
    // color vector (r,g,b,a,1) is multiplied on the left: v' = v * M.
    fn saturation_matrix(saturation: f32) -> [[f32; 5]; 5] {
        let s = saturation.clamp(0.0, 1.0);
        let lr = 0.2126;
        let lg = 0.7;
        let lb = 0.0722;
        let inv = 1.0 - s;
        [
            [inv * lr + s, inv * lr, inv * lr, 0.0, 0.0],
            [inv * lg, inv * lg + s, inv * lg, 0.0, 0.0],
            [inv * lb, inv * lb, inv * lb + s, 0.0, 0.0],
            [0.0, 0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 0.0, 1.0],
        ]
    }

    fn warmth_matrix(normalized_warmth: f32) -> [[f32; 5]; 5] {
        let w = normalized_warmth.clamp(0.0, 1.0);
        let r = (1.0 + 0.15 * w).clamp(0.0, 1.25);
        let g = (1.0 + 0.02 * w).clamp(0.0, 1.15);
        let b = (1.0 - 0.20 * w).clamp(0.0, 1.0);
        [
            [r, 0.0, 0.0, 0.0, 0.0],
            [0.0, g, 0.0, 0.0, 0.0],
            [0.0, 0.0, b, 0.0, 0.0],
            [0.0, 0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 0.0, 1.0],
        ]
    }

    fn brightness_matrix(brightness: f32) -> [[f32; 5]; 5] {
        let b = brightness.clamp(0.0, 1.0);
        [
            [b, 0.0, 0.0, 0.0, 0.0],
            [0.0, b, 0.0, 0.0, 0.0],
            [0.0, 0.0, b, 0.0, 0.0],
            [0.0, 0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 0.0, 1.0],
        ]
    }

    fn normalize_warmth(warmth_kelvin: u32) -> f32 {
        let neutral = 6500.0;
        let min = 2500.0;
        ((neutral - warmth_kelvin as f32) / (neutral - min)).clamp(0.0, 1.0)
    }

    #[derive(Debug, Clone, Copy)]
    struct WindowsCueStyleModifiers {
        warmth: f32,
        saturation: f32,
        brightness_warmth: f32,
        min_brightness: f32,
    }

    impl CueStyle {
        fn modifiers_windows(self) -> WindowsCueStyleModifiers {
            match self {
                CueStyle::Dim => WindowsCueStyleModifiers {
                    warmth: 0.18,
                    saturation: 0.74,
                    brightness_warmth: 0.14,
                    min_brightness: 0.58,
                },
                CueStyle::Full => WindowsCueStyleModifiers {
                    warmth: 1.18,
                    saturation: 0.88,
                    brightness_warmth: 0.07,
                    min_brightness: 0.66,
                },
                CueStyle::Warm => WindowsCueStyleModifiers {
                    warmth: 0.9,
                    saturation: 0.97,
                    brightness_warmth: 0.06,
                    min_brightness: 0.74,
                },
            }
        }
    }

    fn snapshot_to_matrix(snapshot: &EffectSnapshot, cue_style: CueStyle) -> [[f32; 5]; 5] {
        if is_neutral_snapshot(snapshot) {
            return identity_matrix();
        }

        let m = cue_style.modifiers_windows();
        let warmth = (normalize_warmth(snapshot.warmth_kelvin) * m.warmth).clamp(0.0, 1.0);
        let raw_sat = snapshot.saturation.clamp(0.0, 1.0);
        let sat_modifier = m.saturation + (1.0 - m.saturation) * raw_sat;
        let saturation = (raw_sat * sat_modifier).clamp(0.0, 1.0);
        let brightness = (1.0 - warmth * m.brightness_warmth).clamp(m.min_brightness, 1.0);

        let ms = saturation_matrix(saturation);
        let mw = warmth_matrix(warmth);
        let mb = brightness_matrix(brightness);
        mul(mul(ms, mw), mb)
    }

    #[derive(Debug, thiserror::Error)]
    enum WindowsDisplayError {
        #[error("windows display state lock was poisoned")]
        StatePoisoned,
        #[error("windows api call failed: {0}")]
        ApiCallFailed(&'static str),
    }

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
        use super::{identity_matrix, matrix_nearly_equal, snapshot_to_matrix};
        use crate::engine::{EffectSnapshot, Phase};
        use crate::platform::CueStyle;

        #[test]
        fn identity_is_stable() {
            assert!(matrix_nearly_equal(
                &identity_matrix(),
                &identity_matrix(),
                0.0
            ));
        }

        #[test]
        fn snapshot_matrix_is_finite() {
            let snapshot = EffectSnapshot {
                phase: Phase::Evolution,
                saturation: 0.5,
                warmth_kelvin: 4000,
            };
            let m = snapshot_to_matrix(&snapshot, CueStyle::Warm);
            for v in m.iter().flatten() {
                assert!(v.is_finite());
            }
        }

        #[test]
        fn neutral_like_snapshot_is_close_to_identity() {
            let snapshot = EffectSnapshot {
                phase: Phase::Stable,
                saturation: 1.0,
                warmth_kelvin: 6500,
            };
            let m = snapshot_to_matrix(&snapshot, CueStyle::Warm);
            assert!(matrix_nearly_equal(&m, &identity_matrix(), 0.001));
        }
    }

    pub use WindowsDisplayEffectApplier as PublicWindowsDisplayEffectApplier;
}

#[cfg(target_os = "windows")]
pub use windows::PublicWindowsDisplayEffectApplier as WindowsDisplayEffectApplier;

pub fn apply_preview(
    applier: State<'_, ManagedDisplayEffectApplier>,
    snapshot: &EffectSnapshot,
    cue_style: CueStyle,
) -> ApplyResult {
    applier.apply(snapshot, cue_style)
}

fn neutral_snapshot() -> EffectSnapshot {
    EffectSnapshot {
        phase: Phase::Stable,
        saturation: 1.0,
        warmth_kelvin: 6500,
    }
}

fn is_neutral_snapshot(snapshot: &EffectSnapshot) -> bool {
    snapshot.phase == Phase::Stable
        && (snapshot.saturation - 1.0).abs() <= 0.001
        && snapshot.warmth_kelvin >= 6500
}
