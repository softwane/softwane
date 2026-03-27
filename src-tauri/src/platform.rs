use serde::Serialize;
use tauri::State;

use crate::engine::EffectSnapshot;

#[derive(Debug, Clone, Serialize)]
pub struct ApplyResult {
    pub applied: bool,
    pub backend: &'static str,
}

pub trait DisplayEffectApplier {
    fn apply(&self, snapshot: &EffectSnapshot) -> ApplyResult;
}

#[derive(Default)]
pub struct MockDisplayEffectApplier;

impl DisplayEffectApplier for MockDisplayEffectApplier {
    fn apply(&self, _snapshot: &EffectSnapshot) -> ApplyResult {
        ApplyResult {
            applied: false,
            backend: "mock",
        }
    }
}

pub fn apply_preview(
    applier: State<'_, MockDisplayEffectApplier>,
    snapshot: &EffectSnapshot,
) -> ApplyResult {
    applier.apply(snapshot)
}
