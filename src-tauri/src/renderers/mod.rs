//! Platform-agnostic renderer dispatcher layer.
//!
//! [`RendererDispatcher`] resolves to the correct platform dispatcher at
//! compile time via `#[cfg]`.  The Engine instantiates one at startup and
//! calls its methods every frame.

mod utils;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
mod win_mag_api_color_transformer;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
mod mac_core_graphics_gamma_tweaker;

#[cfg(test)]
mod mock;

// ── Platform type alias ──────────────────────────────────────────────

#[cfg(target_os = "windows")]
pub type RendererDispatcher = windows::WindowsRendererDispatcher;

#[cfg(target_os = "macos")]
pub type RendererDispatcher = macos::MacOSRendererDispatcher;

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
compile_error!("Erode only supports Windows and macOS");
