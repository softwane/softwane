use serde::{Deserialize, Serialize};

pub const DEFAULT_SETTLING_DURATION_MS: u64 = 5_000;
pub const DEFAULT_REVERSE_DURATION_MS: u64 = 2_000;

// ── Persistent config ────────────────────────────────────────────────

pub const STORE_KEY_TIMER: &str = "timer";

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct TimerConfig {
    pub settling_duration_ms: u64,
    pub reverse_duration_ms: u64,
}

impl Default for TimerConfig {
    fn default() -> Self {
        Self {
            settling_duration_ms: DEFAULT_SETTLING_DURATION_MS,
            reverse_duration_ms: DEFAULT_REVERSE_DURATION_MS,
        }
    }
}

pub fn store_defaults() -> Vec<(String, serde_json::Value)> {
    vec![(
        STORE_KEY_TIMER.into(),
        serde_json::to_value(TimerConfig::default())
            .expect("PersistentTimerConfig serialization is infallible"),
    )]
}
