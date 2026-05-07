use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};

use crate::engine::EngineHandle;
use crate::events::{EngineEvent, StateCommand, WindowCommands, forward_engine_sync, get_preset_session_durations, toggle_main_window_sync};

pub fn setup_global_shortcuts(app: &AppHandle) {
    let gs = app.global_shortcut();
    let ds = get_preset_session_durations(app.clone());
    // Alt + Shift on Windows; Option + Shift on macOS
    let alt_shift = Modifiers::ALT | Modifiers::SHIFT;

    // ── Start sessions ─────────────────────────────────────────────
    {
        let d0 = ds[0];
        let _ = gs.on_shortcut(Shortcut::new(Some(alt_shift), Code::Digit1), move |app, _sc, _ev| {
            forward_engine_sync(app.state::<EngineHandle>().tx.clone(), EngineEvent::State(StateCommand::StartSession { target_duration_ms: d0 }));
        });
    }
    {
        let d1 = ds[1];
        let _ = gs.on_shortcut(Shortcut::new(Some(alt_shift), Code::Digit2), move |app, _sc, _ev| {
            forward_engine_sync(app.state::<EngineHandle>().tx.clone(), EngineEvent::State(StateCommand::StartSession { target_duration_ms: d1 }));
        });
    }
    {
        let d2 = ds[2];
        let _ = gs.on_shortcut(Shortcut::new(Some(alt_shift), Code::Digit3), move |app, _sc, _ev| {
            forward_engine_sync(app.state::<EngineHandle>().tx.clone(), EngineEvent::State(StateCommand::StartSession { target_duration_ms: d2 }));
        });
    }

    // ── State commands ──────────────────────────────────────────────
    {
        let _ = gs.on_shortcut(Shortcut::new(Some(alt_shift), Code::KeyB), move |app, _sc, _ev| {
            forward_engine_sync(app.state::<EngineHandle>().tx.clone(), EngineEvent::State(StateCommand::TakeBreakNow));
        });
    }
    {
        let _ = gs.on_shortcut(Shortcut::new(Some(alt_shift), Code::KeyS), move |app, _sc, _ev| {
            forward_engine_sync(app.state::<EngineHandle>().tx.clone(), EngineEvent::State(StateCommand::StopSession));
        });
    }
    {
        let _ = gs.on_shortcut(Shortcut::new(Some(alt_shift), Code::KeyP), move |app, _sc, _ev| {
            forward_engine_sync(app.state::<EngineHandle>().tx.clone(), EngineEvent::State(StateCommand::EnterPreview));
        });
    }
    {
        let _ = gs.on_shortcut(Shortcut::new(Some(alt_shift), Code::KeyE), move |app, _sc, _ev| {
            forward_engine_sync(app.state::<EngineHandle>().tx.clone(), EngineEvent::State(StateCommand::ExitPreview));
        });
    }
    {
        let _ = gs.on_shortcut(Shortcut::new(Some(alt_shift), Code::KeyR), move |app, _sc, _ev| {
            forward_engine_sync(app.state::<EngineHandle>().tx.clone(), EngineEvent::ForceReset);
        });
    }

    // ── Toggle window ───────────────────────────────────────────────
    {
        let _ = gs.on_shortcut(Shortcut::new(Some(alt_shift), Code::KeyH), move |app, _sc, _ev| {
            toggle_main_window_sync(app.clone(), WindowCommands::Close);
        });
    }
}
