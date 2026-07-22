//! **Scene-agnostic render scaffolding** (`docs/33`) — the wgpu primitives and helpers every scene
//! builds its pipelines out of.
//!
//! `GpuMesh`, `UniformSlot`, `Camera`, the uniform POD blocks, and the small helpers around them are not
//! terrain code, space-band code, or globe code: all three scenes use them identically. They sat inside
//! `#[cfg(target_arch = "wasm32")] mod app` only because the scenes do, which put shared scaffolding out
//! of reach of every native build and made "which parts of `mod app` are actually scene-specific?"
//! unanswerable without reading 5,000 lines.
//!
//! Third and last of the mechanical lifts (`gpu_sph` → `gpu_particles` → here). What remains in `mod app`
//! after this is the part that genuinely is per-scene: the scene structs themselves, and the pipeline
//! builders that name a specific shader and bind-group layout.
//!
//! Not runnable natively — wgpu here has only the `webgpu` backend — but it type-checks, which is what
//! keeps a refactor from reporting green on code no native build compiled.
//!
//! **`Camera` is the one to watch.** The realignment's next step gives every scene a camera accessor so
//! the resolution controller (`docs/49`) can ask what is in view without knowing which scene it is
//! looking at. That is only possible with one `Camera` type, in one place.

#![allow(dead_code)] // each scene uses a different subset; wasm-only consumers are invisible natively

use crate::mesher::{Mesh, Vertex};

pub(crate) const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Uniforms {
    pub(crate) view_proj: [[f32; 4]; 4],
    pub(crate) model: [[f32; 4]; 4],
    pub(crate) light_dir: [f32; 4],
    pub(crate) camera_pos: [f32; 4],
}

/// Sky-pass uniforms — the per-pixel view ray (inverse view-projection), the sun direction (the
/// SAME light the terrain is lit by), and the declared atmosphere's Rayleigh optical depth + sun
/// gain. Everything the honest sky needs; nothing hand-painted. Matches `sky.wgsl`'s `SkyU`.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct SkyUniforms {
    pub(crate) inv_view_proj: [[f32; 4]; 4],
    pub(crate) sun_dir: [f32; 4], // xyz = direction to the sun (world), normalized
    pub(crate) tau: [f32; 4],     // xyz = Rayleigh optical depth per band, w = sun gain
    pub(crate) camera_pos: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct InstanceRaw {
    pub(crate) offset: [f32; 3],
    pub(crate) color: [f32; 3],
    pub(crate) emission: [f32; 3], // incandescent glow from temperature (docs/20); 0 for cold debris
}

pub(crate) struct Camera {
    pub(crate) yaw: f32,
    pub(crate) pitch: f32,
    pub(crate) zoom: f32,
    pub(crate) base_distance: f32,
}

pub(crate) struct GpuMesh {
    pub(crate) vertex_buf: wgpu::Buffer,
    pub(crate) index_buf: wgpu::Buffer,
    pub(crate) index_count: u32,
}

pub(crate) struct UniformSlot {
    pub(crate) buf: wgpu::Buffer,
    pub(crate) bind: wgpu::BindGroup,
}

pub(crate) fn draw<'a>(pass: &mut wgpu::RenderPass<'a>, uni: &'a UniformSlot, mesh: &'a GpuMesh) {
    pass.set_bind_group(0, &uni.bind, &[]);
    pass.set_vertex_buffer(0, mesh.vertex_buf.slice(..));
    pass.set_index_buffer(mesh.index_buf.slice(..), wgpu::IndexFormat::Uint32);
    pass.draw_indexed(0..mesh.index_count, 0, 0..1);
}

pub(crate) fn uniform_entry(binding: u32, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

pub(crate) fn make_uniform_buffer(device: &wgpu::Device) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("uniforms"),
        size: std::mem::size_of::<Uniforms>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

pub(crate) fn upload_mesh(device: &wgpu::Device, label: &str, mesh: &Mesh) -> GpuMesh {
    GpuMesh {
        vertex_buf: make_buffer(
            device,
            label,
            bytemuck::cast_slice(&mesh.vertices),
            wgpu::BufferUsages::VERTEX,
        ),
        index_buf: make_buffer(
            device,
            label,
            bytemuck::cast_slice(&mesh.indices),
            wgpu::BufferUsages::INDEX,
        ),
        index_count: mesh.indices.len() as u32,
    }
}

pub(crate) fn create_depth_view(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

pub(crate) fn make_buffer(
    device: &wgpu::Device,
    label: &str,
    bytes: &[u8],
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: bytes.len() as u64,
        usage,
        mapped_at_creation: true,
    });
    buffer
        .slice(..)
        .get_mapped_range_mut()
        .copy_from_slice(bytes);
    buffer.unmap();
    buffer
}

/// A GpuMesh whose vertex buffer is writable (VERTEX | COPY_DST) and pre-sized for `vert_capacity` vertices,
/// with a fixed index buffer. For geometry rebuilt every frame (the ground cap) — write vertices, don't
/// reallocate.
pub(crate) fn make_dynamic_mesh(device: &wgpu::Device, label: &str, vert_capacity: usize, indices: &[u32]) -> GpuMesh {
    let vertex_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: (vert_capacity * std::mem::size_of::<Vertex>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let index_buf = make_buffer(device, label, bytemuck::cast_slice(indices), wgpu::BufferUsages::INDEX);
    GpuMesh { vertex_buf, index_buf, index_count: indices.len() as u32 }
}

/// Uniforms for the star field (matches `StarU` in `shaders/stars.wgsl`).
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct StarUniforms {
    pub(crate) view_proj: [[f32; 4]; 4],
    /// Inertial (ICRS) → world. Identity where the scene's frame is already inertial; Earth's rotation
    /// where the world frame is Earth-fixed.
    pub(crate) spin: [[f32; 4]; 4],
    /// The eye in DISPLAY units — where to hang the billboards so they ride with the camera.
    pub(crate) cam_pos: [f32; 4],
    /// The eye in PARSECS from Sol, in the catalogue's own frame. This is what makes the sky real rather
    /// than a shell: every star's direction and brightness is computed against it, so moving the observer
    /// moves the sky. Inside a solar system it is ~1e-5 pc and the parallax is correctly invisible.
    pub(crate) cam_pc: [f32; 4],
    /// x = billboard distance (display units), y = PSF width (px), z = viewport height (px), w = exposure.
    pub(crate) params: [f32; 4],
}

/// One catalogued star, as the GPU wants it.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct StarInstance {
    /// The star's real position, parsecs, Sol at the origin.
    pub(crate) pos_pc: [f32; 3],
    pub(crate) _pad0: f32,
    pub(crate) color: [f32; 3],
    /// Flux the star would show at 10 pc; the shader applies the inverse-square law for the real distance.
    pub(crate) luminosity: f32,
}

/// **The sky, as engine machinery.** A scene owns one of these and draws it; it does not get to decide
/// what the sky looks like. The catalogue is real, the colours are derived from real temperatures, and
/// the placement uses the same geography conversion as the continents.
pub(crate) struct StarField {
    pipeline: wgpu::RenderPipeline,
    instances: wgpu::Buffer,
    count: u32,
    uni: wgpu::Buffer,
    bind: wgpu::BindGroup,
}

impl StarField {
    /// Build from a parsed catalogue. `format` is the surface format; the pipeline reads but never writes
    /// depth, and is meant to be drawn FIRST — nothing occludes a star except whatever is drawn over it.
    pub(crate) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        stars: &[crate::sky::Star],
    ) -> Self {
        use wgpu::util::DeviceExt;
        let data: Vec<StarInstance> = stars
            .iter()
            .map(|s| StarInstance { pos_pc: s.pos_pc, _pad0: 0.0, color: s.color, luminosity: s.luminosity })
            .collect();
        let instances = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("stars"),
            contents: bytemuck::cast_slice(&data),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let uni = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("star-uniforms"),
            size: std::mem::size_of::<StarUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("star-bind-layout"),
            entries: &[uniform_entry(0, wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT)],
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("star-bind"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: uni.as_entire_binding() }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("stars"),
            source: wgpu::ShaderSource::Wgsl(concat!(include_str!("../../../shaders/tonemap.wgsl"), include_str!("../../../shaders/stars.wgsl")).into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("star-pipeline-layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("stars"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<StarInstance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &[
                        wgpu::VertexAttribute { offset: 0, shader_location: 0, format: wgpu::VertexFormat::Float32x3 },
                        wgpu::VertexAttribute { offset: 16, shader_location: 1, format: wgpu::VertexFormat::Float32x3 },
                        wgpu::VertexAttribute { offset: 28, shader_location: 2, format: wgpu::VertexFormat::Float32 },
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            // Stars sit behind everything: test nothing, write nothing, and draw first.
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
        Self { pipeline, instances, count: data.len() as u32, uni, bind }
    }

    /// Update and draw. `spin` carries the scene's frame (identity for an inertial world); `radius` places
    /// the sphere well inside the far plane; `exposure` scales measured flux to display.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw(
        &self,
        queue: &wgpu::Queue,
        pass: &mut wgpu::RenderPass<'_>,
        view_proj: glam::Mat4,
        spin: glam::Mat4,
        cam_pos: glam::Vec3,
        cam_pc: glam::Vec3,
        radius: f32,
        viewport_w: f32,
        viewport_h: f32,
        exposure: f32,
    ) {
        let u = StarUniforms {
            view_proj: view_proj.to_cols_array_2d(),
            spin: spin.to_cols_array_2d(),
            cam_pos: [cam_pos.x, cam_pos.y, cam_pos.z, 1.0],
            cam_pc: [cam_pc.x, cam_pc.y, cam_pc.z, (viewport_w / viewport_h.max(1.0)).max(1e-6)],
            params: [radius, 2.2, viewport_h.max(1.0), exposure],
        };
        queue.write_buffer(&self.uni, 0, bytemuck::bytes_of(&u));
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind, &[]);
        pass.set_vertex_buffer(0, self.instances.slice(..));
        pass.draw(0..6, 0..self.count);
    }
}
