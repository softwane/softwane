use std::slice;

use objc2_core_foundation::{CFRetained, CFData};

/// Decode an ICC XYZ tag from CFData → [X, Y, Z] as f64.
///
/// Format (`s15Fixed16Number`):
///   bytes[0..4]   = "XYZ "  (magic)
///   bytes[4..8]   = 0       (reserved)
///   bytes[8..12]  = X       big-endian u32 → (i32 as f64) / 65536.0
///   bytes[12..16] = Y
///   bytes[16..20] = Z
pub(super) unsafe fn decode_xyz_tag(data: &CFData) -> Option<[f64; 3]> {
    let len = data.len();
    if len < 20 { return None; }

    let bytes_ptr = data.byte_ptr();
    if bytes_ptr.is_null() { return None; }

    let bytes: &[u8] = slice::from_raw_parts(bytes_ptr, len);
    if !bytes.starts_with(b"XYZ ") {
        return None;
    }

    fn read_s15f16(ptr: &[u8]) -> f64 {
        let v = u32::from_be_bytes([ptr[0], ptr[1], ptr[2], ptr[3]]);
        (v as i32 as f64) / 65536.0
    }
    Some([
        read_s15f16(&bytes[8..12]),
        read_s15f16(&bytes[12..16]),
        read_s15f16(&bytes[16..20]),
    ])
}

/// Encode [X, Y, Z] f64 → ICC XYZ tag CFData.
pub(super) fn encode_xyz_tag(xyz: [f64; 3]) -> CFRetained<CFData> {
    let mut bytes = [0u8; 20];
    bytes[0..4].copy_from_slice(b"XYZ ");

    fn write_s15f16(ptr: &mut [u8], v: f64) {
        let fixed = (v * 65536.0).round() as i32;
        ptr[..4].copy_from_slice(&(fixed as u32).to_be_bytes());
    }
    write_s15f16(&mut bytes[8..12], xyz[0]);
    write_s15f16(&mut bytes[12..16], xyz[1]);
    write_s15f16(&mut bytes[16..20], xyz[2]);

    CFData::from_bytes(&bytes)
}
