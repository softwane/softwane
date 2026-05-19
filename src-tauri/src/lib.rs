mod engine;
mod timer_state_machine;
mod channels;
mod renderers;
mod state;
mod utils;
mod commands;
mod shortcuts;
mod tray;
mod window;

use std::collections::HashMap;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, RunEvent, WindowEvent};
use tauri_plugin_autostart::ManagerExt as AutostartManagerExt;
use tauri_plugin_store::StoreBuilder;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use engine::{Engine, EngineHandle, EngineEvent, ProgressCommandInner};
use commands::{DEFAULT_DURATIONS, STORE_KEY_LAST_CRASH, STORE_KEY_PRESET_SESSION_DURATIONS};
use shortcuts::{STORE_KEY_SHORTCUT_BINDINGS, default_shortcut_bindings};
use tray::notify_crash;
use window::{STORE_KEY_SILENT_START, WindowManager, create_main_window};

static CLEANUP_DONE: AtomicBool = AtomicBool::new(false);

pub fn run() {
    let log_guard = Arc::new(Mutex::new(None));
    let log_guard_clone = log_guard.clone();
    
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(move |app| {
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

            let mut log_guard = log_guard_clone.lock().expect("This is the first call");
            *log_guard = Some(guard);

            // ── Store ──────────────────────────────────────
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
                    if let Err(err) = create_main_window(&app_handle).await {
                        tracing::error!("Failed to create main window on startup: {err:?}.");
                    }
                });
            }

            // Autostart enabled by default
            app.autolaunch().enable().ok();

            // ── Panic hook ───────────────────────────────────────────
            // TODO: crash info should be stored in a distinguished file
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

                let _ = app_for_hook.emit("crashed", crash_json);
                notify_crash(&app_for_hook);
            }));

            let shared_state = state::SharedTimerState::new();
            let (event_tx, event_rx) = tokio::sync::mpsc::channel(256);

            let app_for_engine = app.handle().clone();
            let store_for_engine = store.clone();
            let shared_state_for_engine = shared_state.clone();
            let event_tx_for_engine = event_tx.clone();
            let engine_join = std::thread::spawn(move || {
                Engine::new(
                    app_for_engine,
                    event_rx,
                    event_tx_for_engine,
                    store_for_engine,
                    shared_state_for_engine,
                )
                .run();
            });

            app.manage(EngineHandle {
                tx: event_tx,
                join: std::sync::Mutex::new(Some(engine_join)),
            });
            app.manage(shared_state);
            app.manage(Mutex::new(WindowManager::default()));

            tray::setup_tray(app.handle())?;
            shortcuts::setup_global_shortcuts(app.handle());

            // Crash recovery: notify tray if last run crashed
            if store.get(STORE_KEY_LAST_CRASH).is_some() {
                tray::notify_crash(app.handle());
            }

            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } if window.label() == "main" => {
                let app_handle = window.app_handle();
                let state = app_handle.state::<Mutex<WindowManager>>();
                let mut mgr = state.lock().unwrap();

                // Defence 1: stale timer close — a previous timer's close event
                // that was invalidated by a subsequent show.
                if mgr.stale_timer_closes > 0 {
                    mgr.stale_timer_closes -= 1;
                    api.prevent_close();
                    return;
                }

                // Defence 2: legitimate timer close.
                if mgr.expected_timer_closes > 0 {
                    mgr.expected_timer_closes -= 1;
                    // Allow close: clean up progress channel.
                    let tx = app_handle.state::<EngineHandle>().tx.clone();
                    let event = EngineEvent::Progress(ProgressCommandInner::ClearChannel{
                        window: window.label().to_string()
                    });
                    tauri::async_runtime::spawn(async move {
                        if let Err(err) = tx.send(event).await {
                            tracing::error!("Failed to clear channel: {err:?}.");
                        }
                    });
                    return;
                }

                // Defence 3: user clicked X — hide and start 1-min timer.
                api.prevent_close();
                if let Some(w) = app_handle.get_webview_window("main") {
                    drop(mgr);
                    window::initiate_delayed_close(app_handle.clone(), &w);
                }
            },
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            engine::commands::command_engine,
            engine::commands::command_engine_nowait,
            engine::commands::get_available_stored_config,
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

    let log_guard_another_clone = log_guard.clone();

    app.run(move |app_handle, event| match event {
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
            if CLEANUP_DONE.load(Ordering::Acquire) {
                return;
            }
            
            CLEANUP_DONE.store(true, Ordering::Release);

            let engine_handle = app_handle.state::<EngineHandle>();
            
            if let Err(_) = engine_handle.tx.blocking_send(EngineEvent::AbnormalShutdown) {
                return;
            }

            let mut locked = match engine_handle.join.lock() {
                Ok(locked) => locked,
                Err(err) => {
                    tracing::error!(
                        "Panic during last cleanup when ExitRequested:\n{:#?}\n",
                        err,
                    );
                    err.into_inner()
                }
            };
            let Some(jh) = locked.take() else {
                tracing::warn!("Already been cleaning or cleaned up.");
                return;
            };

            if let Err(err) = jh.join() {
                tracing::error!("Panic in the engine thread when joining it:\n{:#?}", err);
            }

            let _log_guard = log_guard_another_clone.lock().expect("Exit should be called only once").take();
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
                    "Panic during last cleanup:\n{:#?}\n Current exit code: {}. Give up cleaning up!",
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
