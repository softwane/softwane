//! Configurable global keyboard shortcuts.
//!
//! # Architecture
//!
//! The set of *actions* is hard-coded ([`ShortcutAction`]).  Each
//! action's *binding* — modifiers + key code — is configurable and
//! persisted via `tauri-plugin-store` under
//! [`STORE_KEY_SHORTCUT_BINDINGS`].
//!
//! All shortcuts share a single multiplexed handler that looks up the
//! action by `Shortcut::id()` via [`ShortcutRouter`].  This avoids
//! capturing per-action state in closures (which would force
//! re-registering them whenever the underlying state changed —
//! e.g. preset durations).
//!
//! # Update protocol
//!
//! 1. Frontend posts a new map of bindings.
//! 2. We validate (≥ 1 modifier per binding, no duplicate (mods, code),
//!    every code parses).
//! 3. We `unregister_all` and register the new set under one handler.
//! 4. On any failure we attempt to roll back to the previous set so
//!    the OS state and our store stay in sync.
//! 5. Only on success do we write the new bindings to the store.

use std::collections::HashMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{
    Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutEvent, ShortcutState,
};
use tauri_plugin_store::StoreExt;
use thiserror::Error;

use crate::engine::EngineHandle;
use crate::events::{
    CommandError, EngineEvent, StateCommand, forward_engine_sync,
    get_preset_session_durations,
};
use crate::state::SharedTimerState;
use crate::timer_state_machine::TimerState;
use crate::window::{WindowCommands, toggle_main_window_sync};

pub const STORE_KEY_SHORTCUT_BINDINGS: &str = "shortcut_bindings";

// ── Domain types ─────────────────────────────────────────────────────

/// User-facing shortcut actions.
///
/// Variants are persisted as their `snake_case` representation.  The
/// list is closed (compile-time) — adding a new shortcut is a code
/// change, not a config change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutAction {
    StartPreset1,
    StartPreset2,
    StartPreset3,
    TakeBreakNow,
    StopSession,
    /// Single hotkey that toggles in / out of Preview based on the
    /// current `TimerState` snapshot from `SharedTimerState`.
    TogglePreview,
    ForceReset,
    ToggleMainWindow,
}

impl ShortcutAction {
    /// Exhaustive list of variants in canonical UI order.  Useful for
    /// iteration; consumers should prefer this over relying on
    /// `HashMap` iteration order.
    #[allow(dead_code)]
    pub const ALL: [ShortcutAction; 8] = [
        ShortcutAction::StartPreset1,
        ShortcutAction::StartPreset2,
        ShortcutAction::StartPreset3,
        ShortcutAction::TakeBreakNow,
        ShortcutAction::StopSession,
        ShortcutAction::TogglePreview,
        ShortcutAction::ForceReset,
        ShortcutAction::ToggleMainWindow,
    ];
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd,
    Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ModifierKey {
    Alt,
    Shift,
    Control,
    Meta,
}

impl ModifierKey {
    fn as_modifiers(&self) -> Modifiers {
        match self {
            ModifierKey::Alt => Modifiers::ALT,
            ModifierKey::Shift => Modifiers::SHIFT,
            ModifierKey::Control => Modifiers::CONTROL,
            // `Modifiers::META` is silently rewritten to `SUPER` by
            // `HotKey::new`, so use SUPER directly here for clarity.
            // SUPER = Win on Windows, Command on macOS.
            ModifierKey::Meta => Modifiers::SUPER,
        }
    }
}

/// One key binding: at least one modifier + exactly one main key.
///
/// `code` follows the [`keyboard_types::Code`] string form
/// (e.g. `"Digit1"`, `"KeyB"`) — same as JavaScript's
/// `KeyboardEvent.code`, which simplifies frontend recording.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyBinding {
    pub modifiers: Vec<ModifierKey>,
    pub code: String,
}

impl KeyBinding {
    /// Convert to a plugin [`Shortcut`].
    fn to_shortcut(&self) -> Result<Shortcut, BindingError> {
        if self.modifiers.is_empty() {
            return Err(BindingError::EmptyModifiers);
        }
        let mut mods = Modifiers::empty();
        for m in &self.modifiers {
            mods |= m.as_modifiers();
        }
        let code: Code = self.code.parse().map_err(|_| {
            BindingError::UnrecognisedCode(self.code.clone())
        })?;
        Ok(Shortcut::new(Some(mods), code))
    }
}

pub type ShortcutBindings = HashMap<ShortcutAction, KeyBinding>;

/// Hard-coded factory defaults.  Used both as the
/// [`StoreBuilder::defaults`] entry and as a fallback when the stored
/// JSON fails to deserialise.
pub fn default_shortcut_bindings() -> ShortcutBindings {
    use ModifierKey::*;
    let alt_shift = vec![Alt, Shift];
    let mk = |code: &str| KeyBinding {
        modifiers: alt_shift.clone(),
        code: code.into(),
    };
    HashMap::from([
        (ShortcutAction::StartPreset1, mk("Digit1")),
        (ShortcutAction::StartPreset2, mk("Digit2")),
        (ShortcutAction::StartPreset3, mk("Digit3")),
        (ShortcutAction::TakeBreakNow, mk("KeyB")),
        (ShortcutAction::StopSession, mk("KeyQ")),
        (ShortcutAction::TogglePreview, mk("KeyT")),
        (ShortcutAction::ForceReset, mk("KeyF")),
        (ShortcutAction::ToggleMainWindow, mk("KeyH")),
    ])
}

// ── Routing table ────────────────────────────────────────────────────

/// Live mapping from registered Shortcut id → action.
///
/// Stored in `app.manage(...)`; the multiplexed handler reads it on
/// every keypress to know which action to dispatch.
#[derive(Default)]
pub struct ShortcutRouter(Mutex<ShortcutRouterInner>);

#[derive(Debug, Default, Clone)]
struct ShortcutRouterInner {
    by_id: HashMap<u32, ShortcutAction>,
    bindings: ShortcutBindings,
}

impl ShortcutRouter {
    fn snapshot(&self) -> ShortcutRouterInner {
        self.0
            .lock()
            .expect("shortcut router lock poisoned")
            .clone()
    }
}

// ── Errors ───────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum BindingError {
    #[error("must have at least one modifier")]
    EmptyModifiers,
    #[error("unrecognised key code {0:?}")]
    UnrecognisedCode(String),
}

#[derive(Debug, Error)]
pub enum ApplyShortcutError {
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("global shortcut plugin error: {0}")]
    Plugin(String),
}

impl From<ApplyShortcutError> for CommandError {
    fn from(e: ApplyShortcutError) -> Self {
        CommandError::BadArguments(e.to_string())
    }
}

// ── Apply / register ─────────────────────────────────────────────────

/// Re-apply the routing table.
///
/// Validates → unregisters old → registers new (updates state & sets on_shortcuts handler) → on failure,
/// best-effort rollback to the previous set so the OS state and our
/// stored config stay aligned.
pub fn apply_bindings(
    app: &AppHandle,
    bindings: &ShortcutBindings,
) -> Result<(), ApplyShortcutError> {
    let (by_id, shortcuts) = try_build_by_id_and_shortcuts(bindings)?;

    // Cache the previous routing table so we can roll back.  An empty
    // snapshot means "first apply, nothing to roll back to."
    let previous = app
        .try_state::<ShortcutRouter>()
        .map(|r| r.snapshot())
        .unwrap_or_default();

    let gs = app.global_shortcut();
    if let Err(err) = gs.unregister_all() {
        return Err(ApplyShortcutError::Plugin(err.to_string()));
    }

    if let Err(err) = register_set(app, &by_id, bindings, shortcuts) {
        // Try to restore the prior routing.
        let _ = gs.unregister_all();
        if !previous.bindings.is_empty() {
            if let Err(rollback_err) = register_set(
                app,
                &previous.by_id,
                &previous.bindings,
                previous.bindings.values().map(|b| b.to_shortcut().expect("`previous` have been validated before")).collect()
            ) {
                tracing::error!(
                    "shortcut rollback failed after register error: {rollback_err}",
                );
            }
        } else {
            // No previous set; clear router state.
            if let Some(router) = app.try_state::<ShortcutRouter>() {
                let mut g = router.0.lock().expect("shortcut router lock");
                g.by_id.clear();
                g.bindings.clear();
            }
        }
        return Err(err);
    }

    Ok(())
}

/// Validates and builds the by_id HashMap and shortcut
fn try_build_by_id_and_shortcuts(
    bindings: &ShortcutBindings
) -> Result<
    (HashMap<u32, ShortcutAction>, Vec<Shortcut>),
    ApplyShortcutError
> {
    let mut by_id: HashMap<u32, ShortcutAction> =
        HashMap::with_capacity(bindings.len());
    let mut shortcuts: Vec<Shortcut> = Vec::with_capacity(bindings.len());
    let mut errors: Vec<String> = Vec::with_capacity(bindings.len());
    for (action, binding) in bindings {
        let shortcut = match binding.to_shortcut() {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("Got unvalid binding for {action:?}: {e}");
                errors.push(e.to_string());
                continue;
            }
        };
        if let Some(prev) = by_id.insert(shortcut.id(), *action) {
            let e_msg = if prev == *action {
                format!("{prev:?} and {action:?} duplicate")
            } else {
                format!("{prev:?} and {action:?} share the same key combination ({binding:?})")
            };
            tracing::error!(e_msg);
            return Err(ApplyShortcutError::Validation(
                format!("{}; {e_msg}", errors.join("; "))
            ));
        }
        shortcuts.push(shortcut);
    }
    return Ok((by_id, shortcuts));
}

/// Register the giving bindings, including updating the global state.
fn register_set(
    app: &AppHandle,
    by_id: &HashMap<u32, ShortcutAction>,
    bindings: &ShortcutBindings,
    shortcuts: Vec<Shortcut>,
) -> Result<(), ApplyShortcutError> {
    // Update router *before* registering: the multiplexed handler
    // reads it as soon as the OS dispatches an event, so a missing
    // entry would silently drop the action.
    if let Some(router) = app.try_state::<ShortcutRouter>() {
        let mut guard = router.0.lock().expect("shortcut router lock");
        guard.by_id = by_id.clone();
        guard.bindings = bindings.clone();
    } else {
        return Err(ApplyShortcutError::Plugin(
            "ShortcutRouter not managed; call setup_global_shortcuts first".into(),
        ));
    }

    let gs = app.global_shortcut();
    if let Err(err) = gs.on_shortcuts(shortcuts, multiplex_handler) {
        return Err(ApplyShortcutError::Plugin(err.to_string()));
    }

    Ok(())
}

fn multiplex_handler(
    app: &AppHandle,
    shortcut: &Shortcut,
    event: ShortcutEvent,
) {
    if event.state != ShortcutState::Pressed {
        // Ignore the Released edge.  This keeps action callbacks
        // single-shot per keypress (otherwise StartSession etc. would
        // fire twice per tap).
        return;
    }
    let Some(router) = app.try_state::<ShortcutRouter>() else {
        tracing::warn!("ShortcutRouter not managed; dropping shortcut event");
        return;
    };
    let action = {
        let guard = router.0.lock().expect("shortcut router lock");
        tracing::debug!("shortcut router: {guard:?}");
        guard.by_id.get(&shortcut.id()).copied()
    };
    let Some(action) = action else {
        tracing::debug!(
            shortcut_id = shortcut.id(),
            "no action registered for shortcut id (stale fire?)",
        );
        return;
    };
    tracing::debug!("shortcut: {shortcut:?}, event: {event:?}");
    handle_action(app, action);
}

fn handle_action(app: &AppHandle, action: ShortcutAction) {
    tracing::debug!("action: {action:?}");
    let event = match action {
        ShortcutAction::StartPreset1 => start_preset_event(app, 0),
        ShortcutAction::StartPreset2 => start_preset_event(app, 1),
        ShortcutAction::StartPreset3 => start_preset_event(app, 2),
        ShortcutAction::TakeBreakNow => {
            EngineEvent::State(StateCommand::TakeBreakNow)
        }
        ShortcutAction::StopSession => {
            EngineEvent::State(StateCommand::StopSession)
        }
        ShortcutAction::ForceReset => EngineEvent::ForceReset,
        ShortcutAction::TogglePreview => {
            // Toggle decision uses the most recent published TimerState
            // (see SharedTimerState).  When SharedTimerState is missing
            // (very early startup), default to Idle → enter preview.
            let in_preview = app
                .try_state::<SharedTimerState>()
                .map(|s| matches!(s.get(), TimerState::Preview { .. }))
                .unwrap_or(false);
            if in_preview {
                EngineEvent::State(StateCommand::ExitPreview)
            } else {
                EngineEvent::State(StateCommand::EnterPreview)
            }
        }
        ShortcutAction::ToggleMainWindow => {
            // Pure window operation; bypasses the engine entirely.
            toggle_main_window_sync(app.clone(), WindowCommands::Close);
            return;
        }
    };
    forward_engine_sync(app.state::<EngineHandle>().tx.clone(), event);
}

fn start_preset_event(app: &AppHandle, idx: usize) -> EngineEvent {
    // Read durations at fire time so that `update_preset_session_durations`
    // does NOT need to re-register shortcuts.
    let durations = get_preset_session_durations(app.clone());
    EngineEvent::State(StateCommand::StartSession {
        target_duration_ms: durations[idx],
    })
}

// ── Public command surface ───────────────────────────────────────────

/// Read the current bindings (falls back to defaults if the store
/// entry is missing or malformed).
#[tauri::command]
pub fn get_shortcut_bindings(
    app: AppHandle,
) -> Result<ShortcutBindings, CommandError> {
    let store = app.store("config.json")?;
    Ok(store.get(STORE_KEY_SHORTCUT_BINDINGS)
        .and_then(|v| Some(serde_json::from_value(v)
            .inspect_err(|e| {
                tracing::warn!(?e, "stored shortcut bindings are failed to deserialization, using default");
                let value = serde_json::to_value(default_shortcut_bindings())
                    .expect("default_shortcut_bindings serialization is infalliible");
                store.set(STORE_KEY_SHORTCUT_BINDINGS, value);
            })
            .unwrap_or(default_shortcut_bindings())
        ))
        .expect("Defaults are set when setting up"))
}

/// Validate, apply, and persist a new set of bindings as a single
/// transaction.
#[tauri::command]
pub fn update_shortcut_bindings(
    app: AppHandle,
    bindings: ShortcutBindings,
) -> Result<(), CommandError> {
    apply_bindings(&app, &bindings)?;

    let store = app.store("config.json")?;
    let value = serde_json::to_value(&bindings).map_err(|e| {
        CommandError::BadArguments(format!(
            "failed to serialise shortcut bindings: {e}"
        ))
    })?;
    store.set(STORE_KEY_SHORTCUT_BINDINGS.to_string(), value);
    Ok(())
}

// ── Bootstrap ────────────────────────────────────────────────────────

/// Load bindings from the store and register them.
///
/// Idempotent: safe to call multiple times.  Manages
/// [`ShortcutRouter`] on first call.
pub fn setup_global_shortcuts(app: &AppHandle) {
    if app.try_state::<ShortcutRouter>().is_none() {
        app.manage(ShortcutRouter::default());
    }

    let bindings = match get_shortcut_bindings(app.clone()) {
        Ok(b) => b,
        Err(e) => {
            tracing::error!("Failed to load shortcut bindings: {e}");
            default_shortcut_bindings()
        }
    };

    if let Err(err) = apply_bindings(app, &bindings) {
        tracing::error!("Failed to register global shortcuts: {err}");
    }
}
