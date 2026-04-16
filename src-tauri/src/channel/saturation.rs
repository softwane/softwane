use serde::{Deserialize, Serialize};

use super::{
    exponential_chase, normalized_sigmoid, Channel, ChannelStatus, ChannelType, ChannelValue, TickContext,
};
use crate::phase::SessionPhase;

const DEFAULT_TARGET_SATURATION: f32 = 0.18;
const DEFAULT_CURVE_STEEPNESS: f32 = 10.0;
const DEFAULT_SETTLE_DURATION_MS: u64 = 5000;
const NEUTRAL_SATURATION: f32 = 1.0;
/// Chase speed during normal Forward ticking (units: 1/s).
const FORWARD_CHASE_SPEED: f32 = 4.0;
/// Chase speed during Settling (fast ramp to max).
const SETTLE_CHASE_SPEED: f32 = 3.0;
/// Chase speed during Reverse (fade back to neutral).
const REVERSE_CHASE_SPEED: f32 = 3.5;
/// Chase speed during shutdown (fade out).
const SHUTDOWN_CHASE_SPEED: f32 = 3.0;
const INTENSITY_EPSILON: f32 = 0.001;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaturationConfig {
    /// Saturation level at full Sabi intensity (e.g. 0.18 = near-grayscale).
    pub target_saturation: f32,
    /// Sigmoid steepness parameter.
    pub curve_steepness: f32,
    /// Duration in ms for Settling ramp.
    pub settle_duration_ms: u64,
}

impl Default for SaturationConfig {
    fn default() -> Self {
        Self {
            target_saturation: DEFAULT_TARGET_SATURATION,
            curve_steepness: DEFAULT_CURVE_STEEPNESS,
            settle_duration_ms: DEFAULT_SETTLE_DURATION_MS,
        }
    }
}

pub struct SaturationChannel {
    config: SaturationConfig,
    /// Normalized effect intensity: 0.0 = no effect, 1.0 = full effect.
    current_intensity: f32,
    /// Intensity snapshot captured when entering Reverse or Settling.
    intensity_at_phase_entry: f32,
    is_shutting_down: bool,
}

impl SaturationChannel {
    pub fn new(config: SaturationConfig) -> Self {
        Self {
            config,
            current_intensity: 0.0,
            intensity_at_phase_entry: 0.0,
            is_shutting_down: false,
        }
    }

    fn compute_forward_target(&self, elapsed_ms: u64, target_duration_ms: u64) -> f32 {
        let target = target_duration_ms.max(1) as f32;
        let progress = (elapsed_ms as f32 / target).clamp(0.0, 1.0);
        normalized_sigmoid(progress, self.config.curve_steepness)
    }

    fn intensity_to_saturation(&self, intensity: f32) -> f32 {
        NEUTRAL_SATURATION + (self.config.target_saturation - NEUTRAL_SATURATION) * intensity
    }
}

impl Channel for SaturationChannel {
    fn channel_type(&self) -> ChannelType {
        ChannelType::Saturation
    }

    fn tick(&mut self, ctx: &TickContext) -> ChannelStatus {
        let dt_s = ctx.dt_ms as f32 / 1000.0;

        if self.is_shutting_down {
            self.current_intensity =
                exponential_chase(self.current_intensity, 0.0, SHUTDOWN_CHASE_SPEED, dt_s);
            if self.current_intensity <= INTENSITY_EPSILON {
                self.current_intensity = 0.0;
                return ChannelStatus::Dead;
            }
            return ChannelStatus::ShuttingDown;
        }

        let target = match ctx.phase {
            SessionPhase::Idle => 0.0,
            SessionPhase::Forward {
                elapsed_ms,
                target_duration_ms,
            } => self.compute_forward_target(*elapsed_ms, *target_duration_ms),
            SessionPhase::Settling { .. } => 1.0,
            SessionPhase::Sabi => 1.0,
            SessionPhase::Reverse {
                elapsed_ms,
                max_duration_ms,
            } => {
                let max = (*max_duration_ms).max(1) as f32;
                let scaled_max = max * self.intensity_at_phase_entry.max(0.05);
                let progress = (*elapsed_ms as f32 / scaled_max).clamp(0.0, 1.0);
                let eased = super::ease_in_out(progress);
                (self.intensity_at_phase_entry * (1.0 - eased)).max(0.0)
            }
        };

        let chase_speed = match ctx.phase {
            SessionPhase::Settling { .. } => SETTLE_CHASE_SPEED,
            SessionPhase::Reverse { .. } => REVERSE_CHASE_SPEED,
            _ => FORWARD_CHASE_SPEED,
        };

        self.current_intensity = exponential_chase(self.current_intensity, target, chase_speed, dt_s);

        if self.current_intensity <= INTENSITY_EPSILON && target <= INTENSITY_EPSILON {
            self.current_intensity = 0.0;
        }
        if (self.current_intensity - 1.0).abs() <= INTENSITY_EPSILON && target >= 1.0 - INTENSITY_EPSILON {
            self.current_intensity = 1.0;
        }

        ChannelStatus::Active
    }

    fn current_value(&self) -> ChannelValue {
        ChannelValue::Saturation(self.intensity_to_saturation(self.current_intensity))
    }

    fn current_intensity(&self) -> f32 {
        self.current_intensity
    }

    fn shutdown(&mut self) {
        self.is_shutting_down = true;
    }

    fn snapshot_intensity_for_phase_entry(&mut self) {
        self.intensity_at_phase_entry = self.current_intensity;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn forward_ctx(elapsed_ms: u64, target_ms: u64) -> SessionPhase {
        SessionPhase::Forward {
            elapsed_ms,
            target_duration_ms: target_ms,
        }
    }

    #[test]
    fn starts_at_zero_intensity() {
        let ch = SaturationChannel::new(SaturationConfig::default());
        assert!((ch.current_intensity() - 0.0).abs() < 0.001);
        match ch.current_value() {
            ChannelValue::Saturation(s) => assert!((s - 1.0).abs() < 0.01),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn forward_increases_intensity() {
        let mut ch = SaturationChannel::new(SaturationConfig::default());
        let phase = forward_ctx(2_900_000, 3_000_000);
        let ctx = TickContext {
            phase: &phase,
            dt_ms: 250,
        };
        for _ in 0..40 {
            ch.tick(&ctx);
        }
        assert!(ch.current_intensity() > 0.3);
    }

    #[test]
    fn shutdown_fades_to_dead() {
        let mut ch = SaturationChannel::new(SaturationConfig::default());
        ch.current_intensity = 0.8;
        ch.shutdown();
        let phase = SessionPhase::Sabi;
        for _ in 0..200 {
            let ctx = TickContext {
                phase: &phase,
                dt_ms: 33,
            };
            let status = ch.tick(&ctx);
            if status == ChannelStatus::Dead {
                return;
            }
        }
        panic!("channel did not reach Dead state");
    }
}
