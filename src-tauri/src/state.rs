//! Shared application state managed by [`tauri::App::manage`].
//!
//! Glue types exposed to all Tauri commands (and the tray/shortcuts
//! layer) so they can read information that is owned by the engine
//! thread without holding a reference to the engine itself.
//!
//! Each type:
//! - is `Clone + Send + Sync` (cheap clone, ref-counted internals)
//! - is single-writer (engine), multi-reader (commands, tray, shortcuts)
//!
//! Engine clones one and stores its private handle; it also passes a
//! clone to `app.manage(...)` so command handlers can `app.state::<T>()`
//! it.

use std::sync::{Arc, RwLock};

use crate::timer_state_machine::TimerState;

/// Shared snapshot of the latest `TimerState` published by the engine.
///
/// Engine writes on every `frame_events.just_transited`; readers see
/// the most recent state at the cost of a small read-lock acquisition.
/// `RwLock` over `Mutex` because reads dominate (any tray refresh, any
/// shortcut callback that needs to know the phase) and writes happen at
/// most a few times per minute.
#[derive(Debug, Clone)]
pub struct SharedTimerState(Arc<RwLock<TimerState>>);

impl SharedTimerState {
    pub fn new() -> Self {
        Self(Arc::new(RwLock::new(TimerState::Idle)))
    }

    /// Read the current state.
    ///
    /// Falls back to `TimerState::Idle` if the lock is poisoned (which
    /// can only happen if a writer panicked while holding the write
    /// lock — defensive default since callers should never block on
    /// poisoning during UI refresh).
    pub fn get(&self) -> TimerState {
        match self.0.read() {
            Ok(guard) => *guard,
            Err(poisoned) => {
                tracing::warn!(
                    "SharedTimerState read lock poisoned; returning Idle"
                );
                *poisoned.into_inner()
            }
        }
    }

    /// Overwrite the snapshot. Engine-only.
    pub fn set(&self, state: TimerState) {
        match self.0.write() {
            Ok(mut guard) => *guard = state,
            Err(poisoned) => {
                tracing::warn!(
                    "SharedTimerState write lock poisoned; recovering"
                );
                *poisoned.into_inner() = state;
            }
        }
    }
}
