//! Horizon lock: cancel roll drift so the horizon stays level regardless of
//! the smoothed flight-path rotation, like Gyroflow's "horizon lock".

use crate::quaternion::Quaternion;

/// Compute the correction quaternion that cancels roll in `orientation`,
/// scaled by `strength` in `[0, 1]` (1.0 = fully level horizon, 0.0 = no
/// correction). Only the roll (rotation about the camera's forward/X axis,
/// per [`Quaternion::to_euler_rpy`]'s convention) is touched.
pub fn horizon_lock_correction(orientation: Quaternion, strength: f32) -> Quaternion {
    let strength = strength.clamp(0.0, 1.0) as f64;
    let (roll, _pitch, _yaw) = orientation.to_euler_rpy();
    Quaternion::from_axis_angle([1.0, 0.0, 0.0], -roll * strength)
}

/// Apply horizon lock to an orientation, returning the corrected orientation.
pub fn apply_horizon_lock(orientation: Quaternion, strength: f32) -> Quaternion {
    (horizon_lock_correction(orientation, strength) * orientation).normalized()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    fn assert_close(a: f64, b: f64, eps: f64) {
        assert!((a - b).abs() < eps, "{a} !~= {b}");
    }

    #[test]
    fn full_strength_zeroes_out_pure_roll() {
        let rolled = Quaternion::from_axis_angle([1.0, 0.0, 0.0], 0.4);
        let corrected = apply_horizon_lock(rolled, 1.0);
        let (roll, _, _) = corrected.to_euler_rpy();
        assert_close(roll, 0.0, 1e-9);
    }

    #[test]
    fn zero_strength_leaves_roll_untouched() {
        let rolled = Quaternion::from_axis_angle([1.0, 0.0, 0.0], 0.4);
        let corrected = apply_horizon_lock(rolled, 0.0);
        let (roll, _, _) = corrected.to_euler_rpy();
        assert_close(roll, 0.4, 1e-9);
    }

    #[test]
    fn half_strength_halves_the_roll_angle() {
        let rolled = Quaternion::from_axis_angle([1.0, 0.0, 0.0], 0.4);
        let corrected = apply_horizon_lock(rolled, 0.5);
        let (roll, _, _) = corrected.to_euler_rpy();
        assert_close(roll, 0.2, 1e-9);
    }

    #[test]
    fn horizon_lock_does_not_disturb_pure_yaw() {
        let yawed = Quaternion::from_axis_angle([0.0, 0.0, 1.0], PI / 3.0);
        let corrected = apply_horizon_lock(yawed, 1.0);
        let (roll, _pitch, yaw) = corrected.to_euler_rpy();
        assert_close(roll, 0.0, 1e-9);
        assert_close(yaw, PI / 3.0, 1e-6);
    }
}
