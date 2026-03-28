use tauri::{
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, Runtime,
};

pub fn setup_tray<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    build_tray_menu(app, "Idle", "Idle")?;
    Ok(())
}

fn build_tray_menu<R: Runtime>(
    app: &AppHandle<R>,
    session_stage: &str,
    session_status: &str,
) -> tauri::Result<()> {
    let is_active = session_stage == "Work";
    let is_paused = session_status == "Paused";

    let show = MenuItem::with_id(app, "show", "Show Window", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let pause_label = if is_paused { "Resume" } else { "Pause" };
    let pause = MenuItem::with_id(app, "pause", pause_label, is_active, None::<&str>)?;
    let reset = MenuItem::with_id(app, "reset", "Reset", is_active, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &separator, &pause, &reset, &separator, &quit])?;

    if let Some(tray) = app.tray_by_id("main") {
        tray.set_menu(Some(menu))?;
    } else {
        let (icon, use_template) = if cfg!(target_os = "macos") {
            let icon_bytes = include_bytes!("../icons/icon-tray-template.png");
            (Image::from_bytes(icon_bytes).unwrap(), true)
        } else {
            (app.default_window_icon().unwrap().clone(), false)
        };

        let mut builder = TrayIconBuilder::with_id("main")
            .icon(icon)
            .menu(&menu);

        if use_template {
            builder = builder.icon_as_template(true);
        }

        builder
            .on_menu_event(|app, event| match event.id.as_ref() {
                "quit" => app.exit(0),
                "show" => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
                "pause" => {
                    let _ = app.emit("tray-pause-session", ());
                }
                "reset" => {
                    let _ = app.emit("tray-reset-session", ());
                }
                _ => {}
            })
            .on_tray_icon_event(|tray, event| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    let app = tray.app_handle();
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
            })
            .build(app)?;
    }

    Ok(())
}

pub fn update_tray_title<R: Runtime>(app: &AppHandle<R>, title: &str) {
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_title(Some(title));
    }
}

pub fn update_tray_menu<R: Runtime>(
    app: &AppHandle<R>,
    session_stage: &str,
    session_status: &str,
) -> tauri::Result<()> {
    build_tray_menu(app, session_stage, session_status)
}
