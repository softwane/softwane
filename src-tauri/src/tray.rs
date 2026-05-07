use tauri::{
    AppHandle,
    Manager,
    Runtime,
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

use crate::{
    engine::EngineHandle,
    events::{
        EngineEvent, ProgressPayload, StateCommand, WindowCommands, forward_engine_sync, get_preset_session_durations, open_main_window, toggle_main_window_sync
    },
    timer_state_machine::TimerState
};

pub fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    build_tray(app, "Idle")?;
    Ok(())
}

fn build_tray(app: &AppHandle, phase_label: &str) -> tauri::Result<()> {
    let menu = build_menu(app, phase_label)?;

    let (icon, use_template) = if cfg!(target_os = "macos") {
        let icon_bytes = include_bytes!("../icons/icon-tray-template.png");
        (Image::from_bytes(icon_bytes).unwrap(), true)
    } else {
        (app.default_window_icon().unwrap().clone(), false)
    };

    if let Some(tray) = app.tray_by_id("main") {
        tray.set_menu(Some(menu))?;
        tray.set_icon(Some(icon))?;
        return Ok(());
    }

    let mut builder = TrayIconBuilder::with_id("main").icon(icon).menu(&menu);

    if use_template {
        builder = builder.icon_as_template(true);
    }

    builder
        .on_menu_event(move |app, event| {
            match event.id.as_ref() {
                "quit" => app.exit(0),
                other => {
                    let app_handle = app.clone();
                    let event = match other {
                        "open" => {
                            tauri::async_runtime::spawn(async move {
                                if let Err(err) = open_main_window(app_handle).await {
                                    tracing::error!("Failed to open main window from tray: {err:?}.");
                                }
                            });
                            return;
                        },
                        "start_preset1" => EngineEvent::State(StateCommand::StartSession{ target_duration_ms: 25 * 60_000 }),
                        "start_preset2" => EngineEvent::State(StateCommand::StartSession{ target_duration_ms: 50 * 60_000 }),
                        "start_preset3" => EngineEvent::State(StateCommand::StartSession{ target_duration_ms: 90 * 60_000 }),
                        "take_break_now" => EngineEvent::State(StateCommand::TakeBreakNow),
                        "stop" => EngineEvent::State(StateCommand::StopSession),
                        "force_reset" => EngineEvent::ForceReset,
                        _ => return,
                    };
                    forward_engine_sync(app_handle.state::<EngineHandle>().tx.clone(), event);
                }
            }
        })
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event {
                let app_handle = tray.app_handle().clone();
                toggle_main_window_sync(app_handle, WindowCommands::Hide);
            }
        })
        .build(app)?;
    
    Ok(())
}

fn build_menu<R: Runtime>(app: &AppHandle<R>, phase_label: &str) -> Result<Menu<R>, tauri::Error> {
    let durations = get_preset_session_durations(app.clone());
    let is_idle = phase_label == "Idle";
    let is_progress = phase_label == "Progress";
    let is_settling = phase_label == "Settling";
    let is_sabi = phase_label == "Sabi";

    let seperator = PredefinedMenuItem::separator(app)?;

    let open_win = MenuItem::with_id(app, "open", "Open window", true, None::<&str>)?;

    let start_preset1_label = format!("{} min", durations[0] / 60_000);
    let start_preset2_label = format!("{} min", durations[1] / 60_000);
    let start_preset3_label = format!("{} min", durations[2] / 60_000);

    let start_preset1 = MenuItem::with_id(app, "start_preset1", start_preset1_label, is_idle, None::<&str>)?;
    let start_preset2 = MenuItem::with_id(app, "start_preset2", start_preset2_label, is_idle, None::<&str>)?;
    let start_preset3 = MenuItem::with_id(app, "start_preset3", start_preset3_label, is_idle, None::<&str>)?;
    let take_break = MenuItem::with_id(app, "take_break_now", "Take a break now", is_progress, None::<&str>)?;
    let stop = MenuItem::with_id(app, "stop", "Stop", is_progress || is_settling || is_sabi, None::<&str>)?;
    
    let reset = MenuItem::with_id(app, "force_reset", "Force reset", true, None::<&str>)?;
    
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
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
        let _ = tray.set_title(Some("Err"));
    }
}

pub fn update_tray_progress(app: &AppHandle, _progress: ProgressPayload) -> tauri::Result<()> {
    let Some(_tray) = app.tray_by_id("main") else {
        tracing::warn!("Trying to update tray progress but tray is not found.");
        return Ok(());
    };

    Ok(())
}

pub fn update_tray_state(
    app: &AppHandle,
    state: TimerState,
) -> tauri::Result<()> {
    let Some(tray) = app.tray_by_id("main") else {
        tracing::warn!("Trying to update tray state but tray is not found.");
        return Ok(());
    };

    let menu = build_menu(app, state.label())?;
    tray.set_menu(Some(menu))?;
    Ok(())
}
