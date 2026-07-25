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

use crate::gpu_layout::GpuParticle;
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
    /// Look-target offset from the focus body, in display units, expressed in the frame that RIDES
    /// the focused body: the orbit renderer re-centres the world on the focus every frame, so an
    /// offset held here follows the body through its orbital motion instead of smearing against a
    /// fixed point in space. Zero means the classic framing (target = the body's centre).
    /// Representation only: this moves what the camera looks at, never any matter.
    pub(crate) pan: glam::Vec3,
}

impl Camera {
    /// Unit vector from the look target toward the eye, from the yaw/pitch orbit angles. The same
    /// construction `view_proj` and the per-grain view path use, kept in one place.
    pub(crate) fn eye_dir(&self) -> glam::Vec3 {
        let cp = self.pitch.cos();
        glam::Vec3::new(cp * self.yaw.sin(), self.pitch.sin(), cp * self.yaw.cos())
    }

    /// Translate the look target in the camera's screen plane by a pointer delta, in pixels. The
    /// scale is exact, not a feel dial: at the focal plane (distance `base_distance * zoom`) one
    /// pixel of a `fov_y` frustum `viewport_h` pixels tall spans `2 * d * tan(fov_y / 2) / h`
    /// display units, so the world tracks the pointer one-for-one. Dragging right moves the target
    /// left (the scene follows the pointer, map-style); screen y grows downward.
    pub(crate) fn pan_by_pixels(&mut self, dx_px: f32, dy_px: f32, fov_y: f32, viewport_h: f32) {
        let dist = self.base_distance * self.zoom;
        let per_px = 2.0 * dist * (0.5 * fov_y).tan() / viewport_h.max(1.0);
        // The camera's screen basis: forward is target-minus-eye; pitch is clamped short of the
        // poles everywhere it is set, so forward is never parallel to world up.
        let forward = -self.eye_dir();
        let right = forward.cross(glam::Vec3::Y).normalize();
        let up = right.cross(forward);
        self.pan += (right * -dx_px + up * dy_px) * per_px;
    }
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

pub(crate) fn uniform_entry(
    binding: u32,
    visibility: wgpu::ShaderStages,
) -> wgpu::BindGroupLayoutEntry {
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

pub(crate) fn create_depth_view(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> wgpu::TextureView {
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
pub(crate) fn make_dynamic_mesh(
    device: &wgpu::Device,
    label: &str,
    vert_capacity: usize,
    indices: &[u32],
) -> GpuMesh {
    let vertex_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: (vert_capacity * std::mem::size_of::<Vertex>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let index_buf = make_buffer(
        device,
        label,
        bytemuck::cast_slice(indices),
        wgpu::BufferUsages::INDEX,
    );
    GpuMesh {
        vertex_buf,
        index_buf,
        index_count: indices.len() as u32,
    }
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
            .map(|s| StarInstance {
                pos_pc: s.pos_pc,
                _pad0: 0.0,
                color: s.color,
                luminosity: s.luminosity,
            })
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
            entries: &[uniform_entry(
                0,
                wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
            )],
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("star-bind"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uni.as_entire_binding(),
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("stars"),
            source: wgpu::ShaderSource::Wgsl(
                concat!(
                    include_str!("../../../shaders/tonemap.wgsl"),
                    include_str!("../../../shaders/stars.wgsl")
                )
                .into(),
            ),
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
                        wgpu::VertexAttribute {
                            offset: 0,
                            shader_location: 0,
                            format: wgpu::VertexFormat::Float32x3,
                        },
                        wgpu::VertexAttribute {
                            offset: 16,
                            shader_location: 1,
                            format: wgpu::VertexFormat::Float32x3,
                        },
                        wgpu::VertexAttribute {
                            offset: 28,
                            shader_location: 2,
                            format: wgpu::VertexFormat::Float32,
                        },
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
        Self {
            pipeline,
            instances,
            count: data.len() as u32,
            uni,
            bind,
        }
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
            cam_pc: [
                cam_pc.x,
                cam_pc.y,
                cam_pc.z,
                (viewport_w / viewport_h.max(1.0)).max(1e-6),
            ],
            params: [radius, 2.2, viewport_h.max(1.0), exposure],
        };
        queue.write_buffer(&self.uni, 0, bytemuck::bytes_of(&u));
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind, &[]);
        pass.set_vertex_buffer(0, self.instances.slice(..));
        pass.draw(0..6, 0..self.count);
    }
}

/// **One piece of matter the engine is holding, as it must be drawn** — and nothing more.
///
/// Every field is PHYSICAL: where it is, how fast, how big, what it is made of, how hot. There is no
/// colour here, no brightness, no "effect type". The picture is DERIVED from these (Law VI: physics
/// drives the render), which is why the derivation belongs to the engine and not to whoever is holding
/// a canvas.
///
/// The reason this type exists: a scene was deciding what a meteor looks like. `ground_scene` built
/// `GpuParticle`s twice — once for grains, once for meteors in flight — reading albedo out of the
/// material table and calling the incandescence law itself, and a third copy would have been needed for
/// the entry trail, and a fourth for a swarm. Each copy is a place a scene can quietly disagree with the
/// physics about what is real. Now the engine answers "what am I holding?" and a scene's only job is to
/// put it on the screen — so a scene that can draw ANY of the engine's matter can draw ALL of it, which
/// is what makes a capability like the meteor swarm (docs/59) work in a scene that knows nothing about
/// meteors.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drawn {
    /// Position, in the scene's own frame (centred world coords).
    pub pos: glam::Vec3,
    pub vel: glam::Vec3,
    /// Its real radius (m) — a grain's contact radius, a body's radius, a vapour parcel's expansion into
    /// the local air. Size is matter, not a sprite scale.
    pub radius_m: f32,
    /// Index into the material catalogue — what it IS. Its colour is that material's own measured albedo.
    pub material: usize,
    /// Temperature (K). What it glows AT; nothing glows because it was designated glowing.
    pub temp_k: f32,
    /// Settled matter, as opposed to matter in flight. Physical state, carried for the solver's use.
    pub resting: bool,
}

impl GpuParticle {
    /// The ONE physics→instance mapping. Colour is the material's own measured albedo, from the same
    /// catalogue row the physics reads, and emission is the incandescence of its real temperature — so a
    /// grain, a meteor, and the vapour it shed are drawn by one rule rather than three.
    ///
    /// (Incandescence still goes through `emission::incandescence`, the ramp the scenes already used, NOT
    /// `blackbody::blackbody_srgb`. Collapsing those two curves is docs/46 row 13 and changes how every
    /// hot thing looks, so it needs its own rig evidence — this is a consolidation, not a repaint.)
    pub(crate) fn of_matter(d: &Drawn, mats: &[crate::materials::Material]) -> Self {
        GpuParticle {
            offset: d.pos.to_array(),
            u: 0.0,
            vel: d.vel.to_array(),
            resting: if d.resting { 1.0 } else { 0.0 },
            color: mats
                .get(d.material)
                .map(|m| m.albedo)
                .unwrap_or([0.5, 0.5, 0.5]),
            material: d.material as f32,
            emission: crate::emission::incandescence(d.temp_k),
            rho: 0.0,
            radius: d.radius_m,
            _p0: 0.0,
            _p1: 0.0,
            _p2: 0.0,
        }
    }
}

/// Uniforms for [`MatterField`] — matches `matter.wgsl`'s `Cam`.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct MatterUniforms {
    view_proj: [[f32; 4]; 4],
    /// x = DISPLAY_SCALE (m → display units), y/z = projection x/y scale, w = one pixel as an NDC
    /// half-extent (2/viewport_height).
    params: [f32; 4],
}

/// **A scene-agnostic renderer for the engine's own matter** (docs/50 render path, docs/59).
///
/// The counterpart of [`StarField`], and deliberately shaped like it: a scene holds one, hands it the
/// `Drawn` items the engine reported (already mapped to the shared [`GpuParticle`] instance layout), and
/// draws. Nothing here knows what the matter IS — a grain, a body in flight, a parcel of shed vapour —
/// which is precisely what lets a capability like the meteor swarm appear in a scene that was never
/// taught about meteors.
pub(crate) struct MatterField {
    pipeline: wgpu::RenderPipeline,
    /// One buffer, rewritten each frame.
    ///
    /// A RING of three was tried here and **measured no improvement**, so it was removed rather than kept
    /// as plausible-looking complexity. The stall it was meant to fix (roughly one frame per second taking
    /// 450–520 ms while the median was 1.5 ms) turned out to be an artifact of the RIG: `scripts/rig.sh`
    /// runs with `--disable-frame-rate-limit`, so the page rendered at 170–350 fps and pushed several
    /// times more per second through `queue.write_buffer` than any vsynced browser will. Paced at ~60 fps
    /// the same scene never exceeds ~10 ms and never stalls at all. Recorded because the ablation ladder
    /// that found it is reusable: upload-only stalled as badly as upload+draw, and with the upload skipped
    /// there were no stalls — which priced the write, not the physics and not the draw.
    instances: wgpu::Buffer,
    capacity: u32,
    count: u32,
    uni: wgpu::Buffer,
    bind: wgpu::BindGroup,
}

impl MatterField {
    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat, capacity: u32) -> Self {
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("matter-instances"),
            size: (capacity as usize * std::mem::size_of::<GpuParticle>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uni = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("matter-uniforms"),
            size: std::mem::size_of::<MatterUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("matter-bind-layout"),
            entries: &[uniform_entry(
                0,
                wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
            )],
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("matter-bind"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uni.as_entire_binding(),
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("matter"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../../shaders/matter.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("matter-pipeline-layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("matter"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                // Straight into `GpuParticle`: offset @0, color @32, emission @48, radius @64.
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<GpuParticle>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &[
                        wgpu::VertexAttribute {
                            offset: 0,
                            shader_location: 0,
                            format: wgpu::VertexFormat::Float32x3,
                        },
                        wgpu::VertexAttribute {
                            offset: 32,
                            shader_location: 1,
                            format: wgpu::VertexFormat::Float32x3,
                        },
                        wgpu::VertexAttribute {
                            offset: 48,
                            shader_location: 2,
                            format: wgpu::VertexFormat::Float32x3,
                        },
                        wgpu::VertexAttribute {
                            offset: 64,
                            shader_location: 3,
                            format: wgpu::VertexFormat::Float32,
                        },
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    // Additive: incandescent matter ADDS light to whatever is behind it, which is what
                    // emission means. A trail over the night side brightens it; it does not replace it.
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent::OVER,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            // Occluded by the planet (a meteor behind Earth is behind Earth), but writes no depth: these
            // are small emissive marks, not surfaces for anything else to sort against.
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
        Self {
            pipeline,
            instances,
            capacity,
            count: 0,
            uni,
            bind,
        }
    }

    /// Hand it this frame's matter. Silently drawing only what fits would hide matter, so the overflow is
    /// reported to the caller's log rather than swallowed.
    pub(crate) fn upload(&mut self, queue: &wgpu::Queue, inst: &[GpuParticle]) {
        let n = inst.len().min(self.capacity as usize);
        if n > 0 {
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&inst[..n]));
        }
        self.count = n as u32;
    }

    pub(crate) fn drawn_count(&self) -> u32 {
        self.count
    }

    /// `proj` is the projection matrix's (x, y) scales, `viewport_h` the height in pixels — together they
    /// are what lets the shader hold a mark at one pixel when the matter itself is smaller than that.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        queue: &wgpu::Queue,
        view_proj: glam::Mat4,
        display_scale: f32,
        proj: (f32, f32),
        viewport_h: f32,
    ) {
        if self.count == 0 {
            return;
        }
        let u = MatterUniforms {
            view_proj: view_proj.to_cols_array_2d(),
            params: [display_scale, proj.0, proj.1, 2.0 / viewport_h.max(1.0)],
        };
        queue.write_buffer(&self.uni, 0, bytemuck::bytes_of(&u));
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind, &[]);
        pass.set_vertex_buffer(0, self.instances.slice(..));
        pass.draw(0..6, 0..self.count);
    }
}

#[cfg(test)]
mod tests {
    use super::Camera;
    use glam::Vec3;

    fn cam() -> Camera {
        Camera {
            yaw: 0.3,
            pitch: 0.2,
            zoom: 1.0,
            base_distance: 100.0,
            pan: Vec3::ZERO,
        }
    }

    /// The pan scale is derived, not tuned: dragging the full viewport height translates the
    /// target by exactly the frustum's height at the focal plane, so the world tracks the pointer
    /// one-for-one at every zoom (half the focal distance, half the translation).
    #[test]
    fn a_full_viewport_drag_pans_exactly_one_frustum_height() {
        let (fov_y, h) = (0.9_f32, 768.0_f32);
        let mut c = cam();
        c.pan_by_pixels(0.0, h, fov_y, h);
        let expect = 2.0 * c.base_distance * c.zoom * (0.5 * fov_y).tan();
        assert!(
            (c.pan.length() - expect).abs() < 1e-3,
            "{} vs {}",
            c.pan.length(),
            expect
        );

        let mut near = cam();
        near.zoom = 0.5;
        near.pan_by_pixels(0.0, h, fov_y, h);
        assert!(
            (near.pan.length() - 0.5 * expect).abs() < 1e-3,
            "pan scales with the focal distance"
        );
    }

    /// The offset stays in the camera's screen plane (never along the view axis), dragging right
    /// moves the target left (the scene follows the pointer), and the gesture is reversible.
    #[test]
    fn pan_moves_in_the_screen_plane_and_reverses_cleanly() {
        let mut c = cam();
        c.pan_by_pixels(120.0, 0.0, 0.9, 768.0);
        let forward = -c.eye_dir();
        assert!(
            c.pan.dot(forward).abs() < 1e-4,
            "no component along the view axis"
        );
        let right = forward.cross(Vec3::Y).normalize();
        assert!(
            c.pan.dot(right) < 0.0,
            "dragging right carries the target left"
        );

        c.pan_by_pixels(-120.0, 0.0, 0.9, 768.0);
        assert!(
            c.pan.length() < 1e-4,
            "panning back returns the classic framing"
        );
    }
}
