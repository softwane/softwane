use tauri::Manager;

mod commands;
mod config;
mod engine;
mod platform;
mod session;

pub fn run() {
    tauri::Builder::default()
        .manage(platform::ManagedDisplayEffectApplier::default())
        .manage(session::ManagedSessionController::default())
        .setup(|app| {
            app.state::<session::ManagedSessionController>()
                .start(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::preview_effect,
            commands::apply_effect_snapshot,
            session::get_timer_session_state,
            session::start_timer_session,
            session::toggle_pause_timer_session,
            session::reset_timer_session,
            session::end_timer_session_early,
            session::update_timer_session_settings,
            session::set_timer_session_remaining_seconds
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Erode App");
}
