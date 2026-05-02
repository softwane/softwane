use tauri::Manager;

mod channels;
mod configs;
mod engine;
mod events;
mod observability;
mod renderers;
mod timer_state_machine;
mod tray;
mod utils;

pub fn run() {
    tauri::Builder::default()
        .manage(observability::ManagedObservability::default())
        .setup(|app| {
            let (event_tx, event_rx) = tokio::sync::mpsc::channel(256);
            
            let engine = engine::Engine::new(
                app.handle().clone(), 
                event_rx,
                (&event_tx).clone(),
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
