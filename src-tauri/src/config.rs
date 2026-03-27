use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    pub work_duration_minutes: u32,
    pub min_supported_work_duration_minutes: u32,
    pub prewarm_ratio_of_session: f32,
    pub evolution_ratio_of_session: f32,
    pub prewarm_cap_minutes: u32,
    pub evolution_cap_minutes: u32,
    pub recovery_duration_seconds: u32,
    pub target_warmth_kelvin: u32,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            work_duration_minutes: 50,
            min_supported_work_duration_minutes: 2,
            prewarm_ratio_of_session: 0.10,
            evolution_ratio_of_session: 0.10,
            prewarm_cap_minutes: 5,
            evolution_cap_minutes: 5,
            recovery_duration_seconds: 30,
            target_warmth_kelvin: 2500,
        }
    }
}
