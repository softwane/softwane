use serde::Serialize;

use crate::channel::ChannelValue;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompositeFrame {
    pub saturation: f32,
    pub warmth_kelvin: u32,
    pub brightness: f32,
}

impl CompositeFrame {
    pub fn neutral() -> Self {
        Self {
            saturation: 1.0,
            warmth_kelvin: 6500,
            brightness: 1.0,
        }
    }

    pub fn is_neutral(&self) -> bool {
        (self.saturation - 1.0).abs() <= 0.001
            && self.warmth_kelvin >= 6498
            && (self.brightness - 1.0).abs() <= 0.001
    }
}

pub fn compose(values: &[ChannelValue]) -> CompositeFrame {
    let mut frame = CompositeFrame::neutral();
    for value in values {
        match value {
            ChannelValue::Saturation(s) => frame.saturation = frame.saturation.min(*s),
            ChannelValue::WarmthKelvin(k) => frame.warmth_kelvin = frame.warmth_kelvin.min(*k),
            ChannelValue::Brightness(b) => frame.brightness = frame.brightness.min(*b),
        }
    }
    frame
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_frame_is_neutral() {
        assert!(CompositeFrame::neutral().is_neutral());
    }

    #[test]
    fn compose_empty_is_neutral() {
        assert_eq!(compose(&[]), CompositeFrame::neutral());
    }

    #[test]
    fn compose_takes_min_per_dimension() {
        let values = vec![
            ChannelValue::Saturation(0.5),
            ChannelValue::Saturation(0.8),
            ChannelValue::WarmthKelvin(3000),
            ChannelValue::WarmthKelvin(4000),
            ChannelValue::Brightness(0.7),
        ];
        let frame = compose(&values);
        assert!((frame.saturation - 0.5).abs() < 0.001);
        assert_eq!(frame.warmth_kelvin, 3000);
        assert!((frame.brightness - 0.7).abs() < 0.001);
    }
}
