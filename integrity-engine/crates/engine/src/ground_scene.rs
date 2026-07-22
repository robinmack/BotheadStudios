//! **The ground scene, built from a definition** (`docs/55`).
//!
//! The terrain scene was deleted (docs/50) because it was the first thing designed and had accumulated
//! a decade of habits. Robin: *"terrain needs a complete rebuild with the new physics engine."* This is
//! that rebuild, and the difference is where the content lives: **every world-defining number comes from
//! a `world.json`** — patch size, relief, sea level, the material strata, the camera, gravity. The scene
//! contributes a camera rig, a meteor button, and three render passes. Nothing about *this* world is
//! compiled in.
//!
//! It also gives the granular GPU pipeline a visible consumer again. Since terrain was removed,
//! `gpu_particles` has been reachable only from `GpuProbe`, a compute-only diagnostic with no canvas.
//!
//! **The lifecycle you can watch**, and it is the engine's whole thesis:
//! 1. Undisturbed ground is BULK — a meshed heightfield with a procedural material texture. No particles
//!    exist, because nothing is happening (`docs/44`: necessity decides existence).
//! 2. A meteor resolves **only the region its energy actually disturbs** — grains appear in the crater,
//!    not across the map.
//! 3. Those grains are real matter under real contact physics until they come to rest.
//! 4. At rest they **de-resolve back into the world** and the surface is remeshed, so the crater persists
//!    as geometry. The matter is not deleted; it becomes ground again.
//!
//! Honest scope: this is still a `#[wasm_bindgen]` struct inside the engine crate, so adding a scene KIND
//! is an engine edit (`docs/46` ledger row 14's remaining half). What this scene proves is the other
//! half — that a scene's CONTENT is data.

use crate::gravity::MassField;
use crate::materials;
use crate::gpu_layout::GpuParticle;
use crate::mesher::{self, Vertex};
use crate::render::*;
use crate::simulation::Simulation;
use crate::texture;
use glam::{Mat4, Vec3};
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

fn make_world_bind(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    ubuf: &wgpu::Buffer,
    tex_view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
    matparams: &wgpu::Buffer,
    normal_view: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("world-bind-group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: ubuf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(tex_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: matparams.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(normal_view),
            },
        ],
    })
}

fn build_pipeline(
    device: &wgpu::Device,
    bind_layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("world-shader"),
        source: wgpu::ShaderSource::Wgsl(concat!(include_str!("../../../shaders/surface_normal.wgsl"), include_str!("../../../shaders/world.wgsl")).into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("pipeline-layout"),
        bind_group_layouts: &[bind_layout],
        push_constant_ranges: &[],
    });
    const ATTRS: [wgpu::VertexAttribute; 4] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x3, 3 => Uint32];
    let vertex_layout = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Vertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &ATTRS,
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("world-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[vertex_layout],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview: None,
        cache: None,
    })
}

/// The sky pipeline: a fullscreen triangle (no vertex buffer) that fills the background with the
/// derived Rayleigh sky. It does NOT test or write depth (it is the backdrop — the world geometry
/// drawn afterwards depth-tests against the cleared buffer and paints over it wherever it is nearer).

fn build_sky_pipeline(
    device: &wgpu::Device,
    bind_layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("sky-shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../../../shaders/sky.wgsl").into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("sky-pipeline-layout"),
        bind_group_layouts: &[bind_layout],
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("sky-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        // The sky is the backdrop: it must never occlude terrain, so depth-testing is disabled and
        // it writes no depth. It is drawn first; the world pass then overwrites it where geometry is.
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: false,
            depth_compare: wgpu::CompareFunction::Always,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview: None,
        cache: None,
    })
}

fn build_particle_pipeline(
    device: &wgpu::Device,
    bind_layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("particle-shader"),
        source: wgpu::ShaderSource::Wgsl(
            include_str!("../../../shaders/particles.wgsl").into(),
        ),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("particle-pipeline-layout"),
        bind_group_layouts: &[bind_layout],
        push_constant_ranges: &[],
    });
    const CUBE_ATTRS: [wgpu::VertexAttribute; 4] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x3, 3 => Uint32];
    // Instance attributes point straight into a `GpuParticle` (64 bytes): offset @0, color @32,
    // emission @48 — so the GPU-computed particle buffer is drawn directly (zero-copy, docs/22).
    const INST_ATTRS: [wgpu::VertexAttribute; 3] = [
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x3,
            offset: 0,
            shader_location: 4,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x3,
            offset: 32,
            shader_location: 5,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x3,
            offset: 48,
            shader_location: 6,
        },
    ];
    let buffers = [
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &CUBE_ATTRS,
        },
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GpuParticle>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &INST_ATTRS,
        },
    ];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("particle-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &buffers,
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview: None,
        cache: None,
    })
}

/// Near-clip distance. The camera shell's half-extent must be at least this, or the frustum's near
/// plane can cross into matter even while the shell itself cannot — which is exactly how a camera ends
/// up seeing under the skin.
const NEAR_CLIP: f32 = 0.2;

/// The ground scene: a world defined by data, rendered and made destructible.
#[wasm_bindgen]
pub struct Ground {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    depth_view: wgpu::TextureView,

    world_pipeline: wgpu::RenderPipeline,
    world_uni: UniformSlot,
    sky_pipeline: wgpu::RenderPipeline,
    sky_uni: UniformSlot,
    particle_pipeline: wgpu::RenderPipeline,
    particle_bind: wgpu::BindGroup,
    particle_instances: wgpu::Buffer,
    cube_gpu: GpuMesh,

    world_gpu: GpuMesh,
    sea_gpu: GpuMesh,

    sim: Simulation,
    mats: Vec<materials::Material>,
    camera: Camera,
    /// Metres of camera altitude above the surface directly beneath it — the scene's framing, declared.
    eye_height_m: f32,
    /// Last frame's resolved eye — the start of the swept camera-shell collision.
    last_eye: Vec3,
    /// Rayleigh optical depth DERIVED from the declared atmosphere's emergent surface pressure — the
    /// same λ⁻⁴ scattering that gives the blue marble its veil (docs/26). Not a painted sky colour.
    atm_tau: [f64; 3],
    frame: u64,
    max_particles: usize,
}

#[wasm_bindgen]
impl Ground {
    /// Build the scene from a world DEFINITION. Everything about the world comes from `world_json`.
    pub async fn create(canvas: HtmlCanvasElement, world_json: String) -> Result<Ground, JsValue> {
        console_error_panic_hook::set_once();
        let _ = console_log::init_with_level(log::Level::Info);
        let (width, height) = (canvas.width().max(1), canvas.height().max(1));

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU,
            ..Default::default()
        });
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .map_err(|e| JsValue::from_str(&format!("create_surface failed: {e}")))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await
            .ok_or_else(|| JsValue::from_str("no suitable GPU adapter found"))?;
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("ground-device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: adapter.limits(),
                    ..Default::default()
                },
                None,
            )
            .await
            .map_err(|e| JsValue::from_str(&format!("request_device failed: {e}")))?;

        let caps = surface.get_capabilities(&adapter);
        let format = caps.formats.iter().copied().find(|f| f.is_srgb()).unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);
        let depth_view = create_depth_view(&device, width, height);

        // --- THE WORLD, FROM DATA. Materials are the cited DB; the world is the definition's. ---
        let mats = materials::load();
        let sim = Simulation::from_json(&world_json, mats.clone())
            .map_err(|e| JsValue::from_str(&format!("world definition: {e}")))?;

        // --- Procedural material textures: one array layer per material, synthesized from each
        // material's CITED optical properties (albedo, colour variance, metallic). No image assets, so
        // the regolith you see is the regolith the physics is using — same row of the same database.
        let textures = texture::generate_all(&mats);
        let (n_layers, mip_count) = (textures.len() as u32, textures[0].mips.len() as u32);
        let material_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("material-textures"),
            size: wgpu::Extent3d {
                width: texture::TEX_SIZE as u32,
                height: texture::TEX_SIZE as u32,
                depth_or_array_layers: n_layers,
            },
            mip_level_count: mip_count,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (layer, t) in textures.iter().enumerate() {
            for (mip, data) in t.mips.iter().enumerate() {
                let msize = (texture::TEX_SIZE >> mip) as u32;
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &material_tex,
                        mip_level: mip as u32,
                        origin: wgpu::Origin3d { x: 0, y: 0, z: layer as u32 },
                        aspect: wgpu::TextureAspect::All,
                    },
                    data,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(4 * msize),
                        rows_per_image: Some(msize),
                    },
                    wgpu::Extent3d { width: msize, height: msize, depth_or_array_layers: 1 },
                );
            }
        }
        // The NORMAL array: same shape, same mips, uploaded from the same `Texture` values, so the
        // albedo and the relief can never describe different surfaces (docs/12).
        let normal_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("material-normals"),
            size: wgpu::Extent3d {
                width: texture::TEX_SIZE as u32,
                height: texture::TEX_SIZE as u32,
                depth_or_array_layers: n_layers,
            },
            mip_level_count: mip_count,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (layer, t) in textures.iter().enumerate() {
            for (mip, data) in t.normal_mips.iter().enumerate() {
                let msize = (texture::TEX_SIZE >> mip) as u32;
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &normal_tex,
                        mip_level: mip as u32,
                        origin: wgpu::Origin3d { x: 0, y: 0, z: layer as u32 },
                        aspect: wgpu::TextureAspect::All,
                    },
                    data,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(4 * msize),
                        rows_per_image: Some(msize),
                    },
                    wgpu::Extent3d { width: msize, height: msize, depth_or_array_layers: 1 },
                );
            }
        }
        let normal_view = normal_tex.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });

        let tex_view = material_tex.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("material-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            ..Default::default()
        });

        // Per-material shading params the world shader reads: (metallic, roughness-ish, _, _).
        let mut params = [[0.0f32; 4]; 32];
        for (i, m) in mats.iter().take(32).enumerate() {
            params[i] = [m.metallic, m.color_variance, 0.0, 0.0];
        }
        let matparams_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("matparams"),
            size: (32 * std::mem::size_of::<[f32; 4]>()) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&matparams_buf, 0, bytemuck::cast_slice(&params));

        let world_bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("world-bind-layout"),
            entries: &[
                uniform_entry(0, wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                uniform_entry(3, wgpu::ShaderStages::FRAGMENT),
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let world_ubuf = make_uniform_buffer(&device);
        let world_bind = make_world_bind(&device, &world_bind_layout, &world_ubuf, &tex_view, &sampler, &matparams_buf, &normal_view);
        let world_uni = UniformSlot { buf: world_ubuf, bind: world_bind };
        let world_pipeline = build_pipeline(&device, &world_bind_layout, format);

        let sky_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sky-bind-layout"),
            entries: &[uniform_entry(0, wgpu::ShaderStages::FRAGMENT)],
        });
        let sky_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sky-uniforms"),
            size: std::mem::size_of::<SkyUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sky_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sky-bind"),
            layout: &sky_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: sky_buf.as_entire_binding() }],
        });
        let sky_uni = UniformSlot { buf: sky_buf, bind: sky_bind };
        let sky_pipeline = build_sky_pipeline(&device, &sky_layout, format);

        let particle_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("particle-bind-layout"),
            entries: &[uniform_entry(0, wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT)],
        });
        let particle_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("particle-bind"),
            layout: &particle_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: world_uni.buf.as_entire_binding() }],
        });
        let particle_pipeline = build_particle_pipeline(&device, &particle_layout, format);

        let max_particles = 60_000usize;
        let particle_instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particle-instances"),
            size: (max_particles * std::mem::size_of::<GpuParticle>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let grain_half = 0.5 * sim.grain_size_m();
        let cube_gpu = upload_mesh(&device, "grain-cube", &mesher::build_cube(grain_half, [1.0, 1.0, 1.0]));

        let world_gpu = upload_mesh(&device, "world", &mesher::build_surface_nets(&sim.world, &mats));
        let sea_gpu = upload_mesh(&device, "sea", &mesher::build_sea(&sim.world, &mats));

        let eye_height_m = sim.eye_height_m();
        Ok(Ground {
            surface, device, queue, config, depth_view,
            world_pipeline, world_uni, sky_pipeline, sky_uni,
            particle_pipeline, particle_bind, particle_instances, cube_gpu,
            world_gpu, sea_gpu, sim, mats,
            camera: Camera { yaw: 0.6, pitch: -0.55, zoom: 1.0, base_distance: eye_height_m * 3.0 },
            eye_height_m,
            last_eye: Vec3::new(0.0, eye_height_m * 3.0, eye_height_m * 3.0),
            atm_tau: crate::atmosphere::rayleigh_tau(
                crate::planet::earth().surface_pressure() / 101_325.0),
            frame: 0,
            max_particles,
        })
    }
}

#[wasm_bindgen]
impl Ground {
    /// **Drop a meteor.** The impact resolves ONLY the region its energy actually disturbs — grains
    /// appear in the crater, not across the map (`docs/44`: necessity decides existence). Aimed at the
    /// point the camera is looking at, so what you clicked is what gets hit.
    /// **Throw a meteor.** The caller creates a rock — mass, material, where it is, how fast it is
    /// going — and lets go. The engine flies it under the planet's own gravity and handles the impact,
    /// the excavation and the settling. There is no energy parameter: the crater is ½mv² of the matter
    /// that arrives, so you get a bigger one by throwing a bigger or faster rock.
    pub fn throw_meteor(&mut self, mass_kg: f32, speed_ms: f32) {
        let iron = crate::materials::index_of(&self.mats, "iron");
        let rho = self.mats.get(iron).map(|m| m.density).unwrap_or(7870.0);
        let radius_m = (3.0 * mass_kg / (4.0 * std::f32::consts::PI * rho)).cbrt();
        // Launched high and inbound at an angle, so it ARRIVES — an impactor that materialises at the
        // surface is not an impact, it is a hole appearing.
        let (_, target) = self.eye_and_target();
        let start = target + Vec3::new(-140.0, 220.0, -90.0);
        let dir = (target - start).normalize_or(Vec3::new(0.0, -1.0, 0.0));
        self.sim.throw_meteor(crate::simulation::Meteor {
            pos: start,
            vel: dir * speed_ms,
            mass_kg,
            material: iron,
            radius_m,
        });
    }

    /// Meteors currently in flight — the HUD says one is incoming.
    pub fn meteors_in_flight(&self) -> usize {
        self.sim.meteors().len()
    }

    pub fn set_orbit(&mut self, yaw: f32, pitch: f32, zoom: f32) {
        self.camera.yaw = yaw;
        self.camera.pitch = pitch.clamp(-1.4, 0.4);
        self.camera.zoom = zoom.clamp(0.15, 6.0);
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(&self.device, &self.config);
        self.depth_view = create_depth_view(&self.device, self.config.width, self.config.height);
    }

    pub fn particle_count(&self) -> usize { self.sim.particle_count() }
    pub fn created_total(&self) -> usize { self.sim.created_total() }
    pub fn world_name(&self) -> String { self.sim.name().to_string() }
    pub fn surface_material(&self) -> String { self.sim.surface_material().to_string() }
    pub fn eye_altitude_m(&self) -> f32 { self.eye_height_m }

    /// One frame: step the physics, re-mesh if matter changed the ground, then draw sky → world → grains.
    pub fn render(&mut self) -> Result<(), JsValue> {
        self.frame = self.frame.wrapping_add(1);
        self.sim.step(1.0 / 60.0);

        // Grains that came to rest de-resolved back into voxels, so the surface changed: re-mesh. The
        // crater persists as GEOMETRY — the matter became ground again rather than being deleted.
        if self.sim.take_dirty() {
            self.world_gpu = upload_mesh(&self.device, "world",
                &mesher::build_surface_nets(&self.sim.world, &self.mats));
            self.sea_gpu = upload_mesh(&self.device, "sea",
                &mesher::build_sea(&self.sim.world, &self.mats));
        }

        let (view_proj, eye) = self.view_proj();
        self.last_eye = eye;
        let light = Vec3::new(0.45, 0.9, 0.4).normalize();
        write_uniform(&self.queue, &self.world_uni, view_proj, Mat4::IDENTITY, eye, light);
        write_sky(&self.queue, &self.sky_uni, view_proj, eye, light, self.atm_tau);

        // Grain instances: position + the material's own albedo + incandescence from its temperature.
        let inst: Vec<GpuParticle> = self
            .sim
            .particles()
            .iter()
            .take(self.max_particles)
            .map(|p| GpuParticle {
                offset: p.pos.to_array(),
                u: 0.0,
                vel: p.vel.to_array(),
                resting: 0.0,
                // The grain is drawn in ITS OWN material's albedo, from the same cited database row the
                // physics reads — so what you see is what is being simulated, not a decorative colour.
                color: self.mats.get(p.material).map(|m| m.albedo).unwrap_or([0.5, 0.5, 0.5]),
                material: p.material as f32,
                emission: crate::emission::incandescence(p.temp_k),
                rho: 0.0,
                radius: 0.5 * self.sim.grain_size_m(),
                _p0: 0.0, _p1: 0.0, _p2: 0.0,
            })
            .collect();
        // Meteors in flight are matter too, drawn through the SAME instanced path as the grains —
        // they are not a special effect layered on top.
        let mut inst = inst;
        for m in self.sim.meteors() {
            inst.push(GpuParticle {
                offset: m.pos.to_array(),
                u: 0.0,
                vel: m.vel.to_array(),
                resting: 0.0,
                color: self.mats.get(m.material).map(|mm| mm.albedo).unwrap_or([0.6, 0.5, 0.45]),
                material: m.material as f32,
                // Hypervelocity iron glows; the same incandescence law the ejecta uses.
                emission: crate::emission::incandescence(1600.0),
                rho: 0.0,
                radius: m.radius_m,
                _p0: 0.0, _p1: 0.0, _p2: 0.0,
            });
        }
        if !inst.is_empty() {
            self.queue.write_buffer(&self.particle_instances, 0, bytemuck::cast_slice(&inst));
        }

        let output = self
            .surface
            .get_current_texture()
            .map_err(|e| JsValue::from_str(&format!("get_current_texture failed: {e}")))?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("ground-frame") });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ground-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.02, g: 0.03, b: 0.05, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            // Sky first (fullscreen, behind everything), then the textured ground, then the grains.
            pass.set_pipeline(&self.sky_pipeline);
            pass.set_bind_group(0, &self.sky_uni.bind, &[]);
            pass.draw(0..3, 0..1);

            pass.set_pipeline(&self.world_pipeline);
            draw(&mut pass, &self.world_uni, &self.world_gpu);
            draw(&mut pass, &self.world_uni, &self.sea_gpu);

            if !inst.is_empty() {
                pass.set_pipeline(&self.particle_pipeline);
                pass.set_bind_group(0, &self.particle_bind, &[]);
                pass.set_vertex_buffer(0, self.cube_gpu.vertex_buf.slice(..));
                pass.set_vertex_buffer(1, self.particle_instances.slice(..));
                pass.set_index_buffer(self.cube_gpu.index_buf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..self.cube_gpu.index_count, 0, 0..inst.len() as u32);
            }
        }
        self.queue.submit(std::iter::once(enc.finish()));
        output.present();
        Ok(())
    }
}

impl Ground {
    /// Camera eye + look target. The eye orbits at the DECLARED altitude above the ground beneath it, so
    /// "20 m above the surface" stays true as you orbit over hills rather than only at the centre.
    fn eye_and_target(&self) -> (Vec3, Vec3) {
        // The subject is the ground at the origin — where the meteor lands. The eye sits at the DECLARED
        // altitude ABOVE that point and looks down at it, rather than orbiting at a fixed radius: the
        // first framing put the camera at ground level behind a dune, so half the frame was foreground
        // sand and the impact was a distant smudge.
        let ground = self.ground_at(0.0, 0.0);
        let target = Vec3::new(0.0, ground, 0.0);
        // THE RIG decides where the camera tries to be: standing off at the declared altitude above the
        // ground it is watching. Pitch aims the look; it does not drag the eye into the dirt.
        let dist = self.camera.base_distance * self.camera.zoom;
        let desired = Vec3::new(
            -dist * self.camera.yaw.sin(),
            ground + self.eye_height_m,
            -dist * self.camera.yaw.cos(),
        );
        // PHYSICS decides where it may actually be. The shell cannot enter matter, so a rig pose that
        // would put the eye inside a dune is corrected out along the surface normal — it slides, it does
        // not pop straight up, and the near plane comes with it.
        (self.camera_shell_resolve(desired), target)
    }

    /// **The camera is MATTER** — a tiny transparent shell that obeys the SAME universal terrain
    /// contact every grain does (`granular::terrain_contact_resolve`), not a geometric clamp.
    ///
    /// Robin, and it is canonical: *"If the camera isn't material, it can subvert our rules. Let's place
    /// a tiny cube of matter around the camera (transparent) so the camera can't pierce through our
    /// skin."* The `eye.y = eye.y.max(ground + h)` this replaces is precisely the clamp fudge that
    /// principle retired — it is a special case that exempts the camera from the world's rules, and it
    /// leaks (it only ever pushes straight UP, so a camera driven into a steep face pops through it
    /// rather than sliding along).
    ///
    /// CONTACT, not excavation: the shell rests against the surface and slides along it. It is
    /// deliberately NOT routed through the impact/furrow path — nudging the camera into a hillside must
    /// not blast a crater. (Ram it in at real speed and the same energy gate a meteor obeys would
    /// honestly dig; that is the rule being universal, not a bug.)
    ///
    /// **The shell half-extent is ≥ the near-clip distance.** That is what actually kills "seeing under
    /// the skin": if the shell cannot enter matter and the near plane sits inside the shell, the frustum
    /// cannot cross the surface either.
    fn camera_shell_resolve(&self, desired: Vec3) -> Vec3 {
        use glam::DVec3;
        /// ≥ `NEAR_CLIP` (0.2 m) so the near plane can never cross the surface the shell rests on.
        const SHELL_HALF: f64 = 0.35;
        /// Solver relaxation rate, not a tuned edge — the same role it plays for grains.
        const MAX_CORR: f64 = 0.5;

        let mut pos = DVec3::new(desired.x as f64, desired.y as f64, desired.z as f64);
        // SWEPT: resolve along the path from last frame to here, so a fast camera cannot tunnel through
        // the thin surface skin in one frame (a grain needs this for the same reason).
        let from = DVec3::new(self.last_eye.x as f64, self.last_eye.y as f64, self.last_eye.z as f64);
        let steps = ((pos - from).length() / (SHELL_HALF * 2.0)).ceil().clamp(1.0, 24.0) as usize;
        for i in 1..=steps {
            let t = i as f64 / steps as f64;
            let mut p = from.lerp(pos, t);
            let sample = Vec3::new(p.x as f32, p.y as f32, p.z as f32);
            let (h, dhdx, dhdz) = self.sim.world.surface_bilinear_grad(sample);
            let hit = crate::granular::terrain_contact_resolve(
                p,
                DVec3::ZERO, // the shell is carried by the rig, not ballistic: contact corrects POSITION
                h as f64,
                dhdx as f64,
                dhdz as f64,
                SHELL_HALF,
                0.0,                // frictionless: the camera slides, it does not stick to hillsides
                MAX_CORR,
                f64::INFINITY,      // open sky above the camera
            );
            if hit.hit {
                p += hit.dpos;
            }
            pos = p;
        }
        Vec3::new(pos.x as f32, pos.y as f32, pos.z as f32)
    }

    fn ground_at(&self, x: f32, z: f32) -> f32 {
        let c = self.sim.world.center();
        let (xi, zi) = ((x + c.x).floor() as i32, (z + c.z).floor() as i32);
        self.sim.world.surface_top_voxel(xi, zi).map(|t| t as f32 - c.y).unwrap_or(-c.y)
    }

    fn view_proj(&self) -> (Mat4, Vec3) {
        let (eye, target) = self.eye_and_target();
        let aspect = self.config.width as f32 / self.config.height.max(1) as f32;
        let proj = Mat4::perspective_rh(60f32.to_radians(), aspect, NEAR_CLIP, 4_000.0);
        let view = Mat4::look_at_rh(eye, target, Vec3::Y);
        (proj * view, eye)
    }
}

fn write_uniform(queue: &wgpu::Queue, slot: &UniformSlot, vp: Mat4, model: Mat4, eye: Vec3, light: Vec3) {
    let u = Uniforms {
        view_proj: vp.to_cols_array_2d(),
        model: model.to_cols_array_2d(),
        light_dir: [light.x, light.y, light.z, 0.0],
        camera_pos: [eye.x, eye.y, eye.z, 1.0],
    };
    queue.write_buffer(&slot.buf, 0, bytemuck::bytes_of(&u));
}

fn write_sky(queue: &wgpu::Queue, slot: &UniformSlot, vp: Mat4, eye: Vec3, light: Vec3, tau: [f64; 3]) {
    // Sun gain: the exposure the veil is displayed at. Recovered from the working terrain scene — with
    // the 1.0 first guessed here the Rayleigh term is far below display range and the sky renders BLACK,
    // which is exactly what the first rig shot showed.
    const SUN_GAIN: f32 = 22.0;
    // The sky reads the SAME sun direction the ground is lit by, so there is one illumination, not two.
    let u = SkyUniforms {
        inv_view_proj: vp.inverse().to_cols_array_2d(),
        sun_dir: [light.x, light.y, light.z, 0.0],
        tau: [tau[0] as f32, tau[1] as f32, tau[2] as f32, SUN_GAIN],
        camera_pos: [eye.x, eye.y, eye.z, 1.0],
    };
    queue.write_buffer(&slot.buf, 0, bytemuck::bytes_of(&u));
}
