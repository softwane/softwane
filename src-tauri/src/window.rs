use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow};
use tauri_plugin_store::StoreExt;

use crate::commands::CommandError;

pub const STORE_KEY_SILENT_START: &str = "silent_start";
const DELAYED_CLOSE_DURATION_SECS: u64 = 60;

// ── WindowManager ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WindowState {
    VisibleFocused,
    VisibleUnfocused,
    Hidden,
    Minimized,
    Closed,
}

#[derive(Default)]
pub struct WindowManager {
    pub stale_timer_closes: u32,
    pub expected_timer_closes: u32,
    pub timer_task: Option<tauri::async_runtime::JoinHandle<()>>,
}

// ── State helpers ─────────────────────────────────────────────────────

pub fn get_main_window_state(app_handle: &AppHandle) -> WindowState {
    let Some(window) = app_handle.get_webview_window("main") else {
        return WindowState::Closed;
    };
    if window.is_minimized().unwrap_or(false) {
        return WindowState::Minimized;
    }
    if !window.is_visible().unwrap_or(false) {
        return WindowState::Hidden;
    }
    if window.is_focused().unwrap_or(false) {
        WindowState::VisibleFocused
    } else {
        WindowState::VisibleUnfocused
    }
}

// ── create_window ─────────────────────────────────────────────────────

pub async fn create_main_window(app_handle: &AppHandle) -> Result<(), CommandError> {
    if app_handle.get_webview_window("main").is_some() {
        return Ok(());
    }
    match tauri::WebviewWindowBuilder::new(app_handle, "main", WebviewUrl::App("index.html".into()))
        .title("Softwane")
        .inner_size(560.0, 470.0)
        .resizable(true)
        .visible(false)
        .build() {
            Ok(_) | Err(tauri::Error::WindowLabelAlreadyExists(..)) => Ok(()),
            Err(e) => Err(CommandError::CreateWindowFailed(e))
        }
}

// ── show_window ───────────────────────────────────────────────────────

pub async fn show_main_window(app_handle: AppHandle) {
    {
        let mgr_state = app_handle.state::<Mutex<WindowManager>>();
        let mut mgr = mgr_state.lock().unwrap();
        mgr.stale_timer_closes += mgr.expected_timer_closes;
        mgr.expected_timer_closes = 0;
        if let Some(task) = mgr.timer_task.take() {
            task.abort();
        }
    }

    let Some(window) = app_handle.get_webview_window("main") else {
        if let Err(e) = create_main_window(&app_handle).await {
            tracing::error!("Failed to create main window in show_window: {e:?}");
            return;
        }
        let Some(window) = app_handle.get_webview_window("main") else {
            tracing::error!("Main window was created but is still unavailable.");
            return;
        };
        if let Err(e) = window.show() {
            tracing::warn!("Main window showing failed: {e:?}");
        }
        if let Err(e) = window.set_focus() {
            tracing::warn!("Main window focusing failed: {e:?}");
        }
        return;
    };
    if window.is_minimized().unwrap_or(false) {
        if let Err(e) = window.unminimize() {
            tracing::warn!("Main window unminimization failed: {e:?}");
        };
    }
    if let Err(e) = window.show() {
        tracing::warn!("Main window showing failed: {e:?}");
    }
    if let Err(e) = window.set_focus() {
        tracing::warn!("Main window focusing failed: {e:?}");
    }
}

// ── toggle_window ─────────────────────────────────────────────────────

pub async fn toggle_main_window(app_handle: AppHandle) {
    let state = get_main_window_state(&app_handle);
    match state {
        WindowState::VisibleFocused => {
            if let Some(window) = app_handle.get_webview_window("main") {
                initiate_delayed_close(app_handle.clone(), &window);
            }
        }
        _ => show_main_window(app_handle).await,
    }
}

// ── initiate_delayed_close ────────────────────────────────────────────

pub fn initiate_delayed_close(app_handle: AppHandle, window: &WebviewWindow) {
    if let Err(e) = window.hide() {
        tracing::warn!("{} window hiding failed: {e:?}", window.label());
    }

    let mgr_state = app_handle.state::<Mutex<WindowManager>>();
    let mut mgr = mgr_state.lock().unwrap();
    if let Some(task) = mgr.timer_task.take() {
        task.abort();
    }

    let app_c = app_handle.clone();
    let label = window.label().to_string();
    let task = tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(DELAYED_CLOSE_DURATION_SECS)).await;
        let state = app_c.state::<Mutex<WindowManager>>();
        let mut mgr = state.lock().unwrap();
        mgr.expected_timer_closes += 1;
        if let Some(win) = app_c.get_webview_window(label.as_str()) {
            let _ = win.close();
        }
    });

    mgr.timer_task = Some(task);
}

// ── Tauri commands ────────────────────────────────────────────────────

#[tauri::command]
pub fn get_silent_start(app_handle: AppHandle) -> Result<bool, CommandError> {
    let store = app_handle.store("config.json")?;
    Ok(store
        .get(STORE_KEY_SILENT_START)
        .and_then(|v| v.as_bool().or_else(|| {
            tracing::warn!("stored config for {STORE_KEY_SILENT_START} is not a boolean (it is {v}), using default: false");
            store.set(STORE_KEY_SILENT_START, false);
            Some(false)
        }))
        .expect("Defaults are set."))
}

#[tauri::command]
pub fn set_silent_start(app_handle: AppHandle, enabled: bool) -> Result<(), CommandError> {
    let store = app_handle.store("config.json")?;
    store.set(
        STORE_KEY_SILENT_START.to_string(),
        serde_json::Value::Bool(enabled),
    );
    Ok(())
}
