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

mod body;
mod damage;
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
    use crate::{body, gravity, materials, matter, texture, world};
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
    const GRAVITY_BLOCK: usize = 8; // voxel aggregation for the mass field (coarser = cheaper queries)
    const PHYS_SUBSTEPS: u32 = 8;
    const DEFAULT_TIME_SCALE: f32 = 250.0; // sim-seconds per real-second (fast-forward)

    // Phase 3 dig/fracture.
    const MAX_PARTICLES: usize = 60_000;
    const PARTICLE_CUBE_HALF: f32 = 0.42;
    const DIG_RADIUS: f32 = 3.0;
    const DIG_POWER: f32 = 1.5e6; // breaks soil/grass, not granite
    const BLAST_POWER: f32 = 3.0e7; // breaks granite too

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
        sphere_gpu: GpuMesh,
        world_uni: UniformSlot,
        sphere_uni: UniformSlot,

        // Simulation
        mats: Vec<materials::Material>,
        world: world::World,
        field: gravity::MassField,
        sphere: body::Sphere,
        matter: matter::MatterSim,
        spawn: Vec3,
        time_scale: f32,

        // Debris (particle) rendering
        cube_gpu: GpuMesh,
        particle_pipeline: wgpu::RenderPipeline,
        particle_instances: wgpu::Buffer,
        particle_bind: wgpu::BindGroup,

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
            let iron_idx = materials::index_of(&mats, "iron");
            let sphere_mesh = mesher::build_uv_sphere(
                SPHERE_RADIUS,
                iron_idx as u32,
                mats[iron_idx].albedo,
                16,
                24,
            );
            let world_gpu = upload_mesh(&device, "world", &world_mesh);
            let sphere_gpu = upload_mesh(&device, "sphere", &sphere_mesh);
            log::info!(
                "meshes: world {} tris, sphere {} tris",
                world_mesh.indices.len() / 3,
                sphere_mesh.indices.len() / 3
            );

            // --- Spawn the probe above the center column ---
            let c = world.center();
            let surf = world
                .surface_top_voxel(c.x as i32, c.z as i32)
                .map(|t| t as f32 - c.y)
                .unwrap_or(0.0);
            let spawn = Vec3::new(0.0, surf + SPHERE_RADIUS + SPAWN_HEIGHT, 0.0);
            let sphere = body::Sphere::new(spawn, SPHERE_MASS, SPHERE_RADIUS);

            log::info!(
                "greenfield-engine: world mass = {:.3e} kg, surface g ~ {:.3e} m/s^2",
                field.total_mass,
                field.acceleration_at(spawn, GRAVITY_SOFTENING).length()
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
            let sphere_ubuf = make_uniform_buffer(&device);
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
            let sphere_uni = UniformSlot {
                bind: make_world_bind(
                    &device,
                    &world_bind_layout,
                    &sphere_ubuf,
                    &tex_view,
                    &sampler,
                    &matparams_buf,
                ),
                buf: sphere_ubuf,
            };
            let pipeline = build_pipeline(&device, &world_bind_layout, config.format);

            // Debris: a unit cube instanced per particle, tinted by material albedo.
            let matter = matter::MatterSim::new(MAX_PARTICLES);
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
                sphere_gpu,
                world_uni,
                sphere_uni,
                mats,
                world,
                field,
                sphere,
                matter,
                spawn,
                time_scale: DEFAULT_TIME_SCALE,
                cube_gpu,
                particle_pipeline,
                particle_instances,
                particle_bind,
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
        /// Gravitational field magnitude the probe currently feels (m/s²) — the "measured g".
        pub fn surface_gravity(&self) -> f32 {
            self.field
                .acceleration_at(self.sphere.pos, GRAVITY_SOFTENING)
                .length()
        }
        pub fn sphere_altitude(&self) -> f32 {
            self.sphere.altitude(self.ground_under_sphere())
        }
        pub fn sphere_speed(&self) -> f32 {
            self.sphere.vel.length()
        }
        pub fn is_resting(&self) -> bool {
            self.sphere.resting
        }
        pub fn time_scale(&self) -> f32 {
            self.time_scale
        }
        pub fn set_time_scale(&mut self, s: f32) {
            self.time_scale = s.clamp(1.0, 5000.0);
        }
        /// Re-drop the probe from its spawn point.
        pub fn reset_drop(&mut self) {
            self.sphere = body::Sphere::new(self.spawn, SPHERE_MASS, SPHERE_RADIUS);
        }

        /// Number of airborne debris particles (HUD).
        pub fn particle_count(&self) -> u32 {
            self.matter.particle_count() as u32
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
            }
        }

        fn remesh_world(&mut self) {
            let mesh = mesher::build_surface_nets(&self.world, &self.mats);
            self.world_gpu = upload_mesh(&self.device, "world", &mesh);
        }

        fn upload_particles(&self) -> u32 {
            let instances: Vec<InstanceRaw> = self
                .matter
                .particles
                .iter()
                .map(|p| InstanceRaw {
                    offset: [p.pos.x, p.pos.y, p.pos.z],
                    color: self.mats[p.material].albedo,
                })
                .collect();
            if !instances.is_empty() {
                self.queue.write_buffer(
                    &self.particle_instances,
                    0,
                    bytemuck::cast_slice(&instances),
                );
            }
            instances.len() as u32
        }

        /// Render one frame (advances the simulation first).
        pub fn render(&mut self) -> Result<(), JsValue> {
            self.step_physics();
            if self.matter.take_dirty() {
                self.remesh_world();
            }
            let particle_count = self.upload_particles();

            let (view_proj, eye) = self.view_proj();
            let light = Vec3::new(0.45, 0.9, 0.4).normalize();
            self.write_uniform(&self.world_uni, view_proj, Mat4::IDENTITY, eye, light);
            self.write_uniform(
                &self.sphere_uni,
                view_proj,
                Mat4::from_translation(self.sphere.pos),
                eye,
                light,
            );

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
                draw(&mut pass, &self.sphere_uni, &self.sphere_gpu);

                if particle_count > 0 {
                    pass.set_pipeline(&self.particle_pipeline);
                    pass.set_bind_group(0, &self.particle_bind, &[]);
                    pass.set_vertex_buffer(0, self.cube_gpu.vertex_buf.slice(..));
                    pass.set_vertex_buffer(1, self.particle_instances.slice(..));
                    pass.set_index_buffer(
                        self.cube_gpu.index_buf.slice(..),
                        wgpu::IndexFormat::Uint32,
                    );
                    pass.draw_indexed(0..self.cube_gpu.index_count, 0, 0..particle_count);
                }
            }
            self.queue.submit(std::iter::once(encoder.finish()));
            output.present();
            Ok(())
        }

        // --- internals ---

        fn step_physics(&mut self) {
            let sim_dt = self.time_scale / 60.0;
            let dt = sim_dt / PHYS_SUBSTEPS as f32;
            for _ in 0..PHYS_SUBSTEPS {
                // Unified awake-set dynamics: every not-at-rest solid — the probe today, arbitrarily
                // many bodies tomorrow — feels the *same* gravity field, then contacts are resolved
                // against the world and against each other. One coherent loop, no per-object scripting
                // (see docs/16; honesty invariant docs/15).
                let accel = self
                    .field
                    .acceleration_at(self.sphere.pos, GRAVITY_SOFTENING);
                self.sphere.integrate(accel, dt);
                self.sphere.collide(&self.world, accel, dt);
                // Debris steps under the same field and must not deposit inside the body...
                self.matter.step(
                    &mut self.world,
                    &self.field,
                    std::slice::from_ref(&self.sphere),
                    dt,
                );
                // ...and debris↔body contacts exchange momentum both ways (then re-settle the body
                // against the world so an impact can't shove it into terrain).
                self.matter.couple_body(&mut self.sphere, dt);
                self.sphere.collide(&self.world, accel, dt);
            }
        }

        /// Terrain surface height (centered coords) directly under the sphere; far below if it has
        /// drifted off the world footprint.
        fn ground_under_sphere(&self) -> f32 {
            let c = self.world.center();
            let xi = (self.sphere.pos.x + c.x).floor() as i32;
            let zi = (self.sphere.pos.z + c.z).floor() as i32;
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
        const INST_ATTRS: [wgpu::VertexAttribute; 2] =
            wgpu::vertex_attr_array![4 => Float32x3, 5 => Float32x3];
        let buffers = [
            wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &CUBE_ATTRS,
            },
            wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<InstanceRaw>() as u64,
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
    }

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
            for _ in 0..ORBIT_SUBSTEPS {
                crate::orbit::verlet_step(&mut self.bodies, &mut self.acc, dt);
                // Earth (index 1) vs each moon (index 2..): solid bodies collide at their surfaces —
                // they don't tunnel through each other as point masses into a 1/r² singularity. Each
                // moon's impact energy is counted once (the two-moon scene sums both).
                let (head, tail) = self.bodies.split_at_mut(2);
                let earth = &mut head[1];
                for (k, moon) in tail.iter_mut().enumerate() {
                    // Measure the impact energy BEFORE the (energy-removing) contact resolution — honest
                    // damage reporting (docs/17), even though fragmentation isn't modelled yet.
                    let dissipated = crate::orbit::inelastic_dissipation(earth, moon);
                    if crate::orbit::resolve_contact(earth, moon, contact) {
                        if !self.moon_hit[k] {
                            self.moon_hit[k] = true;
                            self.impact_energy_j += dissipated;
                        }
                        self.impacted = true;
                    }
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
                for uni in &self.moon_unis {
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
        assert_eq!(mats.len(), 19, "seed database should have 19 materials");
        for id in ["granite", "dirt", "grass", "iron"] {
            let i = materials::index_of(&mats, id);
            assert!(mats[i].density > 0.0, "{id} must have positive density");
        }
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
