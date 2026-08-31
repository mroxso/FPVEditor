//! Ties gyro integration, smoothing, and horizon lock together into a
//! per-frame correction transform, driven by a [`fpv_core::StabilizationProfile`]
//! (the settings a `Command::ApplyStabilization` attaches to a clip).

use fpv_core::StabilizationProfile;

use crate::gyro::{GyroSample, GyroTrack};
use crate::horizon::horizon_lock_correction;
use crate::quaternion::Quaternion;
use crate::smoothing::{correction_quaternion, smooth_track};

/// The transform to apply when rendering a given frame/scanline: a
/// rotational warp plus a uniform crop/zoom factor needed to hide the edges
/// that warp reveals.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameTransform {
    pub rotation: Quaternion,
    /// >= 1.0: how much to zoom in / crop to hide the warp's edges.
    pub fov_scale: f32,
}

pub struct StabilizationEngine {
    raw: GyroTrack,
    smoothed_smoothed_samples: Vec<crate::gyro::OrientationSample>,
    profile: StabilizationProfile,
}

impl StabilizationEngine {
    pub fn new(
        gyro_samples: &[GyroSample],
        profile: StabilizationProfile,
    ) -> Result<Self, crate::gyro::GyroError> {
        let raw = GyroTrack::from_samples(gyro_samples)?;
        // Re-derive a plain sample list (same timestamps as input) to smooth.
        let raw_samples: Vec<_> = gyro_samples
            .iter()
            .map(|s| crate::gyro::OrientationSample {
                timestamp_us: s.timestamp_us,
                orientation: raw.orientation_at(s.timestamp_us),
            })
            .collect();
        let smoothed_samples = smooth_track(&raw_samples, profile.smoothness);
        Ok(Self {
            raw,
            smoothed_smoothed_samples: smoothed_samples,
            profile,
        })
    }

    fn smoothed_orientation_at(&self, timestamp_us: i64) -> Quaternion {
        interpolate(&self.smoothed_smoothed_samples, timestamp_us)
    }

    /// Compute the correction transform for a frame captured at
    /// `timestamp_us`, optionally offset per-scanline for rolling-shutter
    /// correction (pass the scanline's own capture time, already offset by
    /// `scanline_fraction * readout_time_us`).
    pub fn frame_transform(&self, timestamp_us: i64) -> FrameTransform {
        let raw_orientation = self.raw.orientation_at(timestamp_us);
        let smoothed_orientation = self.smoothed_orientation_at(timestamp_us);

        let strength = self.profile.strength.clamp(0.0, 1.0) as f64;
        let full_correction = correction_quaternion(raw_orientation, smoothed_orientation);
        let mut rotation = Quaternion::slerp(Quaternion::IDENTITY, full_correction, strength);

        if self.profile.horizon_lock {
            let horizon = horizon_lock_correction(raw_orientation, 1.0);
            rotation = (horizon * rotation).normalized();
        }

        FrameTransform {
            rotation,
            fov_scale: 1.0 + self.profile.dynamic_fov.clamp(0.0, 1.0),
        }
    }

    /// Rolling-shutter-corrected transform for one scanline of a frame.
    /// `readout_time_us` is the total time to scan the whole sensor;
    /// `scanline_fraction` is 0.0 (top) .. 1.0 (bottom).
    pub fn scanline_transform(
        &self,
        frame_timestamp_us: i64,
        scanline_fraction: f64,
        readout_time_us: i64,
    ) -> FrameTransform {
        let offset = (readout_time_us as f64 * scanline_fraction.clamp(0.0, 1.0)) as i64;
        self.frame_transform(frame_timestamp_us + offset)
    }
}

fn interpolate(samples: &[crate::gyro::OrientationSample], timestamp_us: i64) -> Quaternion {
    if samples.is_empty() {
        return Quaternion::IDENTITY;
    }
    if timestamp_us <= samples[0].timestamp_us {
        return samples[0].orientation;
    }
    let last = samples.len() - 1;
    if timestamp_us >= samples[last].timestamp_us {
        return samples[last].orientation;
    }
    let idx = samples
        .partition_point(|s| s.timestamp_us <= timestamp_us)
        .saturating_sub(1)
        .min(last.saturating_sub(1));
    let a = samples[idx];
    let b = samples[idx + 1];
    let span = (b.timestamp_us - a.timestamp_us).max(1) as f64;
    let t = (timestamp_us - a.timestamp_us) as f64 / span;
    Quaternion::slerp(a.orientation, b.orientation, t.clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    fn shaky_samples() -> Vec<GyroSample> {
        let mut samples = Vec::new();
        let mut t = 0i64;
        for i in 0..60 {
            // High-frequency, high-rate oscillation "shake" (e.g. prop wash).
            let rate = if i % 2 == 0 { 50.0 } else { -50.0 };
            samples.push(GyroSample {
                timestamp_us: t,
                gyro: [0.0, 0.0, rate],
            });
            t += 8_333; // ~120Hz gyro sample rate
        }
        samples
    }

    #[test]
    fn zero_strength_profile_produces_identity_rotation() {
        let profile = StabilizationProfile {
            smoothness: 0.8,
            strength: 0.0,
            horizon_lock: false,
            dynamic_fov: 0.1,
        };
        let engine = StabilizationEngine::new(&shaky_samples(), profile).unwrap();
        let t = engine.frame_transform(200_000);
        assert!((t.rotation.w.abs() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn full_strength_with_heavy_smoothing_reduces_jitter_amplitude() {
        let samples = shaky_samples();
        let profile = StabilizationProfile {
            smoothness: 1.0,
            strength: 1.0,
            horizon_lock: false,
            dynamic_fov: 0.1,
        };
        let engine = StabilizationEngine::new(&samples, profile).unwrap();

        // Sample at an odd gyro index (t=8_333us), where the raw path has
        // swung away from center but the heavily-smoothed path has barely
        // moved — the correction rotation's magnitude should be non-trivial,
        // proving strength=1 is not a no-op.
        let t = engine.frame_transform(8_333);
        assert!(t.rotation.w.abs() < 0.9999, "correction should be non-identity, got w={}", t.rotation.w);
    }

    #[test]
    fn dynamic_fov_maps_linearly_to_fov_scale() {
        let profile = StabilizationProfile {
            smoothness: 0.5,
            strength: 1.0,
            horizon_lock: false,
            dynamic_fov: 0.2,
        };
        let engine = StabilizationEngine::new(&shaky_samples(), profile).unwrap();
        let t = engine.frame_transform(100_000);
        assert!((t.fov_scale - 1.2).abs() < 1e-6);
    }

    #[test]
    fn horizon_lock_zeroes_roll_component_of_output_rotation_combined_with_raw() {
        // Pure-roll gyro: horizon lock at strength=1 should fully cancel it
        // regardless of the smoothing/strength settings for translation-free roll.
        let mut t = 0i64;
        let mut samples = Vec::new();
        for _ in 0..10 {
            samples.push(GyroSample {
                timestamp_us: t,
                gyro: [PI / 2.0, 0.0, 0.0],
            });
            t += 16_667;
        }
        let profile = StabilizationProfile {
            smoothness: 0.0,
            strength: 0.0,
            horizon_lock: true,
            dynamic_fov: 0.0,
        };
        let engine = StabilizationEngine::new(&samples, profile).unwrap();
        let transform = engine.frame_transform(t - 16_667);
        let raw = engine.raw.orientation_at(t - 16_667);
        let combined = (transform.rotation * raw).normalized();
        let (roll, _, _) = combined.to_euler_rpy();
        assert!(roll.abs() < 1e-6, "roll={roll}");
    }

    #[test]
    fn scanline_transform_at_zero_readout_time_equals_frame_transform() {
        let profile = StabilizationProfile::default();
        let engine = StabilizationEngine::new(&shaky_samples(), profile).unwrap();
        let a = engine.frame_transform(100_000);
        let b = engine.scanline_transform(100_000, 1.0, 0);
        assert_eq!(a, b);
    }
}
