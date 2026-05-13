mod commands;
mod engine;
mod timer_state_machine;
mod channels;
mod renderers;
mod utils;
mod shortcuts;
mod state;
mod tray;
mod window;

use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, RunEvent, WindowEvent};
use tauri_plugin_autostart::ManagerExt as AutostartManagerExt;
use tauri_plugin_store::StoreBuilder;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use commands::{DEFAULT_DURATIONS, STORE_KEY_LAST_CRASH, STORE_KEY_PRESET_SESSION_DURATIONS};
use engine::{Engine, EngineHandle, commands::{clear_progress_channel, EngineEvent}};
use shortcuts::{STORE_KEY_SHORTCUT_BINDINGS, default_shortcut_bindings};
use tray::notify_crash;
use window::{STORE_KEY_SILENT_START, open_main_window};

#[allow(dead_code)]
struct LogGuard(tracing_appender::non_blocking::WorkerGuard);

static CLEANUP_DONE: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn softwane_macos_colorsync_reset_saturation() -> bool;
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            // ── Tracing ──────────────────────────────────────────────
            let log_dir = app.path().app_log_dir()
                .or_else(|_| app.path().app_local_data_dir())?;
            std::fs::create_dir_all(&log_dir)?;

            let file_appender = tracing_appender::rolling::daily(&log_dir, "softwane.log");
            let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

            if cfg!(debug_assertions) {
                tracing_subscriber::registry()
                    .with(EnvFilter::from_default_env().add_directive(
                        "softwane_lib=debug".parse().expect("directive")
                    ))
                    .with(fmt::layer().with_writer(std::io::stderr))
                    .init();
            } else {
                tracing_subscriber::registry()
                    .with(EnvFilter::from_default_env().add_directive(
                        "softwane_lib=info".parse().expect("directive")
                    ))
                    .with(fmt::layer().with_writer(non_blocking).json())
                    .init();
            }

            app.manage(LogGuard(guard));

            // ── Channel & store ──────────────────────────────────────
            let (event_tx, event_rx) = tokio::sync::mpsc::channel(256);

            let mut defaults: HashMap<String, serde_json::Value> = HashMap::new();
            defaults.extend(channels::store_defaults());
            defaults.extend(timer_state_machine::store_defaults());
            defaults.insert(
                STORE_KEY_PRESET_SESSION_DURATIONS.into(),
                serde_json::json!(DEFAULT_DURATIONS),
            );
            defaults.insert(
                STORE_KEY_SHORTCUT_BINDINGS.into(),
                serde_json::to_value(default_shortcut_bindings())
                    .expect("default shortcut bindings serialise"),
            );
            defaults.insert(
                STORE_KEY_SILENT_START.into(),
                serde_json::json!(false),
            );
            let store = StoreBuilder::new(app.handle(), "config.json")
                .auto_save(Duration::from_secs(1))
                .defaults(defaults)
                .build()?;

            // Silent start: only open window if disabled in config
            let silent_start = store
                .get(STORE_KEY_SILENT_START)
                .and_then(|v| v.as_bool().or_else(|| {
                    tracing::warn!("stored config of {STORE_KEY_SILENT_START} is not a boolean (it is {v}), using default: false");    
                    store.set(STORE_KEY_SILENT_START, false);
                    Some(false)
                }))
                .expect("Defaults are set.");
            if !silent_start {
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(err) = open_main_window(app_handle).await {
                        tracing::error!("Failed to open main window on startup: {err:?}.");
                    }
                });
            }

            // Autostart enabled by default
            app.autolaunch().enable().ok();

            // ── Panic hook ───────────────────────────────────────────
            let store_for_hook = store.clone();
            let app_for_hook = app.handle().clone();
            std::panic::set_hook(Box::new(move |info| {
                let msg = format!("{info}");
                let crash_json = serde_json::json!({
                    "message": msg,
                    "thread": std::thread::current()
                        .name()
                        .unwrap_or("unknown")
                        .to_string(),
                    "time": std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .expect("system clock went backwards")
                        .as_millis() as u64,
                });

                tracing::error!("{crash_json}");
                let _ = std::io::stderr().write_fmt(format_args!("{crash_json}"));

                let _ = store_for_hook.set(
                    STORE_KEY_LAST_CRASH.to_string(),
                    crash_json.clone(),
                );
                let _ = store_for_hook.save();

                let _ = app_for_hook.emit("crash_recovered", crash_json);
                notify_crash(&app_for_hook);
            }));

            let shared_state = state::SharedTimerState::new();

            let engine = Engine::new(
                app.handle().clone(),
                event_rx,
                event_tx.clone(),
                store.clone(),
                shared_state.clone(),
            );
            let engine_join = std::thread::spawn(move || engine.run());

            app.manage(EngineHandle {
                tx: event_tx,
                join: std::sync::Mutex::new(Some(engine_join)),
            });
            app.manage(shared_state);

            tray::setup_tray(app.handle())?;
            shortcuts::setup_global_shortcuts(app.handle());

            // Crash recovery: notify tray if last run crashed
            if store.get(STORE_KEY_LAST_CRASH).is_some() {
                tray::notify_crash(app.handle());
            }

            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { .. } if window.label() == "main" => {
                clear_progress_channel(window.app_handle().state::<EngineHandle>());
            },
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            timer_state_machine::commands::start_session,
            timer_state_machine::commands::take_break_now,
            timer_state_machine::commands::stop_session,
            timer_state_machine::commands::enter_preview,
            timer_state_machine::commands::exit_preview,
            timer_state_machine::commands::update_preview_progress,
            timer_state_machine::commands::update_settling_duration,
            timer_state_machine::commands::update_reverse_duration,
            commands::force_reset,
            channels::commands::toggle_channel_switch,
            channels::commands::update_target_channel_value,
            channels::commands::update_progress_begin_ratio,
            channels::commands::update_progress_curve_params,
            channels::commands::update_settling_curve_params,
            channels::commands::update_reverse_curve_params,
            engine::commands::register_progress_channel,
            engine::commands::clear_progress_channel,
            commands::get_available_stored_config,
            commands::get_preset_session_durations,
            commands::update_preset_session_durations,
            commands::get_last_crash,
            commands::acknowledge_crash,
            commands::set_autostart_enabled,
            commands::is_autostart_enabled,
            shortcuts::get_shortcut_bindings,
            shortcuts::update_shortcut_bindings,
            window::get_silent_start,
            window::set_silent_start,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build softwane");

    app.run(|app_handle, event| match event {
        RunEvent::ExitRequested { api, code,.. } => {
            let Some(exit_code) = code else {
                // Prevent exit when all windows are desdroyed.
                // See: https://github.com/tauri-apps/tauri/issues/13511.
                api.prevent_exit();
                return;
            };
            if !CLEANUP_DONE.load(Ordering::Acquire) {
                api.prevent_exit();
                let app_handle_for_thread = app_handle.clone();
                std::thread::spawn(move || cleanup(app_handle_for_thread, exit_code));
            }
        }
        RunEvent::Exit => {
            #[cfg(target_os = "macos")]
            unsafe {
                softwane_macos_colorsync_reset_saturation();
            }
            // FIXME: Sometimes (cmd+Q, quiting from menu, and quiting from dock) tauri program exits
            // without emitting `RunEvent::ExitRequested` and emits `RunEvent::Exit` directly on macOS.
            // See: https://github.com/tauri-apps/tauri/issues/9198.
            // If I want to fix it without tauri's team fix it in tauri, I have to get Engine back
            // from its thread, because the event loop has been terminated when `Exit` is emitted.
        }
        _ => {}
    });

    fn cleanup(app_handle: AppHandle, exit_code: i32) {
        let engine_handle = app_handle.state::<EngineHandle>();
        // Do not need to care about the error, because it means the receiver was dropped;
        // This due to either panic or normal shutdown.
        let _ = engine_handle.tx.blocking_send(EngineEvent::Shutdown);
        let mut locked = match engine_handle.join.lock() {
            Ok(locked) => locked,
            Err(err) => {
                tracing::error!(
                    "Panic during last cleanup:\n{:#?}\n Exit code: {}. Give up cleaning up!",
                    err,
                    exit_code
                );
                CLEANUP_DONE.store(true, Ordering::Release);
                return;
            }
        };
        let Some(jh) = locked.take() else {
            tracing::warn!("Already been cleaning or cleaned up. Exit code: {}.", exit_code);
            return;
        };
        if let Err(err) = jh.join() {
            tracing::error!("Panic in the engine thread when joining it:\n{:#?}", err);
        };

        CLEANUP_DONE.store(true, Ordering::Release);
        app_handle.exit(exit_code);
    }
}
