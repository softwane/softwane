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

#[derive(Debug)]
pub(super) struct MacColorSyncSaturationFilter {
    name: &'static str,
    switch_on: bool,
    profiles: HashMap<CFRetained<CFUUID>, ProfileInfo>,
}

impl MacColorSyncSaturationFilter {
    pub(super) fn new() -> Self {
        Self {
            name: "MacOS-ColorSync-Saturation-Filter",
            switch_on: false,
            profiles: HashMap::new(),
        }
    }

    pub(super) fn render(
        &mut self,
        color_temperature: Update<ChannelValue>,
        saturation: Update<ChannelValue>,
        brightness: Update<ChannelValue>,
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

        let need_colorants = saturation.is_changed();
        let need_vcgt = color_temperature.is_changed() || brightness.is_changed();
        if !need_colorants && !need_vcgt {
            let _ = tx.try_send(EngineEvent::Renderer(
                RendererEvent::RenderUnappliedDueToUnchanged {
                    renderer_name: self.name,
                },
            ));
            return;
        }

        // TODO: bucket-based early-exit for unchanged saturation values
        // after Update de-duplication (currently always recomputes on float changes).

        // 预计算 colorant 矩阵
        let s_inv = if need_colorants {
            let s = match saturation.get_value() {
                ChannelValue::Saturation(s) => s.clamp(0.2, 1.0),
                _ => panic!("Invalid ChannelValue when render. Expect Saturation, but get: {saturation:?}."),
            };
            let s_mat = utils::saturation_to_ct_matrix3(s);
            match s_mat.try_inverse() {
                Some(m) => Some(m),
                None => {
                    let _ = tx.try_send(EngineEvent::Renderer(RendererEvent::RenderFailed {
                        renderer_name: self.name,
                        error: "saturation matrix not invertible".into(),
                    }));
                    return;
                }
            }
        } else {
            None
        };

        // 预计算 vcgt 通道系数
        let vcgt_coefs = if need_vcgt {
            let ct_kelvin = match color_temperature.get_value() {
                ChannelValue::ColorTempKelvin(k) => *k,
                _ => panic!("Invalid ChannelValue when calculate_matrix. Expect Color Temperature, but get: {color_temperature:?}."),
            };
            let br = match brightness.get_value() {
                ChannelValue::Brightness(b) => b.clamp(0.0, 1.0),
                _ => panic!("Invalid ChannelValue when calculate_matrix. Expect Brightness, but get: {brightness:?}."),
            };
            let (r, g, b_val) = utils::color_temperature_to_rgb(ct_kelvin);
            Some((r * br, g * br, b_val * br))
        } else {
            None
        };

        for (uuid, info) in &self.profiles {
            let mut_profile = match info.mut_profile.as_ref() {
                Some(mp) => mp,
                None => {
                    tracing::error!("mut_profile already taken for {}", info.baseline_path.display());
                    continue;
                }
            };

            unsafe {
                // 饱和度 → colorants
                if let Some(ref s_inv) = s_inv {
                    let result_mat = info.baseline_mat * s_inv;
                    tracing::debug!("set_colorants: {}, matrix={result_mat:.2}", info.baseline_path.display());
                    profile_ops::set_main_colorants(mut_profile, &result_mat);
                }

                // 色温+亮度 → vcgt
                if let (Some((r, g, b_val)), Some(ref baseline)) = (vcgt_coefs, &info.vcgt_baseline) {
                    let coef = nalgebra::Matrix3::from_diagonal(
                        &nalgebra::Vector3::new(r, g, b_val),
                    );
                    let modified = coef * baseline;
                    let vcgt_data = profile_ops::encode_vcgt_table(&modified);
                    let vcgt_sig = objc2_core_foundation::CFString::from_str("vcgt");
                    mut_profile.set_tag(&vcgt_sig, &vcgt_data);
                }

                // 验证 + 写出 + 应用
                if let Err(e) = profile_ops::verify_and_write_profile(mut_profile, &info.mut_profile_path) {
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
                    tracing::warn!("initialize display {display_id} fail: {e}");
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

        // Read vcgt baseline
        let (vcgt_baseline, _vcgt_sample_count) = profile_ops::read_vcgt_baseline(&baseline_profile);

        let uuid_str = CFUUID::new_string(None, Some(&uuid))
            .ok_or("CFUUID to string")?
            .to_string();
        let mut_profile_path = cache_dir.join(format!("{uuid_str}.icc"));
        let mut_profile_url = CFURL::from_file_path(&mut_profile_path)
            .ok_or("mut_profile_path to URL")?;

        tracing::info!("display {display_id} (uuid: {uuid_str}) initialized, has_vcgt={}", vcgt_baseline.is_some());
        Ok((uuid, ProfileInfo {
            baseline_path,
            mut_profile: Some(mut_profile),
            baseline_mat,
            baseline_md5,
            mut_profile_path,
            mut_profile_url,
            vcgt_baseline,
        }))
    }
}
