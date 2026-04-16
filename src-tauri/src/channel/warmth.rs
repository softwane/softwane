use serde::{Deserialize, Serialize};

use super::{
    exponential_chase,
    Channel, ChannelStatus, TickContext,
    ChannelType, ChannelValue,
    curve_functions, ChannelCalculatorHelper,
};
use crate::{channel::SharedChannelConfig, phase::SessionPhase};

const DEFAULT_TARGET_KELVIN: u32 = 2500;
const DEFAULT_CURVE_STEEPNESS: f32 = 10.0;
const DEFAULT_FORWARD_REMIND_BEGIN_RATIO: f64 = 0.5;
const DEFAULT_SETTLE_DURATION_MS: u64 = 5000;
const DEFAULT_REVERSE_DURATION_MS: u64 = 5000;
const NEUTRAL_KELVIN: u32 = 6500;
const FORWARD_CHASE_SPEED: f32 = 4.0;
const SETTLE_CHASE_SPEED: f32 = 3.0;
const REVERSE_CHASE_SPEED: f32 = 3.5;
const SHUTDOWN_CHASE_SPEED: f32 = 3.0;
const INTENSITY_EPSILON: f32 = 0.001;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarmthConfig {
    /// Color temperature at full Sabi intensity (e.g. 2500K).
    pub target_kelvin: u32,
    pub shared_config: SharedChannelConfig,
    pub curve_steepness: f32,
}

impl Default for WarmthConfig {
    fn default() -> Self {
        Self {
            target_kelvin: DEFAULT_TARGET_KELVIN,
            curve_steepness: DEFAULT_CURVE_STEEPNESS,
            shared_config: SharedChannelConfig { 
                forward_remind_begin_ratio: DEFAULT_FORWARD_REMIND_BEGIN_RATIO,
                settle_duration_ms: DEFAULT_SETTLE_DURATION_MS,
                reverse_duration_ms: DEFAULT_REVERSE_DURATION_MS,
            }
        }
    }
}

pub struct WarmthChannel {
    config: WarmthConfig,
    current_intensity: f32,
    intensity_at_phase_entry: f32,
    is_shutting_down: bool,
}

impl WarmthChannel {
    pub fn new(config: WarmthConfig) -> Self {
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
        curve_functions::normalized_sigmoid(progress, self.config.curve_steepness)
    }

    fn intensity_to_kelvin(&self, intensity: f32) -> u32 {
        let neutral = NEUTRAL_KELVIN as f32;
        let target = self.config.target_kelvin as f32;
        (neutral + (target - neutral) * intensity).round() as u32
    }
}

impl Channel for WarmthChannel {
    fn channel_type(&self) -> ChannelType {
        ChannelType::Warmth
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

        // 并行了两套逻辑，第一套是平滑函数，第二套是指数逼近作为target的前者
        // TODO：将其压为同一种
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
        ChannelValue::WarmthKelvin(self.intensity_to_kelvin(self.current_intensity))
    }

    fn current_intensity(&self) -> f32 {
        self.current_intensity
    }

    fn shutdown(&mut self) {
        self.is_shutting_down = true;
    }

    // TODO: 不应当由引擎显式告诉Channel什么时候保存状态，
    // 要么次次保存，要么根据传入的TickContext
    fn snapshot_intensity_for_phase_entry(&mut self) {
        self.intensity_at_phase_entry = self.current_intensity;
    }

    fn calculate_curve_intensity(
        &self,
        steepness: f32,
        progress: f32,  // should in [0, 1]
        begin_intensity: f32,
        target_intensity: f32,
    ) -> f32 {
        let intensity_ratio = curve_functions::normalized_sigmoid(progress, steepness);
        begin_intensity + (target_intensity - begin_intensity) * intensity_ratio
    }
}

pub struct WarmthChannelCalculator {
    curve_parameters: curve_functions::CurveParameters,
    curve_begin_time_ms: u64,
    curve_begin_value: ChannelValue,
    target_duration_ms: u64,
    target_value: ChannelValue,
}

impl WarmthChannelCalculator {
    pub fn new() -> Self {
        Self {
            curve_parameters: curve_functions::CurveParameters::NormalizedSigmoid { steepness: DEFAULT_CURVE_STEEPNESS },
            curve_begin_time_ms: 0,
            curve_begin_value: ChannelValue::WarmthKelvin(NEUTRAL_KELVIN),
            target_duration_ms: 0,
            target_value: ChannelValue::WarmthKelvin(DEFAULT_TARGET_KELVIN),
        }
    }
}

impl ChannelCalculatorHelper for WarmthChannelCalculator {
    fn calculate_curve_value(
        &self,
        elapsed_ms: u64,
    ) -> ChannelValue {
        if elapsed_ms < self.curve_begin_time_ms {
            return self.curve_begin_value;
        }
        if elapsed_ms >= self.target_duration_ms {
            return self.target_value;
        }
        let progress = (elapsed_ms - self.curve_begin_time_ms) as f32 / (self.target_duration_ms - self.curve_begin_time_ms) as f32;
        let curve_intensity = match self.curve_parameters {
            curve_functions::CurveParameters::NormalizedSigmoid { steepness } => {
                curve_functions::normalized_sigmoid(progress, steepness)
            }
        };
        self.curve_begin_value + (self.target_value - self.curve_begin_value) * curve_intensity
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_warmth_at_zero_intensity() {
        let ch = WarmthChannel::new(WarmthConfig::default());
        match ch.current_value() {
            ChannelValue::WarmthKelvin(k) => assert_eq!(k, NEUTRAL_KELVIN),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn max_intensity_reaches_target_kelvin() {
        let mut ch = WarmthChannel::new(WarmthConfig {
            target_kelvin: 2000,
            ..Default::default()
        });
        ch.current_intensity = 1.0;
        match ch.current_value() {
            ChannelValue::WarmthKelvin(k) => assert_eq!(k, 2000),
            _ => panic!("wrong variant"),
        }
    }
}
