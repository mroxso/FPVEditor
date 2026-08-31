//! Gyro-log ingestion and orientation integration (Betaflight/INAV
//! blackbox-style logs, or embedded camera gyro metadata).

use crate::quaternion::Quaternion;
use serde::{Deserialize, Serialize};

/// One gyroscope sample: angular velocity in rad/s on each axis, at a given
/// timestamp (microseconds since the start of the recording).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GyroSample {
    pub timestamp_us: i64,
    /// Angular velocity, rad/s, camera-body frame.
    pub gyro: [f64; 3],
}

/// The camera's orientation at a point in time, relative to its orientation
/// at the start of the log.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrientationSample {
    pub timestamp_us: i64,
    pub orientation: Quaternion,
}

/// Integrate a sequence of gyro samples into an absolute orientation track
/// via rectangular (Euler) integration: for each interval, treat the
/// angular velocity as constant and rotate by `omega * dt`.
///
/// Samples must be sorted by ascending `timestamp_us`; this is checked by
/// the caller via [`GyroTrack::from_samples`] which is the only public
/// entry point.
fn integrate(samples: &[GyroSample]) -> Vec<OrientationSample> {
    if samples.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(samples.len());
    let mut orientation = Quaternion::IDENTITY;
    out.push(OrientationSample {
        timestamp_us: samples[0].timestamp_us,
        orientation,
    });
    for window in samples.windows(2) {
        let (prev, cur) = (window[0], window[1]);
        let dt = (cur.timestamp_us - prev.timestamp_us) as f64 / 1_000_000.0;
        let omega = prev.gyro;
        let angle = (omega[0] * omega[0] + omega[1] * omega[1] + omega[2] * omega[2]).sqrt() * dt;
        let delta = if angle.abs() < 1e-15 {
            Quaternion::IDENTITY
        } else {
            Quaternion::from_axis_angle(omega, angle)
        };
        orientation = (orientation * delta).normalized();
        out.push(OrientationSample {
            timestamp_us: cur.timestamp_us,
            orientation,
        });
    }
    out
}

/// An integrated orientation track built from a gyro log, with lookup by
/// time (via interpolation) for sampling at arbitrary frame/scanline times.
#[derive(Debug)]
pub struct GyroTrack {
    samples: Vec<OrientationSample>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum GyroError {
    #[error("gyro log is empty")]
    Empty,
    #[error("gyro samples are not sorted by ascending timestamp")]
    Unsorted,
}

impl GyroTrack {
    pub fn from_samples(samples: &[GyroSample]) -> Result<Self, GyroError> {
        if samples.is_empty() {
            return Err(GyroError::Empty);
        }
        if !samples.windows(2).all(|w| w[0].timestamp_us < w[1].timestamp_us) {
            return Err(GyroError::Unsorted);
        }
        Ok(Self {
            samples: integrate(samples),
        })
    }

    pub fn start_us(&self) -> i64 {
        self.samples[0].timestamp_us
    }

    pub fn end_us(&self) -> i64 {
        self.samples[self.samples.len() - 1].timestamp_us
    }

    /// Orientation at an arbitrary time, via slerp between the two nearest
    /// integrated samples. Clamps to the track's start/end for out-of-range
    /// queries.
    pub fn orientation_at(&self, timestamp_us: i64) -> Quaternion {
        if timestamp_us <= self.start_us() {
            return self.samples[0].orientation;
        }
        if timestamp_us >= self.end_us() {
            return self.samples[self.samples.len() - 1].orientation;
        }
        // Binary search for the bracketing pair.
        let idx = self
            .samples
            .partition_point(|s| s.timestamp_us <= timestamp_us)
            .saturating_sub(1)
            .min(self.samples.len() - 2);
        let a = self.samples[idx];
        let b = self.samples[idx + 1];
        let span = (b.timestamp_us - a.timestamp_us).max(1) as f64;
        let t = (timestamp_us - a.timestamp_us) as f64 / span;
        Quaternion::slerp(a.orientation, b.orientation, t.clamp(0.0, 1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    #[test]
    fn empty_log_is_rejected() {
        assert_eq!(GyroTrack::from_samples(&[]).unwrap_err(), GyroError::Empty);
    }

    #[test]
    fn unsorted_log_is_rejected() {
        let samples = vec![
            GyroSample {
                timestamp_us: 1000,
                gyro: [0.0; 3],
            },
            GyroSample {
                timestamp_us: 500,
                gyro: [0.0; 3],
            },
        ];
        assert_eq!(
            GyroTrack::from_samples(&samples).unwrap_err(),
            GyroError::Unsorted
        );
    }

    #[test]
    fn constant_yaw_rate_for_one_second_produces_expected_rotation() {
        // 90 deg/sec around Z for 1 second -> 90 degree yaw at t=1s.
        let rate = PI / 2.0; // rad/s
        let samples = vec![
            GyroSample {
                timestamp_us: 0,
                gyro: [0.0, 0.0, rate],
            },
            GyroSample {
                timestamp_us: 1_000_000,
                gyro: [0.0, 0.0, rate],
            },
        ];
        let track = GyroTrack::from_samples(&samples).unwrap();
        let end = track.orientation_at(1_000_000);
        let (_, _, yaw) = end.to_euler_rpy();
        assert!((yaw - PI / 2.0).abs() < 1e-6, "yaw={yaw}");
    }

    #[test]
    fn zero_gyro_produces_identity_throughout() {
        let samples = vec![
            GyroSample {
                timestamp_us: 0,
                gyro: [0.0; 3],
            },
            GyroSample {
                timestamp_us: 500_000,
                gyro: [0.0; 3],
            },
            GyroSample {
                timestamp_us: 1_000_000,
                gyro: [0.0; 3],
            },
        ];
        let track = GyroTrack::from_samples(&samples).unwrap();
        for t in [0, 250_000, 500_000, 999_999] {
            let o = track.orientation_at(t);
            assert!((o.w - 1.0).abs() < 1e-9);
        }
    }

    #[test]
    fn orientation_at_queries_out_of_range_clamp_to_endpoints() {
        let samples = vec![
            GyroSample {
                timestamp_us: 1000,
                gyro: [0.0, 0.0, 1.0],
            },
            GyroSample {
                timestamp_us: 2000,
                gyro: [0.0, 0.0, 1.0],
            },
        ];
        let track = GyroTrack::from_samples(&samples).unwrap();
        assert_eq!(track.orientation_at(0), track.orientation_at(1000));
        assert_eq!(track.orientation_at(999_999), track.orientation_at(2000));
    }

    #[test]
    fn interpolated_midpoint_is_between_endpoints() {
        let rate = PI / 2.0;
        let samples = vec![
            GyroSample {
                timestamp_us: 0,
                gyro: [0.0, 0.0, rate],
            },
            GyroSample {
                timestamp_us: 1_000_000,
                gyro: [0.0, 0.0, rate],
            },
        ];
        let track = GyroTrack::from_samples(&samples).unwrap();
        let mid = track.orientation_at(500_000);
        let (_, _, yaw) = mid.to_euler_rpy();
        assert!((yaw - PI / 4.0).abs() < 1e-3, "yaw={yaw}");
    }
}
