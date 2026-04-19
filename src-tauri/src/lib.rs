use tauri::Manager;

// mod channel;
mod channels;
mod compositor;
mod compositors;
mod configs;
mod engine;
mod events;
mod observability;
mod renderer;
mod timer_state_machine;
mod tray;
mod utils;
mod phase;
mod platform;
mod session;

pub fn run() {
    tauri::Builder::default()
        .manage(observability::ManagedObservability::default())
        .manage(platform::ManagedPlatformAdapter::default())
        .manage(session::ManagedSessionController::default())
        .setup(|app| {
            tray::setup_tray(app.handle())?;
            app.state::<session::ManagedSessionController>()
                .start(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::preview_frame,
            commands::reset_display,
            session::get_session_state,
            session::start_session,
            session::take_break_now,
            session::start_reverse,
            session::update_channels,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Erode App");
}
