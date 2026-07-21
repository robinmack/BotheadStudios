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
