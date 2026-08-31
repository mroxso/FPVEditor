//! Low-pass filtering of an orientation track (the "smoothness" knob),
//! following the Gyroflow-style approach of an exponential moving average
//! over quaternions via repeated slerp, rather than filtering Euler angles
//! directly (which suffers from gimbal-lock artifacts).

use crate::gyro::OrientationSample;
use crate::quaternion::Quaternion;

/// Smooth a raw orientation track. `smoothness` in `[0, 1]`: 0 disables
/// smoothing (output == input), 1 uses a ~2 second time constant (very
/// heavy smoothing, large virtual-FOV crop needed to hide the resulting
/// lag).
pub fn smooth_track(raw: &[OrientationSample], smoothness: f32) -> Vec<OrientationSample> {
    let smoothness = smoothness.clamp(0.0, 1.0) as f64;
    if raw.is_empty() {
        return Vec::new();
    }
    let time_constant_s = smoothness * 2.0;
    let mut out = Vec::with_capacity(raw.len());
    let mut smoothed = raw[0].orientation;
    out.push(OrientationSample {
        timestamp_us: raw[0].timestamp_us,
        orientation: smoothed,
    });
    for window in raw.windows(2) {
        let (prev, cur) = (window[0], window[1]);
        let dt = (cur.timestamp_us - prev.timestamp_us) as f64 / 1_000_000.0;
        let alpha = if time_constant_s <= 1e-9 {
            1.0
        } else {
            1.0 - (-dt / time_constant_s).exp()
        };
        smoothed = Quaternion::slerp(smoothed, cur.orientation, alpha.clamp(0.0, 1.0));
        out.push(OrientationSample {
            timestamp_us: cur.timestamp_us,
            orientation: smoothed,
        });
    }
    out
}

/// The rotation that must be applied to a captured frame to make it appear
/// as if the camera had followed the smoothed path instead of the raw one.
pub fn correction_quaternion(raw: Quaternion, smoothed: Quaternion) -> Quaternion {
    (smoothed.inverse() * raw).normalized()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quaternion::Quaternion;
    use std::f64::consts::PI;

    fn track_with_step_motion() -> Vec<OrientationSample> {
        // Held still, then a sudden 90-degree yaw "shake" that snaps back —
        // the kind of high-frequency jitter stabilization should smooth out.
        let mut samples = Vec::new();
        let mut t = 0i64;
        let mut o = Quaternion::IDENTITY;
        for _ in 0..10 {
            samples.push(OrientationSample {
                timestamp_us: t,
                orientation: o,
            });
            t += 16_667; // ~60fps
        }
        o = Quaternion::from_axis_angle([0.0, 0.0, 1.0], PI / 2.0);
        for _ in 0..2 {
            samples.push(OrientationSample {
                timestamp_us: t,
                orientation: o,
            });
            t += 16_667;
        }
        o = Quaternion::IDENTITY;
        for _ in 0..10 {
            samples.push(OrientationSample {
                timestamp_us: t,
                orientation: o,
            });
            t += 16_667;
        }
        samples
    }

    #[test]
    fn zero_smoothness_leaves_track_unchanged() {
        let raw = track_with_step_motion();
        let smoothed = smooth_track(&raw, 0.0);
        for (r, s) in raw.iter().zip(smoothed.iter()) {
            assert!((r.orientation.w - s.orientation.w).abs() < 1e-9);
        }
    }

    #[test]
    fn heavy_smoothing_reduces_peak_deviation_from_the_jitter() {
        let raw = track_with_step_motion();
        let smoothed = smooth_track(&raw, 1.0);

        let peak_raw = raw
            .iter()
            .map(|s| s.orientation.to_euler_rpy().2.abs())
            .fold(0.0_f64, f64::max);
        let peak_smoothed = smoothed
            .iter()
            .map(|s| s.orientation.to_euler_rpy().2.abs())
            .fold(0.0_f64, f64::max);

        assert!(
            peak_smoothed < peak_raw * 0.5,
            "peak_smoothed={peak_smoothed} peak_raw={peak_raw}"
        );
    }

    #[test]
    fn correction_quaternion_is_identity_when_raw_equals_smoothed() {
        let q = Quaternion::from_axis_angle([1.0, 0.0, 0.0], 0.3);
        let c = correction_quaternion(q, q);
        assert!((c.w.abs() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn smoothed_track_preserves_endpoints_length() {
        let raw = track_with_step_motion();
        let smoothed = smooth_track(&raw, 0.5);
        assert_eq!(raw.len(), smoothed.len());
        assert_eq!(raw[0].timestamp_us, smoothed[0].timestamp_us);
        assert_eq!(
            raw.last().unwrap().timestamp_us,
            smoothed.last().unwrap().timestamp_us
        );
    }
}
