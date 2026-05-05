//! Platform-agnostic renderer dispatcher layer.
//!
//! [`RendererDispatcher`] resolves to the correct platform dispatcher at
//! compile time via `#[cfg]`.  The Engine instantiates one at startup and
//! calls its methods every frame.

mod utils;
mod win_magapi_color_transformer;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "macos")]
mod macos;

// ── Platform type alias ──────────────────────────────────────────────

#[cfg(target_os = "windows")]
pub type RendererDispatcher = windows::WindowsRendererDispatcher;

#[cfg(target_os = "macos")]
pub type RendererDispatcher = macos::MacOSRendererDispatcher;

// TODO: add mock renderer dispatcher for other platforms and tests

#[cfg(test)]
mod tests {
    use super::utils::*;

    #[test]
    fn test_kelvin_to_rgb() {
        let (r, g, b) = kelvin_to_rgb(6500);
        assert!((r - 1.0).abs() < 0.001);
        assert!((g - 1.0).abs() < 0.001);
        assert!((b - 1.0).abs() < 0.001);
        let (_, _, b) = kelvin_to_rgb(1900);
        assert!((b - 0.0).abs() < 0.001);
    }
}
