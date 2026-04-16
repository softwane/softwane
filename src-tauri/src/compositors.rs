use crate::utils::{
    Update,
    ColorTransformMatrix,
};
use crate::channels::{
    ChannelValue,
    ChannelType,
    SENSORY_CHANNELS_COUNT,
    SENSORY_CHANNEL_TYPES,
};


#[derive(Debug, Clone, Copy)]
pub struct CompositeSensoryFrame {
    color_transform_matrix: Update<ColorTransformMatrix>,
}

impl Default for CompositeSensoryFrame {
    fn default() -> Self {
        Self {
            color_transform_matrix: Update::Changed(ColorTransformMatrix::identity()),
        }
    }
}

impl CompositeSensoryFrame {
    pub fn color_transform_matrix(&self) -> &Update<ColorTransformMatrix> {
        &self.color_transform_matrix
    }
}

#[derive(Debug, Clone)]
pub struct SensoryCompositor {
    channels_value: [Update<ChannelValue>; SENSORY_CHANNELS_COUNT],
    composite_frame: CompositeSensoryFrame,
}

impl Default for SensoryCompositor {
    fn default() -> Self {
        Self {
            channels_value: SENSORY_CHANNEL_TYPES.map(|channel_type| Update::Changed(channel_type.neutral_value())),
            composite_frame: CompositeSensoryFrame::default(),
        }
    }
}

impl std::ops::Index<usize> for SensoryCompositor {
    type Output = Update<ChannelValue>;
    fn index(&self, index: usize) -> &Self::Output {
        &self.channels_value[index]
    }
}
impl std::ops::IndexMut<usize> for SensoryCompositor {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.channels_value[index]
    }
}

impl std::ops::Index<ChannelType> for SensoryCompositor {
    type Output = Update<ChannelValue>;
    fn index(&self, channel_type: ChannelType) -> &Self::Output {
        &self.channels_value[channel_type as usize]
    }
}
impl std::ops::IndexMut<ChannelType> for SensoryCompositor {
    fn index_mut(&mut self, channel_type: ChannelType) -> &mut Self::Output {
        &mut self.channels_value[channel_type as usize]
    }
}

type R = f64;
type G = f64;
type B = f64;

impl SensoryCompositor {
    fn compose(&mut self){
        self.compose_color_transform_matrix();
    }

    fn composite_frame(&self) -> CompositeSensoryFrame {
        self.composite_frame
    }
}

// Color transformation functions
impl SensoryCompositor {    
    fn compose_color_transform_matrix(&mut self) {
        if !self[ChannelType::Saturation].is_changed()
        && !self[ChannelType::ColorTemperature].is_changed()
        && !self[ChannelType::Brightness].is_changed() {
            self.composite_frame.color_transform_matrix = 
                Update::Unchanged(*self.composite_frame.color_transform_matrix.get_value());
            return;
        }

        let s = match self[ChannelType::Saturation].get_value() {
            ChannelValue::Saturation(s) => *s,
            _ => unreachable!(),
        };
        let saturation_matrix = Self::saturation_to_matrix(s);

        let c_kelvin = match self[ChannelType::ColorTemperature].get_value() {
            ChannelValue::ColorKelvin(t) => *t,
            _ => unreachable!(),
        };
        let color_temperature_matrix = Self::color_temperature_to_matrix(c_kelvin);

        let b = match self[ChannelType::Brightness].get_value() {
            ChannelValue::Brightness(b) => *b,
            _ => unreachable!(),
        };

        self.composite_frame.color_transform_matrix = 
            Update::Changed(b * (color_temperature_matrix * saturation_matrix));
    }

    fn saturation_to_matrix(s: f64) -> ColorTransformMatrix {
        // Rec. 709 luminance weights
        const LR: f64 = 0.2126;
        const LG: f64 = 0.7152;
        const LB: f64 = 0.0722;

        /// Fully desaturated color transform matrix (saturation = 0).
        /// Maps any RGB input to its perceived luminance using Rec. 709 weights.
        /// Column-major storage: each inner array is one column.
        const GRAYSCALE_COLOR_TRANSFORM_MATRIX: ColorTransformMatrix =
            ColorTransformMatrix::new(
                LR, LG, LB, 0.0, 0.0,
                LR, LG, LB, 0.0, 0.0,
                LR, LG, LB, 0.0, 0.0,
                0.0, 0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 0.0, 1.0,
            );
        
        // Lerp: identity (s=1, full color) ↔ grayscale (s=0, desaturated)
        GRAYSCALE_COLOR_TRANSFORM_MATRIX * (1.0 - s)
            + ColorTransformMatrix::identity() * s
    }

    fn color_temperature_to_matrix(c_kelvin: u32) -> ColorTransformMatrix {
        // Use 6500 K as the white point: the algorithm's R channel is already
        // pinned to 255 there (t=65 ≤ 66), while G and B are at their natural
        // peak values just below 255. Dividing by these values normalises every
        // channel so that 6500 K yields identity scaling (all coefficients = 1).
        const WHITE_POINT_K: u32 = match ChannelType::ColorTemperature.neutral_value() {
            ChannelValue::ColorKelvin(k) => k,
            _ => unreachable!(),
        };
        const WHITE_POINT_RGB: (R, G, B) = SensoryCompositor::kelvin_to_rgb(WHITE_POINT_K);

        let (r,  g,  b)  = SensoryCompositor::kelvin_to_rgb(c_kelvin);
        // Attenuation coefficients in [0, 1] relative to the 6500 K white point.
        // Values above the white point are clamped to 1.0 (no amplification).
        let mr = (r / WHITE_POINT_RGB.0).clamp(0.0, 1.0);
        let mg = (g / WHITE_POINT_RGB.1).clamp(0.0, 1.0);
        let mb = (b / WHITE_POINT_RGB.2).clamp(0.0, 1.0);

        // Diagonal 5x5 matrix – column-major (nalgebra): each inner array is a column.
        // m11=mr (R attenuation), m22=mg (G), m33=mb (B), m44=1 (A), m55=1 (const)
        ColorTransformMatrix::new(
            mr,  0.0, 0.0, 0.0, 0.0,
            0.0, mg,  0.0, 0.0, 0.0,
            0.0, 0.0, mb,  0.0, 0.0,
            0.0, 0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 0.0, 1.0,
        )
    }

    /// Tanner Helland algorithm: approximated blackbody RGB for a given temperature.
    /// Returns channel values on a 0–255 scale.
    /// Valid range: 1000 K – 40000 K. See https://tannerhelland.com/2012/09/18/convert-temperature-rgb-algorithm-code.html
    // TODO: update the fitting model, referring to "/refs/Black body temperature.nb".
    // It's from https://github.com/rgatkinson/ParticleDmxNeopixel
    fn kelvin_to_rgb(kelvin: u32) -> (R, G, B) {
        let t = kelvin.clamp(1000, 40000);

        let r = if t <= 6600 {
            255.0
        } else {
            (329.698727446 * (((t - 6000)/100) as f64).powf(-0.1332047592)).clamp(0.0, 255.0)
        };

        let g = if t <= 6600 {
            (99.4708025861 * (t/100 as f64).ln() - 161.1195681661).clamp(0.0, 255.0)
        } else {
            (288.1221695283 * (((t - 6000)/100) as f64).powf(-0.0755148492)).clamp(0.0, 255.0)
        };

        // The pseudocode boundary for blue is 66 (6600 K), but the algorithm's
        // actual white point sits at 6500–6600 K. At 6500 K (t=65), blue is still
        // computed via the log formula rather than pinned to 255.
        let b = if t >= 6600 {
            255.0
        } else if t <= 1900 {
            0.0
        } else {
            (138.5177312231 * (((t - 1000)/100) as f64).ln() - 305.0447927307).clamp(0.0, 255.0)
        };

        (r, g, b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kelvin_to_rgb() {
        let (r, g, b) = SensoryCompositor::kelvin_to_rgb(6500);
        let r_abs_diff = (r - 255.0).abs();
        let g_abs_diff = (g - 254.1101).abs();
        let b_abs_diff = (b - 250.0419).abs();
        assert!(r_abs_diff < 0.001);
        assert!(g_abs_diff < 0.001);
        assert!(b_abs_diff < 0.001);
        let (_, _, b) = SensoryCompositor::kelvin_to_rgb(1900);
        let b_abs_diff = (b - 0.0).abs();
        assert!(b_abs_diff < 0.001);
    }

    // TODO: Add tests for the SensoryCompositor
}