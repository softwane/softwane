use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindow};
use tauri_plugin_store::StoreExt;

use crate::events::CommandError;

pub const STORE_KEY_SILENT_START: &str = "silent_start";

/// Show the main window to foreground and set focus; create the main window it does not exist.
pub async fn open_main_window<R: Runtime>(app_handle: AppHandle<R>) -> Result<(), CommandError> {
    let window = match app_handle.get_webview_window("main") {
        Some(window) => window,
        None => {
            tauri::WebviewWindowBuilder::new(
                &app_handle,
                "main",
                WebviewUrl::App("index.html".into()),
            )
            .title("Softwane")
            .inner_size(560.0, 420.0)
            .resizable(true)
            // .min_inner_size(480.0, 380.0)   // 除非明确需要，不限制窗口大小，最讨厌不能自定义窗口形状的应用
            .visible(false) 
            .build()
            .map_err(|e| CommandError::CreateWindowFailed(e))?
            // TODO: 如果启动窗口加载时间太长，幽灵启动，让前端展示自己
            // 若如此，则拆分为show_and_focus与build。
            // return Ok(())
        }
    };
    window.show().map_err(|e| CommandError::ShowWindowFailed(e))?;
    if let Err(err) = window.set_focus() {
        tracing::warn!("Failed to set focus to main window: {err:?}.")
    };
    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub enum WindowCommands {
    Close,
    Hide,
}

fn close_main_window<R: Runtime>(window: WebviewWindow<R>) -> Result<(), CommandError>{
    window.close().map_err(|e| CommandError::CloseWindowFailed(e))?;
    Ok(())
}

fn hide_main_window<R: Runtime>(window: WebviewWindow<R>) -> Result<(), CommandError> {
    window.hide().map_err(|e| CommandError::HideWindowFailed(e))?;
    Ok(())
}

pub async fn toggle_main_window<R: Runtime>(app_handle: AppHandle<R>, wincmd: WindowCommands) -> Result<(), CommandError> {
    if let Some(window) = app_handle.get_webview_window("main") {
        if !window.is_visible().map_err(|e| CommandError::OtherWindowError(e))? 
        || !window.is_focused().map_err(|e| CommandError::OtherWindowError(e))? {
            return open_main_window(app_handle).await;
        }

        match wincmd {
            WindowCommands::Close => { return close_main_window(window);}
            WindowCommands::Hide => {
                if window.is_visible().map_err(|e| CommandError::OtherWindowError(e))? {
                    return hide_main_window(window);
                }
            }
        }
        
    };
    open_main_window(app_handle).await
}

pub fn toggle_main_window_sync(app_handle: AppHandle, wincmd: WindowCommands) {
    tauri::async_runtime::spawn(async move {
        if let Err(err) = toggle_main_window(app_handle, wincmd).await {
            tracing::error!("Failed to toggle main window from shortcut: {err:?}.");
        }
    });
}

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
