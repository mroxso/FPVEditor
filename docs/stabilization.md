# Stabilization

`fpv-stabilize` (`crates/fpv-stabilize/src`) implements gyro/blackbox-based video
stabilization: orientation integration from gyro samples, smoothing, horizon lock,
rolling-shutter correction, and a lens distortion model. This document explains how each
piece works and, importantly, how much of it is currently wired into actual clip export.

## The `StabilizationProfile`

A clip opts into stabilization by having a `StabilizationProfile` attached via
`Command::ApplyStabilization` (see [Architecture](architecture.md#the-command-enum)),
exposed as `fpv stabilize` on the CLI and the `apply_stabilization` AI/MCP tool:

```rust
pub struct StabilizationProfile {
    pub smoothness: f32,   // 0.0 (off) .. 1.0 (max smoothing)
    pub strength: f32,     // 0.0 (no correction) .. 1.0 (full correction)
    pub horizon_lock: bool,
    pub dynamic_fov: f32,  // extra crop/zoom to hide warped edges, 0.0..1.0
}
```

Defaults: `smoothness: 0.5`, `strength: 1.0`, `horizon_lock: false`, `dynamic_fov: 0.1`.

## Pipeline

`fpv_stabilize::engine::StabilizationEngine` (`crates/fpv-stabilize/src/engine.rs`) ties
the pieces below together into a per-frame (or per-scanline) `FrameTransform { rotation,
fov_scale }`:

1. **Gyro integration** (`gyro.rs`) — `GyroTrack::from_samples` integrates a sequence of
   `GyroSample { timestamp_us, gyro: [f64; 3] }` (angular velocity in rad/s, camera-body
   frame) into an absolute orientation track via rectangular (Euler) integration: each
   interval treats the angular velocity as constant and rotates by `omega * dt`.
   `GyroTrack::orientation_at(timestamp_us)` interpolates the raw orientation at any time.
   This is where a Betaflight/INAV blackbox log or embedded camera gyro metadata would
   feed in.
2. **Smoothing** (`smoothing.rs`) — `smooth_track` low-pass filters the raw orientation
   track according to `profile.smoothness`; `correction_quaternion` derives the rotation
   needed to go from the raw orientation to the smoothed one.
3. **Strength blending** — `StabilizationEngine::frame_transform` blends the identity
   rotation and the full correction quaternion via spherical linear interpolation
   (`Quaternion::slerp`) by `profile.strength`, so `strength = 0.0` is a guaranteed
   identity transform and `strength = 1.0` applies the full correction.
4. **Horizon lock** (`horizon.rs`) — when `profile.horizon_lock` is set,
   `horizon_lock_correction` is derived from the orientation the frame will actually end
   up at (`rotation * raw_orientation`), not from the raw orientation alone, and composed
   on top so the visible roll is cancelled regardless of what `strength` and `smoothness`
   already did to the rotation.
5. **Rolling-shutter correction** — `StabilizationEngine::scanline_transform(frame_ts,
   scanline_fraction, readout_time_us)` computes a separate `FrameTransform` per scanline
   by offsetting the sample time by `scanline_fraction * readout_time_us` (0.0 = top of
   frame, 1.0 = bottom), so a rolling-shutter sensor's per-row capture-time skew can be
   corrected instead of treating the whole frame as captured at one instant.
6. **Lens distortion** (`lens.rs`) — `LensProfile` models a Brown-Conrady radial
   distortion (`k1`, `k2` coefficients on normalized image coordinates) plus horizontal
   and vertical field of view; `LensProfile::distort` maps an undistorted point to where
   it actually falls in the raw source frame. This module is the math only — a database
   of profiles for specific FPV cameras and lenses (Caddx, RunCam, DJI O3/O4, etc.) is
   listed as future work in [PLAN.md](../PLAN.md#5-fpv-specific-features) and is not
   implemented yet.
7. **Dynamic FOV** — `fov_scale` is simply `1.0 + dynamic_fov.clamp(0.0, 1.0)`: how much
   to zoom in / crop to hide the edges a rotational warp reveals.

## Warp/reprojection

`fpv-gpu`'s `warp` module (`crates/fpv-gpu/src/warp.rs`, `reproject`) implements the pure
reprojection math for actually resampling a frame according to a `FrameTransform` and
lens profile. It is a standalone, GPU-independent function, separate from the `wgpu`
color compute pipeline described in
[Architecture](architecture.md#gpu-pipeline).

## Current limitation: stabilization is not yet applied during export

**The `fpv-stabilize` math and the `fpv-gpu` warp pipeline are implemented and unit
tested, but nothing in the export path invokes them yet.** Verified directly in
`crates/fpv-media/src/export.rs` (`export_clip_args`): when a clip has a
`StabilizationProfile`, the exporter only adds an `ffmpeg` `crop` filter sized by
`dynamic_fov` — it reserves the zoom/crop that a rotational warp would need to hide its
edges, but never calls `StabilizationEngine::frame_transform`/`scanline_transform` or
`fpv_gpu::warp::reproject` to actually de-shake the footage. The `crf`-encoding
comment in that function states this explicitly: *"nothing in the export path (or
anywhere else yet) actually invokes fpv_stabilize's per-frame rotation or fpv-gpu's warp
pipeline, so exported clips are cropped/zoomed but not actually de-shaken."*

In practice, today:

- `fpv stabilize` / `apply_stabilization` successfully attach a `StabilizationProfile` to
  a clip, and it round-trips through save/load and undo/redo like any other clip
  property.
- Exporting that clip (`fpv export`, `export_timeline`, or the preview renderers)
  reserves the `dynamic_fov` crop but applies no rotational correction — the picture is
  simply zoomed in, not stabilized.
- Wiring frame-by-frame (or per-scanline, for rolling-shutter) GPU warp into the
  `ffmpeg`-based export pipeline is tracked as follow-up work; see
  [PLAN.md](../PLAN.md) and the [README's project status](../README.md#project-status).

If you are building on this crate, `StabilizationEngine::frame_transform`/
`scanline_transform` and `fpv_gpu::warp::reproject` are the pieces to compose into a
frame-rendering path — they are already correct and tested in isolation.
