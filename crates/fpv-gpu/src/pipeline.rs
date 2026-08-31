//! A minimal wgpu compute pipeline that applies [`crate::color::ColorAdjustments`]
//! to a buffer of RGBA pixels on the GPU. This is the real compositing path;
//! [`crate::color::apply`] is the CPU reference it's checked against.

use bytemuck::{Pod, Zeroable};

use crate::color::ColorAdjustments;

const SHADER_SRC: &str = r#"
struct Adjustments {
    exposure: f32,
    contrast: f32,
    saturation: f32,
    _pad: f32,
};

@group(0) @binding(0) var<storage, read> input_pixels: array<vec4<f32>>;
@group(0) @binding(1) var<storage, read_write> output_pixels: array<vec4<f32>>;
@group(0) @binding(2) var<uniform> adj: Adjustments;

fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x;
    if (idx >= arrayLength(&input_pixels)) {
        return;
    }
    var c = input_pixels[idx].rgb;
    c = c * pow(2.0, adj.exposure);
    c = (c - vec3<f32>(0.5)) * adj.contrast + vec3<f32>(0.5);
    let l = luma(c);
    c = l + (c - l) * adj.saturation;
    c = clamp(c, vec3<f32>(0.0), vec3<f32>(1.0));
    output_pixels[idx] = vec4<f32>(c, input_pixels[idx].a);
}
"#;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct AdjustmentsUniform {
    exposure: f32,
    contrast: f32,
    saturation: f32,
    _pad: f32,
}

impl From<ColorAdjustments> for AdjustmentsUniform {
    fn from(a: ColorAdjustments) -> Self {
        Self {
            exposure: a.exposure,
            contrast: a.contrast,
            saturation: a.saturation,
            _pad: 0.0,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GpuError {
    #[error("no compatible GPU adapter available")]
    NoAdapter,
    #[error("failed to request device: {0}")]
    RequestDevice(String),
}

pub struct GpuColorPipeline {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl GpuColorPipeline {
    /// Set up a headless (no surface) wgpu device and compile the color
    /// compute shader. Returns `Err(GpuError::NoAdapter)` on machines with
    /// no usable GPU backend (e.g. some CI runners) — callers should treat
    /// that as "skip the GPU path", not a hard failure.
    pub fn new() -> Result<Self, GpuError> {
        pollster::block_on(Self::new_async())
    }

    async fn new_async() -> Result<Self, GpuError> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .map_err(|_| GpuError::NoAdapter)?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .map_err(|e| GpuError::RequestDevice(e.to_string()))?;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fpv-gpu color shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER_SRC.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fpv-gpu color bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("fpv-gpu color pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("fpv-gpu color pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        Ok(Self {
            device,
            queue,
            pipeline,
            bind_group_layout,
        })
    }

    /// Apply color adjustments to a flat buffer of RGBA pixels on the GPU,
    /// returning the adjusted buffer.
    pub fn apply(&self, pixels: &[[f32; 4]], adj: ColorAdjustments) -> Vec<[f32; 4]> {
        use wgpu::util::DeviceExt;

        let input_bytes: &[u8] = bytemuck::cast_slice(pixels);
        let input_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("input pixels"),
            contents: input_bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        });

        let output_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("output pixels"),
            size: input_bytes.len() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let readback_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: input_bytes.len() as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform: AdjustmentsUniform = adj.into();
        let uniform_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("adjustments"),
            contents: bytemuck::bytes_of(&uniform),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("fpv-gpu color bind group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: input_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: output_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniform_buffer.as_entire_binding(),
                },
            ],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("fpv-gpu color pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            let workgroups = (pixels.len() as u32).div_ceil(64);
            pass.dispatch_workgroups(workgroups.max(1), 1, 1);
        }
        encoder.copy_buffer_to_buffer(&output_buffer, 0, &readback_buffer, 0, input_bytes.len() as u64);
        self.queue.submit(Some(encoder.finish()));

        let slice = readback_buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).ok();
        });
        self.device.poll(wgpu::PollType::wait_indefinitely()).ok();
        rx.recv().unwrap().expect("buffer mapping should succeed");

        let data = slice.get_mapped_range();
        let out: Vec<[f32; 4]> = bytemuck::cast_slice(&data).to_vec();
        drop(data);
        readback_buffer.unmap();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color;

    fn gpu_or_skip() -> Option<GpuColorPipeline> {
        match GpuColorPipeline::new() {
            Ok(p) => Some(p),
            Err(e) => {
                eprintln!("skipping GPU test: {e}");
                None
            }
        }
    }

    #[test]
    fn gpu_color_pass_matches_cpu_reference_within_tolerance() {
        let Some(pipeline) = gpu_or_skip() else {
            return;
        };
        let pixels = vec![
            [0.1, 0.2, 0.3, 1.0],
            [0.9, 0.5, 0.05, 1.0],
            [0.0, 0.0, 0.0, 1.0],
            [1.0, 1.0, 1.0, 0.5],
        ];
        let adj = ColorAdjustments {
            exposure: 0.5,
            contrast: 1.3,
            saturation: 0.7,
        };

        let gpu_out = pipeline.apply(&pixels, adj);

        for (i, px) in pixels.iter().enumerate() {
            let cpu = color::apply([px[0], px[1], px[2]], &adj);
            assert!((gpu_out[i][0] - cpu[0]).abs() < 1e-4, "r mismatch at {i}: {gpu_out:?} vs {cpu:?}");
            assert!((gpu_out[i][1] - cpu[1]).abs() < 1e-4, "g mismatch at {i}");
            assert!((gpu_out[i][2] - cpu[2]).abs() < 1e-4, "b mismatch at {i}");
            assert!((gpu_out[i][3] - px[3]).abs() < 1e-6, "alpha should pass through unchanged");
        }
    }

    #[test]
    fn identity_adjustments_leave_pixels_unchanged_on_gpu() {
        let Some(pipeline) = gpu_or_skip() else {
            return;
        };
        let pixels = vec![[0.42, 0.17, 0.88, 1.0]];
        let out = pipeline.apply(&pixels, ColorAdjustments::default());
        assert!((out[0][0] - 0.42).abs() < 1e-5);
        assert!((out[0][1] - 0.17).abs() < 1e-5);
        assert!((out[0][2] - 0.88).abs() < 1e-5);
    }
}
