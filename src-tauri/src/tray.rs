use tauri::{
    AppHandle,
    Manager,
    Wry,
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
};

use crate::{
    engine::{EngineHandle, EngineEvent, ProgressPayload},
    timer_state_machine::{TimerState, commands::StateCommand},
    state::SharedTimerState,
    commands::get_preset_session_durations,
    i18n::{resolve_app_locale, tr, tr_format, AppLocale},
    window,
};

use std::sync::{LazyLock, Mutex};

static TRAY_STATUS_ITEM: LazyLock<Mutex<Option<MenuItem<Wry>>>> = LazyLock::new(|| Mutex::new(None));
static LAST_TRAY_STATUS: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));

pub fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let durations = get_preset_session_durations(app.clone());
    build_tray(app, TimerState::Idle, durations)
}

fn build_tray(app: &AppHandle, state: TimerState, durations: [u64; 3]) -> tauri::Result<()> {
    let menu = build_menu(app, state, durations)?;

    // Build icon
    let (icon, use_template) = if cfg!(target_os = "macos") {
        let icon_bytes = include_bytes!("../icons/icon-tray-template.png");
        (Image::from_bytes(icon_bytes).unwrap(), true)
    } else if cfg!(target_os = "windows") {
        let icon_bytes = include_bytes!("../icons/icon-tray-windows.png");
        (Image::from_bytes(icon_bytes).unwrap(), false)
    } else {
        (app.default_window_icon().unwrap().clone(), false)
    };

    if let Some(tray) = app.tray_by_id("main") {
        tray.set_menu(Some(menu))?;
        tray.set_icon(Some(icon))?;
        return Ok(());
    }

    TrayIconBuilder::with_id("main")
        .menu(&menu)
        .icon(icon)
        .icon_as_template(use_template)
        .on_menu_event(move |app, event| { match event.id.as_ref() {
            "quit" => app.exit(0),
            "open" => {
                let app_c = app.clone();
                tauri::async_runtime::spawn(async move {
                    tracing::debug!("begin showing window from tray");
                    window::show_main_window(app_c).await;
                });
            },
            other => {
                let event = match other {
                    "start_preset1" => EngineEvent::State(StateCommand::StartSession{ target_duration_ms: durations[0] }),
                    "start_preset2" => EngineEvent::State(StateCommand::StartSession{ target_duration_ms: durations[1] }),
                    "start_preset3" => EngineEvent::State(StateCommand::StartSession{ target_duration_ms: durations[2] }),
                    "take_break_now" => EngineEvent::State(StateCommand::TakeBreakNow),
                    "stop" => EngineEvent::State(StateCommand::StopSession),
                    "force_reset" => EngineEvent::ForceReset,
                    _ => return,
                };
                let tx = app.state::<EngineHandle>().tx.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(err) = tx.send(event).await {
                        tracing::error!("Tray failed to forward engine event: {err:?}.");
                    }
                });
            }
        }})
        .build(app)?;
    
    Ok(())
}

fn build_menu(app: &AppHandle, state: TimerState, durations: [u64; 3]) -> Result<Menu<Wry>, tauri::Error> {
    let is_idle = matches!(state, TimerState::Idle);
    let is_progress = matches!(state, TimerState::Progress { .. });
    let is_settling = matches!(state, TimerState::Settling { .. });
    let is_rest = matches!(state, TimerState::Rest);
    let locale = resolve_app_locale(app);

    let seperator = PredefinedMenuItem::separator(app)?;

    let status = MenuItem::with_id(app, "status", tray_status_label(locale, state), false, None::<&str>)?;
    remember_tray_status_item(&status);
    let open_win = MenuItem::with_id(app, "open", tr(locale, "tray.openWindow"), true, None::<&str>)?;

    let start_preset1_label = tr_format(locale, "tray.startSession", &[("{minutes}", (durations[0] / 60_000).to_string())]);
    let start_preset2_label = tr_format(locale, "tray.startSession", &[("{minutes}", (durations[1] / 60_000).to_string())]);
    let start_preset3_label = tr_format(locale, "tray.startSession", &[("{minutes}", (durations[2] / 60_000).to_string())]);

    let start_preset1 = MenuItem::with_id(app, "start_preset1", start_preset1_label, is_idle, None::<&str>)?;
    let start_preset2 = MenuItem::with_id(app, "start_preset2", start_preset2_label, is_idle, None::<&str>)?;
    let start_preset3 = MenuItem::with_id(app, "start_preset3", start_preset3_label, is_idle, None::<&str>)?;
    let take_break = MenuItem::with_id(app, "take_break_now", tr(locale, "tray.takeBreakNow"), is_progress, None::<&str>)?;
    let stop = MenuItem::with_id(app, "stop", tr(locale, "tray.stop"), is_progress || is_settling || is_rest, None::<&str>)?;
    
    let reset = MenuItem::with_id(app, "force_reset", tr(locale, "tray.forceReset"), true, None::<&str>)?;
    
    let quit = MenuItem::with_id(app, "quit", tr(locale, "tray.quit"), true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &status,
            &seperator,
            &open_win,
            &seperator,
            &start_preset1,
            &start_preset2,
            &start_preset3,
            &take_break,
            &stop,
            &seperator,
            &reset,
            &seperator,
            &quit
        ],
    )?;
    Ok(menu)
}

pub fn notify_crash(app: &AppHandle) {
    if let Some(tray) = app.tray_by_id("main") {
        let locale = resolve_app_locale(app);
        let _ = tray.set_title(Some(tr(locale, "tray.crashIndicator")));
    }
}

pub fn update_tray_progress(app: &AppHandle, progress: ProgressPayload) -> tauri::Result<()> {
    let status = tray_status_label(resolve_app_locale(app), progress.timer_state);

    #[cfg(windows)]
    {
        let Some(tray) = app.tray_by_id("main") else {
            tracing::warn!("Trying to update tray progress but tray is not found.");
            return Ok(());
        };
        tray.set_tooltip(Some(&status))?;
    }

    #[cfg(not(windows))]
    let _ = app;

    if mark_tray_status_changed(&status) {
        update_tray_status_item(status)?;
    }

    Ok(())
}

/// Rebuild the tray menu using the most recently published
/// [`SharedTimerState`].
///
/// Use from non-engine threads (Tauri command handlers, etc.) when the
/// caller does not own the current `TimerState` but still needs to
/// refresh enable/disable flags after some external mutation
/// (e.g. updated preset durations).  Falls back to `Idle` when
/// `SharedTimerState` has not been managed yet.
pub fn refresh_tray_menu(app: &AppHandle) -> tauri::Result<()> {
    let state = app
        .try_state::<SharedTimerState>()
        .map(|s| s.get())
        .unwrap_or(TimerState::Idle);
    let durations = get_preset_session_durations(app.clone());

    refresh_tray_menu_inner(app, state, durations)
}

/// Inner helper function for rebuilding the tray menu using a caller-supplied `TimerState`.
fn refresh_tray_menu_inner(
    app: &AppHandle,
    state: TimerState,
    durations: [u64; 3],
) -> tauri::Result<()> {
    let Some(tray) = app.tray_by_id("main") else {
        tracing::warn!("Trying to refresh tray menu but tray is not found.");
        return Ok(());
    };

    let menu = build_menu(app, state, durations)?;
    tray.set_menu(Some(menu))?;
    Ok(())
}

fn remember_tray_status_item(item: &MenuItem<Wry>) {
    if let Ok(mut status_item) = TRAY_STATUS_ITEM.lock() {
        *status_item = Some(item.clone());
    }
}

fn update_tray_status_item(status: String) -> tauri::Result<()> {
    let item = TRAY_STATUS_ITEM.lock().ok().and_then(|item| item.clone());
    if let Some(item) = item {
        item.set_text(status)?;
    }

    Ok(())
}

fn mark_tray_status_changed(status: &str) -> bool {
    let Ok(mut last_status) = LAST_TRAY_STATUS.lock() else {
        return true;
    };
    if last_status.as_deref() == Some(status) {
        return false;
    }
    *last_status = Some(status.to_string());
    true
}

fn tray_status_label(locale: AppLocale, state: TimerState) -> String {
    match state {
        TimerState::Idle => tr(locale, "tray.status.idle").to_string(),
        TimerState::Preview { .. } => tr(locale, "tray.status.preview").to_string(),
        TimerState::Progress { elapsed_ms, target_duration_ms } => tr_format(
            locale,
            "tray.status.workLeft",
            &[("{time}", format_remaining_ms(target_duration_ms.saturating_sub(elapsed_ms)))],
        ),
        TimerState::Settling { elapsed_ms, target_duration_ms } => tr_format(
            locale,
            "tray.status.settling",
            &[("{time}", format_remaining_ms(target_duration_ms.saturating_sub(elapsed_ms)))],
        ),
        TimerState::Rest => tr(locale, "tray.status.rest").to_string(),
        TimerState::Reverse { elapsed_ms, target_duration_ms } => tr_format(
            locale,
            "tray.status.resuming",
            &[("{time}", format_remaining_ms(target_duration_ms.saturating_sub(elapsed_ms)))],
        ),
    }
}

fn format_remaining_ms(ms: u64) -> String {
    let total_seconds = ms.div_ceil(1000);
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    format!("{minutes:02}:{seconds:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_active_work_remaining_time_for_tray_menu() {
        let state = TimerState::Progress {
            elapsed_ms: 61_200,
            target_duration_ms: 25 * 60_000,
        };

        assert_eq!(tray_status_label(AppLocale::En, state), "Work left: 23:59");
    }

    #[test]
    fn clamps_over_elapsed_sessions_to_zero_remaining() {
        let state = TimerState::Settling {
            elapsed_ms: 8_000,
            target_duration_ms: 5_000,
        };

        assert_eq!(tray_status_label(AppLocale::En, state), "Settling: 00:00");
    }

    #[test]
    fn rounds_partial_seconds_up_for_display() {
        assert_eq!(format_remaining_ms(1), "00:01");
        assert_eq!(format_remaining_ms(60_001), "01:01");
    }

    #[test]
    fn localizes_tray_status_in_chinese() {
        let state = TimerState::Progress {
            elapsed_ms: 61_200,
            target_duration_ms: 25 * 60_000,
        };

        assert_eq!(tray_status_label(AppLocale::ZhCn, state), "剩余工作时间：23:59");
    }
}
