mod events;
mod engine;
mod timer_state_machine;
mod channels;
mod renderers;
mod utils;
mod observability;
mod tray;

use engine::EngineHandle;
use events::EngineEvent;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Manager, RunEvent};
use tauri_plugin_store::StoreBuilder;

static CLEANUP_DONE: AtomicBool = AtomicBool::new(false);

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .manage(observability::ManagedObservability::default())
        .setup(|app| {
            let (event_tx, event_rx) = tokio::sync::mpsc::channel(256);

            // Store preparation
            let mut defaults: HashMap<String, serde_json::Value> = HashMap::new();
            defaults.extend(channels::store_defaults());
            defaults.extend(timer_state_machine::store_defaults());
            let store = StoreBuilder::new(app.handle(), "config.json")
                .auto_save(Duration::from_secs(1))
                .defaults(defaults)
                .build()?;

            // Panic hook
            let store_for_hook = store.clone();
            let app_for_hook = app.handle().clone();
            std::panic::set_hook(Box::new(move |info| {
                let msg = format!("{info}");
                let _ = store_for_hook.set(
                    "program_last_crash".to_string(),
                    serde_json::json!({
                        "message": msg,
                        "thread": std::thread::current()
                            .name()
                            .unwrap_or("unknown")
                            .to_string(),
                        "time": std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .expect("system clock went backwards")
                            .as_millis() as u64,
                    }),
                );
                let _ = store_for_hook.save();

                // TODO: confer "8. Panic 提示策略：tray 双信号" in refactor.plan.md
                if let Some(tray) = app_for_hook.tray_by_id("main") {
                    let _ = tray.set_title(Some("Err"));
                }
            }));

            let engine = engine::Engine::new(
                app.handle().clone(),
                event_rx,
                event_tx.clone(),
                store,
            );
            let engine_join = std::thread::spawn(move || engine.run());

            app.manage(EngineHandle {
                tx: event_tx,
                join: std::sync::Mutex::new(Some(engine_join)),
            });

            tray::setup_tray(app.handle())?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to build Erode App");

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
