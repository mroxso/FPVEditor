//! Minimal quaternion math for orientation tracking — deliberately
//! self-contained rather than pulling in a full linear-algebra crate, since
//! stabilization only needs unit-quaternion composition/interpolation.

use std::ops::Mul;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quaternion {
    pub w: f64,
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Quaternion {
    pub const IDENTITY: Quaternion = Quaternion {
        w: 1.0,
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    pub fn new(w: f64, x: f64, y: f64, z: f64) -> Self {
        Self { w, x, y, z }
    }

    /// Build a rotation quaternion from an axis-angle representation.
    /// `axis` need not be normalized; `angle` is in radians.
    pub fn from_axis_angle(axis: [f64; 3], angle: f64) -> Self {
        let len = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
        if len < 1e-12 {
            return Quaternion::IDENTITY;
        }
        let half = angle / 2.0;
        let s = half.sin() / len;
        Quaternion {
            w: half.cos(),
            x: axis[0] * s,
            y: axis[1] * s,
            z: axis[2] * s,
        }
    }

    pub fn norm(&self) -> f64 {
        (self.w * self.w + self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    pub fn normalized(&self) -> Quaternion {
        let n = self.norm();
        if n < 1e-12 {
            return Quaternion::IDENTITY;
        }
        Quaternion {
            w: self.w / n,
            x: self.x / n,
            y: self.y / n,
            z: self.z / n,
        }
    }

    pub fn conjugate(&self) -> Quaternion {
        Quaternion {
            w: self.w,
            x: -self.x,
            y: -self.y,
            z: -self.z,
        }
    }

    /// Inverse of a (not necessarily unit) quaternion.
    pub fn inverse(&self) -> Quaternion {
        let n2 = self.w * self.w + self.x * self.x + self.y * self.y + self.z * self.z;
        let c = self.conjugate();
        Quaternion {
            w: c.w / n2,
            x: c.x / n2,
            y: c.y / n2,
            z: c.z / n2,
        }
    }

    pub fn dot(&self, other: &Quaternion) -> f64 {
        self.w * other.w + self.x * other.x + self.y * other.y + self.z * other.z
    }

    /// Rotate a 3D vector by this (assumed unit) quaternion.
    pub fn rotate_vector(&self, v: [f64; 3]) -> [f64; 3] {
        let qv = Quaternion::new(0.0, v[0], v[1], v[2]);
        let r = *self * qv * self.conjugate();
        [r.x, r.y, r.z]
    }

    /// Spherical linear interpolation between two unit quaternions, t in [0, 1].
    pub fn slerp(a: Quaternion, b: Quaternion, t: f64) -> Quaternion {
        let a = a.normalized();
        let mut b = b.normalized();
        let mut cos_theta = a.dot(&b);

        // Take the shorter path around the hypersphere.
        if cos_theta < 0.0 {
            b = Quaternion::new(-b.w, -b.x, -b.y, -b.z);
            cos_theta = -cos_theta;
        }

        if cos_theta > 0.9995 {
            // Nearly parallel: linear interpolation avoids a division by ~0.
            let lerp = Quaternion::new(
                a.w + t * (b.w - a.w),
                a.x + t * (b.x - a.x),
                a.y + t * (b.y - a.y),
                a.z + t * (b.z - a.z),
            );
            return lerp.normalized();
        }

        let theta_0 = cos_theta.acos();
        let theta = theta_0 * t;
        let sin_theta_0 = theta_0.sin();
        let s0 = (theta_0 - theta).sin() / sin_theta_0;
        let s1 = theta.sin() / sin_theta_0;
        Quaternion::new(
            a.w * s0 + b.w * s1,
            a.x * s0 + b.x * s1,
            a.y * s0 + b.y * s1,
            a.z * s0 + b.z * s1,
        )
    }

    /// Extract Tait-Bryan (roll, pitch, yaw) angles in radians, assuming
    /// this quaternion rotates from the world frame into the camera frame
    /// with X-forward, Y-right, Z-up conventions collapsed to a simple
    /// aerospace-style roll/pitch/yaw (X=roll, Y=pitch, Z=yaw) extraction.
    pub fn to_euler_rpy(&self) -> (f64, f64, f64) {
        let q = self.normalized();

        // roll (x-axis rotation)
        let sinr_cosp = 2.0 * (q.w * q.x + q.y * q.z);
        let cosr_cosp = 1.0 - 2.0 * (q.x * q.x + q.y * q.y);
        let roll = sinr_cosp.atan2(cosr_cosp);

        // pitch (y-axis rotation)
        let sinp = 2.0 * (q.w * q.y - q.z * q.x);
        let pitch = if sinp.abs() >= 1.0 {
            std::f64::consts::FRAC_PI_2.copysign(sinp)
        } else {
            sinp.asin()
        };

        // yaw (z-axis rotation)
        let siny_cosp = 2.0 * (q.w * q.z + q.x * q.y);
        let cosy_cosp = 1.0 - 2.0 * (q.y * q.y + q.z * q.z);
        let yaw = siny_cosp.atan2(cosy_cosp);

        (roll, pitch, yaw)
    }
}

impl Mul for Quaternion {
    type Output = Quaternion;

    fn mul(self, rhs: Quaternion) -> Quaternion {
        Quaternion {
            w: self.w * rhs.w - self.x * rhs.x - self.y * rhs.y - self.z * rhs.z,
            x: self.w * rhs.x + self.x * rhs.w + self.y * rhs.z - self.z * rhs.y,
            y: self.w * rhs.y - self.x * rhs.z + self.y * rhs.w + self.z * rhs.x,
            z: self.w * rhs.z + self.x * rhs.y - self.y * rhs.x + self.z * rhs.w,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    fn assert_close(a: f64, b: f64, eps: f64) {
        assert!((a - b).abs() < eps, "{a} !~= {b}");
    }

    #[test]
    fn identity_rotates_nothing() {
        let v = Quaternion::IDENTITY.rotate_vector([1.0, 2.0, 3.0]);
        assert_close(v[0], 1.0, 1e-9);
        assert_close(v[1], 2.0, 1e-9);
        assert_close(v[2], 3.0, 1e-9);
    }

    #[test]
    fn ninety_degree_z_rotation_maps_x_to_y() {
        let q = Quaternion::from_axis_angle([0.0, 0.0, 1.0], PI / 2.0);
        let v = q.rotate_vector([1.0, 0.0, 0.0]);
        assert_close(v[0], 0.0, 1e-9);
        assert_close(v[1], 1.0, 1e-9);
        assert_close(v[2], 0.0, 1e-9);
    }

    #[test]
    fn composing_two_45_degree_rotations_equals_one_90_degree_rotation() {
        let q45 = Quaternion::from_axis_angle([0.0, 0.0, 1.0], PI / 4.0);
        let q90 = Quaternion::from_axis_angle([0.0, 0.0, 1.0], PI / 2.0);
        let composed = q45 * q45;
        let v_composed = composed.rotate_vector([1.0, 0.0, 0.0]);
        let v_direct = q90.rotate_vector([1.0, 0.0, 0.0]);
        assert_close(v_composed[0], v_direct[0], 1e-9);
        assert_close(v_composed[1], v_direct[1], 1e-9);
    }

    #[test]
    fn slerp_at_t0_and_t1_returns_endpoints() {
        let a = Quaternion::IDENTITY;
        let b = Quaternion::from_axis_angle([0.0, 1.0, 0.0], PI / 2.0);
        let at0 = Quaternion::slerp(a, b, 0.0);
        let at1 = Quaternion::slerp(a, b, 1.0);
        assert_close(at0.w, a.w, 1e-9);
        assert_close(at1.w, b.w, 1e-6);
        assert_close(at1.x, b.x, 1e-6);
    }

    #[test]
    fn slerp_midpoint_of_0_and_90_degrees_is_45_degrees() {
        let a = Quaternion::IDENTITY;
        let b = Quaternion::from_axis_angle([0.0, 0.0, 1.0], PI / 2.0);
        let mid = Quaternion::slerp(a, b, 0.5);
        let (_, _, yaw) = mid.to_euler_rpy();
        assert_close(yaw, PI / 4.0, 1e-6);
    }

    #[test]
    fn inverse_composed_with_self_is_identity() {
        let q = Quaternion::from_axis_angle([1.0, 1.0, 0.0], 1.234);
        let inv = q.inverse();
        let result = (q * inv).normalized();
        assert_close(result.w.abs(), 1.0, 1e-9);
    }

    #[test]
    fn euler_round_trip_for_small_roll_pitch_yaw() {
        let axis_angle_roll = Quaternion::from_axis_angle([1.0, 0.0, 0.0], 0.2);
        let (roll, pitch, yaw) = axis_angle_roll.to_euler_rpy();
        assert_close(roll, 0.2, 1e-9);
        assert_close(pitch, 0.0, 1e-9);
        assert_close(yaw, 0.0, 1e-9);
    }
}
