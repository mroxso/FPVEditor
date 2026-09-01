//! `fpv-stabilize`: gyro/blackbox-based stabilization — orientation
//! integration, low-pass smoothing, horizon lock, rolling-shutter
//! correction, and lens distortion, per PLAN.md section 5.

pub mod engine;
pub mod gyro;
pub mod horizon;
pub mod lens;
pub mod quaternion;
pub mod smoothing;

pub use engine::{FrameTransform, StabilizationEngine};
pub use gyro::{GyroError, GyroSample, GyroTrack, OrientationSample};
pub use lens::LensProfile;
pub use quaternion::Quaternion;
