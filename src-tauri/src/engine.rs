use serde::Serialize;

use crate::config::SessionConfig;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Stable,
    Jnd,
    Evolution,
    Statue,
}

#[derive(Debug, Clone, Serialize)]
pub struct EffectSnapshot {
    pub phase: Phase,
    pub saturation: f32,
    pub grayscale: f32,
    pub warmth_kelvin: u32,
}

pub fn sigmoid(x: f32, steepness: f32) -> f32 {
    1.0 / (1.0 + f32::exp(-steepness * (x - 0.5)))
}

fn resolve_cue_windows(config: &SessionConfig, session_duration_minutes: f32) -> (f32, f32) {
    let capped_duration = session_duration_minutes
        .max(config.min_supported_work_duration_minutes as f32)
        .min(config.work_duration_minutes as f32);
    let prewarm =
        (capped_duration * config.prewarm_ratio_of_session).min(config.prewarm_cap_minutes as f32);
    let evolution = (capped_duration * config.evolution_ratio_of_session)
        .min(config.evolution_cap_minutes as f32);

    (prewarm, evolution)
}

pub fn calculate_snapshot(
    config: &SessionConfig,
    session_duration_minutes: f32,
    remaining_minutes: f32,
) -> EffectSnapshot {
    let (prewarm, evolution) = resolve_cue_windows(config, session_duration_minutes);
    let stable_threshold = prewarm + evolution;

    if remaining_minutes > stable_threshold {
        return EffectSnapshot {
            phase: Phase::Stable,
            saturation: 1.0,
            grayscale: 0.0,
            warmth_kelvin: 6500,
        };
    }

    if remaining_minutes > evolution {
        let jnd_remaining = remaining_minutes - evolution;
        let progress = ((prewarm - jnd_remaining) / prewarm.max(0.001)).clamp(0.0, 1.0);
        let curve = sigmoid(progress, 10.0);

        return EffectSnapshot {
            phase: Phase::Jnd,
            saturation: 1.0 - 0.28 * curve,
            grayscale: 0.18 * curve,
            warmth_kelvin: (6500.0 - 1200.0 * curve) as u32,
        };
    }

    let progress = ((evolution - remaining_minutes.max(0.0)) / evolution.max(0.1)).clamp(0.0, 1.0);
    let curve = sigmoid(progress, 10.0);

    EffectSnapshot {
        phase: if remaining_minutes <= 0.0 {
            Phase::Statue
        } else {
            Phase::Evolution
        },
        saturation: 0.72 - 0.54 * curve,
        grayscale: 0.18 + 0.74 * curve,
        warmth_kelvin: (5300.0 - 2800.0 * curve) as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::{calculate_snapshot, Phase};
    use crate::config::SessionConfig;

    #[test]
    fn keeps_stable_outside_dynamic_cue_window() {
        let snapshot = calculate_snapshot(&SessionConfig::default(), 50.0, 41.0);
        assert_eq!(snapshot.phase, Phase::Stable);
        assert_eq!(snapshot.saturation, 1.0);
    }

    #[test]
    fn enters_jnd_inside_scaled_prewarm_window() {
        let snapshot = calculate_snapshot(&SessionConfig::default(), 25.0, 4.0);
        assert_eq!(snapshot.phase, Phase::Jnd);
        assert!(snapshot.saturation < 1.0);
    }

    #[test]
    fn reaches_evolution_for_two_minute_sessions() {
        let snapshot = calculate_snapshot(&SessionConfig::default(), 2.0, 0.1);
        assert_eq!(snapshot.phase, Phase::Evolution);
        assert!(snapshot.grayscale > 0.18);
    }

    #[test]
    fn caps_cue_windows_after_fifty_minutes() {
        let snapshot = calculate_snapshot(&SessionConfig::default(), 90.0, 11.0);
        assert_eq!(snapshot.phase, Phase::Stable);
    }

    #[test]
    fn reaches_statue_at_zero() {
        let snapshot = calculate_snapshot(&SessionConfig::default(), 50.0, 0.0);
        assert_eq!(snapshot.phase, Phase::Statue);
        assert!(snapshot.grayscale > 0.5);
    }
}
