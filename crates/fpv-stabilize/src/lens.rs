//! Lens/camera distortion profiles (FPV-specific database entries: Caddx,
//! RunCam, DJI O3/O4, etc. — this module is the math, the database itself
//! is future work per PLAN.md section 5).

use serde::{Deserialize, Serialize};

/// A simple Brown-Conrady radial distortion model, parameterized on
/// normalized image coordinates (image center = `[0, 0]`, half-height = `1.0`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LensProfile {
    pub name: String,
    pub fov_x_deg: f64,
    pub fov_y_deg: f64,
    /// Radial distortion coefficients (2nd and 4th order).
    pub k1: f64,
    pub k2: f64,
}

impl LensProfile {
    pub fn rectilinear(name: impl Into<String>, fov_x_deg: f64, fov_y_deg: f64) -> Self {
        Self {
            name: name.into(),
            fov_x_deg,
            fov_y_deg,
            k1: 0.0,
            k2: 0.0,
        }
    }

    /// Map an undistorted normalized point to its distorted position, i.e.
    /// "where in the raw source frame does this rectified pixel come from."
    pub fn distort(&self, point: [f64; 2]) -> [f64; 2] {
        let r2 = point[0] * point[0] + point[1] * point[1];
        let factor = 1.0 + self.k1 * r2 + self.k2 * r2 * r2;
        [point[0] * factor, point[1] * factor]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_distortion_is_identity() {
        let lens = LensProfile::rectilinear("flat", 90.0, 60.0);
        let p = [0.3, -0.6];
        let out = lens.distort(p);
        assert!((out[0] - p[0]).abs() < 1e-12);
        assert!((out[1] - p[1]).abs() < 1e-12);
    }

    #[test]
    fn center_point_is_never_moved_by_radial_distortion() {
        let lens = LensProfile {
            name: "wide".into(),
            fov_x_deg: 150.0,
            fov_y_deg: 110.0,
            k1: -0.25,
            k2: 0.05,
        };
        let out = lens.distort([0.0, 0.0]);
        assert_eq!(out, [0.0, 0.0]);
    }

    #[test]
    fn negative_k1_barrel_distortion_pulls_points_toward_center() {
        let lens = LensProfile {
            name: "fpv-wide".into(),
            fov_x_deg: 150.0,
            fov_y_deg: 110.0,
            k1: -0.3,
            k2: 0.0,
        };
        let p = [0.5, 0.0];
        let out = lens.distort(p);
        assert!(out[0].abs() < p[0].abs(), "expected barrel pull-in, got {out:?}");
    }

    #[test]
    fn positive_k1_pincushion_distortion_pushes_points_outward() {
        let lens = LensProfile {
            name: "tele".into(),
            fov_x_deg: 40.0,
            fov_y_deg: 25.0,
            k1: 0.3,
            k2: 0.0,
        };
        let p = [0.5, 0.0];
        let out = lens.distort(p);
        assert!(out[0].abs() > p[0].abs(), "expected pincushion push-out, got {out:?}");
    }
}
