use std::{path::{Path, PathBuf}, slice};

use objc2_color_sync::{
    ColorSyncProfile, ColorSyncMutableProfile, ColorSyncMD5,
    ColorSyncDeviceSetCustomProfiles,
    kColorSyncDisplayDeviceClass, kColorSyncDeviceDefaultProfileID,
    kColorSyncSigRedColorantTag, kColorSyncSigGreenColorantTag, kColorSyncSigBlueColorantTag,
};
use objc2_core_foundation::{
    CFRetained, CFURL, CFUUID, CFDictionary, kCFNull,
};

use super::xyz_tag;
use super::super::utils::ColorTransformMatrix;

// ── ProfileInfo ───────────────────────────────────────────────────────

pub(super) struct ProfileInfo {
    pub baseline_path: PathBuf,
    pub mut_profile: CFRetained<ColorSyncMutableProfile>,
    pub baseline_mat: ColorTransformMatrix,
    pub baseline_md5: ColorSyncMD5,
    pub mut_profile_path: PathBuf,
    pub mut_profile_url: CFRetained<CFURL>,
}

impl std::fmt::Debug for ProfileInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProfileInfo")
            .field("baseline_path", &self.baseline_path)
            .field("baseline_mat", &self.baseline_mat)
            .field("baseline_md5", &self.baseline_md5)
            .field("mut_profile_path", &self.mut_profile_path)
            .finish_non_exhaustive()
    }
}

// ── 提取基线矩阵 ─────────────────────────────────────────────────────

pub(super) unsafe fn extract_baseline_mat(
    profile: &ColorSyncProfile,
) -> Option<ColorTransformMatrix> {
    let r_tag = profile.tag(kColorSyncSigRedColorantTag)?;
    let g_tag = profile.tag(kColorSyncSigGreenColorantTag)?;
    let b_tag = profile.tag(kColorSyncSigBlueColorantTag)?;
    let r = xyz_tag::decode_xyz_tag(&r_tag)?;
    let g = xyz_tag::decode_xyz_tag(&g_tag)?;
    let b = xyz_tag::decode_xyz_tag(&b_tag)?;
    Some(ColorTransformMatrix::new(
        r[0], g[0], b[0],
        r[1], g[1], b[1],
        r[2], g[2], b[2],
    ))
}

// ── 写回变换后的主色矩阵 ─────────────────────────────────────────────

pub(super) unsafe fn set_main_colorants(
    mut_profile: &ColorSyncMutableProfile,
    mat: &ColorTransformMatrix,
) {
    let write_tag = |sig: &objc2_core_foundation::CFString, idx: usize| {
        let col = mat.column(idx);
        let data = xyz_tag::encode_xyz_tag([col.x, col.y, col.z]);
        mut_profile.set_tag(sig, &data);
    };
    write_tag(kColorSyncSigRedColorantTag, 0);
    write_tag(kColorSyncSigGreenColorantTag, 1);
    write_tag(kColorSyncSigBlueColorantTag, 2);
}

// ── 验证 + 导出 + 写入磁盘 ───────────────────────────────────────────

pub(super) unsafe fn verify_and_write_profile(
    mut_profile: &ColorSyncMutableProfile,
    path: &Path,
) -> Result<(), String> {
    if !mut_profile.verify(std::ptr::null_mut(), std::ptr::null_mut()) {
        return Err("profile verify failed".into());
    }
    let data = mut_profile.data(std::ptr::null_mut());
    let len = data.len();
    let bytes_ptr = data.byte_ptr();
    if bytes_ptr.is_null() {
        return Err(format!("null pointer! It should point to the data of the given mut_profile."));
    }
    let bytes: &[u8] = slice::from_raw_parts(bytes_ptr, len);

    std::fs::write(path, bytes)
        .map_err(|e| format!("write profile: {e}"))
}

// ── 应用 profile 到显示器 ────────────────────────────────────────────

pub(super) unsafe fn apply_profile_to_display(
    uuid: &CFUUID,
    url: &CFURL,
) -> bool {
    let dict = CFDictionary::from_slices(
        &[kColorSyncDeviceDefaultProfileID],
        &[url],
    );
    ColorSyncDeviceSetCustomProfiles(
        kColorSyncDisplayDeviceClass,
        uuid,
        dict.as_ref(),
    )
}

// ── 恢复工厂默认 ─────────────────────────────────────────────────────

pub(super) unsafe fn reset_display_to_factory(uuid: &CFUUID) -> bool {
    let Some(null) = kCFNull else {
        return false;
    };
    let dict = CFDictionary::from_slices(
        &[kColorSyncDeviceDefaultProfileID],
        &[null],
    );
    ColorSyncDeviceSetCustomProfiles(
        kColorSyncDisplayDeviceClass,
        uuid,
        dict.as_ref(),
    )
}

// ── path → profile ────────────────────────────────────────────────────

pub(super) unsafe fn load_profile_from_path(
    path: &Path,
) -> Option<CFRetained<ColorSyncProfile>> {
    let url = CFURL::from_file_path(path)?;
    ColorSyncProfile::with_url(&url, std::ptr::null_mut())
}

// ── 判断 path 是否在本应用缓存目录下 ─────────────────────────────────

pub(super) fn is_our_profile(path: &Path, cache_dir: &Path) -> bool {
    path.starts_with(cache_dir)
}
