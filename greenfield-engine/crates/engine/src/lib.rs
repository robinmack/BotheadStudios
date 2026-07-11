//! greenfield-engine core.
//!
//! Phase 2: real Newtonian **self-gravity** from the world's aggregate voxel mass, and a rigid
//! sphere that falls under it (`F = ma`) and rests on the terrain. The layered voxel world and its
//! renderer come from Phase 1; densities in `data/materials.json` are now physically active — summed
//! voxel mass produces the gravitational field the sphere obeys.
//!
//! ## Scale & time
//! The Phase-1 test world is ~96 m across, so its real surface gravity is asteroid-scale micro-g
//! (~1e-5 m/s²) — correct physics, but far too slow to watch. `G` stays real; instead a **time
//! scale** fast-forwards the simulation for viewing (time-lapse, not fake gravity).
//!
//! ## Structure & testing
//! The pure simulation logic (materials, voxel store, mesher, gravity, body) compiles and unit-tests
//! **natively** (`cargo test`). Only the rendering/host layer is gated to the wasm target. TDD is
//! canonical for this project.

// On native builds the sim modules' only non-test consumer (the wasm renderer) is compiled out, so
// their API reads as "unused" there. The wasm build still enforces dead-code detection, and tests
// exercise them. (A future `matter-core` crate split, per docs, removes the need for this.)
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

mod aggregate;
mod body;
mod damage;
mod emission;
mod granular;
mod gravity;
#[cfg(test)]
mod isotropy;
mod materials;
mod matter;
mod mesher;
mod orbit;
mod texture;
mod world;

#[cfg(target_arch = "wasm32")]
pub use app::{Engine, OrbitDemo};

/// The rendering + browser-host layer. wasm/`wgpu`-only; excluded from native builds and tests.
#[cfg(target_arch = "wasm32")]
mod app {
    use crate::mesher::{self, Mesh, Vertex};
    use crate::{aggregate, emission, gravity, materials, matter, texture, world};
    use glam::{Mat4, Vec3};
    use wasm_bindgen::prelude::*;
    use web_sys::HtmlCanvasElement;

    const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

    // Probe / simulation parameters.
    const SPAWN_HEIGHT: f32 = 12.0; // metres of clearance above the surface at spawn
    const SPHERE_RADIUS: f32 = 3.0; // rendered/collision radius — enlarged for visibility (a real
                                    // 5 kg iron ball is ~5 cm; free-fall is size- and mass-independent, so this doesn't affect the
                                    // measured acceleration).
    const SPHERE_MASS: f32 = 5.0; // kg
    const GRAVITY_SOFTENING: f32 = 6.0; // ~ mass-aggregation block size
                                        // The terrain slab is a patch of a planet, so it feels the planet's ~uniform surface gravity
                                        // (down), not the slab's own micro-g self-gravity (docs/22). Self-gravity is demonstrated at
                                        // planetary scale in the space band; here it is negligible vs the planet below.
    const SURFACE_GRAVITY: f32 = 9.81; // m/s² (Earth-like)
    const GRAVITY_BLOCK: usize = 8; // voxel aggregation for the mass field (coarser = cheaper queries)
    /// Debris substeps per frame. Higher = densely-packed grains settle cleanly (less residual energy
    /// leak from the explicit integrator) at a proportional GPU cost (docs/23). The probe substeps
    /// itself, sized to its bond stiffness (`Aggregate::stable_substeps`).
    const DEBRIS_SUBSTEPS: u32 = 16;
    const DEFAULT_TIME_SCALE: f32 = 1.0; // real-time: Earth-like surface gravity needs no fast-forward

    /// Cohesive-bond geometry + stability for the steel probe (`docs/23`). The bond stiffness is the
    /// material's REAL elastic modulus (k = E·L for a lattice of spacing L) — rigidity is cohesive
    /// force, not a fudge. But true iron (E ≈ 2.05e11 → k ≈ 2e11 N/m) would need thousands of explicit
    /// substeps/frame to stay stable; we cap k here and reach true steel only with implicit integration
    /// (flagged). The cap is still ~1000× the old hand-tuned 5e6, so the ball reads as rigid.
    const PROBE_LATTICE: f64 = 1.0; // particle spacing (m)
    const PROBE_STIFFNESS_CAP: f64 = 5.0e9; // N/m — real-time explicit-stability ceiling (flagged)

    /// Granular debris contact (`docs/23`) — the DEM model in `granular.rs`, run on the GPU and TUNED +
    /// verified on real hardware by `tools/gpu-verify`. Grains push apart, stack, settle, and flow to a
    /// slope. The PHYSICS is one grain per 1 m voxel (radius 0.5 ⇒ neighbours touch at rest); the finer
    /// look is a render-only 8× subdivision (`cs_expand`). Values chosen for explicit stability at the
    /// debris substep with coordination z≈6: soft contacts + a normal-force cap + sub-critical damping.
    const GRID_TABLE_SIZE: u32 = 1 << 18; // spatial-hash cells (≥ ~2× particle capacity → few collisions)
    const GRID_BUCKET_K: u32 = 16; // max particles recorded per cell (overflow is dropped — flagged)
    const CONTACT_RADIUS: f32 = 0.5; // = ½ the 1 m grain spacing ⇒ grains just touch at rest
    const DEBRIS_PART_HALF: f32 = 0.5; // a debris grain's collision half-extent (rests on the ground)
                                       // Stiff (real-ish) contact — kept stable by IMPLICIT integration (1/(1+dt²K) in the shader), not by
                                       // a force cap or a freeze (both removed as fudges). Verified energy-conserving on the 2070
                                       // (tools/gpu-verify scene I: total mechanical energy only ever decreases). A real angle of repose
                                       // emerges from the friction (docs/23).
    const CONTACT_STIFFNESS: f32 = 5.0e5; // normal repulsion (1/s²) per metre of overlap
    // Normal damping is no longer a constant — it's DERIVED per-material from restitution (docs/24
    // Stage 1), see `granular::damping_for_restitution` in `gpu_step_params`.
    const CONTACT_TANGENT_DAMP: f32 = 100.0; // friction ramp with slip speed

    // Phase 3 dig/fracture.
    const MAX_PARTICLES: usize = 60_000;
    const PARTICLE_CUBE_HALF: f32 = 0.21; // half of the old 0.42 — finer debris, now GPU can afford it

    /// Each physics particle (one per 1 m³ voxel) is DRAWN as 8 half-size sub-cubes at the octant
    /// centres of its cell — 8× the cubes at ½ the size (2³, cubed in volume). Purely a rendering
    /// subdivision: the physics model stays one particle per voxel (mass/conservation unchanged); this
    /// just resolves the debris more finely now that the sim runs on the GPU.
    const SUB_Q: f32 = 0.25;
    const SUB8: [[f32; 3]; 8] = [
        [-SUB_Q, -SUB_Q, -SUB_Q],
        [SUB_Q, -SUB_Q, -SUB_Q],
        [-SUB_Q, SUB_Q, -SUB_Q],
        [SUB_Q, SUB_Q, -SUB_Q],
        [-SUB_Q, -SUB_Q, SUB_Q],
        [SUB_Q, -SUB_Q, SUB_Q],
        [-SUB_Q, SUB_Q, SUB_Q],
        [SUB_Q, SUB_Q, SUB_Q],
    ];
    const DIG_RADIUS: f32 = 3.0;
    const DIG_POWER: f32 = 1.5e6; // breaks soil/grass, not granite
    const BLAST_POWER: f32 = 3.0e7; // breaks granite too
                                    // A meteor is a real nickel-iron body, not an abstract energy: its impact energy is ½·m·v²
                                    // (docs/23). ~91% iron / ~8% nickel; it vaporizes on impact into its own matter.
    const METEOR_MASS: f32 = 1_000.0; // kg (~0.3 m Fe-Ni body)
    const METEOR_SPEED: f32 = 17_000.0; // m/s (typical hypervelocity impact speed)

    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Uniforms {
        view_proj: [[f32; 4]; 4],
        model: [[f32; 4]; 4],
        light_dir: [f32; 4],
        camera_pos: [f32; 4],
    }

    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct InstanceRaw {
        offset: [f32; 3],
        color: [f32; 3],
        emission: [f32; 3], // incandescent glow from temperature (docs/20); 0 for cold debris
    }

    struct Camera {
        yaw: f32,
        pitch: f32,
        zoom: f32,
        base_distance: f32,
    }

    struct GpuMesh {
        vertex_buf: wgpu::Buffer,
        index_buf: wgpu::Buffer,
        index_count: u32,
    }

    struct UniformSlot {
        buf: wgpu::Buffer,
        bind: wgpu::BindGroup,
    }

    /// The engine handle exposed to JavaScript.
    #[wasm_bindgen]
    pub struct Engine {
        surface: wgpu::Surface<'static>,
        device: wgpu::Device,
        queue: wgpu::Queue,
        config: wgpu::SurfaceConfiguration,
        depth_view: wgpu::TextureView,
        pipeline: wgpu::RenderPipeline,

        world_gpu: GpuMesh,
        world_uni: UniformSlot,

        // Simulation
        mats: Vec<materials::Material>,
        world: world::World,
        field: gravity::MassField,
        /// The probe: a **cohesive iron ball of real matter** (`docs/23`) — falls under gravity, rests
        /// on the terrain (its bonds settle to a ground state), and **shatters emergently** when an
        /// impact breaks its bonds. No longer a rigid primitive; no special case can obliterate it.
        probe: aggregate::Aggregate,
        probe_acc: Vec<glam::DVec3>,
        probe_instances: wgpu::Buffer, // GpuParticle instances, drawn with the particle pipeline
        matter: matter::MatterSim,
        spawn: Vec3,
        time_scale: f32,

        // Debris (particle) rendering
        cube_gpu: GpuMesh,
        particle_pipeline: wgpu::RenderPipeline,
        particle_instances: wgpu::Buffer,
        particle_bind: wgpu::BindGroup,

        // GPU-compute debris (docs/22): constructed here so the compute shader/pipeline validate on the
        // device; stepping/rendering are wired incrementally.
        gpu_particles: GpuParticles,

        camera: Camera,
    }

    #[wasm_bindgen]
    impl Engine {
        /// Initialize the engine: acquire the GPU, build the world + gravity field, spawn the probe.
        pub async fn create(canvas: HtmlCanvasElement) -> Result<Engine, JsValue> {
            console_error_panic_hook::set_once();
            let _ = console_log::init_with_level(log::Level::Info);

            let width = canvas.width().max(1);
            let height = canvas.height().max(1);

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
                        label: Some("greenfield-device"),
                        required_features: wgpu::Features::empty(),
                        required_limits: adapter.limits(),
                        ..Default::default()
                    },
                    None,
                )
                .await
                .map_err(|e| JsValue::from_str(&format!("request_device failed: {e}")))?;

            let caps = surface.get_capabilities(&adapter);
            let format = caps
                .formats
                .iter()
                .copied()
                .find(|f| f.is_srgb())
                .unwrap_or(caps.formats[0]);
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

            // --- World, gravity field, and meshes ---
            let mats = materials::load();
            let world = world::generate(&mats);
            let field = gravity::MassField::build(&world, &mats, GRAVITY_BLOCK);

            let world_mesh = mesher::build_surface_nets(&world, &mats);
            let world_gpu = upload_mesh(&device, "world", &world_mesh);
            log::info!("meshes: world {} tris", world_mesh.indices.len() / 3);

            // --- Spawn the probe: a cohesive iron ball of real matter (docs/23) ---
            let c = world.center();
            let surf = world
                .surface_top_voxel(c.x as i32, c.z as i32)
                .map(|t| t as f32 - c.y)
                .unwrap_or(0.0);
            let spawn = Vec3::new(0.0, surf + SPHERE_RADIUS + SPAWN_HEIGHT, 0.0);
            let probe = build_probe(&mats, spawn);
            let probe_acc = probe.accelerations();
            let probe_instances = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("probe-instances"),
                size: (probe.particles.len() * 8 * std::mem::size_of::<GpuParticle>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            log::info!(
                "greenfield-engine: world mass = {:.3e} kg, surface g = {} m/s^2 (planetary)",
                field.total_mass,
                SURFACE_GRAVITY
            );

            // --- Procedural material textures (Phase 4): a mip-mapped array, one layer per material.
            let textures = texture::generate_all(&mats);
            let n_layers = textures.len() as u32;
            let mip_count = textures[0].mips.len() as u32;
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
                            origin: wgpu::Origin3d {
                                x: 0,
                                y: 0,
                                z: layer as u32,
                            },
                            aspect: wgpu::TextureAspect::All,
                        },
                        data,
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(4 * msize),
                            rows_per_image: Some(msize),
                        },
                        wgpu::Extent3d {
                            width: msize,
                            height: msize,
                            depth_or_array_layers: 1,
                        },
                    );
                }
            }
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
            // Per-material shine params: [roughness, metallic, _, _] (padded to 32 for the shader).
            let mut params: Vec<[f32; 4]> = vec![[0.0; 4]; 32];
            for (i, m) in mats.iter().enumerate().take(32) {
                params[i] = [m.roughness, m.metallic, 0.0, 0.0];
            }
            let matparams_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("matparams"),
                size: (32 * std::mem::size_of::<[f32; 4]>()) as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&matparams_buf, 0, bytemuck::cast_slice(&params));

            // --- Bind group layouts: world (uniform + texture + sampler + params); particles (uniform) ---
            let world_bind_layout =
                device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
                    ],
                });
            let particle_bind_layout =
                device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("particle-bind-layout"),
                    entries: &[uniform_entry(
                        0,
                        wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    )],
                });

            // --- Uniform buffers + bind groups ---
            let world_ubuf = make_uniform_buffer(&device);
            let world_uni = UniformSlot {
                bind: make_world_bind(
                    &device,
                    &world_bind_layout,
                    &world_ubuf,
                    &tex_view,
                    &sampler,
                    &matparams_buf,
                ),
                buf: world_ubuf,
            };
            let pipeline = build_pipeline(&device, &world_bind_layout, config.format);

            // Debris: a unit cube instanced per particle, tinted by material albedo.
            let matter = matter::MatterSim::new(MAX_PARTICLES);

            // GPU-compute debris (docs/22): construct the storage buffer + compute pipeline (this
            // validates `particle_step.wgsl` on the device) and upload the terrain heightfield the step
            // collides against.
            let mut gpu_particles = GpuParticles::new(
                &device,
                MAX_PARTICLES as u32, // physics grains (1 per voxel); render_buf is ×8 internally
                (world.w * world.d) as u32,
            );
            let mut tops: Vec<i32> = Vec::with_capacity(world.w * world.d);
            for z in 0..world.d {
                for x in 0..world.w {
                    tops.push(world.surface_top_voxel(x as i32, z as i32).unwrap_or(-1));
                }
            }
            gpu_particles.upload_heightfield(&queue, &tops);
            let cube_gpu = upload_mesh(
                &device,
                "cube",
                &mesher::build_cube(PARTICLE_CUBE_HALF, [1.0, 1.0, 1.0]),
            );
            let particle_instances = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("particle-instances"),
                size: (MAX_PARTICLES * std::mem::size_of::<InstanceRaw>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let particle_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("particle-bind"),
                layout: &particle_bind_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: world_uni.buf.as_entire_binding(),
                }],
            });
            let particle_pipeline =
                build_particle_pipeline(&device, &particle_bind_layout, config.format);

            let max_dim = world.w.max(world.h).max(world.d) as f32;
            let camera = Camera {
                yaw: 0.7,
                pitch: 0.5,
                zoom: 1.0,
                base_distance: max_dim * 1.6,
            };

            Ok(Engine {
                surface,
                device,
                queue,
                config,
                depth_view,
                pipeline,
                world_gpu,
                world_uni,
                mats,
                world,
                field,
                probe,
                probe_acc,
                probe_instances,
                matter,
                spawn,
                time_scale: DEFAULT_TIME_SCALE,
                cube_gpu,
                particle_pipeline,
                particle_instances,
                particle_bind,
                gpu_particles,
                camera,
            })
        }

        /// Update the orbit camera. `yaw`/`pitch` in radians; `zoom` scales the base distance.
        pub fn set_orbit(&mut self, yaw: f32, pitch: f32, zoom: f32) {
            self.camera.yaw = yaw;
            self.camera.pitch = pitch.clamp(-1.5, 1.5);
            self.camera.zoom = zoom.clamp(0.2, 6.0);
        }

        /// Reconfigure the surface and depth buffer when the canvas size changes.
        pub fn resize(&mut self, width: u32, height: u32) {
            if width > 0 && height > 0 {
                self.config.width = width;
                self.config.height = height;
                self.surface.configure(&self.device, &self.config);
                self.depth_view = create_depth_view(&self.device, width, height);
            }
        }

        // --- Live stats for the HUD ---
        pub fn total_mass(&self) -> f64 {
            self.field.total_mass as f64
        }
        /// The planetary surface gravity the probe feels (m/s²) — the "measured g".
        pub fn surface_gravity(&self) -> f32 {
            SURFACE_GRAVITY
        }
        pub fn sphere_altitude(&self) -> f32 {
            // Lowest particle of the ball above the terrain directly under its centre of mass.
            let com = self.probe.com();
            let ground = self.ground_under(com.x as f32, com.z as f32);
            let low = self
                .probe
                .particles
                .iter()
                .map(|p| p.pos.y as f32)
                .fold(f32::MAX, f32::min);
            low - ground
        }
        pub fn sphere_speed(&self) -> f32 {
            // COM speed of the ball.
            let m = self.probe.total_mass();
            if m <= 0.0 {
                return 0.0;
            }
            let v = self
                .probe
                .particles
                .iter()
                .fold(glam::DVec3::ZERO, |s, p| s + p.vel * p.mass)
                / m;
            v.length() as f32
        }
        /// Fraction of the probe's bonds still intact — 1.0 whole, 0.0 fully shattered (HUD).
        pub fn probe_integrity(&self) -> f32 {
            let total = self.probe.bonds.len();
            if total == 0 {
                return 0.0;
            }
            self.probe.active_bonds() as f32 / total as f32
        }
        pub fn time_scale(&self) -> f32 {
            self.time_scale
        }
        pub fn set_time_scale(&mut self, s: f32) {
            self.time_scale = s.clamp(1.0, 5000.0);
        }
        /// Re-drop a fresh probe from its spawn point (re-forms a whole ball).
        pub fn reset_drop(&mut self) {
            self.probe = build_probe(&self.mats, self.spawn);
            self.probe_acc = self.probe.accelerations();
        }

        /// Number of airborne debris particles (HUD).
        /// The number of debris particles ACTUALLY in the scene — the GPU count (the CPU `matter`
        /// buffer is cleared to ~0 right after each flush, so it was misreporting). This is the honest
        /// conservation readout: it only ever rises on a dig/meteor and never per-frame, so if the
        /// scene looks like it is "creating matter" while this number holds constant, the culprit is
        /// recirculating energy, not new matter (docs/23).
        pub fn particle_count(&self) -> u32 {
            self.gpu_particles.count
        }

        /// Dig at a screen point (normalized device coords, y up). `blast` uses a stronger tool that
        /// can break rock. Casts a ray from the camera and fractures the first solid voxel region.
        pub fn dig(&mut self, ndc_x: f32, ndc_y: f32, blast: bool) {
            let (view_proj, eye) = self.view_proj();
            let inv = view_proj.inverse();
            let near = inv.project_point3(Vec3::new(ndc_x, ndc_y, 0.0));
            let far = inv.project_point3(Vec3::new(ndc_x, ndc_y, 1.0));
            let dir = (far - near).normalize_or_zero();
            if let Some((_x, _y, _z, hit)) = self.world.raycast(eye, dir, 6000.0) {
                let power = if blast { BLAST_POWER } else { DIG_POWER };
                self.matter
                    .dig(&mut self.world, &self.mats, hit, DIG_RADIUS, power);
                // Anything the dig undercut or isolated now collapses and falls.
                self.matter.collapse(&mut self.world, &self.mats);
                self.flush_debris_to_gpu();
            }
        }

        /// Fire a **meteor** at a screen point: a high-energy `impact` that carves a crater and throws
        /// incandescent ejecta — the centre melts and glows, the rim is cold rubble (`docs/20`). Same
        /// operator as a bullet or a moon, just more energy.
        pub fn meteor(&mut self, ndc_x: f32, ndc_y: f32) {
            let (view_proj, eye) = self.view_proj();
            let inv = view_proj.inverse();
            let near = inv.project_point3(Vec3::new(ndc_x, ndc_y, 0.0));
            let far = inv.project_point3(Vec3::new(ndc_x, ndc_y, 1.0));
            let dir = (far - near).normalize_or_zero();
            if let Some((_x, _y, _z, hit)) = self.world.raycast(eye, dir, 6000.0) {
                // The meteor is a real Fe-Ni body: its impact energy is its kinetic energy, ½·m·v².
                let energy = 0.5 * METEOR_MASS * METEOR_SPEED * METEOR_SPEED;

                // TERRAIN-AS-MATTER (docs/24 Stage 3): instead of carving a crater and handing every
                // ejecta grain a scripted outward velocity (the old `impact` fudge), we MATERIALIZE the
                // impact region into real grains and drive them with the meteor's real momentum. The
                // crater, the ejecta curtain, and the fallback all EMERGE from the same conservative
                // grain contact the debris already uses — no assigned ejecta speed anywhere.
                //   1. size the disturbed region from the σ·V crater relation (docs/19), LOD-capped;
                //   2. materialize its solid voxels into grains at rest (mass + PE conserved);
                //   3. deposit the meteor's momentum p = m·v as an impulse on the coupling core (exact
                //      momentum conservation) — ejection emerges from the compression/rebound;
                //   4. the energy the impulse did NOT turn into motion is shock HEAT (radial gradient).
                let hitv = hit + self.world.center();
                let strength = self
                    .world
                    .material_at(hitv.x as i32, hitv.y as i32, hitv.z as i32)
                    .map_or(1.2e7, |m| self.mats[m].fracture_strength);
                let crater_r = crate::damage::crater_radius(crate::damage::crater_volume(
                    energy as f64,
                    strength as f64,
                ));
                const MATERIALIZE_CAP: f32 = 14.0; // LOD guard: bound the materialized grain count
                let mat_r = (crater_r as f32).min(MATERIALIZE_CAP);
                let start = self.matter.particle_count();
                self.matter
                    .materialize_region(&mut self.world, &self.mats, hit, mat_r);
                // Path B (docs/24): turn any STEEP terrain the ejecta will hit (old crater walls, cliffs)
                // into grains too — a heightfield can't represent a vertical wall conservatively, and that
                // was the last energy injector the drag fudge masked. Now the terrain the debris touches
                // is either grains or a gentle bilinear surface — both conservative.
                self.matter
                    .materialize_steep_terrain(&mut self.world, &self.mats, hit, mat_r * 2.0, 3);
                let momentum = dir * (METEOR_MASS * METEOR_SPEED);
                let core_r = (mat_r * 0.35).max(2.0); // the impactor's coupling footprint
                self.matter.deposit_impulse(start, hit, momentum, core_r);
                // Heat = impact energy minus the bulk KE the impulse just added (a small % — most of a
                // fast impactor's ½mv² is heat, which falls out of momentum-vs-energy, not a magic 5%).
                let bulk_ke: f32 = self.matter.particles[start..]
                    .iter()
                    .map(|p| 0.5 * p.mass * p.vel.length_squared())
                    .sum();
                self.matter.deposit_shock_heat(
                    start,
                    hit,
                    (energy - bulk_ke).max(0.0),
                    &self.mats,
                );
                // Vapor-driven ejection (docs/24, Robin's model): the shock heat that pushed matter past
                // vaporization flashes to gas; its expansion throws the ejecta curtain and carves the
                // bowl. Routes the heat we just deposited into radial ejecta KE (thermal → kinetic,
                // conserved) — the honest engine of the crater, replacing any scripted ejecta speed.
                self.matter.deposit_vapor_expansion(start, hit, &self.mats);
                self.matter.collapse(&mut self.world, &self.mats);
                self.flush_debris_to_gpu();

                // The meteor is NOT assumed to vaporize — at 1 m resolution its 1000 kg of Fe-Ni is ~0.13 m³,
                // SUB-GRAIN, so we don't model its body at all; we couple its ENERGY and MOMENTUM into the
                // ground (materialize + impulse + shock heat + vapor expansion) and let the outcome emerge.
                // (Whether that energy density vaporizes anything is decided by the target material's own
                // threshold — for this 17 km/s strike into soil it does, and the incandescent plume is the
                // real vaporized GROUND core (~9800 K), not a scripted Fe-Ni cloud. Removed the old
                // `spawn_vaporized_meteor`: a cosmetic 64-particle burst with a SCRIPTED 22 m/s expansion
                // that stayed an intact-looking clump AND double-counted the meteor's momentum.)

                // Couple the meteor into every body in its path — NOT just the probe (docs/23: everything
                // is matter; the impact doesn't special-case any object). One body today, N later; the
                // loop is the same.
                let eye_d = glam::DVec3::new(eye.x as f64, eye.y as f64, eye.z as f64);
                let dir_d =
                    glam::DVec3::new(dir.x as f64, dir.y as f64, dir.z as f64).normalize_or_zero();
                let hit_d = glam::DVec3::new(hit.x as f64, hit.y as f64, hit.z as f64);
                self.couple_impact_to_bodies(eye_d, dir_d, hit_d, energy as f64);
            }
        }

        /// Couple a meteor/blast into EVERY impactable body in the zone — the impact is object-agnostic
        /// (docs/23: everything is matter, no special-case for "the probe"). For each body: if the ray
        /// passes through it before the ground it takes a DIRECT hit (full energy at the entry point);
        /// otherwise the blast wave reaches it, energy falling off with distance from ground zero. Each
        /// body is struck by the SAME honest pipeline as the terrain — momentum impulse + shock heat +
        /// vapor expansion (`Aggregate::deposit_impact`), no scripted kick. `self.probe` is the only body
        /// today; add more to the `bodies` slice and the loop is unchanged — the multi-object case is
        /// built in, not retrofitted.
        fn couple_impact_to_bodies(
            &mut self,
            eye: glam::DVec3,
            dir: glam::DVec3,
            ground: glam::DVec3,
            energy: f64,
        ) {
            const RAY_CAPTURE: f64 = 0.6; // ~ a body's particle spacing
            let terrain_t = (ground - eye).dot(dir); // along-ray distance to the ground
            let momentum_mag = (METEOR_MASS * METEOR_SPEED) as f64;
            let sigma = self.mats[materials::index_of(&self.mats, "granite")].fracture_strength as f64;
            let reach =
                crate::damage::crater_radius(crate::damage::crater_volume(energy, sigma)).max(1.0);
            let mats = &self.mats;
            // The impactable bodies. One probe today; extend this slice to N bodies — nothing else changes.
            for body in [&mut self.probe] {
                // Direct hit? The ray passes through this body before it reaches the ground.
                let mut direct: Option<glam::DVec3> = None;
                let mut best_t = terrain_t;
                for p in &body.particles {
                    let rel = p.pos - eye;
                    let t = rel.dot(dir);
                    if t <= 0.0 || t >= best_t {
                        continue;
                    }
                    if (rel - dir * t).length() < RAY_CAPTURE {
                        best_t = t;
                        direct = Some(p.pos);
                    }
                }
                let (site, e_at) = match direct {
                    Some(pos) => (pos, energy), // direct hit — full energy at the entry point
                    None => (ground, energy * (-(body.com() - ground).length() / reach).exp()),
                };
                // Real momentum too (p = m·v), with the same falloff as the energy.
                let p_at = dir * momentum_mag * (e_at / energy);
                body.deposit_impact(mats, site, p_at, e_at);
            }
        }

        fn remesh_world(&mut self) {
            let mesh = mesher::build_surface_nets(&self.world, &self.mats);
            self.world_gpu = upload_mesh(&self.device, "world", &mesh);
        }

        /// Move newly-fractured CPU particles into the GPU debris buffer, then clear them from the CPU:
        /// the GPU owns debris now and steps them on the compute shader (docs/22). Called after a
        /// dig/meteor fractures voxels.
        fn flush_debris_to_gpu(&mut self) {
            if self.matter.particles.is_empty() {
                return;
            }
            // ONE physics grain per voxel (the finer look is a render-only expansion, cs_expand).
            let gpu: Vec<GpuParticle> = self
                .matter
                .particles
                .iter()
                .map(|p| GpuParticle {
                    offset: [p.pos.x, p.pos.y, p.pos.z],
                    temp: p.temp_k,
                    vel: [p.vel.x, p.vel.y, p.vel.z],
                    resting: 0.0,
                    color: self.mats[p.material].albedo,
                    material: p.material as f32,
                    emission: emission::incandescence(p.temp_k),
                    _pad: 0.0,
                })
                .collect();
            self.gpu_particles.append(&self.queue, &gpu);
            self.matter.particles.clear();
        }

        /// Re-upload the terrain heightfield (the GPU step collides debris against it) after the world
        /// changes (a dig/impact alters column tops).
        /// Upload the probe's particles to its render instance buffer, glowing by temperature.
        fn upload_probe_instances(&self) {
            let albedo = self.mats[self.probe.material].albedo;
            let mat = self.probe.material as f32;
            let inst: Vec<GpuParticle> = self
                .probe
                .particles
                .iter()
                .zip(self.probe.temps.iter())
                .flat_map(|(p, &t)| {
                    let (px, py, pz) = (p.pos.x as f32, p.pos.y as f32, p.pos.z as f32);
                    let emission = emission::incandescence(t);
                    SUB8.map(|o| GpuParticle {
                        offset: [px + o[0], py + o[1], pz + o[2]],
                        temp: t,
                        vel: [0.0, 0.0, 0.0],
                        resting: 0.0,
                        color: albedo,
                        material: mat,
                        emission,
                        _pad: 0.0,
                    })
                })
                .collect();
            if !inst.is_empty() {
                self.queue
                    .write_buffer(&self.probe_instances, 0, bytemuck::cast_slice(&inst));
            }
        }

        fn upload_heightfield_to_gpu(&self) {
            let (w, d) = (self.world.w, self.world.d);
            let mut tops = Vec::with_capacity(w * d);
            for z in 0..d {
                for x in 0..w {
                    tops.push(
                        self.world
                            .surface_top_voxel(x as i32, z as i32)
                            .unwrap_or(-1),
                    );
                }
            }
            self.gpu_particles.upload_heightfield(&self.queue, &tops);
        }

        /// Uniforms for one GPU debris substep of `dt` seconds (keeps the constants in sync with
        /// `matter.rs`, the single source of truth).
        fn gpu_step_params(&self, dt: f32) -> GpuStepParams {
            let c = self.world.center();
            // Debris friction comes from the REAL material (granite — the bulk rock), not a tuned
            // number: the angle of repose emerges from it (docs/23). Mixed-material debris using one
            // representative μ is a flagged approximation (a per-particle μ is a later refinement).
            let bulk = &self.mats[materials::index_of(&self.mats, "granite")];
            let friction = bulk.friction_coefficient;
            // Normal damping DERIVED from the material's coefficient of restitution (docs/24 Stage 1):
            // how bouncy debris is — and how strongly an impact rebounds into ejecta — is a material
            // property, not a dial. Same representative-material approximation as friction (flagged).
            let normal_damp = crate::granular::damping_for_restitution(
                bulk.restitution as f64,
                CONTACT_STIFFNESS as f64,
            ) as f32;
            // Cohesion (attractive adhesion between grains) DERIVED from the material — how a pile holds a
            // slope, and whether touching grains bond, is a material property (docs/24). Per-mass
            // acceleration = σ·A / (ρ·V) (A = grain cross-section). The INTACT σ is capped at a granular
            // ceiling: loose debris is already fractured and retains only surface adhesion, so rock debris
            // must not re-weld into solid. Representative-material approximation, like the friction (flagged).
            let grain_area = std::f32::consts::PI * CONTACT_RADIUS * CONTACT_RADIUS;
            const GRANULAR_COHESION_CEIL: f32 = 5.0e4; // Pa — clay-level; loose-debris adhesion ceiling
            let c_cohesion =
                bulk.cohesion.min(GRANULAR_COHESION_CEIL) * grain_area / bulk.density.max(1.0);
            GpuStepParams {
                gravity: [0.0, -SURFACE_GRAVITY, 0.0],
                dt,
                center: [c.x, c.y, c.z],
                c_cohesion,
                drag: matter::DRAG,
                contact_damp: matter::CONTACT_DAMP,
                settle_speed: 0.0, // (unused — settle "freeze" removed as a fudge)
                part_half: DEBRIS_PART_HALF, // the 1 m physics grain's collision half-extent
                cool_rate: 0.4,    // 1/s — molten debris fades over a few seconds (docs/20)
                count: self.gpu_particles.count,
                world_w: self.world.w as u32,
                world_d: self.world.d as u32,
                cell_size: 2.0 * CONTACT_RADIUS, // ≥ contact diameter ⇒ contacts stay within ±1 cell
                table_mask: GRID_TABLE_SIZE - 1,
                bucket_k: GRID_BUCKET_K,
                c_radius: CONTACT_RADIUS,
                c_stiffness: CONTACT_STIFFNESS,
                c_normal_damp: normal_damp,
                c_friction: friction,
                c_tangent_damp: CONTACT_TANGENT_DAMP,
            }
        }

        /// Render one frame (advances the simulation first).
        pub fn render(&mut self) -> Result<(), JsValue> {
            self.step_physics();
            if self.matter.take_dirty() {
                self.remesh_world();
                self.upload_heightfield_to_gpu(); // the crater changed the column tops
            }

            let (view_proj, eye) = self.view_proj();
            let light = Vec3::new(0.45, 0.9, 0.4).normalize();
            self.write_uniform(&self.world_uni, view_proj, Mat4::IDENTITY, eye, light);
            self.upload_probe_instances(); // the probe is drawn as its particles now

            let output = self
                .surface
                .get_current_texture()
                .map_err(|e| JsValue::from_str(&format!("get_current_texture failed: {e}")))?;
            let view = output
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("frame-encoder"),
                });

            // GPU debris step (docs/22, docs/23): advance all particles on the compute shader,
            // DEBRIS_SUBSTEPS times (each stage is its own pass, so they chain). Densely-packed grains
            // oscillate fast, so too few substeps leaks a trickle of energy (the residual crater
            // "convection" that settles far too slowly — verified in tools/gpu-verify: 8→16 substeps
            // cut the settled speed ~5×). FPS cost is real; the proper offset is decoupling the 8×
            // render subdivision from the physics particle (a later refinement).
            let particle_count = self.gpu_particles.count;
            if particle_count > 0 {
                let dt = (self.time_scale / 60.0) / DEBRIS_SUBSTEPS as f32;
                self.gpu_particles
                    .set_params(&self.queue, &self.gpu_step_params(dt));
                for _ in 0..MOON_DEBRIS_SUBSTEPS {
                    self.gpu_particles.dispatch(&mut encoder);
                }
                // Expand the settled physics grains into the 8× render sub-cubes (once, after stepping).
                self.gpu_particles.expand(&mut encoder);
            }

            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("world-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.55,
                                g: 0.70,
                                b: 0.90,
                                a: 1.0,
                            }),
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
                pass.set_pipeline(&self.pipeline);
                draw(&mut pass, &self.world_uni, &self.world_gpu);

                // Particle pipeline: the probe (cohesive ball, its particles) + the GPU debris.
                pass.set_pipeline(&self.particle_pipeline);
                pass.set_bind_group(0, &self.particle_bind, &[]);
                pass.set_vertex_buffer(0, self.cube_gpu.vertex_buf.slice(..));
                pass.set_index_buffer(self.cube_gpu.index_buf.slice(..), wgpu::IndexFormat::Uint32);

                let probe_count = (self.probe.particles.len() * 8) as u32; // 8 sub-cubes each
                if probe_count > 0 {
                    pass.set_vertex_buffer(1, self.probe_instances.slice(..));
                    pass.draw_indexed(0..self.cube_gpu.index_count, 0, 0..probe_count);
                }
                if particle_count > 0 {
                    // Draw the 8× render sub-cubes (cs_expand filled them from the physics grains).
                    pass.set_vertex_buffer(1, self.gpu_particles.render_buf.slice(..));
                    pass.draw_indexed(0..self.cube_gpu.index_count, 0, 0..particle_count * 8);
                }
            }
            self.queue.submit(std::iter::once(encoder.finish()));
            output.present();
            Ok(())
        }

        // --- internals ---

        fn step_physics(&mut self) {
            let sim_dt = (self.time_scale / 60.0) as f64;
            // The probe is a cohesive iron ball of real matter (docs/23): step its bonds + gravity
            // (settling to a ground state), then rest its particles on the terrain. Its bonds are stiff
            // (real steel), so explicit integration needs a fine timestep to stay stable — size the
            // substep count to the bond stiffness rather than faking rigidity. Debris runs on the GPU.
            let sub = self.probe.stable_substeps(sim_dt).clamp(1, 256);
            let pdt = sim_dt / sub as f64;
            for _ in 0..sub {
                self.probe.step(&mut self.probe_acc, pdt);
                self.collide_probe_with_terrain();
            }
        }

        /// Rest each probe particle on the terrain surface under it (a fixed floor per column). The
        /// bonds transmit the support up, so the ball rests as a ball; dig the surface away and its
        /// support is really gone — it sags/falls emergently.
        fn collide_probe_with_terrain(&mut self) {
            let c = self.world.center();
            let half = matter::PARTICLE_HALF as f64;
            for p in &mut self.probe.particles {
                let xi = (p.pos.x as f32 + c.x).floor() as i32;
                let zi = (p.pos.z as f32 + c.z).floor() as i32;
                let ground = self
                    .world
                    // −0.5: surface_top_voxel is the air-start voxel, but the surface-nets iso-surface
                    // (what's drawn) sits half a voxel below it — rest on the VISIBLE surface, not above.
                    .surface_top_voxel(xi, zi)
                    .map(|t| t as f64 - 0.5 - c.y as f64)
                    .unwrap_or(-1.0e9);
                let floor = ground + half;
                if p.pos.y < floor {
                    // Correct only the penetration BEYOND a small dead zone, and gently. Hard-snapping
                    // the tiny per-substep penetration of a RESTING probe pumps potential energy into
                    // its stiff bonds every substep — that was the probe's "free energy" (it vibrated
                    // apart and its scattered particles fell forever). The dead zone lets it rest with a
                    // hair of sink and injects nothing; a deep penetration (a hard landing) is eased out,
                    // not snapped. Velocity clamp + friction only ever REMOVE energy. (The clean fix is
                    // implicit integration of the stiff bonds — flagged, docs/23.)
                    const DEAD: f64 = 0.15;
                    let pen = floor - p.pos.y;
                    if pen > DEAD {
                        p.pos.y += 0.5 * (pen - DEAD);
                    }
                    if p.vel.y < 0.0 {
                        p.vel.y = 0.0;
                    }
                    p.vel.x *= 0.5; // ground friction (crude; emergent friction is future, docs/23)
                    p.vel.z *= 0.5;
                }
            }
        }

        /// Terrain surface height (centered coords) under a column; far below off the footprint.
        fn ground_under(&self, x: f32, z: f32) -> f32 {
            let c = self.world.center();
            let (xi, zi) = ((x + c.x).floor() as i32, (z + c.z).floor() as i32);
            self.world
                .surface_top_voxel(xi, zi)
                .map(|t| t as f32 - c.y)
                .unwrap_or(-c.y - 1.0)
        }

        fn view_proj(&self) -> (Mat4, Vec3) {
            let aspect = self.config.width as f32 / self.config.height.max(1) as f32;
            let proj = Mat4::perspective_rh(0.9, aspect, 0.5, 6000.0);
            let cp = self.camera.pitch.cos();
            let dir = Vec3::new(
                cp * self.camera.yaw.sin(),
                self.camera.pitch.sin(),
                cp * self.camera.yaw.cos(),
            );
            let eye = dir * (self.camera.base_distance * self.camera.zoom);
            let view = Mat4::look_at_rh(eye, Vec3::ZERO, Vec3::Y);
            (proj * view, eye)
        }

        fn write_uniform(
            &self,
            slot: &UniformSlot,
            view_proj: Mat4,
            model: Mat4,
            eye: Vec3,
            light: Vec3,
        ) {
            let u = Uniforms {
                view_proj: view_proj.to_cols_array_2d(),
                model: model.to_cols_array_2d(),
                light_dir: [light.x, light.y, light.z, 0.0],
                camera_pos: [eye.x, eye.y, eye.z, 1.0],
            };
            self.queue
                .write_buffer(&slot.buf, 0, bytemuck::bytes_of(&u));
        }
    }

    fn draw<'a>(pass: &mut wgpu::RenderPass<'a>, uni: &'a UniformSlot, mesh: &'a GpuMesh) {
        pass.set_bind_group(0, &uni.bind, &[]);
        pass.set_vertex_buffer(0, mesh.vertex_buf.slice(..));
        pass.set_index_buffer(mesh.index_buf.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..mesh.index_count, 0, 0..1);
    }

    fn uniform_entry(binding: u32, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
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

    fn make_uniform_buffer(device: &wgpu::Device) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn make_world_bind(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        ubuf: &wgpu::Buffer,
        tex_view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
        matparams: &wgpu::Buffer,
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
            source: wgpu::ShaderSource::Wgsl(include_str!("../../../shaders/world.wgsl").into()),
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

    // ============================================================================================
    // GPU-compute debris particles (docs/22). Particles live in a storage buffer stepped by a compute
    // shader (one thread each) and rendered from the SAME buffer (zero-copy sim↔render). This is the
    // engine's north-star architecture and the fix for the single-digit FPS after a big impact.
    // ============================================================================================

    /// One GPU particle — 64 bytes, four 16-byte rows. Layout matches `particle_step.wgsl`'s `Particle`
    /// and is read directly by the renderer (offset @0, color @32, emission @48).
    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct GpuParticle {
        offset: [f32; 3], // position (centered coords) = the render instance offset
        temp: f32,        // K
        vel: [f32; 3],
        resting: f32,       // 0 in flight, 1 settled
        color: [f32; 3],    // material albedo (set on spawn)
        material: f32,      // material index (informational)
        emission: [f32; 3], // incandescent glow (written by the compute step)
        _pad: f32,
    }

    /// Per-dispatch uniforms for the compute step — matches `particle_step.wgsl`'s `Params` (80 bytes).
    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct GpuStepParams {
        gravity: [f32; 3], // uniform planetary surface gravity (m/s²)
        dt: f32,
        center: [f32; 3],
        c_cohesion: f32, // attractive adhesion between touching grains (docs/24)
        drag: f32,
        contact_damp: f32,
        settle_speed: f32,
        part_half: f32,
        cool_rate: f32,
        count: u32,
        world_w: u32,
        world_d: u32,
        // Granular spatial hash + contact (docs/23) — mirrors particle_step.wgsl's Params tail.
        cell_size: f32,
        table_mask: u32,
        bucket_k: u32,
        c_radius: f32,
        c_stiffness: f32,
        c_normal_damp: f32,
        c_friction: f32,
        c_tangent_damp: f32,
    }

    /// GPU-resident debris: a storage+vertex buffer of `GpuParticle`, a compute pipeline that steps it,
    /// and a heightfield the step collides against. The CPU only appends new particles (on fracture)
    /// and updates the per-frame params; the physics runs entirely on the GPU.
    struct GpuParticles {
        buf: wgpu::Buffer,         // STORAGE — the PHYSICS grains (1 per voxel), stepped
        render_buf: wgpu::Buffer, // STORAGE | VERTEX — 8× render sub-cubes (cs_expand fills it), drawn
        params: wgpu::Buffer,     // UNIFORM | COPY_DST
        heightfield: wgpu::Buffer, // STORAGE | COPY_DST
        grid_count: wgpu::Buffer, // STORAGE — atomic per-cell particle count (spatial hash)
        grid_bucket: wgpu::Buffer, // STORAGE — per-cell particle indices
        forces: wgpu::Buffer,     // STORAGE — accumulated contact acceleration per particle
        clear: wgpu::ComputePipeline,
        insert: wgpu::ComputePipeline,
        force_pass: wgpu::ComputePipeline,
        integrate: wgpu::ComputePipeline,
        expand: wgpu::ComputePipeline, // 1 grain → 8 render sub-cubes
        bind: wgpu::BindGroup,
        capacity: u32,
        count: u32,
    }

    impl GpuParticles {
        fn new(device: &wgpu::Device, capacity: u32, world_cells: u32) -> Self {
            let buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("gpu-particles-physics"),
                size: (capacity as usize * std::mem::size_of::<GpuParticle>()) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            // 8× render sub-cubes, filled each frame by cs_expand and drawn as instances.
            let render_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("gpu-particles-render"),
                size: (capacity as usize * 8 * std::mem::size_of::<GpuParticle>()) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::VERTEX,
                mapped_at_creation: false,
            });
            let params = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("gpu-particles-params"),
                size: std::mem::size_of::<GpuStepParams>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let heightfield = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("gpu-particles-heightfield"),
                size: (world_cells as usize * std::mem::size_of::<i32>()).max(4) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            // Spatial-hash grid + per-particle contact-force scratch (docs/23).
            let grid_count = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("gpu-grid-count"),
                size: (GRID_TABLE_SIZE as u64) * std::mem::size_of::<u32>() as u64,
                usage: wgpu::BufferUsages::STORAGE,
                mapped_at_creation: false,
            });
            let grid_bucket = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("gpu-grid-bucket"),
                size: (GRID_TABLE_SIZE as u64)
                    * (GRID_BUCKET_K as u64)
                    * std::mem::size_of::<u32>() as u64,
                usage: wgpu::BufferUsages::STORAGE,
                mapped_at_creation: false,
            });
            let forces = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("gpu-particle-forces"),
                // `Accum` (particle_step.wgsl): contact force + the 6-component stiffness/damping tensor
                // S = Σ g·(n⊗n) + the momentum-coupling vector Σ S·v_neighbor for the DIRECTIONAL implicit
                // solve — 64 bytes (four 16-byte rows).
                size: (capacity as u64) * 64,
                usage: wgpu::BufferUsages::STORAGE,
                mapped_at_creation: false,
            });

            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("particle-step"),
                source: wgpu::ShaderSource::Wgsl(
                    include_str!("../../../shaders/particle_step.wgsl").into(),
                ),
            });

            let storage = |binding: u32, read_only: bool| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            };
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("gpu-particles-layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    storage(1, false), // particles (physics)
                    storage(2, true),  // heightfield
                    storage(3, false), // grid_count
                    storage(4, false), // grid_bucket
                    storage(5, false), // forces
                    storage(6, false), // render_out (8× render sub-cubes)
                ],
            });
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("gpu-particles-pipeline-layout"),
                bind_group_layouts: &[&layout],
                push_constant_ranges: &[],
            });
            let mk = |entry: &str| {
                device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some(entry),
                    layout: Some(&pipeline_layout),
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    cache: None,
                })
            };
            let clear = mk("cs_grid_clear");
            let insert = mk("cs_grid_insert");
            let force_pass = mk("cs_forces");
            let integrate = mk("cs_integrate");
            let expand = mk("cs_expand");
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("gpu-particles-bind"),
                layout: &layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: params.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: heightfield.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: grid_count.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: grid_bucket.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: forces.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: render_buf.as_entire_binding(),
                    },
                ],
            });

            GpuParticles {
                buf,
                render_buf,
                params,
                heightfield,
                grid_count,
                grid_bucket,
                forces,
                clear,
                insert,
                force_pass,
                integrate,
                expand,
                bind,
                capacity,
                count: 0,
            }
        }

        /// Upload the terrain heightfield (per-column air-start Y) the step collides against.
        fn upload_heightfield(&self, queue: &wgpu::Queue, tops: &[i32]) {
            queue.write_buffer(&self.heightfield, 0, bytemuck::cast_slice(tops));
        }

        /// Append newly-spawned particles (from a fracture) to the GPU buffer. Silently caps at
        /// capacity for now (no recycling yet — docs/22).
        fn append(&mut self, queue: &wgpu::Queue, new: &[GpuParticle]) {
            let room = (self.capacity - self.count) as usize;
            let take = new.len().min(room);
            if take == 0 {
                return;
            }
            let offset = self.count as u64 * std::mem::size_of::<GpuParticle>() as u64;
            queue.write_buffer(&self.buf, offset, bytemuck::cast_slice(&new[..take]));
            self.count += take as u32;
        }

        /// Record one substep into `encoder`: rebuild the spatial hash, accumulate granular contact
        /// forces, then integrate (gravity + contact + terrain). Four passes so force-accumulation
        /// (positions read-only) never races integration (docs/23). Params already written this frame.
        fn dispatch(&self, encoder: &mut wgpu::CommandEncoder) {
            if self.count == 0 {
                return;
            }
            // Each stage is its OWN compute pass. The stages have strict data dependencies (insert
            // writes the grid that forces reads; forces writes the accelerations that integrate reads),
            // and a memory barrier between dependent dispatches is only GUARANTEED at pass boundaries.
            // Four dispatches in one pass happened to work on desktop Vulkan (the 2070) but can RACE on
            // other backends (e.g. Metal / the M4) — reading a half-built grid or stale forces injects
            // energy (a "matter fountain"). Separate passes force the barrier on every backend (docs/23).
            let stages: [(&wgpu::ComputePipeline, u32); 4] = [
                (&self.clear, GRID_TABLE_SIZE),
                (&self.insert, self.count),
                (&self.force_pass, self.count),
                (&self.integrate, self.count),
            ];
            for (pipeline, threads) in stages {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("particle-stage"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, &self.bind, &[]);
                pass.dispatch_workgroups(threads.div_ceil(64), 1, 1);
            }
        }

        /// Fill `render_buf` with 8 sub-cubes per physics grain. Run ONCE per frame after the substeps
        /// (the sub-cubes only need the settled positions) — a render-only subdivision.
        fn expand(&self, encoder: &mut wgpu::CommandEncoder) {
            if self.count == 0 {
                return;
            }
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("particle-expand"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.expand);
            pass.set_bind_group(0, &self.bind, &[]);
            pass.dispatch_workgroups(self.count.div_ceil(64), 1, 1);
        }

        fn set_params(&self, queue: &wgpu::Queue, params: &GpuStepParams) {
            queue.write_buffer(&self.params, 0, bytemuck::bytes_of(params));
        }
    }

    /// Build the probe: a **cohesive iron ball** (bonded iron particles) centred at `spawn` — real
    /// matter that falls, rests, and shatters emergently (`docs/23`). Its bond stiffness derives from
    /// iron's real Young's modulus (capped at `PROBE_STIFFNESS_CAP` for explicit-integration stability
    /// — true steel needs implicit integration, flagged), damped sub-critically and substepped so it
    /// stays rigid without detonating.
    fn build_probe(mats: &[materials::Material], spawn: Vec3) -> aggregate::Aggregate {
        let iron = materials::index_of(mats, "iron");
        let density = mats[iron].density as f64; // ~7870 kg/m³
        let radius = SPHERE_RADIUS as f64;
        let s = glam::DVec3::new(spawn.x as f64, spawn.y as f64, spawn.z as f64);
        let ri = radius.ceil() as i32;
        let mut particles = Vec::new();
        for z in -ri..=ri {
            for y in -ri..=ri {
                for x in -ri..=ri {
                    let off = glam::DVec3::new(x as f64, y as f64, z as f64);
                    if off.length() <= radius {
                        particles.push(crate::orbit::Body {
                            pos: s + off,
                            vel: glam::DVec3::ZERO,
                            mass: density, // 1 m³ per particle ⇒ mass = density
                        });
                    }
                }
            }
        }
        // Rigidity comes from the material's OWN elastic force (docs/23): a lattice bond of spacing L
        // has spring constant k = E·A/L = E·L (A = L² tributary area). We use iron's real Young's
        // modulus, capped for real-time explicit stability (true k needs implicit integration — flagged).
        let e = mats[iron].youngs_modulus as f64; // 2.05e11 Pa (real, from the material DB)
        let stiffness = (e * PROBE_LATTICE).min(PROBE_STIFFNESS_CAP);
        // Steel is nearly inextensible: it fractures at a small strain rather than stretching like
        // rubber. Small enough to shatter under a meteor, large enough to survive its own landing.
        let break_strain = 0.06;
        // cutoff 1.75 → bond to face/edge/corner neighbours.
        let mut probe = aggregate::Aggregate::cohesive(
            particles,
            iron,
            0.5,
            1.75,
            stiffness,
            0.0,
            break_strain,
        );
        // Sub-critical, coordination-corrected damping so the ball settles rigidly WITHOUT the
        // explicit integrator exploding (the detonation bug: √(k·m) alone over-damped each particle
        // ~√(bonds)× past critical). See Aggregate::critically_damped (docs/23).
        probe.damping = probe.critically_damped(0.4);
        probe.with_gravity(glam::DVec3::new(0.0, -SURFACE_GRAVITY as f64, 0.0))
    }

    fn upload_mesh(device: &wgpu::Device, label: &str, mesh: &Mesh) -> GpuMesh {
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

    fn create_depth_view(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
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

    fn make_buffer(
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

    // ============================================================================================
    // Space band (scale-relative "orbit-to-ground", Step A): render the Earth + Moon as two lit
    // spheres whose positions come from the *validated* N-body physics (orbit.rs), so what you watch
    // is the same law the `moon_orbits_earth` test proves. Physics runs in real SI units (f64); we
    // map metres to display units (Earth radius -> 1) only for drawing. This is the coarse "celestial
    // band" of docs/13 — the first rung of the scale ladder.
    // ============================================================================================

    // Real-world constants (SI). See docs/04-material-physical-properties / orbit.rs.
    const EARTH_MASS: f64 = 5.972e24; // kg
    const MOON_MASS: f64 = 7.342e22; // kg
    const EARTH_RADIUS_M: f64 = 6.371e6; // m
    const MOON_RADIUS_M: f64 = 1.737e6; // m
    const MOON_DIST_M: f64 = 3.844e8; // m (semi-major axis)
    const MOON_SPEED: f64 = 1022.0; // m/s (mean orbital speed)
    const SUN_MASS: f64 = 1.989e30; // kg — holds and lights the system
    const AU_M: f64 = 1.496e11; // m (Earth–Sun distance)
    const EARTH_HELIO_SPEED: f64 = 29_780.0; // m/s (Earth's mean heliocentric speed = sqrt(G·M_sun/AU))
                                             // Metres -> display units: Earth's radius becomes 1.0, so the Moon sits ~60 units out.
    const DISPLAY_SCALE: f64 = 1.0 / EARTH_RADIUS_M;
    // Fast-forward so a full ~27.3-day orbit plays in ~20 s. Symplectic Verlet stays stable with many
    // substeps per frame (dt ~= 125 s at this scale => thousands of steps per orbit).
    const ORBIT_TIME_SCALE: f64 = 118_000.0; // sim-seconds per real-second
    const ORBIT_SUBSTEPS: u32 = 16;

    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct SpaceUniforms {
        view_proj: [[f32; 4]; 4],
        model: [[f32; 4]; 4],
        light_dir: [f32; 4], // xyz = direction to the "sun"
        tint: [f32; 4],      // body color
    }

    /// The orbital ("space band") demo handle exposed to JavaScript.
    #[wasm_bindgen]
    pub struct OrbitDemo {
        surface: wgpu::Surface<'static>,
        device: wgpu::Device,
        queue: wgpu::Queue,
        config: wgpu::SurfaceConfiguration,
        depth_view: wgpu::TextureView,
        pipeline: wgpu::RenderPipeline,
        sphere_gpu: GpuMesh,
        planet_uni: UniformSlot,
        moon_unis: Vec<UniformSlot>, // one per moon (the two-moon scene has two)
        bodies: Vec<crate::orbit::Body>, // [Sun, Earth, Moon, (Moon2)…]
        acc: Vec<glam::DVec3>,
        time_scale: f64,
        camera: Camera,
        /// The body the view is centred on — the viewport's physical frame of reference (docs/17).
        /// 1 = Earth (default), 2.. = moons.
        focus: usize,
        // Body colours are the *aggregate albedo of a real composition* (materials.json), not painted
        // tints — see `materials::aggregate_albedo` / docs/17. Reflectance only; the shader supplies
        // brightness (illumination × reflectance), so a dark-but-lit body still reads bright.
        earth_tint: [f32; 4],
        moon_tint: [f32; 4],
        /// Snapshot of the initial [Sun, Earth, Moon] state, for the "reset" control.
        initial_bodies: Vec<crate::orbit::Body>,
        /// True once any moon has struck the Earth (contact resolution fired) — for the HUD.
        impacted: bool,
        /// Per-moon "has already hit" flags, so each moon's impact energy is counted exactly once
        /// (the two-moon scene sums both).
        moon_hit: Vec<bool>,
        /// Kinetic energy (J) the impact(s) dissipated — the energy that would become damage. Reported,
        /// not yet turned into actual fragmentation (docs/17 honesty: measure it, don't hide it).
        impact_energy_j: f64,
        // --- Moon-shot Stage A (docs/23): the dropped Moon SHATTERS emergently instead of merging. ---
        mats: Vec<materials::Material>,
        /// The disrupted Moon: on impact the point-mass Moon becomes a self-gravitating aggregate of
        /// fragments (docs/21), and the impact energy — which is ≫ the Moon's binding energy — disperses
        /// it (emergent, no scripted destroy). `None` until the first impact. The fragments then fly out,
        /// arc under Earth's gravity, and some fall back — the ejecta curtain at planetary scale.
        moon_debris: Option<crate::aggregate::Aggregate>,
        debris_acc: Vec<glam::DVec3>,
        /// A pool of sphere-render slots for the fragments (one draw each, like `moon_unis`).
        debris_unis: Vec<UniformSlot>,
    }

    // Moon-shot Stage A constants.
    const DEBRIS_N: usize = 64; // Moon fragments rendered (a coarse rubble cloud)
    const DEBRIS_DT: f64 = 3.0; // s per frame for the shatter — a FIXED observable rate (time-LOD: the
                                // fine impact event plays out at human speed, not the celestial fast-forward)
    const MOON_DEBRIS_SUBSTEPS: u32 = 4;

    #[wasm_bindgen]
    impl OrbitDemo {
        /// Initialize the space band: acquire the GPU, build a unit sphere, seed the Earth + `num_moons`
        /// moons. `num_moons == 1` is the standard scene; `2` places moons on opposite sides of the same
        /// orbit (the de-orbit-both stress test).
        pub async fn create(
            canvas: HtmlCanvasElement,
            num_moons: u32,
        ) -> Result<OrbitDemo, JsValue> {
            console_error_panic_hook::set_once();
            let _ = console_log::init_with_level(log::Level::Info);

            let width = canvas.width().max(1);
            let height = canvas.height().max(1);

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
                        label: Some("greenfield-orbit"),
                        required_features: wgpu::Features::empty(),
                        required_limits: adapter.limits(),
                        ..Default::default()
                    },
                    None,
                )
                .await
                .map_err(|e| JsValue::from_str(&format!("request_device failed: {e}")))?;

            let caps = surface.get_capabilities(&adapter);
            let format = caps
                .formats
                .iter()
                .copied()
                .find(|f| f.is_srgb())
                .unwrap_or(caps.formats[0]);
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

            // One white unit sphere, tinted per-body in the shader.
            let sphere_gpu = upload_mesh(
                &device,
                "orbit-sphere",
                &mesher::build_uv_sphere(1.0, 0, [1.0, 1.0, 1.0], 48, 64),
            );

            // Uniform-only bind layout + the simple lit-sphere pipeline.
            let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("space-bind-layout"),
                entries: &[uniform_entry(
                    0,
                    wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                )],
            });
            let num_moons = num_moons.clamp(1, 2) as usize;
            let planet_uni = make_space_uniform(&device, &bind_layout);
            let debris_unis: Vec<UniformSlot> = (0..DEBRIS_N)
                .map(|_| make_space_uniform(&device, &bind_layout))
                .collect();
            let moon_unis: Vec<UniformSlot> = (0..num_moons)
                .map(|_| make_space_uniform(&device, &bind_layout))
                .collect();
            let pipeline = build_space_pipeline(&device, &bind_layout, config.format);

            // The real three-body system in SI units: [Sun, Earth, Moon] (orbit.rs). The Earth carries
            // its true heliocentric velocity and the Moon co-moves with it plus its own orbital speed,
            // so the whole nesting is emergent — the Moon stays bound to the Earth while the Earth
            // orbits the Sun (verified by `orbit::sun_earth_moon_system_is_bound`), not hand-placed.
            // The Sun both holds the system (gravity) and lights it. At this zoom it sits ~23,000
            // display units off-frame, so it is the *light source*, not a drawn disk — the scale-
            // adaptive choice (docs/17): render what matters at this scale.
            let mut bodies = vec![
                crate::orbit::Body {
                    pos: glam::DVec3::ZERO,
                    vel: glam::DVec3::ZERO,
                    mass: SUN_MASS,
                },
                crate::orbit::Body {
                    pos: glam::DVec3::new(AU_M, 0.0, 0.0),
                    vel: glam::DVec3::new(0.0, EARTH_HELIO_SPEED, 0.0),
                    mass: EARTH_MASS,
                },
            ];
            // Moons on the same circular orbit. For two, place them on OPPOSITE sides and give the
            // second the opposite tangential velocity, so both orbit the Earth the same way and stay
            // diametrically opposed — the symmetric "de-orbit both at once" stress test.
            for i in 0..num_moons {
                let side = if i == 0 { 1.0 } else { -1.0 };
                bodies.push(crate::orbit::Body {
                    pos: glam::DVec3::new(AU_M + side * MOON_DIST_M, 0.0, 0.0),
                    vel: glam::DVec3::new(0.0, EARTH_HELIO_SPEED + side * MOON_SPEED, 0.0),
                    mass: MOON_MASS,
                });
            }
            let acc = crate::orbit::accelerations(&bodies);
            let initial_bodies = bodies.clone();

            // Body colours derived from a real composition, aggregated (docs/17) — NOT hand-picked.
            // Earth: ~71% ocean water, ~24% continental (granitic) rock, ~5% polar ice. This EXCLUDES
            // the atmosphere, so there is no Rayleigh-scattered "blue marble" blue — that blue is an
            // atmospheric effect we don't yet model, and faking it here would be a fudge. Moon: maria
            // basalt; the brighter highland anorthosite isn't in the DB yet, so the Moon renders darker
            // than reality until it's added (a flagged data gap, not a paint job).
            let mats = materials::load();
            let earth_comp = [
                (materials::index_of(&mats, "water"), 0.71),
                (materials::index_of(&mats, "granite"), 0.24),
                (materials::index_of(&mats, "ice"), 0.05),
            ];
            let moon_comp = [(materials::index_of(&mats, "basalt"), 1.0)];
            let rgba = |c: &materials::Composition| {
                let a = materials::aggregate_albedo(c, &mats);
                [a[0], a[1], a[2], 1.0]
            };
            let earth_tint = rgba(&earth_comp);
            let moon_tint = rgba(&moon_comp);

            let camera = Camera {
                yaw: 0.6,
                pitch: 0.5,
                zoom: 1.0,
                base_distance: (MOON_DIST_M * DISPLAY_SCALE) as f32 * 1.7,
            };

            log::info!(
                "orbit demo ready: Sun+Earth+{num_moons} moon(s), sun-lit, {ORBIT_TIME_SCALE:.0}x time"
            );
            Ok(OrbitDemo {
                surface,
                device,
                queue,
                config,
                depth_view,
                pipeline,
                sphere_gpu,
                planet_uni,
                moon_unis,
                bodies,
                acc,
                time_scale: ORBIT_TIME_SCALE,
                camera,
                focus: 1, // start on the planet
                earth_tint,
                moon_tint,
                initial_bodies,
                impacted: false,
                moon_hit: vec![false; num_moons],
                impact_energy_j: 0.0,
                mats,
                moon_debris: None,
                debris_acc: Vec::new(),
                debris_unis,
            })
        }

        // --- Orbital-decay controls: brake the Moon and watch its orbit tighten into a crash. ---

        /// Halve **every** moon's velocity relative to the Earth — the orbital-decay control (all moons
        /// at once, so the two-moon scene de-orbits symmetrically). Each tap tightens the orbit (watch
        /// `moon_perigee_km` fall); a few taps drop the perigee below the surface and they crash. (A
        /// single halving still misses — real orbital mechanics, not a trick.)
        pub fn brake_moon(&mut self) {
            let earth_vel = self.bodies[1].vel;
            for i in 2..self.bodies.len() {
                self.bodies[i].vel = earth_vel + (self.bodies[i].vel - earth_vel) * 0.5;
            }
        }

        /// Cancel every moon's velocity relative to the Earth — they drop straight in and crash. The
        /// dramatic version (both moons at once).
        pub fn drop_moon(&mut self) {
            let earth_vel = self.bodies[1].vel;
            for i in 2..self.bodies.len() {
                self.bodies[i].vel = earth_vel;
            }
        }

        /// Restore the original Sun–Earth–Moon(s) state (undo braking / the crash).
        pub fn reset_moon(&mut self) {
            self.bodies = self.initial_bodies.clone();
            self.acc = crate::orbit::accelerations(&self.bodies);
            self.impacted = false;
            self.impact_energy_j = 0.0;
            for hit in &mut self.moon_hit {
                *hit = false;
            }
        }

        /// Predicted perigee (closest approach) of the Moon's orbit about the Earth, in km — or a
        /// negative value if the orbit is unbound. Drops below Earth's radius (~6,371 km) before a crash.
        pub fn moon_perigee_km(&self) -> f64 {
            let rel_pos = self.bodies[2].pos - self.bodies[1].pos;
            let rel_vel = self.bodies[2].vel - self.bodies[1].vel;
            let mu = crate::orbit::G * (self.bodies[1].mass + self.bodies[2].mass);
            crate::orbit::perigee(rel_pos, rel_vel, mu).map_or(-1.0, |p| p / 1000.0)
        }

        /// The Moon's speed relative to the Earth, km/s (HUD). On a true drop this *climbs* all the way
        /// to impact (~11 km/s) — there is no drag or terminal velocity in vacuum. An eccentric orbit
        /// (a partial brake) instead slows at apogee and speeds at perigee (Kepler), which can look
        /// like "flattening" but is the opposite of drag.
        pub fn moon_speed_kms(&self) -> f64 {
            (self.bodies[2].vel - self.bodies[1].vel).length() / 1000.0
        }

        /// Whether the Moon has struck the planet (HUD).
        pub fn has_impacted(&self) -> bool {
            self.impacted
        }

        /// Energy (J) the impact released — what would become heat, fracture, and ejecta.
        pub fn impact_energy_j(&self) -> f64 {
            self.impact_energy_j
        }

        /// The Moon's gravitational binding energy (J), for comparison: impact ≫ binding ⇒ it shatters.
        pub fn moon_binding_energy_j(&self) -> f64 {
            crate::orbit::binding_energy(MOON_MASS, MOON_RADIUS_M)
        }

        /// The Earth's gravitational binding energy (J). The Moon impact is a small fraction of this,
        /// so the Earth is not destroyed — it takes a planet-scale crater (docs/19 LOD bridge).
        pub fn earth_binding_energy_j(&self) -> f64 {
            crate::orbit::binding_energy(EARTH_MASS, EARTH_RADIUS_M)
        }

        /// Current time multiplier (sim-seconds per real-second), for the HUD.
        pub fn time_scale_value(&self) -> f64 {
            self.time_scale
        }

        /// Cycle the view's frame of reference through the Earth and each moon. The focused body becomes
        /// the origin; everything else moves honestly around it (docs/17).
        pub fn cycle_focus(&mut self) {
            let last = self.bodies.len() - 1; // last moon
            self.focus = if self.focus >= last {
                1
            } else {
                self.focus + 1
            };
        }

        /// Name of the currently-focused body (for the HUD / focus button).
        pub fn focus_label(&self) -> String {
            if self.focus == 1 {
                return "Earth".to_string();
            }
            // Two-moon scene → "Moon A" / "Moon B"; single moon → just "Moon".
            if self.bodies.len() > 3 {
                format!("Moon {}", (b'A' + (self.focus - 2) as u8) as char)
            } else {
                "Moon".to_string()
            }
        }

        pub fn set_orbit(&mut self, yaw: f32, pitch: f32, zoom: f32) {
            self.camera.yaw = yaw;
            self.camera.pitch = pitch.clamp(-1.5, 1.5);
            self.camera.zoom = zoom.clamp(0.2, 6.0);
        }

        pub fn resize(&mut self, width: u32, height: u32) {
            if width == 0 || height == 0 {
                return;
            }
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
            self.depth_view = create_depth_view(&self.device, width, height);
        }

        pub fn set_time_scale(&mut self, scale: f32) {
            self.time_scale = (scale as f64).clamp(1.0, 2_000_000.0);
        }

        /// Live Earth–Moon separation in km (for the HUD). Should hover near 384,400 km.
        pub fn moon_distance_km(&self) -> f64 {
            (self.bodies[2].pos - self.bodies[1].pos).length() / 1000.0
        }

        pub fn render(&mut self) -> Result<(), JsValue> {
            // Advance the N-body orbit (real SI seconds), substepped for a stable symplectic step.
            let sim_dt = self.time_scale / 60.0;
            let dt = sim_dt / ORBIT_SUBSTEPS as f64;
            let contact = EARTH_RADIUS_M + MOON_RADIUS_M; // Earth + Moon radii: surfaces touch here
            let mut shatter: Option<(glam::DVec3, glam::DVec3, f64)> = None; // (site, momentum, energy)
            for _ in 0..ORBIT_SUBSTEPS {
                crate::orbit::verlet_step(&mut self.bodies, &mut self.acc, dt);
                // Earth (index 1) vs each moon (index 2..): solid bodies collide at their surfaces —
                // they don't tunnel through each other as point masses into a 1/r² singularity. Each
                // moon's impact energy is counted once (the two-moon scene sums both).
                let (head, tail) = self.bodies.split_at_mut(2);
                let earth = &mut head[1];
                for (k, moon) in tail.iter_mut().enumerate() {
                    // Measure the impact energy BEFORE the (energy-removing) contact resolution — honest
                    // damage reporting (docs/17).
                    let dissipated = crate::orbit::inelastic_dissipation(earth, moon);
                    let rel_vel = moon.vel - earth.vel; // incoming velocity, before contact resolution
                    if crate::orbit::resolve_contact(earth, moon, contact) {
                        if !self.moon_hit[k] {
                            self.moon_hit[k] = true;
                            self.impact_energy_j += dissipated;
                            // Stage A (docs/23): shatter the first moon to strike — capture its impact
                            // site, incoming momentum, and energy to disperse it as real matter.
                            if k == 0 && shatter.is_none() {
                                shatter = Some((moon.pos, rel_vel * moon.mass, dissipated));
                            }
                        }
                        self.impacted = true;
                    }
                }
            }

            // Stage A: the frame the Moon first strikes, turn the point-mass Moon into a self-gravitating
            // aggregate of fragments and let the impact energy — which is ≫ its binding energy — DISPERSE
            // it (emergent, no scripted destroy: kick vs. binding, docs/21). It then arcs under Earth's
            // gravity, some falling back.
            if let Some((site, momentum, energy)) = shatter {
                if self.moon_debris.is_none() {
                    let moon_mass = self.initial_bodies[2].mass;
                    let earth_pos = self.bodies[1].pos;
                    let (agg, acc0) =
                        build_moon_debris(&self.mats, site, earth_pos, moon_mass, momentum, energy);
                    self.debris_acc = acc0;
                    self.moon_debris = Some(agg);
                }
            }
            // Step the debris at a FIXED observable rate (time-LOD): the fine impact event plays out at
            // human speed, not the celestial fast-forward that would disperse it in a single frame.
            if let Some(agg) = self.moon_debris.as_mut() {
                let ddt = DEBRIS_DT / MOON_DEBRIS_SUBSTEPS as f64;
                for _ in 0..MOON_DEBRIS_SUBSTEPS {
                    agg.step(&mut self.debris_acc, ddt);
                }
            }

            let view_proj = self.view_proj();

            // Render in the focused body's frame of reference (docs/17): its position is the origin,
            // everything else is drawn relative to it. Switching focus re-centres the whole view.
            let focus = self.bodies[self.focus].pos;
            let sun = self.bodies[0].pos;
            let earth_r = (EARTH_RADIUS_M * DISPLAY_SCALE) as f32;
            let moon_r = (MOON_RADIUS_M * DISPLAY_SCALE) as f32;

            // Light direction = TO the real Sun from each body (per-body; the Sun is the illuminant,
            // not a hardcoded direction). So the lit hemisphere and the phases come from the geometry.
            let earth_pos = ((self.bodies[1].pos - focus) * DISPLAY_SCALE).as_vec3();
            let earth_light = (sun - self.bodies[1].pos).as_vec3().normalize();
            write_space_uniform(
                &self.queue,
                &self.planet_uni,
                view_proj,
                Mat4::from_translation(earth_pos) * Mat4::from_scale(Vec3::splat(earth_r)),
                earth_light,
                self.earth_tint, // aggregate albedo of ocean+rock+ice (docs/17), not a painted tint
            );
            for (k, uni) in self.moon_unis.iter().enumerate() {
                if k == 0 && self.moon_debris.is_some() {
                    continue; // moon 0 has SHATTERED — it's drawn as its debris fragments below
                }
                let bi = 2 + k; // body index of this moon
                let mpos = ((self.bodies[bi].pos - focus) * DISPLAY_SCALE).as_vec3();
                let mlight = (sun - self.bodies[bi].pos).as_vec3().normalize();
                write_space_uniform(
                    &self.queue,
                    uni,
                    view_proj,
                    Mat4::from_translation(mpos) * Mat4::from_scale(Vec3::splat(moon_r)),
                    mlight,
                    self.moon_tint, // aggregate albedo of basalt (docs/17); dark, lit bright by the sun
                );
            }
            // The shattered Moon: each surviving fragment is drawn as a small basalt sphere at its real
            // position — the debris cloud (some flying out, some falling back) IS the crater ejecta at
            // planetary scale, emergent from the aggregate physics, not a scripted animation.
            let mut debris_count = 0usize;
            if let Some(agg) = self.moon_debris.as_ref() {
                let frag_r = moon_r / (DEBRIS_N as f32).cbrt(); // N fragments ≈ the Moon's volume
                for (i, p) in agg.particles.iter().enumerate() {
                    if i >= self.debris_unis.len() {
                        break;
                    }
                    let fpos = ((p.pos - focus) * DISPLAY_SCALE).as_vec3();
                    let flight = (sun - p.pos).as_vec3().normalize();
                    write_space_uniform(
                        &self.queue,
                        &self.debris_unis[i],
                        view_proj,
                        Mat4::from_translation(fpos) * Mat4::from_scale(Vec3::splat(frag_r)),
                        flight,
                        self.moon_tint,
                    );
                    debris_count += 1;
                }
            }

            let output = self
                .surface
                .get_current_texture()
                .map_err(|e| JsValue::from_str(&format!("get_current_texture failed: {e}")))?;
            let view = output
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("orbit-frame"),
                });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("orbit-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.01,
                                g: 0.01,
                                b: 0.03,
                                a: 1.0,
                            }),
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
                pass.set_pipeline(&self.pipeline);
                draw(&mut pass, &self.planet_uni, &self.sphere_gpu);
                for (k, uni) in self.moon_unis.iter().enumerate() {
                    if k == 0 && self.moon_debris.is_some() {
                        continue; // shattered — drawn as debris
                    }
                    draw(&mut pass, uni, &self.sphere_gpu);
                }
                for uni in self.debris_unis.iter().take(debris_count) {
                    draw(&mut pass, uni, &self.sphere_gpu);
                }
            }
            self.queue.submit(std::iter::once(encoder.finish()));
            output.present();
            Ok(())
        }

        fn view_proj(&self) -> Mat4 {
            let aspect = self.config.width as f32 / self.config.height.max(1) as f32;
            let proj = Mat4::perspective_rh(0.9, aspect, 0.05, 100_000.0);
            let cp = self.camera.pitch.cos();
            let dir = Vec3::new(
                cp * self.camera.yaw.sin(),
                self.camera.pitch.sin(),
                cp * self.camera.yaw.cos(),
            );
            let eye = dir * (self.camera.base_distance * self.camera.zoom);
            let view = Mat4::look_at_rh(eye, Vec3::ZERO, Vec3::Y);
            proj * view
        }
    }

    /// Stage A (docs/23): build the disrupted Moon — a self-gravitating aggregate of basalt fragments
    /// filling the Moon's volume at the impact `site`, then DISPERSE it with the impact `momentum` +
    /// `energy` (which is ≫ its binding energy → it shatters, emergent: kick vs. binding, docs/21).
    /// Earth's gravity (uniform toward its centre) pulls the debris back. Returns the aggregate and its
    /// initial accelerations.
    fn build_moon_debris(
        mats: &[materials::Material],
        site: glam::DVec3,
        earth_pos: glam::DVec3,
        moon_mass: f64,
        momentum: glam::DVec3,
        energy: f64,
    ) -> (crate::aggregate::Aggregate, Vec<glam::DVec3>) {
        let basalt = materials::index_of(mats, "basalt");
        let moon_r = MOON_RADIUS_M;
        let frag_mass = moon_mass / DEBRIS_N as f64;
        let mut particles = Vec::with_capacity(DEBRIS_N);
        for i in 0..DEBRIS_N {
            // Fibonacci-sphere direction × a volume-filling radius — a coarse rubble Moon at the impact
            // site, momentarily at rest (its bulk momentum went to Earth in the inelastic contact).
            let kk = i as f64 + 0.5;
            let y = 1.0 - 2.0 * kk / DEBRIS_N as f64;
            let rxy = (1.0 - y * y).max(0.0).sqrt();
            let phi = kk * std::f64::consts::PI * (3.0 - 5.0f64.sqrt());
            let dir = glam::DVec3::new(rxy * phi.cos(), y, rxy * phi.sin());
            let rr = moon_r * (kk / DEBRIS_N as f64).cbrt();
            particles.push(crate::orbit::Body {
                pos: site + dir * rr,
                vel: glam::DVec3::ZERO,
                mass: frag_mass,
            });
        }
        let g_dir = (earth_pos - site).normalize_or_zero();
        let mut agg = crate::aggregate::Aggregate::new(particles, moon_r * 0.5)
            .with_material(basalt)
            .with_gravity(g_dir * SURFACE_GRAVITY as f64);
        // The impact energy disperses it — emergent shatter (momentum + shock heat + vapor, docs/24).
        agg.deposit_impact(mats, site, momentum, energy);
        let acc0 = agg.accelerations();
        (agg, acc0)
    }

    fn make_space_uniform(device: &wgpu::Device, layout: &wgpu::BindGroupLayout) -> UniformSlot {
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("space-uniform"),
            size: std::mem::size_of::<SpaceUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("space-bind"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buf.as_entire_binding(),
            }],
        });
        UniformSlot { buf, bind }
    }

    fn write_space_uniform(
        queue: &wgpu::Queue,
        slot: &UniformSlot,
        view_proj: Mat4,
        model: Mat4,
        light: Vec3,
        tint: [f32; 4],
    ) {
        let u = SpaceUniforms {
            view_proj: view_proj.to_cols_array_2d(),
            model: model.to_cols_array_2d(),
            light_dir: [light.x, light.y, light.z, 0.0],
            tint,
        };
        queue.write_buffer(&slot.buf, 0, bytemuck::bytes_of(&u));
    }

    fn build_space_pipeline(
        device: &wgpu::Device,
        bind_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
    ) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("space-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../../shaders/space.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("space-pipeline-layout"),
            bind_group_layouts: &[bind_layout],
            push_constant_ranges: &[],
        });
        // Same vertex layout as the world mesh; the space shader only reads position + normal.
        const ATTRS: [wgpu::VertexAttribute; 4] =
            wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x3, 3 => Uint32];
        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTRS,
        };
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("space-pipeline"),
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
                cull_mode: Some(wgpu::Face::Back),
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
}

#[cfg(test)]
mod tests {
    use crate::{body, gravity, materials, mesher, world};

    #[test]
    fn material_database_loads() {
        let mats = materials::load();
        assert_eq!(mats.len(), 20, "seed database should have 20 materials");
        for id in ["granite", "dirt", "grass", "iron", "nickel"] {
            let i = materials::index_of(&mats, id);
            assert!(mats[i].density > 0.0, "{id} must have positive density");
        }
        // Metals carry a real elastic modulus — the probe's cohesive-bond stiffness derives from it.
        let iron = materials::index_of(&mats, "iron");
        assert!(
            mats[iron].youngs_modulus > 1.0e11,
            "iron's Young's modulus must be ~200 GPa (got {})",
            mats[iron].youngs_modulus
        );
        let g = mats[materials::index_of(&mats, "granite")].density;
        let d = mats[materials::index_of(&mats, "dirt")].density;
        assert!(g > d, "granite ({g}) should be denser than dirt ({d})");
    }

    #[test]
    fn world_is_layered_rock_dirt_grass() {
        let mats = materials::load();
        let w = world::generate(&mats);
        let rock = materials::index_of(&mats, "granite");
        let dirt = materials::index_of(&mats, "dirt");
        let grass = materials::index_of(&mats, "grass");

        let (x, z) = (w.w as i32 / 2, w.d as i32 / 2);
        assert!(w.is_solid(x, 0, z), "world must be solid at the bottom");

        let mut seen_grass = false;
        let mut seen_dirt = false;
        let mut seen_rock = false;
        for y in (0..w.h as i32).rev() {
            match w.material_at(x, y, z) {
                Some(m) if m == grass => seen_grass = true,
                Some(m) if m == dirt => {
                    seen_dirt = true;
                    assert!(seen_grass, "should hit grass before dirt scanning down");
                }
                Some(m) if m == rock => {
                    seen_rock = true;
                    assert!(seen_dirt, "should hit dirt before rock scanning down");
                }
                _ => {}
            }
        }
        assert!(
            seen_grass && seen_dirt && seen_rock,
            "all three layers must be present"
        );
    }

    #[test]
    fn mesher_produces_valid_surface() {
        let mats = materials::load();
        let w = world::generate(&mats);
        let mesh = mesher::build(&w, &mats);
        assert!(!mesh.vertices.is_empty(), "mesh must have vertices");
        assert_eq!(mesh.vertices.len() % 4, 0, "vertices come in quads of 4");
        assert_eq!(
            mesh.indices.len() % 6,
            0,
            "indices come in 2 triangles (6) per quad"
        );
        let vmax = mesh.vertices.len() as u32;
        assert!(mesh.indices.iter().all(|&i| i < vmax), "indices in range");
    }

    #[test]
    fn sphere_mesh_is_valid() {
        let (rings, sectors) = (16, 24);
        let mesh = mesher::build_uv_sphere(3.0, 0, [0.5, 0.5, 0.5], rings, sectors);
        assert_eq!(mesh.vertices.len(), (rings + 1) * (sectors + 1));
        assert_eq!(mesh.indices.len(), rings * sectors * 6);
        let vmax = mesh.vertices.len() as u32;
        assert!(mesh.indices.iter().all(|&i| i < vmax));
        // Every vertex sits on the sphere of the requested radius.
        for v in &mesh.vertices {
            let r = (v.pos[0].powi(2) + v.pos[1].powi(2) + v.pos[2].powi(2)).sqrt();
            assert!((r - 3.0).abs() < 1e-3, "vertex on sphere surface");
        }
    }

    #[test]
    fn sphere_falls_toward_world_and_rests() {
        let mats = materials::load();
        let w = world::generate(&mats);
        let field = gravity::MassField::build(&w, &mats, 4);
        let c = w.center();
        let radius = 1.0;
        let surf = w.surface_top_voxel(c.x as i32, c.z as i32).unwrap() as f32 - c.y;
        let spawn = glam::Vec3::new(0.0, surf + radius + 8.0, 0.0);
        let mut s = body::Sphere::new(spawn, 5.0, radius);
        let start_y = s.pos.y;

        // Fast-forward: the accel is tiny and smooth, so large steps integrate fine.
        let dt = 5.0;
        for _ in 0..8000 {
            let accel = field.acceleration_at(s.pos, 6.0);
            s.integrate(accel, dt);
            s.collide(&w, accel, dt);
            if s.resting {
                break;
            }
        }
        assert!(s.pos.y < start_y, "sphere should fall downward");
        assert!(s.resting, "sphere should come to rest on the surface");
        assert!(
            (s.pos.y - (surf + radius)).abs() < 1.0,
            "rests on the surface"
        );
    }

    #[test]
    fn raycast_hits_terrain_from_above() {
        let mats = materials::load();
        let w = world::generate(&mats);
        let c = w.center();
        let origin = glam::Vec3::new(0.0, c.y + 50.0, 0.0);
        let hit = w.raycast(origin, glam::Vec3::NEG_Y, 1000.0);
        assert!(hit.is_some(), "a downward ray should hit the terrain");
        let (_x, _y, _z, p) = hit.unwrap();
        let surf = w.surface_top_voxel(c.x as i32, c.z as i32).unwrap() as f32 - c.y;
        assert!((p.y - surf).abs() < 2.0, "hit near the surface height");
    }

    #[test]
    fn surface_nets_is_smooth_and_valid() {
        let mats = materials::load();
        let w = world::generate(&mats);
        let mesh = mesher::build_surface_nets(&w, &mats);
        assert!(
            !mesh.vertices.is_empty(),
            "surface nets should produce geometry"
        );
        assert_eq!(mesh.indices.len() % 3, 0);
        let vmax = mesh.vertices.len() as u32;
        assert!(mesh.indices.iter().all(|&i| i < vmax), "indices in range");
        assert!(
            mesh.vertices
                .iter()
                .all(|v| v.pos.iter().chain(v.nrm.iter()).all(|c| c.is_finite())),
            "no NaN/inf in positions or normals"
        );
        // Smooth: unlike the cube mesher, many normals are NOT axis-aligned.
        let non_axis = mesh
            .vertices
            .iter()
            .filter(|v| {
                let n = v.nrm;
                !(n[0].abs() > 0.99 || n[1].abs() > 0.99 || n[2].abs() > 0.99)
            })
            .count();
        assert!(non_axis > 0, "surface nets should yield smooth normals");
    }

    #[test]
    fn surface_nets_mesh_is_closed() {
        // "Hollow from two sides" would mean an open surface. A closed (watertight) mesh shares every
        // undirected edge an even number of times; a boundary edge (odd count) is a hole.
        use std::collections::HashMap;
        let mats = materials::load();
        let w = world::generate(&mats);
        let mesh = mesher::build_surface_nets(&w, &mats);
        let mut edges: HashMap<(u32, u32), u32> = HashMap::new();
        for tri in mesh.indices.chunks_exact(3) {
            for &(a, b) in &[(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
                let key = if a < b { (a, b) } else { (b, a) };
                *edges.entry(key).or_insert(0) += 1;
            }
        }
        let boundary = edges.values().filter(|&&c| c % 2 != 0).count();
        assert_eq!(
            boundary, 0,
            "mesh must be closed (watertight); found {boundary} boundary edges"
        );
    }
}
