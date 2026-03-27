mod commands;
mod config;
mod engine;
mod platform;

pub fn run() {
    tauri::Builder::default()
        .manage(platform::ManagedDisplayEffectApplier::default())
        .invoke_handler(tauri::generate_handler![
            commands::preview_effect,
            commands::apply_effect_snapshot
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Erode App");
}
