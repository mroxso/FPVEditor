//! Reprojection math: given the stabilization correction rotation and
//! FOV-crop scale for a frame ([`fpv_stabilize::FrameTransform`]), compute
//! where in the *raw source frame* a pixel of the *stabilized output frame*
//! should be sampled from. This is the CPU-side reference the GPU warp
//! shader's UV generation is checked against.

use fpv_stabilize::FrameTransform;

/// Map a normalized device coordinate in the stabilized output (`[-1, 1]`
/// on each axis, `y` up) to the normalized device coordinate to sample in
/// the raw source frame, under a simple pinhole-camera model with the given
/// horizontal/vertical field of view (radians).
pub fn reproject(
    transform: &FrameTransform,
    fov_x_rad: f64,
    fov_y_rad: f64,
    output_ndc: [f64; 2],
) -> [f64; 2] {
    let tan_x = (fov_x_rad / 2.0).tan();
    let tan_y = (fov_y_rad / 2.0).tan();

    let dir = [output_ndc[0] * tan_x, output_ndc[1] * tan_y, 1.0];
    let norm = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
    let dir = [dir[0] / norm, dir[1] / norm, dir[2] / norm];

    let src_dir = transform.rotation.rotate_vector(dir);
    let src_ndc = [
        (src_dir[0] / src_dir[2]) / tan_x,
        (src_dir[1] / src_dir[2]) / tan_y,
    ];

    [
        src_ndc[0] / transform.fov_scale as f64,
        src_ndc[1] / transform.fov_scale as f64,
    ]
}

/// Whether a source sample coordinate falls outside the raw frame (the
/// caller should treat it as a hard edge — clamp, or feather to black).
pub fn is_out_of_bounds(src_ndc: [f64; 2]) -> bool {
    src_ndc[0] < -1.0 || src_ndc[0] > 1.0 || src_ndc[1] < -1.0 || src_ndc[1] > 1.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use fpv_stabilize::Quaternion;
    use std::f64::consts::FRAC_PI_4;

    fn identity_transform(fov_scale: f32) -> FrameTransform {
        FrameTransform {
            rotation: Quaternion::IDENTITY,
            fov_scale,
        }
    }

    #[test]
    fn identity_rotation_and_unit_scale_is_a_no_op_mapping() {
        let t = identity_transform(1.0);
        for ndc in [[0.0, 0.0], [0.5, -0.3], [-0.9, 0.9]] {
            let out = reproject(&t, FRAC_PI_4, FRAC_PI_4, ndc);
            assert!((out[0] - ndc[0]).abs() < 1e-9, "x: {out:?} vs {ndc:?}");
            assert!((out[1] - ndc[1]).abs() < 1e-9, "y: {out:?} vs {ndc:?}");
        }
    }

    #[test]
    fn fov_scale_shrinks_the_sampled_region_toward_center() {
        let t = identity_transform(2.0);
        let out = reproject(&t, FRAC_PI_4, FRAC_PI_4, [0.8, 0.0]);
        assert!((out[0] - 0.4).abs() < 1e-9, "got {out:?}");
    }

    #[test]
    fn center_pixel_stays_in_bounds_under_any_rotation() {
        let t = FrameTransform {
            rotation: Quaternion::from_axis_angle([0.0, 1.0, 0.0], 0.05),
            fov_scale: 1.1,
        };
        let out = reproject(&t, FRAC_PI_4, FRAC_PI_4, [0.0, 0.0]);
        assert!(!is_out_of_bounds(out), "got {out:?}");
    }

    #[test]
    fn a_yaw_rotation_shifts_the_center_sample_off_axis() {
        let t = FrameTransform {
            rotation: Quaternion::from_axis_angle([0.0, 1.0, 0.0], 0.2),
            fov_scale: 1.0,
        };
        let out = reproject(&t, FRAC_PI_4, FRAC_PI_4, [0.0, 0.0]);
        assert!(out[0].abs() > 1e-3, "expected a horizontal shift, got {out:?}");
    }

    #[test]
    fn out_of_bounds_detection() {
        assert!(!is_out_of_bounds([0.0, 0.0]));
        assert!(!is_out_of_bounds([1.0, -1.0]));
        assert!(is_out_of_bounds([1.01, 0.0]));
        assert!(is_out_of_bounds([0.0, -1.5]));
    }
}
