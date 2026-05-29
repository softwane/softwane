use tauri::{AppHandle, Runtime};
use tauri_plugin_os::locale;
use tauri_plugin_store::StoreExt;

use crate::{commands::CommandError, tray::refresh_tray_menu};

pub const STORE_KEY_APP_LOCALE: &str = "app_locale";
pub const APP_LOCALE_SYSTEM: &str = "system";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppLocale {
    En,
    ZhCn,
}

impl AppLocale {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::ZhCn => "zh-CN",
        }
    }

    pub fn from_preference(value: &str) -> Option<Self> {
        let normalized = value.trim().to_ascii_lowercase();
        if normalized.starts_with("zh") {
            Some(Self::ZhCn)
        } else if normalized.starts_with("en") {
            Some(Self::En)
        } else {
            None
        }
    }
}

pub fn normalize_locale_preference(value: &str) -> Option<String> {
    if value.trim().eq_ignore_ascii_case(APP_LOCALE_SYSTEM) {
        return Some(APP_LOCALE_SYSTEM.to_string());
    }
    AppLocale::from_preference(value).map(|locale| locale.as_str().to_string())
}

pub fn resolve_app_locale<R: Runtime>(app_handle: &AppHandle<R>) -> AppLocale {
    let store = app_handle.store("config.json").ok();
    let stored_locale = store
        .as_ref()
        .and_then(|store| store.get(STORE_KEY_APP_LOCALE))
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| APP_LOCALE_SYSTEM.to_string());

    if let Some(locale) = AppLocale::from_preference(&stored_locale) {
        return locale;
    }

    let system_locale = locale();
    AppLocale::from_preference(system_locale.as_deref().unwrap_or("en"))
        .unwrap_or(AppLocale::En)
}

pub fn tr(locale: AppLocale, key: &str) -> &'static str {
    match locale {
        AppLocale::En => tr_en(key),
        AppLocale::ZhCn => tr_zh_cn(key),
    }
}

pub fn tr_format(locale: AppLocale, key: &str, replacements: &[(&str, String)]) -> String {
    let mut text = tr(locale, key).to_string();
    for (needle, value) in replacements {
        text = text.replace(needle, value);
    }
    text
}

#[tauri::command]
pub fn get_app_locale(app_handle: AppHandle) -> Result<String, CommandError> {
    let store = app_handle.store("config.json")?;
    let locale = store
        .get(STORE_KEY_APP_LOCALE)
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| APP_LOCALE_SYSTEM.to_string());
    Ok(normalize_locale_preference(&locale).unwrap_or_else(|| APP_LOCALE_SYSTEM.to_string()))
}

// TODO: Merge get_app_locale and get_resolved_app_locale
#[tauri::command]
pub fn get_resolved_app_locale(app_handle: AppHandle) -> String {
    resolve_app_locale(&app_handle).as_str().to_string()
}

#[tauri::command]
pub fn set_app_locale(app_handle: AppHandle, locale: String) -> Result<(), CommandError> {
    let normalized = normalize_locale_preference(&locale)
        .ok_or_else(|| CommandError::BadArguments(format!("unsupported locale: {locale}")))?;
    let store = app_handle.store("config.json")?;
    store.set(
        STORE_KEY_APP_LOCALE.to_string(),
        serde_json::Value::String(normalized),
    );
    if let Err(err) = refresh_tray_menu(&app_handle) {
        tracing::error!("Failed to refresh tray menu after updating locale: {err:?}.");
    }
    Ok(())
}

fn tr_en(key: &str) -> &'static str {
    match key {
        "tray.crashIndicator" => "Err",
        "tray.openWindow" => "Open window",
        "tray.takeBreakNow" => "Take a break now",
        "tray.stop" => "Stop",
        "tray.forceReset" => "Force reset",
        "tray.quit" => "Quit",
        "tray.status.idle" => "Idle",
        "tray.status.preview" => "Preview",
        "tray.status.workLeft" => "Work left: {time}",
        "tray.status.settling" => "Settling: {time}",
        "tray.status.rest" => "Rest",
        "tray.status.resuming" => "Resuming: {time}",
        "tray.startSession" => "Start a {minutes} min session",
        _ => "unknown",
    }
}

fn tr_zh_cn(key: &str) -> &'static str {
    match key {
        "tray.crashIndicator" => "错",
        "tray.openWindow" => "打开窗口",
        "tray.takeBreakNow" => "立即休息",
        "tray.stop" => "停止",
        "tray.forceReset" => "强制重置",
        "tray.quit" => "退出",
        "tray.status.idle" => "空闲",
        "tray.status.preview" => "预览",
        "tray.status.workLeft" => "剩余工作时间：{time}",
        "tray.status.settling" => "缓入休息：{time}",
        "tray.status.rest" => "休息中",
        "tray.status.resuming" => "恢复中：{time}",
        "tray.startSession" => "开始 {minutes} 分钟会话",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_supported_locale_preferences() {
        assert_eq!(normalize_locale_preference("zh-Hans-CN"), Some("zh-CN".to_string()));
        assert_eq!(normalize_locale_preference("en-US"), Some("en".to_string()));
        assert_eq!(normalize_locale_preference("system"), Some("system".to_string()));
    }
}
