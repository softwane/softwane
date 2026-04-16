use serde::Serialize;
use tauri::State;

use crate::compositor::CompositeFrame;

#[derive(Debug, Clone, Serialize)]
pub struct ApplyResult {
    pub applied: bool,
    pub backend: &'static str,
    pub error: Option<String>,
    pub recovery_attempted: bool,
    pub recovery_succeeded: bool,
    pub recovery_error: Option<String>,
}

pub trait PlatformAdapter: Send + Sync {
    fn backend_name(&self) -> &'static str;
    fn try_apply(&self, frame: &CompositeFrame) -> Result<bool, String>;

    fn apply(&self, frame: &CompositeFrame) -> ApplyResult {
        match self.try_apply(frame) {
            Ok(applied) => ApplyResult {
                applied,
                backend: self.backend_name(),
                error: None,
                recovery_attempted: false,
                recovery_succeeded: false,
                recovery_error: None,
            },
            Err(error) => {
                let recovery_attempted = !frame.is_neutral();
                let (recovery_succeeded, recovery_error) = if recovery_attempted {
                    match self.try_apply(&CompositeFrame::neutral()) {
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

pub type ManagedPlatformAdapter = PlatformDisplayAdapter;

#[derive(Default)]
pub struct MockPlatformAdapter;

impl PlatformAdapter for MockPlatformAdapter {
    fn backend_name(&self) -> &'static str {
        "mock"
    }

    fn try_apply(&self, _frame: &CompositeFrame) -> Result<bool, String> {
        Ok(false)
    }
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
pub type PlatformDisplayAdapter = MockPlatformAdapter;

#[cfg(target_os = "macos")]
pub type PlatformDisplayAdapter = MacPlatformAdapter;

#[cfg(target_os = "windows")]
pub type PlatformDisplayAdapter = WindowsPlatformAdapter;

// ---------------------------------------------------------------------------
// macOS: Core Graphics gamma tables
// ---------------------------------------------------------------------------
#[cfg(target_os = "macos")]
mod macos {
    use std::sync::Mutex;

    use super::PlatformAdapter;
    use crate::compositor::CompositeFrame;

    type CGDirectDisplayID = u32;
    type CGDisplayCount = u32;
    type CGTableCount = u32;
    type CGGammaValue = f32;
    type CGError = i32;

    const CG_ERROR_SUCCESS: CGError = 0;
    const MAX_ACTIVE_DISPLAYS: usize = 32;
    const GAMMA_TABLE_CAPACITY: usize = 256;
    const NEUTRAL_WARMTH_KELVIN: f32 = 6500.0;
    const MIN_WARMTH_KELVIN: f32 = 2000.0;

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
    pub struct MacPlatformAdapter {
        state: Mutex<MacDisplayState>,
    }

    impl PlatformAdapter for MacPlatformAdapter {
        fn backend_name(&self) -> &'static str {
            "macos-core-graphics"
        }

        fn try_apply(&self, frame: &CompositeFrame) -> Result<bool, String> {
            self.apply_frame(frame)
                .map_err(|error| error.to_string())
        }
    }

    impl Drop for MacPlatformAdapter {
        fn drop(&mut self) {
            unsafe {
                CGDisplayRestoreColorSyncSettings();
            }
        }
    }

    impl MacPlatformAdapter {
        fn apply_frame(&self, frame: &CompositeFrame) -> Result<bool, MacDisplayError> {
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

            if frame.is_neutral() {
                unsafe {
                    CGDisplayRestoreColorSyncSettings();
                }
                return Ok(true);
            }

            let warmth = normalize_warmth(frame.warmth_kelvin);
            let chroma = frame.saturation.clamp(0.0, 1.0);
            let brightness_base = frame.brightness.clamp(0.0, 1.0);
            let brightness = (brightness_base * (1.0 - warmth * 0.055)).clamp(0.5, 1.0);
            let red_scale = (brightness * (1.0 + warmth * 0.15)).clamp(0.0, 1.25);
            let green_scale = (brightness * (1.0 + warmth * 0.02)).clamp(0.0, 1.15);
            let blue_scale = (brightness * (1.0 - warmth * 0.20)).clamp(0.0, 1.0);
            let gamma = (1.0 + (1.0 - chroma) * 0.35).clamp(1.0, 2.2);

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

    pub use MacPlatformAdapter as PublicMacPlatformAdapter;
}

#[cfg(target_os = "macos")]
pub use macos::PublicMacPlatformAdapter as MacPlatformAdapter;

// ---------------------------------------------------------------------------
// Windows: Magnification API fullscreen color matrix
// ---------------------------------------------------------------------------
#[cfg(target_os = "windows")]
mod windows {
    use std::sync::Mutex;

    use super::PlatformAdapter;
    use crate::compositor::CompositeFrame;

    const NEUTRAL_WARMTH_KELVIN: f32 = 6500.0;
    const MIN_WARMTH_KELVIN: f32 = 2000.0;

    #[derive(Default)]
    struct WindowsState {
        initialized: bool,
        last_matrix: Option<[[f32; 5]; 5]>,
    }

    #[derive(Default)]
    pub struct WindowsPlatformAdapter {
        state: Mutex<WindowsState>,
    }

    impl PlatformAdapter for WindowsPlatformAdapter {
        fn backend_name(&self) -> &'static str {
            "windows-magnification"
        }

        fn try_apply(&self, frame: &CompositeFrame) -> Result<bool, String> {
            self.apply_frame(frame)
                .map_err(|error| error.to_string())
        }
    }

    impl Drop for WindowsPlatformAdapter {
        fn drop(&mut self) {
            let _ = self.restore_identity();
            let _ = self.uninitialize();
        }
    }

    impl WindowsPlatformAdapter {
        fn apply_frame(&self, frame: &CompositeFrame) -> Result<bool, WindowsDisplayError> {
            if frame.is_neutral() {
                self.restore_identity()?;
                return Ok(true);
            }

            self.ensure_initialized()?;

            let matrix = frame_to_matrix(frame);
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

    // GDI+ convention: color vector (r,g,b,a,1) * M
    fn saturation_matrix(saturation: f32) -> [[f32; 5]; 5] {
        let s = saturation.clamp(0.0, 1.0);
        let lr = 0.2126;
        let lg = 0.7152;
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
        ((NEUTRAL_WARMTH_KELVIN - warmth_kelvin as f32)
            / (NEUTRAL_WARMTH_KELVIN - MIN_WARMTH_KELVIN))
            .clamp(0.0, 1.0)
    }

    fn frame_to_matrix(frame: &CompositeFrame) -> [[f32; 5]; 5] {
        if frame.is_neutral() {
            return identity_matrix();
        }

        let warmth = normalize_warmth(frame.warmth_kelvin);
        let saturation = frame.saturation.clamp(0.0, 1.0);
        let brightness = frame.brightness.clamp(0.0, 1.0);

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
        use super::*;

        #[test]
        fn identity_is_stable() {
            assert!(matrix_nearly_equal(
                &identity_matrix(),
                &identity_matrix(),
                0.0
            ));
        }

        #[test]
        fn frame_matrix_is_finite() {
            let frame = CompositeFrame {
                saturation: 0.5,
                warmth_kelvin: 4000,
                brightness: 0.8,
            };
            let m = frame_to_matrix(&frame);
            for v in m.iter().flatten() {
                assert!(v.is_finite());
            }
        }

        #[test]
        fn neutral_frame_produces_identity() {
            let m = frame_to_matrix(&CompositeFrame::neutral());
            assert!(matrix_nearly_equal(&m, &identity_matrix(), 0.001));
        }
    }

    pub use WindowsPlatformAdapter as PublicWindowsPlatformAdapter;
}

#[cfg(target_os = "windows")]
pub use windows::PublicWindowsPlatformAdapter as WindowsPlatformAdapter;

pub fn apply_frame(
    adapter: State<'_, ManagedPlatformAdapter>,
    frame: &CompositeFrame,
) -> ApplyResult {
    adapter.apply(frame)
}
