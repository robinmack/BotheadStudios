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
mod atmosphere;
mod body;
mod damage;
mod emission;
mod granular;
mod gravity;
mod impact;
mod planet;
mod tides;
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
                                        // planetary scale in the space band; here it is negligible vs the planet below. That surface
                                        // gravity is now COMPUTED from planet::earth() (g = GM/R²) at create() — no hardcoded constant.
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
        /// The horizon ground plane (surface-of-a-planet framing). Reuses `world_uni` (same identity
        /// model / view_proj / light / material textures) — it is just another mesh in the world pass.
        ground_gpu: GpuMesh,

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
        /// The planet this terrain is a surface patch of: real Earth matter (planet::earth()). Its mass,
        /// radius, and the surface gravity the patch feels all EMERGE from that body — no magic 9.81
        /// (docs/22, docs/25). "Matter all the way down": the bulk planet below is matter summarized as a
        /// field, the patch is resolved matter sampling its surface.
        planet_mass: f64,
        planet_radius: f64,
        surface_g: f32,

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
            // Horizon ground plane at the patch's centre surface height (world-centered coords). Extent
            // just inside the 6 km far plane so it fills down to a true horizon under the sky.
            let cc = world.center();
            let surf_y = world
                .surface_top_voxel(cc.x as i32, cc.z as i32)
                .map(|t| t as f32 - cc.y)
                .unwrap_or(0.0);
            let ground_mesh = ground_plane_mesh(&mats, surf_y, 5800.0);
            let ground_gpu = upload_mesh(&device, "ground", &ground_mesh);
            log::info!("meshes: world {} tris", world_mesh.indices.len() / 3);

            // --- Spawn the probe: a cohesive iron ball of real matter (docs/23) ---
            let c = world.center();
            let surf = world
                .surface_top_voxel(c.x as i32, c.z as i32)
                .map(|t| t as f32 - c.y)
                .unwrap_or(0.0);
            let spawn = Vec3::new(0.0, surf + SPHERE_RADIUS + SPAWN_HEIGHT, 0.0);
            // The terrain is a surface patch of the SAME declared Earth as the space band (planet::earth()).
            // Surface gravity EMERGES from that body's mass and radius (g = GM/R²), not a hardcoded 9.81.
            let planet = crate::planet::earth();
            let planet_mass = planet.total_mass();
            let planet_radius = planet.radius();
            let surface_g = planet.gravity_at(planet_radius) as f32;
            let mut probe = build_probe(&mats, spawn, surface_g as f64);
            let probe_acc = probe.accelerations();
            let probe_instances = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("probe-instances"),
                size: (probe.particles.len() * 8 * std::mem::size_of::<GpuParticle>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            log::info!(
                "greenfield-engine: planet mass = {:.3e} kg (r = {:.0} km), emergent surface g = {:.2} m/s^2",
                planet_mass,
                planet_radius / 1000.0,
                surface_g
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
                // Low pitch = a surface-grazing look across the terrain toward the horizon (standing on
                // a world), instead of the old top-down look at a floating cube.
                pitch: 0.16,
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
                ground_gpu,
                mats,
                world,
                field,
                probe,
                probe_acc,
                probe_instances,
                matter,
                spawn,
                time_scale: DEFAULT_TIME_SCALE,
                planet_mass,
                planet_radius,
                surface_g,
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
        /// Mass of the resolved terrain patch (its own matter) — the self-gravity-field total. Tiny next
        /// to the planet; kept as a diagnostic. See `planet_mass` for the world it sits on.
        pub fn total_mass(&self) -> f64 {
            self.field.total_mass as f64
        }
        /// Mass of the PLANET this terrain is a patch of (kg) — real Earth matter (planet::earth()).
        pub fn planet_mass(&self) -> f64 {
            self.planet_mass
        }
        /// Radius of that planet (km).
        pub fn planet_radius_km(&self) -> f64 {
            self.planet_radius / 1000.0
        }
        /// The surface gravity the probe feels (m/s²) — EMERGES from the planet's mass and radius
        /// (g = GM/R²), not a hardcoded constant.
        pub fn surface_gravity(&self) -> f32 {
            self.surface_g
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
            self.probe = build_probe(&self.mats, self.spawn, self.surface_g as f64);
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
                self.couple_impact_to_bodies(eye_d, dir_d, hit_d, energy as f64, strength as f64);
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
            sigma: f64,
        ) {
            const RAY_CAPTURE: f64 = 0.6; // ~ a body's particle spacing
            let terrain_t = (ground - eye).dot(dir); // along-ray distance to the ground
            let momentum_mag = (METEOR_MASS * METEOR_SPEED) as f64;
            // `sigma` is the REAL fracture strength of the material the meteor actually struck (read from
            // the hit voxel in `meteor`, not a hardcoded bulk-rock proxy) — so the crater reach that sets
            // the blast falloff to nearby bodies uses the true strength of the strata that were hit
            // (basalt crust vs peridotite mantle vs iron core), faithful to the layered terrain (docs/28).
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
            // Debris friction comes from the REAL material, not a tuned number: the angle of repose
            // emerges from it (docs/23). The representative rock is BASALT — this world's actual crust,
            // the bulk of what a dig/blast excavates (docs/28) — NOT the old hardcoded `granite`, which
            // is not even present in the layered strata (grass → basalt → peridotite → iron).
            // FLAGGED APPROXIMATION (IOU): the GPU debris step takes ONE friction/restitution/cohesion
            // for the whole buffer, so mixed-material debris (iron shrapnel from a deep blast, peridotite
            // from the mantle) piles with basalt's μ. The honest fix is PER-PARTICLE material in
            // particle_step.wgsl (each grain already carries its `material` index; it needs a per-material
            // property table + a pair-combining rule, plus a gpu-verify momentum-conservation pass). That
            // is the deferred, non-tractable-tonight half; the CPU-side dig/fracture threshold below and
            // in `matter::dig`/`meteor` already reads each voxel's own material.
            let bulk = &self.mats[materials::index_of(&self.mats, "basalt")];
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
                gravity: [0.0, -self.surface_g, 0.0],
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
                // Horizon ground plane first (shares world_uni), then the detailed patch on top of it —
                // the plane hides the patch's floating side walls and carries the ground to the horizon.
                draw(&mut pass, &self.world_uni, &self.ground_gpu);
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
            // Camera collision (Robin: "the camera should never penetrate matter"): clamp the eye out of
            // any solid voxel / below-ground position — a third-person boom pull-back along the view ray
            // that keeps yaw/pitch and leaves a free eye untouched. Eliminates the zoom-into-the-ground
            // clip the other engines fake around.
            const CAMERA_CLEARANCE: f32 = 2.0; // metres of air kept between the eye and the surface
            let eye = self.world.clamp_eye_outside(eye, CAMERA_CLEARANCE);
            // Aim near eye height (not down at the world centre) so the gaze is nearly horizontal: the
            // ground plane recedes to a HORIZON with sky above — a surface-of-a-planet view. A little
            // below eye height keeps a gentle downward tilt so the terrain patch stays in frame.
            let target = Vec3::new(0.0, eye.y * 0.82, 0.0);
            let view = Mat4::look_at_rh(eye, target, Vec3::Y);
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
    fn build_probe(mats: &[materials::Material], spawn: Vec3, surface_g: f64) -> aggregate::Aggregate {
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
        // Surface gravity is the field of the WHOLE planet below (matter all the way down), ~uniform over
        // this small patch — passed in, computed from planet::earth(), not a hardcoded constant.
        probe.with_gravity(glam::DVec3::new(0.0, -surface_g, 0.0))
    }

    /// A large flat ground quad at surface level (world-centered coords), meshed with the surface
    /// material. It continues the detailed terrain patch out to the horizon and occludes the patch's
    /// exposed side walls — so the scene reads as a surface OF a planet, not a floating cube. Flat is
    /// honest for an Earth-sized world: the curvature drop over the visible ~km is a couple of metres.
    fn ground_plane_mesh(materials: &[materials::Material], surf_y: f32, extent: f32) -> Mesh {
        let grass = materials::index_of(materials, "grass");
        let col = materials[grass].albedo;
        let n = [0.0f32, 1.0, 0.0];
        let v = |x: f32, z: f32| mesher::Vertex {
            pos: [x, surf_y, z],
            nrm: n,
            col,
            mat: grass as u32,
        };
        // Corners CCW seen from above (+Y up); triplanar sampling tiles the material across it.
        let vertices = vec![
            v(-extent, -extent),
            v(extent, -extent),
            v(extent, extent),
            v(-extent, extent),
        ];
        let indices = vec![0u32, 1, 2, 0, 2, 3];
        Mesh { vertices, indices }
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
        emissive: [f32; 4],  // rgb = incandescent glow, w = intensity (self-lit hot ejecta)
    }

    /// How far (wall-clock seconds) the RENDER runs behind the PHYSICS (docs/13). Humans don't
    /// resolve detail below ~1/10 s, so the physics keeps a 100 ms head start: every collision in the
    /// next 100 ms is already fully resolved before the frame that shows it is drawn — the simulation
    /// drives the visualization and can never be caught mid-lie by a frame boundary.
    const RENDER_LAG_S: f64 = 0.10;

    /// A snapshot of the observable physics state at one physics-clock instant. The renderer
    /// interpolates between snapshots at (now − RENDER_LAG_S); it never reads live physics state.
    struct FrameSnap {
        t: f64,                   // physics wall-clock (s) when taken
        bodies: Vec<glam::DVec3>, // positions of [Sun, Earth, Moon(s)]
        debris: Vec<glam::DVec3>, // impact-cloud particle positions (empty before the shatter)
        temps: Vec<f32>,          // impact-cloud temperatures (glow)
        sizes: Vec<f32>,          // display radius factor ∝ (mass/initial fragment mass)^⅓ — accretion grows moonlets
        mats: Vec<usize>,         // per-fragment material index — snapshotted so tints track the SAME lagged
        //                           order as positions (a live read desynced after drain's swap_remove)
        srcs: Vec<u8>,            // per-fragment provenance (Earth vs Theia) — same lagged order (docs/28 step 1)
        shattered: bool,
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
        /// Snapshot of the initial spin angular momentum, restored on Reset alongside `initial_bodies`.
        /// Without this a Reset kept the impact-induced spin — a world reset that conjured angular
        /// momentum out of the previous run (a render-truth bug, docs/28).
        initial_spin_l: glam::DVec3,
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
        /// The inbound impactor's physical radius/mass — the Moon by default; Theia in the
        /// birth-of-the-Moon scenario (docs/27). Drives CCD contact distance, shell rendering,
        /// excavation scale, and which layered profile materializes at the strike.
        impactor_radius: f64,
        impactor_mass: f64,
        birth_mode: bool,
        /// Sim-seconds per frame for the post-impact debris (time-LOD): 3 s for the moon-drop close-up;
        /// larger for the birth scene, whose disk evolves over hours.
        debris_frame_dt: f64,
        /// Aftermath speed multiplier (1..64): scales how much SIM time each real second covers after
        /// the impact. The integration substep stays FIXED (stability is physics); only the substep
        /// count grows — under overload the observable clock dilates rather than corrupting the model.
        debris_rate_mul: f64,
        /// Volume (m³) of settled matter demoted back into Earth — the crater HEALS by exactly this
        /// much (docs/27): the excavated bowl refills with the matter that fell back, and when the
        /// carved volume is repaid the planet is whole again. Nothing re-solidifies by decree.
        crater_heal_m3: f64,
        /// SIM seconds elapsed since the impact — the honest answer to "what timeframe are we watching
        /// this over?" (the aftermath runs under time-LOD, so real seconds ≠ sim seconds).
        sim_since_impact: f64,
        /// Earth's SPIN angular momentum (docs/27): set by the modern day length in the orbital scenes;
        /// ZERO for proto-Earth in the birth scene (its primordial spin is unknown — flagged) so the
        /// post-impact day length EMERGES from the collision geometry. Fed by the boundary-shear mirror,
        /// demoted matter's orbital L, and drained by tidal torque on the moonlets.
        spin_l: glam::DVec3,
        /// GEOLOGIC time-LOD (docs/27): once the aftermath is quiet, each settled clump IS one body
        /// (orbital elements), evolved by the validated secular tidal law — millennia per real second.
        geologic: bool,
        geo_moonlets: Vec<crate::tides::Moonlet>,
        geo_rate_yr_s: f64,
        /// Accumulated rotation angle (rad) about the spin axis — the VISIBLE rotation of the shell
        /// (and its landmask) at the real rate implied by spin_l.
        spin_angle: f64,
        moon_debris: Option<crate::aggregate::Aggregate>,
        /// Impact site relative to Earth's centre (set at the shatter) — masks the shell over the
        /// materialized region so the excavated crater is visible, and moves with the orbiting Earth.
        impact_site_rel: Option<glam::DVec3>,
        shell_unis: Vec<UniformSlot>,
        /// The bulk interior sphere (the un-materialized deep Earth): visible only through the crater —
        /// the top of the outer core at cap depth, glowing at its REAL temperature ("hollow earth" fix).
        interior_uni: UniformSlot,
        sun_uni: UniformSlot,
        atm_tau: [f64; 3],
        interior_tint: [f32; 4],
        interior_glow: [f32; 4],
        wall_unis: Vec<UniformSlot>,
        // Physics/render decoupling (docs/13): physics advances on its own fixed timestep driven by
        // wall-clock time; the renderer samples snapshots RENDER_LAG_S behind. See `advance`.
        snaps: std::collections::VecDeque<FrameSnap>,
        phys_clock: f64,
        real_accum: f64,
        debris_acc: Vec<glam::DVec3>,
        /// A pool of sphere-render slots for the fragments (one draw each, like `moon_unis`).
        debris_unis: Vec<UniformSlot>,
    }

    // Moon-shot Stage A constants.
    use crate::impact::{DEBRIS_N, IMPACT_N}; // the mutual-impact builder (physics of record, impact.rs)
    /// Earth rendered as a shell of particles (the honest low-res look, docs/15): a smooth sphere is a
    /// representation LIE once matter can be excavated — it hides the damage. The shell is the
    /// VISUALIZATION of the un-materialized bulk summary (whose physics is the boundary + gravity
    /// source); shell points inside the materialized impact region are hidden so the real crater shows.
    const SHELL_N: usize = 512;
    /// Grains lining the crater bowl's wall — the visualization of the carved boundary surface. Their
    /// tint/glow come from the layer profile at each grain's true depth: cool crust rim grading to
    /// white-hot outer-core floor. This (not paint) is why the crater reads as incandescent.
    const WALL_N: usize = 96;
    /// The intact Moon renders as a grain shell too — every solid object in the universe is composed of
    /// matter (Robin); a smooth sphere is the same representation lie we removed from Earth.
    const MOON_SHELL_N: usize = 128;
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
            let debris_unis: Vec<UniformSlot> = (0..IMPACT_N)
                .map(|_| make_space_uniform(&device, &bind_layout))
                .collect();
            let shell_unis: Vec<UniformSlot> = (0..SHELL_N)
                .map(|_| make_space_uniform(&device, &bind_layout))
                .collect();
            let interior_uni = make_space_uniform(&device, &bind_layout);
            let sun_uni = make_space_uniform(&device, &bind_layout);
            // Rayleigh optical depths from the EMERGENT surface pressure (planet::earth's declared
            // atmosphere mass) — the blue marble is derived from the air, never painted (docs/26).
            let atm_tau = crate::atmosphere::rayleigh_tau(
                crate::planet::earth().surface_pressure() / 101_325.0,
            );
            let wall_unis: Vec<UniformSlot> = (0..WALL_N)
                .map(|_| make_space_uniform(&device, &bind_layout))
                .collect();
            let moon_unis: Vec<UniformSlot> = (0..num_moons * MOON_SHELL_N)
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
                    // The Sun's mass EMERGES from its declared composition (planet::sun), like Earth's
                    // from PREM — the constant is retired from the source of truth.
                    mass: crate::planet::sun().total_mass(),
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
            // Modern Earth: the measured sidereal day, spin axis ⊥ the orbital (x-y) plane.
            let spin_l = glam::DVec3::new(0.0, 0.0, 1.0)
                * (crate::tides::moment_of_inertia(EARTH_MASS, EARTH_RADIUS_M)
                    * (2.0 * std::f64::consts::PI / 86_164.0));

            // Body colours derived from a real composition, aggregated (docs/17) — NOT hand-picked.
            // Earth: ~71% ocean water, ~24% continental (granitic) rock, ~5% polar ice. This EXCLUDES
            // the atmosphere, so there is no Rayleigh-scattered "blue marble" blue — that blue is an
            // atmospheric effect we don't yet model, and faking it here would be a fudge. Moon: maria
            // basalt; the brighter highland anorthosite isn't in the DB yet, so the Moon renders darker
            // than reality until it's added (a flagged data gap, not a paint job).
            let mats = materials::load();
            // The interior sphere's material/temperature: the layer at the depth the crater exposes
            // (the cap bottom) — for a Moon-scale impact that is the top of the molten outer core.
            // The bulk just under the crust: OPAQUE DARK ROCK. It sits right beneath the shell grains
            // so nothing shines through the gaps between them — the old white-hot sphere (meant as the
            // crater floor, 3,500 km down) bled through the gaps and made Earth look lit from WITHIN,
            // reading as anti-sun lighting (Robin's "anti-raycasting"). Depth-glow belongs to the
            // CRATER alone, whose wall grains carry the real layer temperatures.
            let int_mat = &mats[materials::index_of(&mats, "basalt")];
            let interior_tint = [int_mat.albedo[0], int_mat.albedo[1], int_mat.albedo[2], 1.0];
            let interior_glow = [0.0f32; 4];
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
                impactor_radius: MOON_RADIUS_M,
                impactor_mass: MOON_MASS,
                birth_mode: false,
                debris_frame_dt: DEBRIS_DT,
                debris_rate_mul: 1.0,
                crater_heal_m3: 0.0,
                sim_since_impact: 0.0,
                spin_l,
                initial_spin_l: spin_l,
                spin_angle: 0.0,
                geologic: false,
                geo_moonlets: Vec::new(),
                geo_rate_yr_s: 1_000.0,
                moon_debris: None,
                impact_site_rel: None,
                shell_unis,
                interior_uni,
                sun_uni,
                atm_tau,
                interior_tint,
                interior_glow,
                wall_unis,
                snaps: std::collections::VecDeque::new(),
                phys_clock: 0.0,
                real_accum: 0.0,
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
            // Un-shatter: clear the debris cloud and the crater mask so Reset restores an intact world.
            self.moon_debris = None;
            self.debris_acc.clear();
            self.impact_site_rel = None;
            self.sim_since_impact = 0.0;
            self.geologic = false;
            self.geo_moonlets.clear();
            // Restore the pristine spin: without this the impact-induced spin_l survived a world reset,
            // conjuring angular momentum from the previous run (render-truth bug, docs/28).
            self.spin_l = self.initial_spin_l;
            self.spin_angle = 0.0;
            self.crater_heal_m3 = 0.0;
            // Drop the snapshot history — the renderer must not interpolate across the reset.
            self.snaps.clear();
            self.real_accum = 0.0;
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

        /// Number of materialized debris fragments (0 until the Moon shatters) — a HUD diagnostic so we
        /// can see, on-device, whether the Stage-A shatter actually fired.
        pub fn debris_count(&self) -> u32 {
            self.moon_debris.as_ref().map_or(0, |a| a.particles.len() as u32)
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

        /// Put the camera's frame of reference on Earth (origin re-centres on the planet).
        pub fn focus_earth(&mut self) {
            self.focus = 1;
        }

        /// Put the camera's frame of reference on the Moon (or, once it has shattered, the impact site,
        /// since the shattered body's point mass stays parked there — so this frames the debris/crater).
        pub fn focus_moon(&mut self) {
            if self.bodies.len() > 2 {
                self.focus = 2;
            }
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
            // Floor low enough for the descent-follow camera (25% of lunar distance ≈ zoom 0.147).
            self.camera.zoom = zoom.clamp(0.05, 6.0);
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

        /// Excavation scale of the current impactor (matches impact.rs: hemispheric clamp for giants).
        fn cap_extent(&self) -> f64 {
            (2.0 * self.impactor_radius).min(0.55 * EARTH_RADIUS_M)
        }

        /// The crater's CURRENT radius: the carved half-ball minus the volume repaid by settled matter
        /// (`crater_heal_m3`). Reaches zero ⇒ healed: hole gone, shell restored, interior covered.
        fn hole_radius(&self) -> f64 {
            let r0 = self.cap_extent();
            let vol0 = (2.0 / 3.0) * std::f64::consts::PI * r0.powi(3);
            let rem = (vol0 - self.crater_heal_m3).max(0.0);
            (rem * 3.0 / (2.0 * std::f64::consts::PI)).cbrt()
        }

        /// Configure the BIRTH OF THE MOON scenario (docs/27): body 2 becomes THEIA — Mars-sized,
        /// differentiated — inbound with a real IMPACT PARAMETER, so the ~45° obliquity of the
        /// giant-impact hypothesis EMERGES from geometry + gravity (recovered at contact by the
        /// conservation laws), never assigned. The approach distance and time scale are chosen so the
        /// strike lands ~5 real seconds after the scene starts (the HUD counts it down).
        pub fn start_birth(&mut self) {
            let theia = crate::planet::theia();
            self.impactor_radius = theia.radius();
            self.impactor_mass = theia.total_mass();
            self.birth_mode = true;
            self.debris_frame_dt = 8.0; // disk-formation time-LOD (the aftermath spans hours)
            let contact = EARTH_RADIUS_M + self.impactor_radius;
            // Inbound geometry (relative to Earth, in the orbital plane): approach from +x at 6 km/s
            // with an impact parameter of 1.30·contact — gravity does the rest. At contact this yields
            // ~10.8 km/s at ~46° obliquity (perigee 5.6e6 m — a solid hit): the giant-impact
            // hypothesis's geometry, EMERGENT from b, never aimed. (0.87·contact gave only 29° — too
            // steep; the ejecta buried instead of lofting. Robin caught it on-screen.)
            // Near-PARABOLIC approach (canonical Theia: v∞ ≈ 0–4 km/s; our 4 km/s at this range gives
            // v∞ ≈ 2.6). The previous 6 km/s arrived ~1.3 km/s hot over escape speed and ejected far
            // too much. Wider aim keeps the ~45° obliquity at the slower closing rate.
            // Proto-Earth spin: UNKNOWN, declared zero (flagged) — the post-impact day must EMERGE.
            self.spin_l = glam::DVec3::ZERO;
            self.initial_spin_l = glam::DVec3::ZERO; // Reset in birth mode restores the non-spinning proto-Earth.
            self.spin_angle = 0.0;
            let d0 = 9.6e7; // ≈ 25% of lunar distance — the scene's opening framing
            let v_in = 5_000.0;
            let b = 1.46 * contact;
            let earth = self.bodies[1];
            self.bodies.truncate(2);
            self.bodies.push(crate::orbit::Body {
                pos: earth.pos + glam::DVec3::new(d0, b, 0.0),
                vel: earth.vel + glam::DVec3::new(-v_in, 0.0, 0.0),
                mass: self.impactor_mass,
            });
            self.acc = crate::orbit::accelerations(&self.bodies);
            self.initial_bodies = self.bodies.clone();
            self.moon_hit = vec![false];
            self.impacted = false;
            self.impact_energy_j = 0.0;
            self.moon_debris = None;
            self.debris_acc.clear();
            self.impact_site_rel = None;
            self.sim_since_impact = 0.0;
            self.crater_heal_m3 = 0.0;
            self.snaps.clear();
            self.real_accum = 0.0;
            // ~5 real seconds to impact: sim time-to-contact / 5.
            let t_sim = (d0 - contact) / v_in;
            self.time_scale = (t_sim / 5.0).max(1.0);
        }

        /// Double/halve the aftermath speed (the ⏩/⏪ controls after an impact). Returns the multiplier.
        pub fn nudge_aftermath_rate(&mut self, faster: bool) -> f64 {
            if self.geologic {
                self.geo_rate_yr_s =
                    (self.geo_rate_yr_s * if faster { 2.0 } else { 0.5 }).clamp(100.0, 1.0e6);
                return self.geo_rate_yr_s;
            }
            self.debris_rate_mul = if faster {
                (self.debris_rate_mul * 2.0).min(64.0)
            } else {
                (self.debris_rate_mul / 2.0).max(1.0)
            };
            self.debris_rate_mul
        }

        /// Live disk statistics — the HUD's answer to "did we achieve orbit?": JSON
        /// {"bound":M,"escaped":M,"biggest":M,"clumps":N} with masses in lunar masses. Bound = aloft
        /// (r > 1.1 R⊕) with negative specific orbital energy; clumps = connected components of contact
        /// adjacency (rubble-pile moonlets). Pure measurement of the particle state — same yardstick as
        /// the native emergence test.
        pub fn disk_stats_json(&self) -> String {
            const M_MOON: f64 = 7.342e22;
            if self.geologic {
                let bound: f64 = self.geo_moonlets.iter().map(|m| m.mass).sum();
                let biggest = self.geo_moonlets.iter().map(|m| m.mass).fold(0.0, f64::max);
                return format!(
                    "{{\"bound\":{:.3},\"escaped\":0,\"biggest\":{:.3},\"clumps\":{}}}",
                    bound / M_MOON,
                    biggest / M_MOON,
                    self.geo_moonlets.len()
                );
            }
            let Some(agg) = self.moon_debris.as_ref() else {
                return String::from("null");
            };
            let earth = self.bodies[1];
            let mu = crate::orbit::G * earth.mass; // live mass — the books moved with the matter
            let touch = agg.contact.map_or(1.0e6, |c| 2.2 * c.radius);
            let mut aloft: Vec<usize> = Vec::new();
            let (mut bound_m, mut escaped_m) = (0.0f64, 0.0f64);
            // Provenance of the BOUND disk (docs/28 step 1): how much of the aloft, bound material is
            // Earth-derived vs Theia-derived. The real Moon is Earth-like; today this reads ~0 Earth —
            // the measurable deficit progressive excavation must close.
            let mut bound_earth_m = 0.0f64;
            for (i, p) in agg.particles.iter().enumerate() {
                let r = (p.pos - earth.pos).length();
                let eps = 0.5 * (p.vel - earth.vel).length_squared() - mu / r;
                if eps >= 0.0 {
                    escaped_m += p.mass;
                } else if r > 1.1 * EARTH_RADIUS_M {
                    bound_m += p.mass;
                    if agg.source.get(i).copied() == Some(crate::aggregate::SOURCE_TARGET) {
                        bound_earth_m += p.mass;
                    }
                    aloft.push(i);
                }
            }
            // Union-find over touching aloft pairs → moonlet clumps.
            let mut parent: Vec<usize> = (0..aloft.len()).collect();
            fn find(p: &mut Vec<usize>, i: usize) -> usize {
                if p[i] != i {
                    let r = find(p, p[i]);
                    p[i] = r;
                }
                p[i]
            }
            for a in 0..aloft.len() {
                for b in (a + 1)..aloft.len() {
                    if (agg.particles[aloft[a]].pos - agg.particles[aloft[b]].pos).length() < touch {
                        let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
                        if ra != rb {
                            parent[ra] = rb;
                        }
                    }
                }
            }
            let mut clump: std::collections::HashMap<usize, f64> = std::collections::HashMap::new();
            for a in 0..aloft.len() {
                let root = find(&mut parent, a);
                *clump.entry(root).or_insert(0.0) += agg.particles[aloft[a]].mass;
            }
            let biggest = clump.values().cloned().fold(0.0f64, f64::max);
            format!(
                "{{\"bound\":{:.3},\"escaped\":{:.3},\"biggest\":{:.3},\"clumps\":{},\"earth\":{:.3}}}",
                bound_m / M_MOON,
                escaped_m / M_MOON,
                biggest / M_MOON,
                clump.len(),
                bound_earth_m / M_MOON
            )
        }

        /// Enter GEOLOGIC time (docs/27): promote each aloft bound clump to ONE body on the
        /// L-conserving circular orbit (tides circularize at ~constant angular momentum — flagged
        /// first-order), demote everything else into Earth (it has landed or will), retire the
        /// particle cloud, and hand evolution to the validated secular law.
        pub fn enter_geologic_time(&mut self) {
            let Some(agg) = self.moon_debris.as_ref() else { return };
            let earth = self.bodies[1];
            let mu = crate::orbit::G * earth.mass;
            // Cluster aloft bound fragments (same union-find as the disk stats).
            let touch = agg.contact.map_or(1.0e6, |c| 2.2 * c.radius);
            let aloft: Vec<usize> = (0..agg.particles.len())
                .filter(|&i| {
                    let p = &agg.particles[i];
                    let r = (p.pos - earth.pos).length();
                    0.5 * (p.vel - earth.vel).length_squared() - mu / r < 0.0
                        && r > 1.1 * EARTH_RADIUS_M
                })
                .collect();
            let mut parent: Vec<usize> = (0..aloft.len()).collect();
            fn find(p: &mut Vec<usize>, i: usize) -> usize {
                if p[i] != i {
                    let r = find(p, p[i]);
                    p[i] = r;
                }
                p[i]
            }
            for a in 0..aloft.len() {
                for b in (a + 1)..aloft.len() {
                    if (agg.particles[aloft[a]].pos - agg.particles[aloft[b]].pos).length() < touch {
                        let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
                        if ra != rb {
                            parent[ra] = rb;
                        }
                    }
                }
            }
            // Clump state: mass, L, and mass-weighted position/velocity (for the perigee test).
            let mut clumps: std::collections::HashMap<usize, (f64, glam::DVec3, glam::DVec3, glam::DVec3)> =
                std::collections::HashMap::new();
            for a in 0..aloft.len() {
                let root = find(&mut parent, a);
                let p = &agg.particles[aloft[a]];
                let e = clumps
                    .entry(root)
                    .or_insert((0.0, glam::DVec3::ZERO, glam::DVec3::ZERO, glam::DVec3::ZERO));
                e.0 += p.mass;
                e.1 += (p.pos - earth.pos).cross((p.vel - earth.vel) * p.mass);
                e.2 += (p.pos - earth.pos) * p.mass;
                e.3 += (p.vel - earth.vel) * p.mass;
            }
            // Promote ONLY clumps whose centre-of-mass PERIGEE clears the surface — a lofted blanket
            // with little angular momentum is fall-back material, not a moon (watched: "moonlets"
            // sitting ON the planet at the a-floor; sub-synchronous orbits spiral IN, Phobos' fate).
            self.geo_moonlets = clumps
                .values()
                .filter(|(m, _, rp, rv)| {
                    crate::orbit::perigee(*rp / *m, *rv / *m, mu)
                        .is_some_and(|p| p > 1.05 * EARTH_RADIUS_M)
                })
                .map(|(m, l, _, _)| crate::tides::Moonlet {
                    a: ((l.length() / m) * (l.length() / m) / mu).max(1.2 * EARTH_RADIUS_M),
                    mass: *m,
                })
                .collect();
            // Everything not promoted has landed or will: its mass and angular momentum go home.
            let promoted: f64 = self.geo_moonlets.iter().map(|m| m.mass).sum();
            let cloud_mass: f64 = agg.particles.iter().map(|p| p.mass).sum();
            let l_rest: glam::DVec3 = agg
                .particles
                .iter()
                .map(|p| (p.pos - earth.pos).cross((p.vel - earth.vel) * p.mass))
                .sum::<glam::DVec3>()
                - clumps.values().map(|(_, l, _, _)| *l).sum::<glam::DVec3>();
            self.bodies[1].mass += cloud_mass - promoted;
            self.spin_l += l_rest;
            self.moon_debris = None;
            self.debris_acc.clear();
            self.geologic = true;
        }

        /// Earth's day length (hours) from its live spin state — ∞ (0.0 returned as -1) if not spinning.
        pub fn earth_day_hours(&self) -> f64 {
            let t = crate::tides::spin_period_s(self.spin_l, self.bodies[1].mass, EARTH_RADIUS_M);
            if t.is_finite() { t / 3600.0 } else { -1.0 }
        }

        /// SIM seconds since the impact (−1 before it) — for the HUD's T+ aftermath clock.
        pub fn sim_since_impact_s(&self) -> f64 {
            if self.moon_debris.is_some() || self.geologic {
                self.sim_since_impact
            } else {
                -1.0
            }
        }

        /// Real seconds until the forecast impact (−1 once it has happened / no closing approach).
        /// The countdown IS the simulation's own forecast — distance and closing speed from the live
        /// N-body state, divided by the observable time rate.
        pub fn impact_countdown_s(&self) -> f64 {
            if self.impacted || self.bodies.len() < 3 {
                return -1.0;
            }
            let rel = self.bodies[2].pos - self.bodies[1].pos;
            let relv = self.bodies[2].vel - self.bodies[1].vel;
            let dist = rel.length() - (EARTH_RADIUS_M + self.impactor_radius);
            let closing = -rel.dot(relv) / rel.length().max(1.0);
            if closing <= 0.0 {
                return -1.0;
            }
            (dist / closing) / self.time_scale
        }

        /// Farthest BOUND debris fragment from Earth (km) — the camera rides the disk outward as it
        /// forms. Escapees are excluded: chasing them zoomed the view out until the whole scene was a
        /// handful of dark pixels (watched via the rig).
        pub fn debris_extent_km(&self) -> f64 {
            if self.geologic {
                return self.geo_moonlets.iter().map(|m| m.a).fold(0.0, f64::max) / 1000.0;
            }
            let earth = self.bodies[1];
            let mu = crate::orbit::G * earth.mass;
            self.moon_debris.as_ref().map_or(0.0, |agg| {
                agg.particles
                    .iter()
                    .filter_map(|p| {
                        let r = (p.pos - earth.pos).length();
                        let eps = 0.5 * (p.vel - earth.vel).length_squared() - mu / r;
                        (eps < 0.0).then_some(r)
                    })
                    .fold(0.0, f64::max)
                    / 1000.0
            })
        }

        pub fn set_time_scale(&mut self, scale: f32) {
            self.time_scale = (scale as f64).clamp(1.0, 2_000_000.0);
        }

        /// Live Earth–Moon separation in km (for the HUD). Should hover near 384,400 km.
        pub fn moon_distance_km(&self) -> f64 {
            (self.bodies[2].pos - self.bodies[1].pos).length() / 1000.0
        }

        /// Advance the PHYSICS by `real_dt` wall-clock seconds. Fixed sim-timestep substeps whose
        /// COUNT (not size) varies with the wall clock — so the physics rate is independent of the
        /// display frame rate (a 30 fps client and a 120 fps client simulate the same world), and the
        /// physics NEVER depends on rendering: the render only samples what this produced, RENDER_LAG_S
        /// later. Under overload the observable clock dilates (we drop backlog) rather than corrupting
        /// the physics with an oversized step — time slows before truth breaks.
        pub fn advance(&mut self, real_dt: f64) {
            let real_dt = real_dt.clamp(0.0, 0.25); // tab-sleep / hiccup guard
            self.phys_clock += real_dt;
            if self.geologic {
                // Millennia per second: the validated secular law in 50-year strides (exactly
                // L-conserving; see tides::secular_step). N-body/cloud machinery is retired — at this
                // LOD the orbit-averaged equations ARE the physics.
                let years = self.geo_rate_yr_s * real_dt;
                let year_s = 3.156e7;
                let mut left = years;
                while left > 0.0 {
                    let step = left.min(50.0);
                    crate::tides::secular_step(
                        &mut self.geo_moonlets,
                        &mut self.spin_l,
                        self.bodies[1].mass,
                        EARTH_RADIUS_M,
                        crate::tides::EARTH_K2_OVER_Q,
                        step * year_s,
                    );
                    left -= step;
                }
                self.sim_since_impact += years * year_s;
                self.push_snapshot();
                return;
            }
            self.real_accum += real_dt;
            // Substep budget per advance: generous for the cheap orbital phase; TIGHT once the O(n²)
            // debris cloud exists — a single slow frame used to trigger a death spiral (0.25 s of
            // backlog ⇒ 128 heavy substeps ⇒ an even slower frame, pinned at ~1 fps forever, watched
            // via the rig). Under load the observable clock dilates; the frame stays interactive.
            let max_substeps: u32 = if self.moon_debris.is_some() { 12 } else { 128 };
            let mut steps = 0u32;
            loop {
                // Per-mode fixed sim timestep: celestial fast-forward until the shatter, then the fixed
                // observable debris rate (time-LOD, docs/13). The mode can flip mid-advance (the shatter
                // substep itself) — the next iteration picks up the new rate immediately, so nothing
                // ever advances past the collision at the old rate.
                let (dt_sub, real_per_sub) = if self.moon_debris.is_some() {
                    let d = self.debris_frame_dt / MOON_DEBRIS_SUBSTEPS as f64;
                    (d, d / (self.debris_frame_dt * 60.0 * self.debris_rate_mul))
                } else {
                    (self.time_scale / 960.0, 1.0 / 960.0)
                };
                if self.real_accum < real_per_sub || steps >= max_substeps {
                    if steps >= max_substeps {
                        self.real_accum = 0.0; // overloaded: dilate observable time, keep physics true
                    }
                    break;
                }
                self.real_accum -= real_per_sub;
                steps += 1;
                self.step_substep(dt_sub);
            }
            self.push_snapshot();
        }

        /// One physics substep: N-body verlet + swept CCD (conservation-law contact state) + the
        /// mutual-impact materialization + the debris cloud. Pure physics — no rendering state.
        fn step_substep(&mut self, dt: f64) {
            let contact = EARTH_RADIUS_M + self.impactor_radius; // surfaces touch here
            let mut shatter: Option<(glam::DVec3, glam::DVec3, f64)> = None; // (site, v_contact, energy)
            let n_moons = self.bodies.len() - 2;

            // Position AND velocity relative to Earth BEFORE the step: the swept CCD finds *where* the
            // path crosses the surface, and the conservation laws recover the true state *there*.
            let earth_before = self.bodies[1].pos;
            let earth_vel_before = self.bodies[1].vel;
            let rel_old: Vec<glam::DVec3> =
                (0..n_moons).map(|k| self.bodies[2 + k].pos - earth_before).collect();
            let vel_old: Vec<glam::DVec3> =
                (0..n_moons).map(|k| self.bodies[2 + k].vel - earth_vel_before).collect();

            crate::orbit::verlet_step(&mut self.bodies, &mut self.acc, dt);
            // The planet visibly ROTATES at the rate its spin angular momentum implies.
            self.spin_angle += dt * self.spin_l.length()
                / crate::tides::moment_of_inertia(self.bodies[1].mass, EARTH_RADIUS_M);

            // SWEPT continuous collision (the general "forecast the path" primitive, docs/13).
            let (earth_pos, earth_vel) = (self.bodies[1].pos, self.bodies[1].vel);
            for k in 0..n_moons {
                if self.moon_hit[k] {
                    continue;
                }
                let moon = self.bodies[2 + k];
                let rel_new = moon.pos - earth_pos;
                if let Some(t) = crate::orbit::swept_first_contact(rel_old[k], rel_new, contact) {
                    self.moon_hit[k] = true;
                    self.impacted = true;
                    // First-contact point on Earth's surface, from the path fraction t; the TRUE state
                    // there from the two-body conservation laws (vis-viva + angular momentum) — never
                    // the post-step sample, which fast-forward renders garbage.
                    let rel_contact = rel_old[k] + (rel_new - rel_old[k]) * t;
                    let site = earth_pos + rel_contact;
                    let n_hat = rel_contact.normalize_or_zero();
                    let mu = crate::orbit::G * (self.bodies[1].mass + moon.mass);
                    let v_contact =
                        crate::orbit::contact_velocity(rel_old[k], vel_old[k], n_hat, contact, mu);
                    let m_red = self.bodies[1].mass * moon.mass / (self.bodies[1].mass + moon.mass);
                    let energy = 0.5 * m_red * v_contact.length_squared();
                    self.impact_energy_j += energy;
                    if k == 0 && shatter.is_none() {
                        // The impactor's fragments CARRY this velocity; the one contact law transfers
                        // the momentum into Earth's materialized matter and dissipation heats it.
                        shatter = Some((site, v_contact, energy));
                    }
                    // Park the point mass AT the impact point, co-moving with Earth.
                    self.bodies[2 + k].pos = site;
                    self.bodies[2 + k].vel = earth_vel;
                }
            }

            // MOON-vs-MOON collisions — the SAME primitives as moon-vs-Earth (every solid object is
            // matter): swept CCD on the pre-step relative path, the true contact state from the
            // conservation laws, an inelastic momentum-conserving merge, and the dissipated energy
            // accounted. (Materializing a moon-moon impact cloud — the same builder with the target's
            // layered profile — is the flagged next step; detection/resolution no longer special-cases
            // Earth.)
            let mm_contact = 2.0 * MOON_RADIUS_M;
            for i in 0..n_moons {
                for j in (i + 1)..n_moons {
                    let (a, b) = (self.bodies[2 + i], self.bodies[2 + j]);
                    let rel_o = rel_old[i] - rel_old[j];
                    let rel_n = (a.pos - self.bodies[1].pos) - (b.pos - self.bodies[1].pos);
                    if let Some(t) = crate::orbit::swept_first_contact(rel_o, rel_n, mm_contact) {
                        let v_rel_o = vel_old[i] - vel_old[j];
                        let rel_c = rel_o + (rel_n - rel_o) * t;
                        let n_hat = rel_c.normalize_or_zero();
                        let mu_g = crate::orbit::G * (a.mass + b.mass);
                        let v_c = crate::orbit::contact_velocity(rel_o, v_rel_o, n_hat, mm_contact, mu_g);
                        let m_red = a.mass * b.mass / (a.mass + b.mass);
                        self.impact_energy_j += 0.5 * m_red * v_c.length_squared();
                        self.impacted = true;
                        // Inelastic merge at the contact configuration: both to the COM velocity,
                        // separated by exactly the contact distance (momentum conserved).
                        let v_com = (a.vel * a.mass + b.vel * b.mass) / (a.mass + b.mass);
                        let mid = (a.pos * a.mass + b.pos * b.mass) / (a.mass + b.mass);
                        self.bodies[2 + i].pos = mid + n_hat * (mm_contact * a.mass / (a.mass + b.mass));
                        self.bodies[2 + j].pos = mid - n_hat * (mm_contact * b.mass / (a.mass + b.mass));
                        self.bodies[2 + i].vel = v_com;
                        self.bodies[2 + j].vel = v_com;
                    }
                }
            }

            // Keep already-hit / overlapping bodies parked at the surface (the slow-approach case and
            // the ongoing merge — the heavier Earth barely moves; momentum conserved).
            let (head, tail) = self.bodies.split_at_mut(2);
            let earth = &mut head[1];
            for moon in tail.iter_mut() {
                crate::orbit::resolve_contact(earth, moon, contact);
            }

            // The substep the Moon first strikes: MATERIALIZE both bodies at the interface (docs/24,
            // docs/25) — layered composition, real internal temperatures, one contact law.
            if let Some((site, v_contact, _energy)) = shatter {
                if self.moon_debris.is_none() {
                    let moon_mass = self.initial_bodies[2].mass;
                    let (earth_pos, earth_vel) = (self.bodies[1].pos, self.bodies[1].vel);
                    // Which matter arrives depends on the scenario: the Moon, or Theia (docs/27).
                    let impactor_profile = if self.birth_mode {
                        crate::planet::theia()
                    } else {
                        crate::planet::moon()
                    };
                    let (agg, acc0) = crate::impact::build_impact_debris_between(
                        &self.mats, site, earth_pos, earth_vel, moon_mass, v_contact,
                        &impactor_profile, &crate::planet::earth(), EARTH_MASS, EARTH_RADIUS_M,
                    );
                    self.debris_acc = acc0;
                    self.moon_debris = Some(agg);
                    self.impact_site_rel = Some(site - earth_pos); // crater mask, in Earth's frame
                    // The materialized cap LEFT Earth's bulk: its mass moves from the summary body to
                    // the particles (it was double-counted — Earth pulled ~22% too hard at Theia scale).
                    let cap_mass = moon_mass * (crate::impact::CAP_N as f64 / DEBRIS_N as f64);
                    self.bodies[1].mass -= cap_mass;
                    // The impactor IS the debris now — its matter exists exactly once. Reduce the parked
                    // point mass to nothing (a 1 kg marker keeps the body-array shape) so its mass isn't
                    // counted twice in the N-body (Theia is 11% of Earth — a real double-count).
                    self.bodies[2].mass = 1.0;
                }
            }

            // The debris cloud: everything it does — colliding with itself, ploughing into the ground,
            // resting, raining back — emerges from the forces inside `accelerations()` (the canonical
            // contact law + the conservative Earth boundary + Gauss-interior gravity).
            if let Some(agg) = self.moon_debris.as_mut() {
                let earth_pos = self.bodies[1].pos;
                // EVERY massive body pulls the debris with its LIVE mass — Earth (which shrank by the
                // materialized cap and regrows by demotion) and the Sun (declared matter). The static
                // build-time source is retired here so nothing is counted twice.
                agg.gravity_source = None;
                agg.set_gravity_bodies(vec![
                    (earth_pos, self.bodies[1].mass, EARTH_RADIUS_M),
                    (self.bodies[0].pos, self.bodies[0].mass, 6.96e8),
                ]);
                agg.set_boundary_center(earth_pos);
                agg.boundary_vel = self.bodies[1].vel; // the ground shears at Earth's velocity (no spin yet)
                if let Some(rel) = self.impact_site_rel {
                    agg.set_boundary_hole_center(earth_pos + rel); // the crater orbits with its planet
                }
                // TWO-WAY coupling, momentum-EXACT (Newton's third law): Earth's impulse is the
                // mirror of what the cloud actually received through this step. An independent
                // first-order estimate (evaluated at different positions/times than the cloud's own
                // integration) is non-symplectic — it PUMPED energy into the Earth–cloud orbit until
                // the debris unbound (Robin watched Earth shudder, then the moonlets escape). Here we
                // measure the cloud's true momentum change, subtract the Sun's share (that reaction
                // belongs to the Sun), and hand Earth the equal-and-opposite rest — which also carries
                // the boundary/shear reaction. Total momentum conserves to roundoff.
                let p_before: glam::DVec3 =
                    agg.particles.iter().map(|p| p.vel * p.mass).sum();
                let earth_vel_now = self.bodies[1].vel;
                let sun_pos = self.bodies[0].pos;
                let sun_mass = self.bodies[0].mass;
                let j_sun: glam::DVec3 = agg
                    .particles
                    .iter()
                    .map(|p| {
                        let d = sun_pos - p.pos;
                        let r2 = d.length_squared().max(1.0);
                        d * (crate::orbit::G * sun_mass * p.mass * (1.0 / (r2 * r2.sqrt()))) * dt
                    })
                    .sum();
                agg.step(&mut self.debris_acc, dt);
                let p_after: glam::DVec3 =
                    agg.particles.iter().map(|p| p.vel * p.mass).sum();
                let m_e = self.bodies[1].mass;
                self.bodies[1].vel -= (p_after - p_before - j_sun) / m_e;
                // ANGULAR reaction (docs/27), measured DIRECTLY at the boundary: the shear torque the
                // cloud received about Earth's centre mirrors into SPIN — this is how the impact sets
                // the day. (The earlier ΔL-differencing about a moving centre FABRICATED angular
                // momentum: a 0.9-h day from an impactor carrying a quarter of that — caught on the
                // HUD by its own physics being impossible.)
                self.spin_l -= agg.boundary_torque_sum * dt;
                // TIDAL torque: the spinning Earth's bulge exchanges angular momentum with every aloft
                // bound moonlet (outward migration for a fast prograde spin) — the 4.5 Gyr mechanism,
                // validated against the Moon's measured 3.8 cm/yr recession (tides.rs).
                let mu_e = crate::orbit::G * m_e;
                let spin_omega = self.spin_l.length()
                    / crate::tides::moment_of_inertia(m_e, EARTH_RADIUS_M);
                let j2 = crate::tides::j2_from_spin(spin_omega, m_e, EARTH_RADIUS_M);
                let s_hat = self.spin_l.try_normalize().unwrap_or(glam::DVec3::Z);
                for p in agg.particles.iter_mut() {
                    let r = (p.pos - earth_pos).length();
                    // The oblate figure's gravity (J2): close orbits around the squashed post-impact
                    // Earth precess. EXTERIOR multipole ONLY — the expansion is invalid inside the
                    // body, and applying it to crater-pile particles (r ≈ 0.5 R⊕, where 1/r⁴ blows up
                    // 16×) pumped the pile against the boundary until EVERYTHING ejected past escape
                    // (Robin: "an explosion of fudge" — it was: an equation used outside its domain).
                    if r > 1.05 * EARTH_RADIUS_M {
                        p.vel +=
                            crate::tides::j2_accel(p.pos - earth_pos, mu_e, EARTH_RADIUS_M, j2, s_hat)
                                * dt;
                    }
                    let eps = 0.5 * (p.vel - earth_vel_now).length_squared() - mu_e / r;
                    if eps < 0.0 && r > 1.1 * EARTH_RADIUS_M {
                        let (kick, d_spin) = crate::tides::tidal_kick(
                            crate::tides::EARTH_K2_OVER_Q,
                            p,
                            earth_pos,
                            earth_vel_now,
                            m_e,
                            EARTH_RADIUS_M,
                            self.spin_l,
                            dt,
                        );
                        p.vel += kick;
                        self.spin_l += d_spin;
                    }
                }
                self.sim_since_impact += dt; // the aftermath clock (sim time, not wall time)
                // DEMOTION (docs/27): settled matter IS Earth again — drain it back into the bulk
                // summary (mass to the planet, particle removed). Fidelity ∝ observability (docs/13);
                // FPS follows from honesty — we stop simulating what has stopped happening. r_tol spans
                // the pile depth; the drained heat is dropped (flagged). Earth's gravity-source mass for
                // the remaining debris still reads the original EARTH_MASS (≤2% low — flagged).
                let frag_r = agg.contact.map_or(5.0e5, |c| c.radius);
                let (n_drained, m_drained, l_drained) = agg.drain_settled(
                    earth_pos,
                    EARTH_RADIUS_M,
                    self.bodies[1].vel,
                    30.0,
                    4.0 * frag_r,
                );
                if n_drained > 0 {
                    self.bodies[1].mass += m_drained; // Earth grows by what it swallowed
                    self.spin_l += l_drained; // ...and spins up by the angular momentum it swallowed
                    // The returned matter refills the bowl: heal by its solid volume (bulk density).
                    // (hole_radius() inlined via field reads — `agg` holds the moon_debris borrow.)
                    let rho = self.mats[agg.material].density.max(1.0) as f64;
                    self.crater_heal_m3 += m_drained / rho;
                    let r0 = (2.0 * self.impactor_radius).min(0.55 * EARTH_RADIUS_M);
                    let vol0 = (2.0 / 3.0) * std::f64::consts::PI * r0.powi(3);
                    let rem = (vol0 - self.crater_heal_m3).max(0.0);
                    agg.set_boundary_hole_radius((rem * 3.0 / (2.0 * std::f64::consts::PI)).cbrt());
                    self.debris_acc = agg.accelerations(); // particle count changed
                }
                // NO merge closure: a pairwise bound-in-contact merge welded disk material to
                // falling-back material mid-curtain and destroyed the disk (measured: 0.55 → 0.00
                // M_moon lofted). Accretion is REAL physics here — inelastic contact + self-gravity
                // clump fragments into rubble-pile moonlets without any rule (Robin: "drive this with
                // real particle physics").
            }
        }

        /// Record the observable state at the current physics clock (the renderer's source of truth).
        fn push_snapshot(&mut self) {
            let frag0 = (self.impactor_mass / DEBRIS_N as f64).max(1.0);
            let (debris, temps, sizes, mats, srcs) = match self.moon_debris.as_ref() {
                Some(agg) => (
                    agg.particles.iter().map(|p| p.pos).collect(),
                    agg.temps.clone(),
                    agg.particles.iter().map(|p| (p.mass / frag0).cbrt() as f32).collect(),
                    agg.mat_ids.clone(),
                    agg.source.clone(),
                ),
                None => (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new()),
            };
            self.snaps.push_back(FrameSnap {
                t: self.phys_clock,
                bodies: self.bodies.iter().map(|b| b.pos).collect(),
                debris,
                temps,
                sizes,
                mats,
                srcs,
                // Shattered is FOREVER (until Replay): keying this off moon_debris alone RESURRECTED
                // the parked impactor's grain shell when geologic mode retired the cloud — a
                // Theia-sized ghost sitting on Earth with no orbit ("pure fudge" — a render-state bug
                // conjuring mass, and I had rationalized it in my own screenshot instead of chasing it).
                shattered: self.moon_debris.is_some() || self.geologic,
            });
            // Keep a little more history than the lag needs; drop the rest.
            let horizon = self.phys_clock - (RENDER_LAG_S + 0.5);
            while self.snaps.len() > 2 && self.snaps.front().is_some_and(|f| f.t < horizon) {
                self.snaps.pop_front();
            }
        }

        /// The state the RENDERER sees: snapshots interpolated at (now − RENDER_LAG_S). Falls back to
        /// the live state before the first snapshot exists.
        #[allow(clippy::type_complexity)]
        fn sampled_state(
            &self,
        ) -> (Vec<glam::DVec3>, Vec<glam::DVec3>, Vec<f32>, Vec<f32>, Vec<usize>, Vec<u8>, bool) {
            if self.snaps.is_empty() {
                let frag0 = (self.impactor_mass / DEBRIS_N as f64).max(1.0);
                let (d, t, sz, mt, sc) = match self.moon_debris.as_ref() {
                    Some(a) => (
                        a.particles.iter().map(|p| p.pos).collect(),
                        a.temps.clone(),
                        a.particles.iter().map(|p| (p.mass / frag0).cbrt() as f32).collect(),
                        a.mat_ids.clone(),
                        a.source.clone(),
                    ),
                    None => (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new()),
                };
                return (
                    self.bodies.iter().map(|b| b.pos).collect(),
                    d,
                    t,
                    sz,
                    mt,
                    sc,
                    self.moon_debris.is_some(),
                );
            }
            let target = self.phys_clock - RENDER_LAG_S;
            // Bracket the target time (snaps are time-ordered).
            let mut s0 = self.snaps.front().unwrap();
            let mut s1 = s0;
            for f in self.snaps.iter() {
                s1 = f;
                if f.t > target {
                    break;
                }
                s0 = f;
            }
            let f = if s1.t > s0.t {
                ((target - s0.t) / (s1.t - s0.t)).clamp(0.0, 1.0)
            } else {
                1.0
            };
            let bodies: Vec<glam::DVec3> = s0
                .bodies
                .iter()
                .zip(s1.bodies.iter())
                .map(|(a, b)| *a + (*b - *a) * f)
                .collect();
            // Debris lerps only when both snapshots carry it (across the shatter/merge boundary, take
            // s1's — counts change when moonlets accrete).
            // mats travels with whichever snapshot supplies temps/sizes, so tints stay aligned to the
            // fragment order those came from.
            let (debris, temps, sizes, mats, srcs) =
                if !s0.debris.is_empty() && s0.debris.len() == s1.debris.len() {
                    (
                        s0.debris
                            .iter()
                            .zip(s1.debris.iter())
                            .map(|(a, b)| *a + (*b - *a) * f)
                            .collect(),
                        s0.temps.clone(),
                        s0.sizes.clone(),
                        s0.mats.clone(),
                        s0.srcs.clone(),
                    )
                } else if s1.shattered {
                    (s1.debris.clone(), s1.temps.clone(), s1.sizes.clone(), s1.mats.clone(), s1.srcs.clone())
                } else {
                    (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new())
                };
            let shattered = if f < 1.0 { s0.shattered } else { s1.shattered };
            let any_debris = !debris.is_empty();
            (bodies, debris, temps, sizes, mats, srcs, shattered || any_debris)
        }

        pub fn render(&mut self) -> Result<(), JsValue> {
            // NO physics here (docs/13): the renderer samples the physics snapshots RENDER_LAG_S behind
            // the live state — every event it draws is already fully resolved. The physics is advanced
            // by `advance(real_dt)`, on wall-clock time, independent of this function's call rate.
            let (r_bodies, r_debris, r_temps, r_sizes, r_mats, r_srcs, r_shattered) =
                self.sampled_state();

            let view_proj = self.view_proj();

            // Render in the focused body's frame of reference (docs/17): its position is the origin,
            // everything else is drawn relative to it. Switching focus re-centres the whole view.
            let focus = r_bodies[self.focus];
            let sun = r_bodies[0];
            let moon_r = (self.impactor_radius * DISPLAY_SCALE) as f32;

            // Light direction = TO the real Sun from each body (per-body; the Sun is the illuminant,
            // not a hardcoded direction). So the lit hemisphere and the phases come from the geometry.
            let earth_light = (sun - r_bodies[1]).as_vec3().normalize();
            // EARTH AS PARTICLES (docs/15): the planet renders as a shell of coarse grains — the honest
            // low-res visualization of the un-materialized bulk (whose PHYSICS is the boundary + gravity
            // source). A smooth sphere would hide excavation; grains can be missing. Shell points inside
            // the materialized impact region are hidden — the real (moving, glowing) cap particles are
            // the matter there now, and the void they leave IS the crater.
            let earth_center = r_bodies[1];
            let shell_spacing = EARTH_RADIUS_M * (4.0 * std::f64::consts::PI / SHELL_N as f64).sqrt();
            let shell_grain_r = ((0.62 * shell_spacing) * DISPLAY_SCALE) as f32;
            // The crater opens only once the RENDERED clock reaches the shatter (not the physics clock).
            let crater_site = if r_shattered {
                self.impact_site_rel.map(|rel| earth_center + rel)
            } else {
                None
            };
            let crater_r = 1.1 * self.hole_radius(); // the crater as it stands — healing shrinks it
            // Camera eye in display coordinates (relative to the focus body) — the same construction
            // as view_proj, needed for the per-grain Rayleigh view path.
            let cp = self.camera.pitch.cos();
            let eye_disp = glam::DVec3::new(
                (cp * self.camera.yaw.sin()) as f64,
                self.camera.pitch.sin() as f64,
                (cp * self.camera.yaw.cos()) as f64,
            ) * (self.camera.base_distance * self.camera.zoom) as f64;
            let sun_dir_earth = (sun - earth_center).normalize_or_zero();
            let spin_axis = self.spin_l.try_normalize().unwrap_or(glam::DVec3::Z);
            let spin_rot = glam::DQuat::from_axis_angle(
                spin_axis,
                self.spin_angle % (2.0 * std::f64::consts::PI),
            );
            // OBLATE figure: the spin flattens the planet (Radau–Darwin) — equator bulges (+f/3),
            // poles sink (−2f/3), volume-preserving to first order. At today's day it's 1/298
            // (imperceptible); at the post-impact 3.8-h day it's ~13% — a visibly squashed world.
            let spin_omega_r = self.spin_l.length()
                / crate::tides::moment_of_inertia(self.bodies[1].mass, EARTH_RADIUS_M);
            let flat = crate::tides::flattening_from_spin(
                spin_omega_r, self.bodies[1].mass, EARTH_RADIUS_M,
            );
            for (i, uni) in self.shell_unis.iter().enumerate() {
                let dir = spin_rot * crate::impact::fib_dir(i, SHELL_N);
                let u = dir.dot(spin_axis);
                let r_oblate = (EARTH_RADIUS_M - 0.62 * shell_spacing)
                    * (1.0 + flat * (1.0 / 3.0 - u * u)); // +f/3 equator, −2f/3 poles
                let pos_w = earth_center + dir * r_oblate;
                let hidden = crater_site.map_or(false, |s| (pos_w - s).length() < crater_r);
                let scale = if hidden { 0.0 } else { shell_grain_r }; // zero-scale ⇒ not drawn
                let spos = ((pos_w - focus) * DISPLAY_SCALE).as_vec3();
                // Continents & oceans (docs/25): each grain samples the landmask for its direction and
                // wears the REAL surface material's reflectance — granite land, water ocean. "Average
                // area particles": the grain is the mean of its ~10°×10° patch, nothing painted.
                let surf = crate::planet::earth_surface_material(dir);
                let m = &self.mats[materials::index_of(&self.mats, surf)];
                // RAYLEIGH (docs/26): the declared air scatters sunlight over this patch — a blue
                // veil (into the emissive channel: it IS added light) whose ground shows through
                // slightly reddened (two-way transmittance). All from the emergent pressure; an
                // airless world renders colorless by the same code.
                let v_dir = (eye_disp - (pos_w - focus) * DISPLAY_SCALE).normalize_or_zero();
                let mu_v = dir.dot(v_dir);
                let mu_s = dir.dot(sun_dir_earth);
                let cos_th = v_dir.dot(sun_dir_earth);
                let veil = crate::atmosphere::rayleigh_veil(mu_v, mu_s, cos_th, self.atm_tau, 22.0);
                let tr = crate::atmosphere::rayleigh_transmit(mu_v, mu_s, self.atm_tau);
                let tint = [m.albedo[0] * tr[0], m.albedo[1] * tr[1], m.albedo[2] * tr[2], 1.0];
                write_space_uniform(
                    &self.queue,
                    uni,
                    view_proj,
                    Mat4::from_translation(spos) * Mat4::from_scale(Vec3::splat(scale)),
                    earth_light,
                    tint,
                    [veil[0], veil[1], veil[2], 1.0], // the sky, added over the ground
                );
            }
            // THE SUN: real matter (planet::sun), rendered where it actually is — a ~0.5° disk of
            // photosphere-temperature plasma (5,772 K → white, via the same incandescence law as hot
            // rock). It enters frame whenever the camera looks sunward — opposition geometry included —
            // because it is drawn at its position, not painted on a skybox.
            {
                let spos = ((r_bodies[0] - focus) * DISPLAY_SCALE).as_vec3();
                let sun_r_disp = (6.96e8 * DISPLAY_SCALE) as f32;
                write_space_uniform(
                    &self.queue,
                    &self.sun_uni,
                    view_proj,
                    Mat4::from_translation(spos) * Mat4::from_scale(Vec3::splat(sun_r_disp)),
                    earth_light,
                    [0.0, 0.0, 0.0, 1.0], // no reflectance — it is the illuminant
                    // The photosphere's radiance is ~4.6e4× a sunlit white surface at 1 AU
                    // (~2e7 vs ~430 W/m²/sr): ANY exposure set for the scene saturates on the Sun.
                    // incandescence()'s rock-glow intensity (~2) tone-mapped to dull grey — honest
                    // brightness is the measured ratio, which pins the Reinhard output at white.
                    [1.0, 1.0, 1.0, 4.6e4],
                );
            }
            // The BULK INTERIOR (the un-materialized deep Earth): an opaque sphere at the depth the
            // crater exposes — the top of the outer core — glowing at its real temperature (docs/25).
            // The planet is not hollow; through the crater you see molten interior, not far-side crust.
            {
                let ipos = ((earth_center - focus) * DISPLAY_SCALE).as_vec3();
                // The interior must wear the SAME oblate figure as the shell, else at the post-impact
                // ~13% flattening the poles sink below a perfect 0.985 R sphere and the interior pokes
                // OUT through the crust at both poles (a render-truth bug). Ellipsoid: equator +f/3,
                // poles −2f/3 about the spin axis — one non-uniform scale, oriented to the spin axis.
                let ir = (EARTH_RADIUS_M * 0.985) * DISPLAY_SCALE;
                let ir_eq = (ir * (1.0 + flat / 3.0)) as f32;
                let ir_pol = (ir * (1.0 - 2.0 * flat / 3.0)) as f32;
                let align = glam::DQuat::from_rotation_arc(glam::DVec3::Z, spin_axis);
                write_space_uniform(
                    &self.queue,
                    &self.interior_uni,
                    view_proj,
                    Mat4::from_translation(ipos)
                        * Mat4::from_quat(align.as_quat())
                        * Mat4::from_scale(Vec3::new(ir_eq, ir_eq, ir_pol)),
                    earth_light,
                    self.interior_tint,
                    self.interior_glow, // outer-core iron: self-lit at its real temperature
                );
            }
            // CRATER WALL: grains on the carved bowl surface (the physical boundary hole), each wearing
            // the layer material + REAL temperature at its own depth — crust rim, mantle wall, glowing
            // floor. The gradient from dark rim to white-hot depth is the honest incandescence read.
            {
                let profile = crate::planet::earth();
                let hole_r = self.hole_radius();
                let wall_grain_r =
                    ((hole_r * (4.0 * std::f64::consts::PI / WALL_N as f64).sqrt() * 0.62)
                        * DISPLAY_SCALE) as f32;
                for (i, uni) in self.wall_unis.iter().enumerate() {
                    let mut scale = 0.0f32;
                    let mut wpos = glam::DVec3::ZERO;
                    let mut tint = [0.0f32; 4];
                    let mut glow = [0.0f32; 4];
                    if let Some(site) = crater_site {
                        let p = site + crate::impact::fib_dir(i, WALL_N) * (hole_r * 0.96);
                        let r = (p - earth_center).length();
                        if r < EARTH_RADIUS_M * 0.985 {
                            // On the buried part of the bowl: real layer material + temperature here.
                            let m = &self.mats
                                [materials::index_of(&self.mats, profile.layer_at(r).material)];
                            tint = [m.albedo[0], m.albedo[1], m.albedo[2], 1.0];
                            glow = incandescence(profile.temperature_at(r) as f32);
                            scale = wall_grain_r;
                            wpos = p;
                        }
                    }
                    let spos = ((wpos - focus) * DISPLAY_SCALE).as_vec3();
                    write_space_uniform(
                        &self.queue,
                        uni,
                        view_proj,
                        Mat4::from_translation(spos) * Mat4::from_scale(Vec3::splat(scale)),
                        earth_light,
                        tint,
                        glow,
                    );
                }
            }
            // MOONS AS MATTER: each intact moon is a grain shell (like Earth) — its basalt crust at
            // its real reflectance, no smooth-sphere summary. A shattered moon is its debris instead.
            let mshell_spacing =
                self.impactor_radius * (4.0 * std::f64::consts::PI / MOON_SHELL_N as f64).sqrt();
            let mshell_grain_r = ((0.62 * mshell_spacing) * DISPLAY_SCALE) as f32;
            for (idx, uni) in self.moon_unis.iter().enumerate() {
                let k = idx / MOON_SHELL_N;
                let i = idx % MOON_SHELL_N;
                if k == 0 && r_shattered {
                    // moon 0 has SHATTERED — drawn as its debris fragments below
                    write_space_uniform(
                        &self.queue, uni, view_proj, Mat4::from_scale(Vec3::ZERO),
                        earth_light, [0.0; 4], [0.0; 4],
                    );
                    continue;
                }
                let bi = 2 + k; // body index of this moon
                let dir = crate::impact::fib_dir(i, MOON_SHELL_N);
                let pos_w = r_bodies[bi] + dir * (self.impactor_radius - 0.62 * mshell_spacing);
                let mpos = ((pos_w - focus) * DISPLAY_SCALE).as_vec3();
                let mlight = (sun - r_bodies[bi]).as_vec3().normalize();
                write_space_uniform(
                    &self.queue,
                    uni,
                    view_proj,
                    Mat4::from_translation(mpos) * Mat4::from_scale(Vec3::splat(mshell_grain_r)),
                    mlight,
                    self.moon_tint, // aggregate albedo of basalt (docs/17); dark, lit bright by the sun
                    [0.0; 4],       // intact moon: reflected light only (its hot core is buried)
                );
            }
            // The shattered Moon: each surviving fragment is drawn as a small basalt sphere at its real
            // position — the debris cloud (some flying out, some falling back) IS the crater ejecta at
            // planetary scale, emergent from the aggregate physics, not a scripted animation.
            let mut debris_count = 0usize;
            if !r_debris.is_empty() {
                let frag_r = moon_r / (DEBRIS_N as f32).cbrt(); // N fragments ≈ the Moon's volume
                // Composition rides the SAME lagged snapshot as positions/temps (r_mats): a live read of
                // moon_debris.mat_ids desynced after drain's swap_remove reordered the live array.
                for (i, pos) in r_debris.iter().enumerate() {
                    if i >= self.debris_unis.len() {
                        break;
                    }
                    let fpos = ((*pos - focus) * DISPLAY_SCALE).as_vec3();
                    let flight = (sun - *pos).as_vec3().normalize();
                    // Incandescence comes free from the fragment's real temperature — its layer's
                    // internal heat plus whatever contact dissipation added (docs/20, docs/25).
                    let glow = incandescence(r_temps.get(i).copied().unwrap_or(0.0));
                    // Each fragment wears ITS material's reflectance: basalt crust, peridotite mantle,
                    // iron core — the excavated composition is visible, not a uniform gray.
                    let _m = &self.mats[r_mats.get(i).copied().unwrap_or(0)];
                    // PROVENANCE overlay (docs/28 step 1): a DIAGNOSTIC categorical reflectance, not the
                    // real material albedo — Earth-derived matter reads blue, Theia-derived warm/orange,
                    // so the disk's origin split is visible AT A GLANCE. The discriminating channel is
                    // kept low (blue≈0 for Theia, red low for Earth) so the hue survives the strong-sun
                    // Reinhard tone-map instead of washing to cream. Today the disk is ~100% Theia (all
                    // orange); Earth-blue specks appearing is how progressive excavation (step 3) proves
                    // itself on screen. Incandescence (temperature) still glows on top for hot fragments.
                    let src = r_srcs.get(i).copied().unwrap_or(crate::aggregate::SOURCE_IMPACTOR);
                    // Low reflectances: under SUN_GAIN×Reinhard, the dominant channel must land ~1–2 in
                    // radiance to read as a SATURATED hue (higher just washes to cream). Discriminating
                    // channel near zero so the tone-map can't wash it out.
                    let tint = if src == crate::aggregate::SOURCE_TARGET {
                        [0.010f32, 0.045, 0.135, 1.0] // Earth: blue
                    } else {
                        [0.110f32, 0.028, 0.006, 1.0] // Theia: warm orange
                    };
                    // Display radius grows with the ⅓ power of accreted mass — you can SEE the Moon
                    // winning: one fragment swells while the count falls.
                    let size = r_sizes.get(i).copied().unwrap_or(1.0);
                    write_space_uniform(
                        &self.queue,
                        &self.debris_unis[i],
                        view_proj,
                        Mat4::from_translation(fpos) * Mat4::from_scale(Vec3::splat(frag_r * size)),
                        flight,
                        tint,
                        glow,
                    );
                    debris_count += 1;
                }
            }
            // GEOLOGIC moonlets: one grain ball per body at its true orbital radius. Orbital PHASE is
            // unresolvable at millennia-per-second (a moonlet completes ~10⁶ orbits per frame), so the
            // drawn angle is a slow golden-spaced drift — a liveliness cue, honestly not a phase.
            if self.geologic {
                let rho = 2_900.0f64; // basalt bulk — the moonlets' crusts have long frozen (docs/27)
                for (i, m) in self.geo_moonlets.iter().enumerate() {
                    if i >= self.debris_unis.len() {
                        break;
                    }
                    let ang = 2.399963 * i as f64 + self.phys_clock * 0.15;
                    let dir = glam::DVec3::new(ang.cos(), ang.sin(), 0.0);
                    let pos_w = earth_center + dir * m.a;
                    let r_disp = ((3.0 * m.mass / (4.0 * std::f64::consts::PI * rho)).cbrt()
                        * DISPLAY_SCALE) as f32;
                    let fpos = ((pos_w - focus) * DISPLAY_SCALE).as_vec3();
                    let flight = (sun - pos_w).as_vec3().normalize();
                    write_space_uniform(
                        &self.queue,
                        &self.debris_unis[i],
                        view_proj,
                        Mat4::from_translation(fpos) * Mat4::from_scale(Vec3::splat(r_disp)),
                        flight,
                        self.moon_tint,
                        [0.0; 4], // crusted over: reflected light only (interior heat is sub-surface)
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
                draw(&mut pass, &self.sun_uni, &self.sphere_gpu); // the Sun, where it really is
                draw(&mut pass, &self.interior_uni, &self.sphere_gpu); // the glowing deep interior
                for uni in self.wall_unis.iter() {
                    draw(&mut pass, uni, &self.sphere_gpu); // crater bowl wall (zero-scale when intact)
                }
                for uni in self.shell_unis.iter() {
                    draw(&mut pass, uni, &self.sphere_gpu); // Earth: a shell of coarse grains
                }
                for (idx, uni) in self.moon_unis.iter().enumerate() {
                    if idx / MOON_SHELL_N == 0 && r_shattered {
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
        emissive: [f32; 4],
    ) {
        let u = SpaceUniforms {
            view_proj: view_proj.to_cols_array_2d(),
            model: model.to_cols_array_2d(),
            light_dir: [light.x, light.y, light.z, 0.0],
            tint,
            emissive,
        };
        queue.write_buffer(&slot.buf, 0, bytemuck::bytes_of(&u));
    }

    /// Blackbody-ish incandescence for a material at temperature `temp` (K): a self-emissive glow colour
    /// (rgb) and intensity (w), ramping dark→red→orange→yellow→white as rock heats past ~800 K. This is
    /// the visual "for free" from the thermal state — the render just reads the fragment's real temperature.
    fn incandescence(temp: f32) -> [f32; 4] {
        const GLOW_START: f32 = 800.0; // K — below this, rock shows no visible self-glow
        const WHITE_HOT: f32 = 3200.0; // K — ramp saturates to white here
        if temp <= GLOW_START {
            return [0.0, 0.0, 0.0, 0.0];
        }
        let x = ((temp - GLOW_START) / (WHITE_HOT - GLOW_START)).clamp(0.0, 1.0);
        // Red saturates first, then green (→orange/yellow), then blue (→white) — a coarse Planckian locus.
        let r = (x * 2.5).clamp(0.0, 1.0);
        let g = ((x - 0.25) * 2.0).clamp(0.0, 1.0);
        let b = ((x - 0.55) * 2.2).clamp(0.0, 1.0);
        // Intensity grows with temperature so the hottest fragments read brightest (Stefan–Boltzmann-ish).
        let intensity = (0.4 + 1.6 * x) * (x.max(0.05));
        [r, g, b, intensity]
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
        assert_eq!(mats.len(), 23, "seed database should have 23 materials");
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
    fn world_column_is_density_sorted_light_skin_over_heavy_depths() {
        // The surface patch is gravitationally sorted like the real Earth: a light organic skin on top,
        // then progressively DENSER matter with depth, down to the iron core. (This supersedes the old
        // granite/dirt/grass game world, which the engine no longer generates; the precise material
        // ORDER — grass → basalt → peridotite → iron — is asserted by world::tests::
        // column_is_earths_real_layers_top_to_bottom. Here we assert the distinct honest property:
        // scanning DOWN a column, density never decreases, and several distinct layers are stacked.)
        let mats = materials::load();
        let w = world::generate(&mats);

        let (x, z) = (w.w as i32 / 2, w.d as i32 / 2);
        assert!(w.is_solid(x, 0, z), "world must be solid at the bottom");
        let top = w.surface_top_voxel(x, z).expect("solid column at centre");

        let mut prev_density = 0.0f32;
        let mut layers = 0usize;
        let mut last_mat: Option<usize> = None;
        for y in (0..top).rev() {
            let m = w.material_at(x, y, z).expect("solid below the surface top (no holes)");
            let d = mats[m].density;
            assert!(
                d >= prev_density - 1e-3,
                "denser matter must sit deeper: {} (ρ={d}) sits below ρ={prev_density}",
                mats[m].id
            );
            prev_density = d;
            if last_mat != Some(m) {
                layers += 1;
                last_mat = Some(m);
            }
        }
        assert!(
            layers >= 3,
            "the column must show multiple stacked layers, not one slab (got {layers})"
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
