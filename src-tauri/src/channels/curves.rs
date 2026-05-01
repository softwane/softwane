use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CurveParameters {
    NormalizedSigmoid { steepness: f64 },
}

pub const DEFAULT_NORMALIZED_SIGMOID_PARAMETERS: CurveParameters 
    = CurveParameters::NormalizedSigmoid { steepness: 10.0 };

fn sigmoid(x: f64, steepness: f64) -> f64 {
    1.0 / (1.0 + f64::exp(-steepness * (x - 0.5)))
}

pub fn normalized_sigmoid(x: f64, steepness: f64) -> f64 {
    debug_assert!(0.0 <= x && x <= 1.0, "x must be in [0, 1], but got {}", x);
    let low = sigmoid(0.0, steepness);
    let high = sigmoid(1.0, steepness);
    let raw = sigmoid(x, steepness);
    let result = ((raw.powi(2) - low.powi(2)) * (high + low))
        / ((raw + low) * (high.powi(2) - low.powi(2)));
    result.clamp(0.0, 1.0)
}
