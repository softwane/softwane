use std::{path::{Path, PathBuf}, slice, ptr::NonNull};

use nalgebra::{Const, Dyn, OMatrix};
use objc2_color_sync::{
    ColorSyncProfile, ColorSyncMutableProfile, ColorSyncMD5,
    ColorSyncDeviceSetCustomProfiles,
    kColorSyncDisplayDeviceClass, kColorSyncDeviceDefaultProfileID,
    kColorSyncSigRedColorantTag, kColorSyncSigGreenColorantTag, kColorSyncSigBlueColorantTag,
};
use objc2_core_foundation::{
    CFRetained, CFURL, CFUUID, CFDictionary, kCFNull, CFData,
};

use super::xyz_tag;
use super::super::utils::ColorTransformMatrix;

pub(super) type Matrix3N<T> = OMatrix<T, Const<3>, Dyn>;

// ── ProfileInfo ───────────────────────────────────────────────────────

pub(super) struct ProfileInfo {
    pub baseline_path: PathBuf,
    pub mut_profile: Option<CFRetained<ColorSyncMutableProfile>>,
    pub baseline_mat: ColorTransformMatrix,
    pub baseline_md5: ColorSyncMD5,
    pub mut_profile_path: PathBuf,
    pub mut_profile_url: CFRetained<CFURL>,
    /// vcgt baseline, shape (3, n_samples). Rows: R, G, B. None if no vcgt.
    pub vcgt_baseline: Option<Matrix3N<f64>>,
}

unsafe impl Send for ProfileInfo {}

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

// ── vcgt: 从 profile 读取展开后的表格 → Matrix3N<f64> ─────────────────

/// Read vcgt baseline from a profile using `display_transfer_tables_from_vcgt`.
/// Returns (baseline matrix, sample count).  `None` matrix = no vcgt.
pub(super) unsafe fn read_vcgt_baseline(
    profile: &ColorSyncProfile,
) -> (Option<Matrix3N<f64>>, usize) {
    let mut n_samples: usize = 0;
    let n_ptr = NonNull::new(&mut n_samples).expect("n_samples is null");
    let data = match profile.display_transfer_tables_from_vcgt(n_ptr) {
        Some(d) => d,
        None => return (None, 0),
    };
    let n = n_samples;
    if n == 0 {
        return (None, 0);
    }

    let len = data.len();
    let bytes_ptr = data.byte_ptr();
    if bytes_ptr.is_null() {
        return (None, 0);
    }
    let bytes: &[u8] = slice::from_raw_parts(bytes_ptr, len);

    // data layout: [red: n*u16] [green: n*u16] [blue: n*u16], big-endian
    if bytes.len() < 3 * n * 2 {
        return (None, 0);
    }
    let mut mat = Matrix3N::<f64>::zeros(n);
    for ch in 0..3 {
        for i in 0..n {
            let off = (ch * n + i) * 2;
            let v = u16::from_be_bytes([bytes[off], bytes[off + 1]]) as f64 / 65535.0;
            mat[(ch, i)] = v;
        }
    }
    (Some(mat), n)
}

/// Encode a Matrix3N<f64> back to the vcgt tag binary for `set_tag`.
///
/// TODO: entry_size is not guaranteed to be 2 (though it almost always is).
/// 8-bit displays may use entry_size=1.  Need to handle dynamic entry_size
/// in both decode/encode.
pub(super) unsafe fn encode_vcgt_table(mat: &Matrix3N<f64>) -> CFRetained<CFData> {
    let n = mat.ncols();
    // vcgt type signature (4) + reserved (4) + "tbl " (4) + channel_count (2)
    // + entry_count (2) + entry_size (2) + 3 * n * 2 bytes of table data
    let total = 18 + 3 * n * 2;
    let mut bytes = vec![0u8; total];
    // type signature (as used by Apple, vcgt uses "vcgt" as its type signature)
    bytes[0..4].copy_from_slice(b"vcgt");
    // bytes[4..8] stays 0 (reserved)
    // format type: "tbl "
    bytes[8..12].copy_from_slice(b"tbl ");
    // channel_count = 3
    bytes[12..14].copy_from_slice(&3u16.to_be_bytes());
    // entry_count = n
    bytes[14..16].copy_from_slice(&(n as u16).to_be_bytes());
    // entry_size = 2 (u16 LE → BE)
    bytes[16..18].copy_from_slice(&2u16.to_be_bytes());
    // table data
    for ch in 0..3 {
        for i in 0..n {
            let v = (mat[(ch, i)] * 65535.0).round() as u16;
            let off = 18 + (ch * n + i) * 2;
            bytes[off..off + 2].copy_from_slice(&v.to_be_bytes());
        }
    }
    CFData::from_bytes(&bytes)
}
