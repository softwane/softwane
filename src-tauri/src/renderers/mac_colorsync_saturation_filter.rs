//! macOS saturation renderer backed by ColorSync display profiles.
//!
//! This avoids screen recording and overlay windows. It generates a temporary
//! RGB display ICC profile from the current display profile by expanding the
//! primary matrix through the inverse of a Rec.709-luma saturation matrix.
//! ColorSync applies the inverse display transform, so this produces perceived
//! desaturation. Shutdown resets the display to its factory/default profile.

use std::{
    ffi::{c_char, c_double, CString},
    fs,
    path::PathBuf,
};

use tauri::AppHandle;
use tokio::sync::mpsc::Sender;

use crate::{
    channels::ChannelValue,
    engine::EngineEvent,
    utils::Update,
};
use super::events::RendererEvent;

#[derive(Debug)]
pub(super) struct MacColorSyncSaturationFilter {
    name: &'static str,
    switch_on: bool,
    last_bucket: Option<u32>,
    profile_dir: PathBuf,
    baseline_profile_path: PathBuf,
}

impl MacColorSyncSaturationFilter {
    pub(super) fn new() -> Self {
        let profile_dir = std::env::temp_dir().join("softwane-colorsync-profiles");
        let baseline_profile_path = profile_dir.join("baseline.icc");
        Self {
            name: "MacOS-ColorSync-Saturation-Filter",
            switch_on: false,
            last_bucket: None,
            profile_dir,
            baseline_profile_path,
        }
    }

    pub(super) fn render(
        &mut self,
        saturation: Update<ChannelValue>,
        _app: &AppHandle,
        tx: Sender<EngineEvent>,
    ) {
        if !self.switch_on {
            let _ = tx.try_send(EngineEvent::Renderer(
                RendererEvent::RenderUnappliedDueToNotStartupped {
                    renderer_name: self.name,
                },
            ));
            return;
        }

        let amount = match saturation.get_value() {
            ChannelValue::Saturation(s) => s.clamp(0.2, 1.0),
            _ => panic!(
                "Invalid ChannelValue when render. Expect Saturation, but get: {saturation:?}."
            ),
        };
        let bucket = (amount * 100.0).round() as u32;
        if self.last_bucket == Some(bucket) {
            let _ = tx.try_send(EngineEvent::Renderer(
                RendererEvent::RenderUnappliedDueToUnchanged {
                    renderer_name: self.name,
                },
            ));
            return;
        }
        let bucketed_amount = bucket as f64 / 100.0;

        if let Err(err) = fs::create_dir_all(&self.profile_dir) {
            let _ = tx.try_send(EngineEvent::Renderer(RendererEvent::RenderFailed {
                renderer_name: self.name,
                error: format!("failed to create ColorSync profile dir: {err}"),
            }));
            return;
        }

        let path = self
            .profile_dir
            .join(format!("inverse-saturation-{bucket:03}.icc"));
        let c_path = match CString::new(path.to_string_lossy().as_bytes()) {
            Ok(path) => path,
            Err(err) => {
                let _ = tx.try_send(EngineEvent::Renderer(RendererEvent::RenderFailed {
                    renderer_name: self.name,
                    error: format!("invalid ICC profile path: {err}"),
                }));
                return;
            }
        };
        let c_baseline_path = match CString::new(self.baseline_profile_path.to_string_lossy().as_bytes()) {
            Ok(path) => path,
            Err(err) => {
                let _ = tx.try_send(EngineEvent::Renderer(RendererEvent::RenderFailed {
                    renderer_name: self.name,
                    error: format!("invalid ICC baseline profile path: {err}"),
                }));
                return;
            }
        };

        let ok = unsafe {
            softwane_macos_colorsync_set_saturation(
                bucketed_amount,
                c_baseline_path.as_ptr(),
                c_path.as_ptr(),
            )
        };
        if ok {
            self.last_bucket = Some(bucket);
            let _ = tx.try_send(EngineEvent::Renderer(
                RendererEvent::RenderSuccessful {
                    renderer_name: self.name,
                },
            ));
        } else {
            let _ = tx.try_send(EngineEvent::Renderer(RendererEvent::RenderFailed {
                renderer_name: self.name,
                error: "ColorSync saturation profile apply failed".into(),
            }));
        }
    }

    pub(super) fn startup(&mut self, _app: &AppHandle, tx: Sender<EngineEvent>) {
        if let Err(err) = fs::create_dir_all(&self.profile_dir) {
            let _ = tx.try_send(EngineEvent::Renderer(RendererEvent::StartupFailed {
                renderer_name: self.name,
                error: format!("failed to create ColorSync profile dir: {err}"),
            }));
            return;
        }
        let c_baseline_path = match CString::new(self.baseline_profile_path.to_string_lossy().as_bytes()) {
            Ok(path) => path,
            Err(err) => {
                let _ = tx.try_send(EngineEvent::Renderer(RendererEvent::StartupFailed {
                    renderer_name: self.name,
                    error: format!("invalid ICC baseline profile path: {err}"),
                }));
                return;
            }
        };
        let captured = unsafe {
            softwane_macos_colorsync_capture_baseline(c_baseline_path.as_ptr())
        };
        if !captured {
            let _ = tx.try_send(EngineEvent::Renderer(RendererEvent::StartupFailed {
                renderer_name: self.name,
                error: "failed to capture ColorSync baseline profile".into(),
            }));
            return;
        }

        self.switch_on = true;
        self.last_bucket = None;
        let _ = tx.try_send(EngineEvent::Renderer(RendererEvent::StartupSuccessful {
            renderer_name: self.name,
        }));
    }

    pub(super) fn shutdown(&mut self, _app: &AppHandle, tx: Sender<EngineEvent>) {
        if self.switch_on {
            unsafe {
                softwane_macos_colorsync_reset_saturation();
            }
        }
        self.switch_on = false;
        self.last_bucket = None;
        let _ = tx.try_send(EngineEvent::Renderer(
            RendererEvent::ShutdownCompleted {
                renderer_name: self.name,
            },
        ));
    }
}

unsafe extern "C" {
    fn softwane_macos_colorsync_capture_baseline(
        baseline_profile_path: *const c_char,
    ) -> bool;
    fn softwane_macos_colorsync_set_saturation(
        saturation: c_double,
        baseline_profile_path: *const c_char,
        profile_path: *const c_char,
    ) -> bool;
    fn softwane_macos_colorsync_reset_saturation() -> bool;
}
