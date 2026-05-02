use std::collections::HashMap;
use std::time::Duration;

use tauri::Manager;
use tauri_plugin_store::StoreBuilder;

mod channels;
mod engine;
mod events;
mod observability;
mod renderers;
mod timer_state_machine;
mod tray;
mod utils;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .manage(observability::ManagedObservability::default())
        .setup(|app| {
            let (event_tx, event_rx) = tokio::sync::mpsc::channel(256);

            let mut defaults: HashMap<String, serde_json::Value> = HashMap::new();
            defaults.extend(channels::store_defaults());
            defaults.extend(timer_state_machine::store_defaults());
            let store = StoreBuilder::new(app.handle(), "config.json")
                .auto_save(Duration::from_secs(2))
                .defaults(defaults)
                .build()?;

            let engine = engine::Engine::new(
                app.handle().clone(),
                event_rx,
                event_tx.clone(),
                store,
            );
            let engine_join = std::thread::spawn(move || engine.run());

            app.manage(engine::EngineHandle {
                tx: event_tx,
                join: std::sync::Mutex::new(Some(engine_join)),
            });

            tray::setup_tray(app.handle())?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run Erode App");
}
