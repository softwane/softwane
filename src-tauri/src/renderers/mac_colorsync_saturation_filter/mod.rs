//! TODO: vcgt tags do not work! try others.
//! Also does not work on main display of Eason's mac

mod profile_ops;
mod xyz_tag;

use std::{collections::HashMap, path::PathBuf};

use tauri::{AppHandle, Manager};
use tokio::sync::mpsc::Sender;
use objc2_core_foundation::{CFURL, CFUUID, CFRetained};
use objc2_color_sync::{ColorSyncProfile, ColorSyncMutableProfile, CGDisplayCreateUUIDFromDisplayID};
use tauri_plugin_store::StoreExt;

use crate::{channels::ChannelValue, engine::EngineEvent, utils::Update};
use super::{utils, events::RendererEvent};
use profile_ops::ProfileInfo;

pub(super) const PROFILE_STORE_KEY: &str = "profile_baseline_path";

const SATURATION_BUCKET: f64 = 0.004; // ~ 1/256

#[derive(Debug)]
pub(super) struct MacColorSyncSaturationFilter {
    name: &'static str,
    switch_on: bool,
    profiles: HashMap<CFRetained<CFUUID>, ProfileInfo>,
    last_s: f64,
}

impl MacColorSyncSaturationFilter {
    pub(super) fn new() -> Self {
        Self {
            name: "MacOS-ColorSync-Saturation-Filter",
            switch_on: false,
            profiles: HashMap::new(),
            last_s: 1.0,
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

        if !saturation.is_changed() {
            let _ = tx.try_send(EngineEvent::Renderer(
                RendererEvent::RenderUnappliedDueToUnchanged {
                    renderer_name: self.name,
                },
            ));
            return;
        }

        let s = match saturation.get_value() {
            ChannelValue::Saturation(s) => s.clamp(0.2, 1.0),
            _ => panic!("Invalid ChannelValue when render. Expect Saturation, but get: {saturation:?}."),
        };

        if (s - self.last_s).abs() < SATURATION_BUCKET {
            let _ = tx.try_send(EngineEvent::Renderer(
                RendererEvent::RenderUnappliedDueToUnchanged {
                    renderer_name: self.name,
                }
            ));
            return;
        }

        self.last_s = s;

        let s_mat = utils::saturation_to_ct_matrix3(s);
        let s_inv = match s_mat.try_inverse() {
            Some(m) => m,
            None => {
                let _ = tx.try_send(EngineEvent::Renderer(RendererEvent::RenderFailed {
                    renderer_name: self.name,
                    error: "saturation matrix not invertible".into(),
                }));
                return;
            }
        };

        for (uuid, info) in &self.profiles {
            let result_mat = info.baseline_mat * s_inv;

            unsafe {
                profile_ops::set_main_colorants(info.mut_profile.as_ref().expect("mut_profile is taken before `prepare_send` (before shutting down)"), &result_mat);
                if let Err(e) = profile_ops::verify_and_write_profile(
                    info.mut_profile.as_ref().expect("mut_profile is taken before `prepare_send` (before shutting down)"),
                    &info.mut_profile_path,
                ) {
                    let _ = tx.try_send(EngineEvent::Renderer(RendererEvent::RenderFailed {
                        renderer_name: self.name,
                        error: format!("verify/write: {e}"),
                    }));
                    continue;
                }
                if !profile_ops::apply_profile_to_display(uuid, &info.mut_profile_url) {
                    let _ = tx.try_send(EngineEvent::Renderer(RendererEvent::RenderFailed {
                        renderer_name: self.name,
                        error: "apply failed".into(),
                    }));
                    continue;
                }
            }

            let _ = tx.try_send(EngineEvent::Renderer(
                RendererEvent::RenderSuccessful {
                    renderer_name: self.name,
                },
            ));
        }
    }

    pub(super) fn startup(&mut self, app: &AppHandle, tx: Sender<EngineEvent>) {
        let cache_dir = app
            .path()
            .app_cache_dir()
            .expect("app_cache_dir")
            .join("ColorSyncProfiles");
        if let Err(e) = std::fs::create_dir_all(&cache_dir) {
            let _ = tx.try_send(EngineEvent::Renderer(RendererEvent::StartupFailed {
                renderer_name: self.name,
                error: format!("create cache dir: {e}"),
            }));
            return;
        }

        let display_ids = match utils::active_display_ids() {
            Ok(ids) => ids,
            Err(e) => {
                let _ = tx.try_send(EngineEvent::Renderer(RendererEvent::StartupFailed {
                    renderer_name: self.name,
                    error: e,
                }));
                return;
            }
        };

        for display_id in display_ids {
            let result = unsafe {
                self.init_profile_for_display(display_id, app, &cache_dir)
            };
            match result {
                Ok((uuid, info)) => {
                    self.profiles.insert(uuid, info);
                }
                Err(e) => {
                    let _ = tx.try_send(EngineEvent::Renderer(
                        RendererEvent::StartupFailed {
                            renderer_name: self.name,
                            error: format!("display {display_id}: {e}"),
                        },
                    ));
                }
            }
        }

        if self.profiles.is_empty() {
            let _ = tx.try_send(EngineEvent::Renderer(RendererEvent::StartupFailed {
                renderer_name: self.name,
                error: "no display profiles".into(),
            }));
            return;
        }

        self.switch_on = true;
        let _ = tx.try_send(EngineEvent::Renderer(
            RendererEvent::StartupSuccessful {
                renderer_name: self.name,
            },
        ));
    }

    pub(super) fn shutdown(&mut self, _app: &AppHandle, tx: Sender<EngineEvent>) {
        if !self.switch_on {
            return;
        }

        for (uuid, info) in self.profiles.drain() {
            unsafe {
                if !profile_ops::load_profile_from_path(&info.baseline_path)
                    .map(|p| {
                        let url = p.url(std::ptr::null_mut());
                        profile_ops::apply_profile_to_display(&uuid, &url)
                    })
                    .unwrap_or(false) {
                        profile_ops::reset_display_to_factory(&uuid);
                    };
                let _ = std::fs::remove_file(&info.mut_profile_path);
            }
        }

        self.switch_on = false;
        let _ = tx.try_send(EngineEvent::Renderer(
            RendererEvent::ShutdownCompleted {
                renderer_name: self.name,
            },
        ));
    }

    pub(super) fn prepare_send(&mut self) {
        for info in self.profiles.values_mut() {
            let _ = info.mut_profile.take();
        }
    }

    // ── private helpers ──────────────────────────────────────────────

    unsafe fn init_profile_for_display(
        &self,
        display_id: u32,
        app: &AppHandle,
        cache_dir: &std::path::Path,
    ) -> Result<(CFRetained<CFUUID>, ProfileInfo), String> {
        let uuid = CGDisplayCreateUUIDFromDisplayID(display_id);

        let profile = ColorSyncProfile::with_display_id(display_id)
            .ok_or("ColorSyncProfileCreateWithDisplayID returned null")?;

        let url = profile.url(std::ptr::null_mut());
        let path = url.to_file_path().ok_or("no file path for profile URL")?;

        // Determine the baseline profile path: if the display is currently
        // using one of our profiles, recover the factory baseline.
        let (baseline_path, baseline_profile) = (|| -> Result<(PathBuf, CFRetained<ColorSyncProfile>), String> {
            if !profile_ops::is_our_profile(&path, cache_dir) {
                return Ok((path, profile));
            }

            let store = app.store("profile_baseline_path.json").map_err(|e| format!("store: {e}"))?;
            let uuid_str = CFUUID::new_string(None, Some(&uuid))
                .ok_or("CFUUID to string")?
                .to_string();
            let stored_path = store.get(PROFILE_STORE_KEY)
                .and_then(|v| v.get(&uuid_str).cloned())
                .and_then(|v| v.as_str().map(std::path::PathBuf::from));

            let set_to_factory = || {
                profile_ops::reset_display_to_factory(&uuid);
                let factory = ColorSyncProfile::with_display_id(display_id)
                    .ok_or("factory profile")?;
                let factory_url = factory.url(std::ptr::null_mut());
                let path = factory_url.to_file_path().ok_or("factory path")?;
                Ok((path, factory))
            };

            let bp = match stored_path {
                Some(ref bp) => bp,
                None => return set_to_factory(),
            };            
            if !bp.exists() {
                return set_to_factory();
            }
            let bp_url = match CFURL::from_file_path(bp) {
                Some(bp_url) => bp_url,
                None => return set_to_factory(),
            };

            let baseline_profile = ColorSyncProfile::with_url(
                &bp_url,
                std::ptr::null_mut(),
            )
            .ok_or("load baseline profile failed")?;
            profile_ops::apply_profile_to_display(&uuid, &bp_url);
            Ok((bp.clone(), baseline_profile))
        })()?;

        // Persist the baseline path to the store
        {
            let store = app.store("profile_baseline_path.json").map_err(|e| format!("store: {e}"))?;
            let uuid_str = CFUUID::new_string(None, Some(&uuid))
                .ok_or("CFUUID to string")?
                .to_string();
            let mut map: serde_json::Map<String, serde_json::Value> = store.get(PROFILE_STORE_KEY)
                .and_then(|v| v.as_object().cloned())
                .unwrap_or_default();
            map.insert(
                uuid_str,
                serde_json::Value::String(baseline_path.to_string_lossy().to_string()),
            );
            store.set(PROFILE_STORE_KEY, serde_json::Value::Object(map));
        }

        // Create mutable copy and populate ProfileInfo
        let mut_profile = ColorSyncMutableProfile::new_copy(&baseline_profile)
            .ok_or("ColorSyncMutableProfile copy")?;
        let baseline_mat = profile_ops::extract_baseline_mat(&baseline_profile)
            .ok_or("extract baseline matrix")?;
        let baseline_md5 = baseline_profile.md_5();

        let uuid_str = CFUUID::new_string(None, Some(&uuid))
            .ok_or("CFUUID to string")?
            .to_string();
        let mut_profile_path = cache_dir.join(format!("{uuid_str}.icc"));
        let mut_profile_url = CFURL::from_file_path(&mut_profile_path)
            .ok_or("mut_profile_path to URL")?;

        Ok((uuid, ProfileInfo {
            baseline_path,
            mut_profile: Some(mut_profile),
            baseline_mat,
            baseline_md5,
            mut_profile_path,
            mut_profile_url,
        }))
    }
}
