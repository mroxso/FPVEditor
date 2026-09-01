//! Basic color-correction math. These are the CPU-reference forms of the
//! per-pixel operations the GPU shader applies during compositing.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorAdjustments {
    /// Stops, additive in log space: output = input * 2^exposure.
    pub exposure: f32,
    /// 1.0 = no change; >1.0 increases contrast around 0.5 mid-gray.
    pub contrast: f32,
    /// 1.0 = no change; 0.0 = grayscale; >1.0 boosts saturation.
    pub saturation: f32,
}

impl Default for ColorAdjustments {
    fn default() -> Self {
        Self {
            exposure: 0.0,
            contrast: 1.0,
            saturation: 1.0,
        }
    }
}

fn luma(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

pub fn apply(color: [f32; 3], adj: &ColorAdjustments) -> [f32; 3] {
    let exposed = color.map(|v| v * 2f32.powf(adj.exposure));
    let contrasted = exposed.map(|v| (v - 0.5) * adj.contrast + 0.5);
    let l = luma(contrasted);
    let saturated = contrasted.map(|v| l + (v - l) * adj.saturation);
    saturated.map(|v| v.clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_adjustments_are_a_no_op() {
        let c = [0.2, 0.5, 0.8];
        let out = apply(c, &ColorAdjustments::default());
        for i in 0..3 {
            assert!((out[i] - c[i]).abs() < 1e-6);
        }
    }

    #[test]
    fn positive_exposure_brightens() {
        let adj = ColorAdjustments {
            exposure: 1.0,
            ..Default::default()
        };
        let out = apply([0.1, 0.1, 0.1], &adj);
        assert!(out[0] > 0.19 && out[0] < 0.21);
    }

    #[test]
    fn zero_saturation_produces_a_gray_pixel() {
        let adj = ColorAdjustments {
            saturation: 0.0,
            ..Default::default()
        };
        let out = apply([0.9, 0.1, 0.1], &adj);
        assert!((out[0] - out[1]).abs() < 1e-6);
        assert!((out[1] - out[2]).abs() < 1e-6);
    }

    #[test]
    fn contrast_pushes_bright_pixels_brighter_and_dark_pixels_darker() {
        let adj = ColorAdjustments {
            contrast: 2.0,
            ..Default::default()
        };
        let bright = apply([0.8, 0.8, 0.8], &adj);
        let dark = apply([0.2, 0.2, 0.2], &adj);
        assert!(bright[0] > 0.8);
        assert!(dark[0] < 0.2);
    }

    #[test]
    fn output_is_always_clamped_to_valid_range() {
        let adj = ColorAdjustments {
            exposure: 10.0,
            ..Default::default()
        };
        let out = apply([0.9, 0.9, 0.9], &adj);
        for v in out {
            assert!((0.0..=1.0).contains(&v));
        }
    }
}
