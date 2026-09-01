//! `fpv-gpu`: wgpu-based rendering — compositing, color grading/LUTs, and
//! warp/reprojection for stabilization, per PLAN.md section 2.
//!
//! The color-math and reprojection-math modules ([`color`], [`lut`],
//! [`warp`]) are pure CPU functions so they're testable without a GPU
//! adapter; [`pipeline`] is the real GPU compute path, checked against the
//! CPU reference where an adapter is available.

pub mod color;
pub mod lut;
pub mod pipeline;
pub mod warp;

pub use color::{apply as apply_color, ColorAdjustments};
pub use lut::{Lut3D, LutCache, LutError};
pub use pipeline::{GpuColorPipeline, GpuError};
pub use warp::reproject;
