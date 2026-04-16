use serde::{Deserialize, Serialize};

use super::{
    exponential_chase, normalized_sigmoid, Channel, ChannelStatus, ChannelType, ChannelValue, TickContext,
};
use crate::phase::SessionPhase;

const DEFAULT_TARGET_BRIGHTNESS: f32 = 0.6;
const DEFAULT_CURVE_STEEPNESS: f32 = 8.0;
const DEFAULT_SETTLE_DURATION_MS: u64 = 6000;
const NEUTRAL_BRIGHTNESS: f32 = 1.0;
const FORWARD_CHASE_SPEED: f32 = 4.0;
const SETTLE_CHASE_SPEED: f32 = 3.0;
const REVERSE_CHASE_SPEED: f32 = 3.5;
const SHUTDOWN_CHASE_SPEED: f32 = 3.0;
const INTENSITY_EPSILON: f32 = 0.001;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrightnessConfig {
    /// Brightness at full Sabi intensity (e.g. 0.6 = 60% of normal).
    pub target_brightness: f32,
    pub curve_steepness: f32,
    pub settle_duration_ms: u64,
}

impl Default for BrightnessConfig {
    fn default() -> Self {
        Self {
            target_brightness: DEFAULT_TARGET_BRIGHTNESS,
            curve_steepness: DEFAULT_CURVE_STEEPNESS,
            settle_duration_ms: DEFAULT_SETTLE_DURATION_MS,
        }
    }
}

pub struct BrightnessChannel {
    config: BrightnessConfig,
    current_intensity: f32,
    intensity_at_phase_entry: f32,
    is_shutting_down: bool,
}

impl BrightnessChannel {
    pub fn new(config: BrightnessConfig) -> Self {
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

    fn intensity_to_brightness(&self, intensity: f32) -> f32 {
        NEUTRAL_BRIGHTNESS + (self.config.target_brightness - NEUTRAL_BRIGHTNESS) * intensity
    }
}

impl Channel for BrightnessChannel {
    fn channel_type(&self) -> ChannelType {
        ChannelType::Brightness
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
        ChannelValue::Brightness(self.intensity_to_brightness(self.current_intensity))
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

    #[test]
    fn neutral_brightness_at_zero_intensity() {
        let ch = BrightnessChannel::new(BrightnessConfig::default());
        match ch.current_value() {
            ChannelValue::Brightness(b) => assert!((b - 1.0).abs() < 0.01),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn max_intensity_reaches_target() {
        let mut ch = BrightnessChannel::new(BrightnessConfig {
            target_brightness: 0.5,
            ..Default::default()
        });
        ch.current_intensity = 1.0;
        match ch.current_value() {
            ChannelValue::Brightness(b) => assert!((b - 0.5).abs() < 0.01),
            _ => panic!("wrong variant"),
        }
    }
}
