use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use tauri::{AppHandle, Manager, RunEvent};
use tauri_plugin_store::StoreBuilder;

use crate::engine::EngineHandle;
use crate::events::EngineEvent;

mod channels;
mod engine;
mod events;
mod observability;
mod renderers;
mod timer_state_machine;
mod tray;
mod utils;

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
                .auto_save(Duration::from_secs(2))
                .defaults(defaults)
                .build()?;

            // Panic hook
            let store_for_hook = store.clone();
            let app_for_hook = app.handle().clone();
            let _prev = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                let msg = format!("{info}");
                let _ = store_for_hook.set(
                    "system.last_crash".to_string(),
                    serde_json::json!({
                        "message": msg,
                        "thread": std::thread::current()
                            .name()
                            .unwrap_or("unknown")
                            .to_string(),
                        "time": SystemTime::now(),
                    }),
                );
                let _ = store_for_hook.save();

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
            if !CLEANUP_DONE.load(Ordering::Acquire) {
                api.prevent_exit();
                let app_handle_for_thread = app_handle.clone();
                let exit_code = code.unwrap_or(0);
                std::thread::spawn(move || shutdown(app_handle_for_thread, exit_code));
            }
        }
        _ => {}
    });

    fn shutdown(app_handle: AppHandle, exit_code: i32) {
        let engine_handle = app_handle.state::<EngineHandle>();
        let _ = engine_handle.tx.blocking_send(EngineEvent::Shutdown);
        if let Some(jh) = engine_handle.join.lock().unwrap().take() {
            let _ = jh.join();
        };
        CLEANUP_DONE.store(true, Ordering::Release);
        app_handle.exit(exit_code);
    }
}
