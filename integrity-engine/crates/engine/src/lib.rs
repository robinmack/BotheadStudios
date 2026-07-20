//! Integrity engine core.
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

mod accretion;
mod aggregate;
mod atmosphere;
mod axle; // docs/47 §3 — the revolute joint: holds a wheel's hub, frees ONE spin axis
mod bhtree;
mod body;
mod damage;
mod emission;
mod eos;
mod gpu_layout; // docs/47 — GPU repr(C) layouts, pinned to the shader by test
mod granular;
mod grid; // docs/47 §1 — the hierarchical spatial hash: no global cell size
#[cfg(target_arch = "wasm32")] // WebGPU host for sph_step.wgsl; only the browser scene uses it (mod app is
mod gpu_sph; //                    wasm-only). The native SPH reference lives in tools/sph-verify + impact-run.
mod gravity;
mod hydrostatic;
mod impact;
mod planet;
mod tides;
#[cfg(test)]
mod isotropy;
mod materials;
mod matter;
mod mesher;
mod neighbors;
mod orbit;
mod terra; // docs/43 — worlds-as-data: the world schema (+ later raster/mesh/camera). The wasm `Terra` scene
           // struct lives in `mod app` below to reuse its render helpers.
mod texture;
mod world;

#[cfg(target_arch = "wasm32")]
pub use app::{Engine, OrbitDemo};

/// World metres spanned by ONE screen pixel at the focal plane (distance `dist_m` from the eye),
/// for a perspective camera with vertical field of view `fov_y` (radians) rendered into a viewport
/// `viewport_h` pixels tall. Pure frustum geometry: the visible slice at `dist_m` is
/// `2·dist_m·tan(fov_y/2)` metres tall, spread over `viewport_h` pixels. Both the terrain scene
/// (world units already metres) and the space scene (convert display units → metres first) feed the
/// HUD scale bar through this one function, so "scale" means the same thing on every screen.
pub(crate) fn metres_per_pixel_at(dist_m: f64, fov_y: f64, viewport_h: f64) -> f64 {
    if viewport_h <= 0.0 {
        return 0.0;
    }
    2.0 * dist_m * (fov_y * 0.5).tan() / viewport_h
}

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
    /// How far the real Earth-surface cap extends (m). It curves down to a horizon at a finite distance
    /// (√(2·R·h) ≈ 16 km for the default ~20 m eye height), well inside this radius and the render far
    /// plane, so the horizon you see is the planet's true curvature — not a cap edge, not infinity.
    const EARTH_CAP_RADIUS: f32 = 26_000.0;
    /// Render far plane (m) — pushed out from 6 km so the curved cap's horizon is in view. The distant
    /// cap is smooth, so the mild depth imprecision far out is acceptable; the near patch is fine.
    const CAMERA_FAR: f32 = 30_000.0;
    const CAMERA_NEAR: f32 = 0.5;
    // SPACE-BAND scene resolution — DECOUPLED from impact.rs's test-facing DEBRIS_N/CAP_N so the on-screen
    // disk can run at the high N the fluid disk actually needs (the grid + Barnes–Hut of docs/30 made this
    // affordable) WITHOUT dragging the native test suite up to high N. The scene's time-LOD keeps it
    // interactive if a step gets heavy (observable time dilates rather than the frame stalling). Trade
    // on-screen disk richness ↔ browser step-rate by bumping these; keep CAP:DEBRIS ≈ 2:1 (docs/28 item 4).
    const SCENE_DEBRIS_N: usize = 512;
    const SCENE_CAP_N: usize = 1024;
    const SCENE_IMPACT_N: usize = SCENE_DEBRIS_N + SCENE_CAP_N;

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
    /// Air temperature (K) for the surface band's density. ISA sea level; the isothermal assumption is
    /// the same one `scale_height` and the settling-column emergence test make (docs/26).
    const AIR_TEMP_K: f64 = 288.0;
    /// Drag coefficient for a voxel grain — a cube, tumbling. DECLARED shape factor (docs/46 §1); the
    /// resolved computation it stands in for is the pressure field of `AirField` parcels flowing around
    /// the grain, so it is deletable when that flow is resolved. ~1.05 is the standard cube value.
    const DRAG_CD_CUBE: f32 = 1.05;

    /// Per-substep position-projection cap for a BODY resolving against the terrain constraint. Mirrors
    /// `particle_step.wgsl::MAX_SURFACE_CORRECTION` (0.01 m) — the bound that makes the projection
    /// stack-safe and stops it doing work, which is what fixed the grains' settling storm
    /// (JOURNAL 2026-07-19). A body's bonds are stiffer than a grain's contacts, so this bound matters
    /// more here, not less: an unbounded snap is exactly what used to pump the probe apart.
    const PROBE_MAX_SURFACE_CORRECTION: f64 = 0.01;
    /// μ used when the column under a contact has no material (empty column / off the voxel footprint).
    /// Basalt's coefficient — this world's actual crust (docs/28), the same representative choice
    /// `gpu_step_params` makes for debris, so a body off the patch grips like the ground it is drawn on.
    const PROBE_GROUND_MU_FALLBACK: f64 = 0.7;
    // Specific heat (J/(kg·K)) for the grain's temp↔u conversion (u = c·T). Generic rock default, matching
    // aggregate/hydrostatic; per-material c is a flagged refinement (like the global contact params). docs/38.
    const GRAIN_SPECIFIC_HEAT: f32 = 1000.0;

    // How often the GPU debris is de-resolved back into voxels (docs/22): a grain that has come to REST
    // on the terrain returns to the voxel grid, matter-conserving, so the debris count falls to ~0 once
    // the excitement passes (no more "rubble hovering forever"). The readback STALLS the pipeline, so we
    // amortise it — every N frames, not per frame. ~4×/s at 60 fps is imperceptible next to the ~30 s
    // settle window and keeps the sky clearing smoothly.
    const SETTLE_READBACK_INTERVAL: u64 = 15;
    // A grounded grain whose vertical velocity is only snap-contact jitter still sits a hair BELOW the
    // heightfield surface under the penalty spring; count it grounded if its base is within this margin
    // of the terrain top (the shader's own bilinear surface uses the same −0.5 mesh offset).
    const SETTLE_GROUND_MARGIN: f32 = 0.1;
    // Consecutive GROUNDED substeps (the shader's `resting` counter) after which a grain deposits even if
    // it is still creeping above SETTLE_SPEED — the GPU port of the CPU `matter::step` SETTLE_FRAMES=10
    // fallback. cs_integrate runs once per substep (~960/s at ×1), so ~150 substeps ≈ 0.16 s of grounded
    // contact, matching the CPU's 10 frames at 60 fps. Without this, soft-contact grains creep forever.
    const SETTLE_REST_SUBSTEPS: f32 = 150.0;

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

    /// Sky-pass uniforms — the per-pixel view ray (inverse view-projection), the sun direction (the
    /// SAME light the terrain is lit by), and the declared atmosphere's Rayleigh optical depth + sun
    /// gain. Everything the honest sky needs; nothing hand-painted. Matches `sky.wgsl`'s `SkyU`.
    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct SkyUniforms {
        inv_view_proj: [[f32; 4]; 4],
        sun_dir: [f32; 4], // xyz = direction to the sun (world), normalized
        tau: [f32; 4],     // xyz = Rayleigh optical depth per band, w = sun gain
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
        /// The SEA: the flat top of the filled `water` matter at the waterline, a SEPARATE open surface
        /// (kept out of `world_gpu` so the solid land stays a watertight manifold). Same world pass /
        /// uniforms / textures; the shader gives the `water` material its Fresnel-sky water shading.
        sea_gpu: GpuMesh,
        world_uni: UniformSlot,
        /// The REAL bulk-Earth surface: a curved spherical cap at Earth's true radius (mesher::build_earth_cap)
        /// that curves down to a finite horizon — NOT a flat decorative plane. Reuses `world_uni` (same
        /// identity model / view_proj / light / material textures); it is just another mesh in the world pass.
        earth_cap_gpu: GpuMesh,
        /// Has a region been RESOLVED into voxels (an impact/dig)? When `false` (the DEFAULT, fresh scene)
        /// the whole terrain is the BULK heightmap — only the full-disk `earth_cap_gpu` renders, so there
        /// is no finite voxel block, nothing special about any square, nothing to see under (Robin:
        /// "dissolve the fixed cube"). When an impact resolves the patch this flips true: the voxel patch
        /// (`world_gpu`, with its crater) renders and the cap is rebuilt WITH a hole over the resolved
        /// region so the crater bowl shows instead of a cap lid. De-resolving back to bulk is increment 3.
        patch_resolved: bool,

        /// The honest sky: a fullscreen Rayleigh single-scatter pass (sky.wgsl) drawn behind the world.
        /// `atm_tau` is the declared atmosphere's optical depth (same as the space band's blue marble).
        sky_pipeline: wgpu::RenderPipeline,
        sky_uni: UniformSlot,
        atm_tau: [f64; 3],

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
        /// Frame counter driving the periodic GPU-debris de-resolution (`settle_gpu_debris`): the
        /// readback stalls the pipeline, so we amortise it over `SETTLE_READBACK_INTERVAL` frames.
        frame: u64,
        /// Diagnostic cache (docs/28 ejecta blanket): the GPU-debris cloud's horizontal span and vertical
        /// reach (m), refreshed from each readback in `settle_gpu_debris`. The watch rig reads these to
        /// tell a LOCAL blanket from a footprint storm. 0 until the first readback lands.
        debris_span_m: f32,
        debris_height_m: f32,

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
            // The sea: a separate open surface (the flat top of the filled water matter) drawn in the
            // same world pass. Kept out of the world mesh so the land stays watertight.
            let sea_mesh = mesher::build_sea(&world, &mats);
            let sea_gpu = upload_mesh(&device, "sea", &sea_mesh);
            // World centre (world-centered frame) — the cap samples the SHARED terrain_height about it, so
            // it joins the resolved patch as ONE continuous rolling surface (no flat-cap-above-valley step).
            let world_center = world.center();
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

            // The REAL bulk-Earth surface as a curved spherical CAP at Earth's true radius (NOT a flat
            // decorative plane): it curves DOWN to a real horizon a few km out under the Rayleigh sky.
            // Same body the space band draws; the resolved voxel patch sits flush at the top of the cap.
            // DEFAULT = BULK EVERYWHERE (no hole): the fresh scene is one continuous smooth bulk surface,
            // no finite voxel block. The cap gains a hole only when an impact resolves the patch (below).
            let earth_cap_mesh = mesher::build_earth_cap(
                &mats,
                world_center,
                planet_radius as f32,
                EARTH_CAP_RADIUS,
                None,
                Some(&world),
            );
            let earth_cap_gpu = upload_mesh(&device, "earth-cap", &earth_cap_mesh);

            let mut probe = build_probe(&mats, spawn, surface_g as f64);
            let probe_acc = probe.accelerations();
            let probe_instances = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("probe-instances"),
                size: (probe.particles.len() * 8 * std::mem::size_of::<GpuParticle>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            log::info!(
                "Integrity engine: planet mass = {:.3e} kg (r = {:.0} km), emergent surface g = {:.2} m/s^2",
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
            // Per-material shine params: [roughness, metallic, is_liquid, _] (padded to 32 for the
            // shader). `is_liquid` (from the DB phase, not a hardcoded index) tells the world shader to
            // render that material as water — Fresnel reflection of the sky over a dark absorbing body,
            // the honest reason calm water reads blue (docs/28).
            let mut params: Vec<[f32; 4]> = vec![[0.0; 4]; 32];
            for (i, m) in mats.iter().enumerate().take(32) {
                let is_liquid = if m.phase == "liquid" { 1.0 } else { 0.0 };
                params[i] = [m.roughness, m.metallic, is_liquid, 0.0];
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

            // --- The honest sky: a fullscreen Rayleigh single-scatter pass ---
            // The terrain is a patch of the SAME declared Earth as the space band, so its sky is derived
            // from the SAME atmosphere: τ from the emergent surface pressure (planet::earth), the SAME
            // λ⁻⁴ molecular scattering that gives the blue marble its veil (docs/26). No painted blue.
            let atm_tau =
                crate::atmosphere::rayleigh_tau(crate::planet::earth().surface_pressure() / 101_325.0);
            let sky_bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("sky-bind-layout"),
                entries: &[uniform_entry(
                    0,
                    wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                )],
            });
            let sky_ubuf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("sky-uniforms"),
                size: std::mem::size_of::<SkyUniforms>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let sky_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("sky-bind-group"),
                layout: &sky_bind_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: sky_ubuf.as_entire_binding(),
                }],
            });
            let sky_uni = UniformSlot {
                bind: sky_bind,
                buf: sky_ubuf,
            };
            let sky_pipeline = build_sky_pipeline(&device, &sky_bind_layout, config.format);

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
                sea_gpu,
                world_uni,
                earth_cap_gpu,
                patch_resolved: false,
                sky_pipeline,
                sky_uni,
                atm_tau,
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
                frame: 0,
                debris_span_m: 0.0,
                debris_height_m: 0.0,
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

        /// Diagnostic (docs/28, the crater-scaled ejecta fix): the horizontal SPREAD of terrain debris —
        /// the larger of the x- and z-extents of the live grain cloud (m). This is the number that says
        /// whether a meteor made a LOCAL ejecta blanket (a few crater radii — tens of m) or the old
        /// footprint-spanning storm (~the whole ~96 m patch, grains flung km-scale). Returns 0 with no
        /// debris. `terrain_debris_height_m` is the companion vertical reach (does the ejecta fill the sky).
        pub fn terrain_debris_spread_m(&self) -> f64 {
            self.debris_span_m as f64
        }

        /// Diagnostic companion to [`Self::terrain_debris_spread_m`]: the vertical reach of the debris
        /// cloud (highest grain minus lowest, m) — a compact blanket stays low, a debris storm fills the sky.
        pub fn terrain_debris_height_m(&self) -> f64 {
            self.debris_height_m as f64
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
                self.resolve_patch(); // resolve voxels + open the cap hole so the dig is visible
                let power = if blast { BLAST_POWER } else { DIG_POWER };
                self.matter
                    .dig(&mut self.world, &self.mats, hit, DIG_RADIUS, power);
                // Anything the dig undercut or isolated now collapses and falls.
                self.matter.collapse(&mut self.world, &self.mats, self.surface_g);
                self.flush_debris_to_gpu();
            }
        }

        /// Fire a **meteor** at a screen point: a high-energy `impact` that carves a crater and throws
        /// incandescent ejecta — the centre melts and glows, the rim is cold rubble (`docs/20`). Same
        /// operator as a bullet or a moon, just more energy.
        /// When the meteor's screen ray misses the voxel patch, pick a real strike ON the patch: a spot
        /// driven by the live camera yaw (so repeated strikes vary and walk around — "anywhere on the
        /// globe") arriving OBLIQUELY at ~45° (so the shared furrow shows its downrange elongation, not a
        /// dead-centre bowl). Returns (site in centered coords, incoming unit direction).
        fn fallback_meteor_aim(&self) -> (Vec3, Vec3) {
            let ang = self.camera.yaw * 1.7; // walks as the scene turns
            let half = (world::W.min(world::D) as f32) * 0.30; // stay well inside the patch
            let (sx, sz) = (ang.cos() * half, ang.sin() * half);
            let sy = self.ground_under(sx, sz); // surface height at that spot (centered coords)
            let az = self.camera.yaw * 2.3; // downrange azimuth, also varied
            let dir = Vec3::new(az.cos() * 0.7071, -0.7071, az.sin() * 0.7071).normalize_or_zero();
            (Vec3::new(sx, sy, sz), dir)
        }

        pub fn meteor(&mut self, ndc_x: f32, ndc_y: f32) {
            self.resolve_patch(); // resolve voxels + open the cap hole so the crater is visible
            let (view_proj, eye) = self.view_proj();
            let inv = view_proj.inverse();
            let near = inv.project_point3(Vec3::new(ndc_x, ndc_y, 0.0));
            let far = inv.project_point3(Vec3::new(ndc_x, ndc_y, 1.0));
            let ray_dir = (far - near).normalize_or_zero();
            // Aim where the screen ray strikes the terrain; if it misses (the button/​'m' key aim at
            // screen-centre, which from the horizon-facing surface camera flies over the patch), FALL
            // BACK to a varied OBLIQUE strike ON the patch — so a meteor always lands a real, angled hit
            // ("a meteor could strike anywhere on the globe"). Same shared physics either way.
            let (hit, dir) = self
                .world
                .raycast(eye, ray_dir, 6000.0)
                .map(|(_, _, _, h)| (h, ray_dir))
                .unwrap_or_else(|| self.fallback_meteor_aim());
            {
                // The meteor is a real Fe-Ni body: its impact energy is its kinetic energy, ½·m·v².
                let energy = 0.5 * METEOR_MASS * METEOR_SPEED * METEOR_SPEED;

                // SHARED IMPACT EXCAVATION (docs/28): the terrain meteor runs the SAME furrow/shock
                // excavation the space-band Theia strike does — `impact::Furrow`. At the impact point the
                // voxel summary is REPLACED with materialized particles (`matter::materialize_furrow`) and
                // the ONE canonical law shapes them: an OBLIQUE strike (the camera ray sets the track)
                // carves a downrange-elongated furrow, a vertical one a symmetric bowl, and every grain
                // carries the furrow's declared shock-ejection velocity. "Improving one improves all":
                // this is the identical `Furrow` the space band fills — a meteor could strike anywhere on
                // the globe and we would see the same physics. Matter is conserved (excavated voxels →
                // grains, each with its own strata material). No scripted dig/blast/ejecta speed.
                //   1. size the disturbed region from the σ·V crater relation (docs/19), LOD-capped;
                //   2. build the shared furrow from the meteor's track + local surface up;
                //   3. materialize its voxels into grains carrying the shared shock-ejection velocity;
                //   4. the meteor's KE not lofted as bulk motion is shock HEAT (radial gradient → glow).
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
                // The meteor's REAL radius is the Housen–Holsapple scaling length a (a 1000 kg Fe-Ni body
                // is ~0.31 m): a = (3m / 4πρ)^⅓ from the struck iron's own density — not a dial.
                let rho_fe =
                    self.mats[materials::index_of(&self.mats, "iron")].density.max(1.0) as f64;
                let a = (3.0 * METEOR_MASS as f64
                    / (4.0 * std::f64::consts::PI * rho_fe))
                    .cbrt();
                // The shared furrow: site = the strike point, outward normal = local up (+y under uniform
                // surface gravity), track = the meteor's velocity (camera ray × impact speed), excavation
                // scale = the crater radius. The SAME struct the space band builds in `impact.rs`.
                let v_impact = glam::DVec3::new(dir.x as f64, dir.y as f64, dir.z as f64)
                    * METEOR_SPEED as f64;
                let site64 = glam::DVec3::new(hit.x as f64, hit.y as f64, hit.z as f64);
                // The gravity-regime ejecta velocity scale K·√(g·R_crater) (docs/28) needs the terrain's
                // surface gravity: for the terrain meteor (R_crater ≈ mat_r, g ≈ 9.88) this gives ~12 m/s
                // (capped ≈ 18.6 m/s), a LOCAL ejecta blanket ~2–3 crater radii wide instead of the km/s
                // impactor-contact scale (C·v_i ≈ 10 km/s) the old ejection law used.
                let furrow = crate::impact::Furrow::new(
                    site64,
                    glam::DVec3::Y,
                    v_impact,
                    a,
                    mat_r as f64,
                    self.surface_g as f64,
                );
                let start = self.matter.particle_count();
                self.matter.materialize_furrow(
                    &mut self.world,
                    &self.mats,
                    &furrow,
                    Vec3::ZERO,
                    METEOR_MASS as f64, // impactor mass → the exact energy-conservation cap (docs/28)
                );
                // Path B (docs/24): turn any STEEP terrain the ejecta will hit (crater walls, cliffs) into
                // grains too — a heightfield can't represent a vertical wall conservatively. Now the
                // terrain the debris touches is either grains or a gentle bilinear surface — both conservative.
                self.matter
                    .materialize_steep_terrain(&mut self.world, &self.mats, hit, mat_r * 2.0);
                // Heat = impact energy minus the bulk KE the shared ejection lofted (most of a fast
                // impactor's ½mv² is heat — the incandescent core glows because it genuinely is that hot).
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
                self.matter.collapse(&mut self.world, &self.mats, self.surface_g);
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

        /// RESOLVE the terrain patch into voxels on demand (Robin: "creating texels as needed around the
        /// impact site"). Called at the start of a dig/meteor: flips the scene from BULK (only the full-disk
        /// cap draws) to RESOLVED (the voxel patch draws, and the cap is rebuilt WITH a hole over the
        /// resolved region so the crater bowl shows instead of a cap lid). Idempotent — a no-op once
        /// resolved. FLAGGED (increment 2, deferred): the resolved region is the WHOLE 96 m footprint, not
        /// yet a LOCAL rect sized from the impact's predicted crater radius — that local sizing + a matching
        /// local cap hole is the next refinement; de-resolving back to bulk after settling is increment 3.
        fn resolve_patch(&mut self) {
            if self.patch_resolved {
                return;
            }
            self.patch_resolved = true;
            self.rebuild_earth_cap();
        }

        /// Rebuild the Earth cap for the current resolution state: a FULL bulk disk when nothing is
        /// resolved, or a disk with a HOLE over the resolved footprint so the resolved voxels render there.
        fn rebuild_earth_cap(&mut self) {
            let center = self.world.center();
            let hole = if self.patch_resolved {
                Some(glam::Vec2::new(center.x, center.z)) // hole over the whole resolved footprint
            } else {
                None // bulk everywhere
            };
            let mesh = mesher::build_earth_cap(
                &self.mats,
                center,
                self.planet_radius as f32,
                EARTH_CAP_RADIUS,
                hole,
                Some(&self.world),
            );
            self.earth_cap_gpu = upload_mesh(&self.device, "earth-cap", &mesh);
        }

        fn remesh_world(&mut self) {
            let mesh = mesher::build_surface_nets(&self.world, &self.mats);
            self.world_gpu = upload_mesh(&self.device, "world", &mesh);
            // The sea rides the same matter store — rebuild it too (a dig/impact that lowers the seabed
            // or a displaced-water level change re-surfaces the sea consistently with the land).
            let sea = mesher::build_sea(&self.world, &self.mats);
            self.sea_gpu = upload_mesh(&self.device, "sea", &sea);
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
                    u: GRAIN_SPECIFIC_HEAT * p.temp_k, // specific internal energy (matches hydrostatic.rs u=c·T)
                    vel: [p.vel.x, p.vel.y, p.vel.z],
                    resting: 0.0,
                    color: self.mats[p.material].albedo,
                    material: p.material as f32,
                    emission: emission::incandescence(p.temp_k),
                    rho: self.mats[p.material].density,
                    // docs/47 §1: size travels WITH the grain. Uniform today (every debris grain is
                    // the same 1 m ejecta scale); the hierarchical grid is what makes mixed sizes correct.
                    radius: CONTACT_RADIUS,
                    _p0: 0.0,
                    _p1: 0.0,
                    _p2: 0.0, // ρ₀ at spawn (placeholder until 4b.2 computes it)
                })
                .collect();
            self.gpu_particles.append(&self.queue, &gpu);
            self.matter.particles.clear();
        }

        /// **De-resolution of GPU debris** (`docs/22`, the on-demand-resolution principle in reverse):
        /// read the live GPU grains back to the CPU and, for every grain that has come to REST on the
        /// terrain, deposit it back into the voxel grid — matter-conserving (one grain → one voxel), then
        /// re-upload the survivors so `gpu_particles.count` drops. Without this the GPU debris path never
        /// returns matter to the grid, so the debris count pinned at thousands forever ("rubble hovers").
        ///
        /// The GPU shader can't write CPU voxels, so this is a non-blocking readback (two-phase, see
        /// `begin_readback`/`take_readback`): a copy is kicked off every `SETTLE_READBACK_INTERVAL`
        /// frames and consumed a frame or two later when the async map completes. "At rest" is the SAME
        /// horizontal-only test the CPU `matter::step` uses (a grounded grain's residual vertical velocity
        /// is snap-contact jitter, not motion). Deposit goes through the SHARED
        /// [`matter::MatterSim::deposit_resting_grain`] primitive, so a grain never lands inside the probe
        /// or a full column — there it stays a grain. Matter is CONSERVED: a grain is removed from the GPU
        /// set ONLY when it became a voxel.
        fn settle_gpu_debris(&mut self) {
            // Phase 1: on the interval, kick off a fresh readback (no-op if one is already in flight).
            if self.frame % SETTLE_READBACK_INTERVAL == 0 {
                self.gpu_particles.begin_readback(&self.device, &self.queue);
            }
            // Phase 2: consume a completed readback, if any.
            let Some(grains) = self.gpu_particles.take_readback() else {
                return;
            };
            // Diagnostic (docs/28 ejecta blanket): cache the live debris cloud's horizontal span and
            // vertical reach so the watch rig can measure LOCAL blanket vs footprint storm.
            if !grains.is_empty() {
                let (mut xmin, mut xmax, mut ymin, mut ymax, mut zmin, mut zmax) =
                    (f32::MAX, f32::MIN, f32::MAX, f32::MIN, f32::MAX, f32::MIN);
                for g in &grains {
                    xmin = xmin.min(g.offset[0]);
                    xmax = xmax.max(g.offset[0]);
                    ymin = ymin.min(g.offset[1]);
                    ymax = ymax.max(g.offset[1]);
                    zmin = zmin.min(g.offset[2]);
                    zmax = zmax.max(g.offset[2]);
                }
                self.debris_span_m = (xmax - xmin).max(zmax - zmin);
                self.debris_height_m = ymax - ymin;
            }
            // If the live buffer was appended to since the snapshot (a new meteor mid-flight), the
            // snapshot indices no longer align with the buffer — do NOT compact against it (that would
            // drop the freshly-appended grains). Discard this snapshot; the next interval retries.
            if self.gpu_particles.count != self.gpu_particles.readback_count {
                return;
            }
            let center = self.world.center();
            // The probe is dynamic matter debris must pile ON, never through — model it as its bounding
            // sphere (COM + collision radius), the same body test `matter::step` applies.
            let com = self.probe.com();
            let probe_body = crate::body::Sphere::new(
                Vec3::new(com.x as f32, com.y as f32, com.z as f32),
                self.probe.total_mass() as f32,
                SPHERE_RADIUS,
            );
            let bodies = [probe_body];

            let (wmax, dmax) = (self.world.w as f32 - 0.5, self.world.d as f32 - 0.5);
            let mut survivors: Vec<GpuParticle> = Vec::with_capacity(grains.len());
            for g in &grains {
                let pos = Vec3::new(g.offset[0], g.offset[1], g.offset[2]);
                let (vx, vz) = (g.vel[0], g.vel[2]);
                let horiz = (vx * vx + vz * vz).sqrt();
                // Which column did the GPU rest this grain on? The shader's `terrain_top` CLAMPS the
                // sample to the patch edge (the terrain extends flat past the border — the bulk Earth the
                // patch is a window onto), so a grain that overflew the finite patch rests on the
                // edge-clamped extension. Deposit it into that SAME column — clamped to the patch edge —
                // so matter that came to rest is returned to the resolved grid, matter-conserving. The
                // clamp is the identical convention `terrain_top` and `materialize_steep_terrain` use.
                // FLAGGED LOD APPROXIMATION: for this over-energetic meteor most ejecta overfly the 96 m
                // patch and land on the unresolved extension; returning them at the patch boundary (not
                // their true, unresolvable landing point 10s–100s of m away) is an approximation — but it
                // CONSERVES matter (never deletes a grain to lower the count) and matches where the GPU
                // physically rested each grain. A larger patch / true bulk-mass accounting is the deeper fix.
                let cxf = (pos.x + center.x).clamp(0.5, wmax);
                let czf = (pos.z + center.z).clamp(0.5, dmax);
                // Grounded against the SAME bilinear surface the GPU step rests grains on (not a single
                // column top). On a slope a grain is binned into the lower column of its cell but is
                // physically held up by the higher corner; testing only the lower column top judged it
                // airborne and it never de-resolved — so the whole pile stacked on it stalled as rubble
                // that never returned to the grid (observed: the debris count plateaus at thousands
                // instead of falling to ~0). The bilinear surface mirrors `particle_step.wgsl::terrain_h`.
                let surf_y = self.world.surface_height_bilinear(pos);
                let grounded = surf_y.is_finite()
                    && pos.y - DEBRIS_PART_HALF <= surf_y + SETTLE_GROUND_MARGIN;
                // "At rest" — the SAME dual criterion as the CPU `matter::step`: horizontal speed below
                // SETTLE_SPEED, OR grounded for enough consecutive substeps (the shader's `resting`
                // counter = the CPU SETTLE_FRAMES fallback) so a grain that has settled onto a pile but
                // still creeps under soft contact still deposits instead of hovering forever.
                let at_rest = horiz < matter::SETTLE_SPEED || g.resting > SETTLE_REST_SUBSTEPS;
                // Deposit into the (clamped) column the grain rests on — pass the clamped x,z.
                let dpos = Vec3::new(cxf - center.x, pos.y, czf - center.z);
                if grounded
                    && at_rest
                    && self.matter.deposit_resting_grain(
                        &mut self.world,
                        dpos,
                        g.material as usize,
                        &bodies,
                    )
                {
                    continue; // became a voxel — drop it from the GPU set
                }
                survivors.push(*g);
            }

            if survivors.len() != grains.len() {
                self.gpu_particles.replace(&self.queue, &survivors);
            }
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
                        u: GRAIN_SPECIFIC_HEAT * t, // render-only instances (renderer reads offset/color/emission)
                        vel: [0.0, 0.0, 0.0],
                        resting: 0.0,
                        color: albedo,
                        material: mat,
                        emission,
                        rho: 0.0,
                        // docs/47 §1: size travels WITH the grain. Uniform today (every debris grain is
                        // the same 1 m ejecta scale); the hierarchical grid is what makes mixed sizes correct.
                        radius: CONTACT_RADIUS,
                        _p0: 0.0,
                        _p1: 0.0,
                        _p2: 0.0, // render-only: unread by the renderer
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
                    // THE ground query (world.rs), not the raw voxels: a column demoted to T0 has no
                    // voxels but still has ground, and uploading -1 for it would drop every grain
                    // resting there straight through the floor. Mirrors `surface_bilinear_grad`.
                    tops.push(
                        self.world
                            .ground_top_voxel(x as i32, z as i32)
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
                // AIR: density derived from the planet's own declared atmosphere mass (docs/48). One
                // value for the patch — the barometric profile varies 1.1% over 96 m, so resolving it
                // here buys nothing (docs/44). `matter::DRAG` is gone: it was a velocity multiply.
                air_rho: crate::atmosphere::air_density_at(
                    crate::planet::earth().surface_pressure(),
                    &self.mats[materials::index_of(&self.mats, "air")],
                    AIR_TEMP_K,
                    self.surface_g as f64,
                    0.0,
                ) as f32,
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
                specific_heat: GRAIN_SPECIFIC_HEAT,
                drag_cd: DRAG_CD_CUBE,
                _hp1: 0.0,
                _hp2: 0.0,
            }
        }

        /// Render one frame (advances the simulation first).
        pub fn render(&mut self) -> Result<(), JsValue> {
            self.step_physics();
            // De-resolve settled GPU debris back into voxels (docs/22): grains at rest on the terrain
            // become voxels again, so the debris count falls to ~0 once the excitement passes and the
            // crater/terrain fills honestly with settled matter. Non-blocking, amortised (a readback is
            // issued on the interval and consumed a frame or two later). Runs BEFORE the dirty check so
            // the deposits it makes remesh + re-upload the heightfield this same frame.
            self.frame = self.frame.wrapping_add(1);
            self.settle_gpu_debris();
            if self.matter.take_dirty() {
                self.remesh_world();
                self.upload_heightfield_to_gpu(); // the crater changed the column tops
            }

            let (view_proj, eye) = self.view_proj();
            let light = Vec3::new(0.45, 0.9, 0.4).normalize();
            self.write_uniform(&self.world_uni, view_proj, Mat4::IDENTITY, eye, light);
            // The sky reads the SAME sun direction the terrain is lit by, and the declared atmosphere's
            // optical depth, so the graded sky and the lit ground are one consistent illumination.
            self.write_sky_uniform(view_proj, eye, light);
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
                // Advance the FULL DEBRIS_SUBSTEPS the `dt` above is sized for. (Regression fix: this loop
                // was dispatching only MOON_DEBRIS_SUBSTEPS=4 while `dt` still divided the frame by 16, so
                // terrain debris advanced a quarter of a frame per render — grains fell in slow motion and
                // hung in the air, never reaching rest. The Moon scene keeps its own substep count in its
                // own step at debris_frame_dt / MOON_DEBRIS_SUBSTEPS; the two must not be crossed.)
                for _ in 0..DEBRIS_SUBSTEPS {
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
                            // Cleared to black only as an initializer — EVERY visible pixel is then
                            // painted by the fullscreen Rayleigh sky pass below. No flat-blue lie: the
                            // blue is derived from the declared atmosphere's scattering (sky.wgsl).
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
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
                // The honest sky first: a fullscreen Rayleigh single-scatter backdrop (no depth), so the
                // world geometry paints over it wherever it is nearer. Derived from the declared air.
                pass.set_pipeline(&self.sky_pipeline);
                pass.set_bind_group(0, &self.sky_uni.bind, &[]);
                pass.draw(0..3, 0..1);

                pass.set_pipeline(&self.pipeline);
                // The BULK Earth surface first (shares world_uni): the full-disk heightmap curving down to
                // a true horizon. In the DEFAULT scene this is the WHOLE terrain — one continuous smooth
                // surface, no finite voxel block. The resolved voxel patch (with its crater) draws on top
                // ONLY once an impact has resolved it; until then it is not drawn at all, so nothing is
                // special about any square and there is nothing to see under (Robin: "dissolve the cube").
                draw(&mut pass, &self.world_uni, &self.earth_cap_gpu);
                if self.patch_resolved {
                    draw(&mut pass, &self.world_uni, &self.world_gpu);
                }
                // The sea on top of the land it fills (same uniforms; the shader gives `water` its
                // Fresnel-sky water shading). Drawn after the land so its open surface reads as water
                // pooling in the basins.
                draw(&mut pass, &self.world_uni, &self.sea_gpu);

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
                // TRACTION (docs/23): resolve against the SAME non-injecting constraint the GPU grains
                // use — `granular::terrain_contact_resolve`, already unit-tested and hardware-verified
                // (gpu-verify scenes K/L/N). This REPLACES the previous `p.vel.x *= 0.5` tangential
                // multiply, which was a raw velocity scale: it did not depend on the normal load, on μ, or
                // on the surface at all, so it could not express traction. Coulomb friction bounded by
                // `μ·jn` is exactly "a harder-pressed contact grips more", which is what makes a driven
                // body accelerate instead of skate.
                //
                // The old dead-zone/half-correction hack is gone with it: `terrain_contact_resolve`'s
                // position projection is velocity-decoupled and bounded by MAX_SURFACE_CORRECTION, which
                // is the mechanism that fixed the settling storm for grains (JOURNAL 2026-07-19) — it adds
                // no KE, so it cannot pump the probe's stiff bonds the way a hard snap did.
                // Surface + gradient. Inside a RESOLVED patch use the voxel iso-surface — the SAME
                // bilinear field the GPU grains collide against, so the probe and the debris around it
                // agree on where the ground is. Outside the footprint (or before any dig) the BULK field
                // continues, exactly as before: `surface_bilinear_grad` edge-clamps, which would wrongly
                // extend the patch rim out over the bulk, so the in-bounds test is load-bearing.
                let in_patch = xi >= 0
                    && zi >= 0
                    && (xi as usize) < self.world.w
                    && (zi as usize) < self.world.d;
                let (h, dhdx, dhdz) = if self.patch_resolved && in_patch {
                    let (h, gx, gz) = self.world.surface_bilinear_grad(p.pos.as_vec3());
                    (h as f64, gx as f64, gz as f64)
                } else {
                    // Bulk band: analytic height, gradient by central difference on the same field.
                    const E: f32 = 0.5; // half a voxel — the finest the bulk field is sampled at
                    let (px, pz) = (p.pos.x as f32, p.pos.z as f32);
                    let hb = |x: f32, z: f32| self.world.bulk_height(x, z);
                    (
                        hb(px, pz) as f64,
                        ((hb(px + E, pz) - hb(px - E, pz)) / (2.0 * E)) as f64,
                        ((hb(px, pz + E) - hb(px, pz - E)) / (2.0 * E)) as f64,
                    )
                };
                // μ is the TERRAIN's own coefficient under this contact — ice is slippery because ice's
                // material datum says so, not because a scene flag says so. FLAGGED APPROXIMATION: this
                // uses the surface material's μ alone; a proper pair-combining rule between the body's
                // material and the ground's does not exist yet (the same gap `gpu_step_params` flags for
                // mixed-material debris).
                let mu = self
                    .world
                    .surface_top_voxel(xi, zi)
                    .and_then(|t| self.world.material_at(xi, t, zi))
                    .map(|m| self.mats[m].friction_coefficient as f64)
                    .unwrap_or(PROBE_GROUND_MU_FALLBACK);
                let hit = crate::granular::terrain_contact_resolve(
                    p.pos,
                    p.vel,
                    h,
                    dhdx,
                    dhdz,
                    half,
                    mu,
                    PROBE_MAX_SURFACE_CORRECTION,
                    f64::INFINITY, // open sky: nothing rests on the probe
                );
                if hit.hit {
                    p.vel = hit.vel;
                    p.pos += hit.dpos;
                }
            }
        }

        /// Terrain surface height (centered coords) under a column. DEFAULT = the BULK heightmap everywhere
        /// (`World::bulk_height`, defined on the whole plane, resolved or not). Only where the patch has
        /// been RESOLVED into voxels (a crater) does it read the resolved column top instead — so the
        /// crater is felt where it exists, and the bulk terrain is felt everywhere else (including off the
        /// finite footprint, where the old code returned a void far below and things fell forever).
        fn ground_under(&self, x: f32, z: f32) -> f32 {
            if self.patch_resolved {
                let c = self.world.center();
                let (xi, zi) = ((x + c.x).floor() as i32, (z + c.z).floor() as i32);
                if let Some(t) = self.world.surface_top_voxel(xi, zi) {
                    return t as f32 - c.y;
                }
            }
            self.world.bulk_height(x, z)
        }

        /// The camera eye and look target in world space, derived from live camera state. Shared by
        /// `view_proj` (the render matrix) and `meters_per_pixel` (the HUD scale bar) so the reported
        /// scale is byte-for-byte the geometry that was rendered.
        fn eye_and_target(&self) -> (Vec3, Vec3) {
            let cp = self.camera.pitch.cos();
            let dir = Vec3::new(
                cp * self.camera.yaw.sin(),
                self.camera.pitch.sin(),
                cp * self.camera.yaw.cos(),
            );
            let eye = dir * (self.camera.base_distance * self.camera.zoom);
            // Camera collision (Robin: "the camera should never penetrate matter"): clamp the eye above
            // the REAL Earth sphere EVERYWHERE (not just over the 96 m patch — the old off-footprint "free"
            // case was only safe behind the fake plane) AND out of the resolved voxel hills. The Earth
            // centre is a full radius straight down under the uniform surface gravity.
            const CAMERA_CLEARANCE: f32 = 2.0; // metres of air kept between the eye and the surface
            let r = self.planet_radius as f32;
            let surf_y = self.ground_under(0.0, 0.0); // patch-centre surface height (centered coords)
            let earth_center = Vec3::new(0.0, surf_y - r, 0.0);
            let eye = self
                .world
                .clamp_eye_above_earth(eye, earth_center, r, CAMERA_CLEARANCE);
            // Aim near eye height (not down at the world centre) so the gaze is nearly horizontal: the
            // ground plane recedes to a HORIZON with sky above — a surface-of-a-planet view. A little
            // below eye height keeps a gentle downward tilt so the terrain patch stays in frame.
            let target = Vec3::new(0.0, eye.y * 0.82, 0.0);
            (eye, target)
        }

        fn view_proj(&self) -> (Mat4, Vec3) {
            let aspect = self.config.width as f32 / self.config.height.max(1) as f32;
            // Far plane pushed out (CAMERA_FAR) so the curved Earth cap's horizon is visible.
            let proj = Mat4::perspective_rh(0.9, aspect, CAMERA_NEAR, CAMERA_FAR);
            let (eye, target) = self.eye_and_target();
            let view = Mat4::look_at_rh(eye, target, Vec3::Y);
            (proj * view, eye)
        }

        /// World metres spanned by one screen pixel at the look target (the focal plane). An honest,
        /// live read of the same camera geometry the frame was rendered with — feeds the HUD scale
        /// bar. In the terrain scene world units ARE metres, so no unit conversion is needed.
        pub fn meters_per_pixel(&self) -> f64 {
            let (eye, target) = self.eye_and_target();
            let dist = (eye - target).length() as f64;
            crate::metres_per_pixel_at(dist, 0.9, self.config.height.max(1) as f64)
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

        /// Upload the sky pass's per-frame uniforms: the inverse view-projection (to reconstruct each
        /// pixel's world view ray), the sun direction, and the declared atmosphere's Rayleigh optical
        /// depth + sun gain. `sun_gain` = 22.0 is the SAME display exposure the space band uses for the
        /// blue marble, so the terrain sky and the orbital atmosphere are one consistent scattering.
        fn write_sky_uniform(&self, view_proj: Mat4, eye: Vec3, sun: Vec3) {
            const SUN_GAIN: f32 = 22.0;
            let inv = view_proj.inverse();
            let u = SkyUniforms {
                inv_view_proj: inv.to_cols_array_2d(),
                sun_dir: [sun.x, sun.y, sun.z, 0.0],
                tau: [
                    self.atm_tau[0] as f32,
                    self.atm_tau[1] as f32,
                    self.atm_tau[2] as f32,
                    SUN_GAIN,
                ],
                camera_pos: [eye.x, eye.y, eye.z, 1.0],
            };
            self.queue
                .write_buffer(&self.sky_uni.buf, 0, bytemuck::bytes_of(&u));
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

    // ============================================================================================
    // GPU-compute debris particles (docs/22). Particles live in a storage buffer stepped by a compute
    // shader (one thread each) and rendered from the SAME buffer (zero-copy sim↔render). This is the
    // engine's north-star architecture and the fix for the single-digit FPS after a big impact.
    // ============================================================================================

    use crate::gpu_layout::{GpuParticle, GpuStepParams};

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
        // Non-blocking readback (docs/22 de-resolution). On WebGPU buffer mapping is genuinely async —
        // we cannot block (`Maintain::Wait` is a no-op in the browser), so the readback is two-phase:
        // `begin_readback` copies the grains into `readback_staging` and calls `map_async`, whose
        // callback flips `readback_ready`; a later frame `take_readback` reads the mapped bytes.
        // `readback_count` snapshots `count` at copy time so `take_readback` can detect an intervening
        // append (a fresh meteor) and discard the now-misaligned snapshot rather than deposit stale data.
        readback_staging: Option<wgpu::Buffer>,
        readback_count: u32,
        readback_ready: std::rc::Rc<std::cell::Cell<bool>>,
    }

    impl GpuParticles {
        fn new(device: &wgpu::Device, capacity: u32, world_cells: u32) -> Self {
            let buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("gpu-particles-physics"),
                size: (capacity as usize * std::mem::size_of::<GpuParticle>()) as u64,
                // COPY_SRC so the de-resolution readback (docs/22) can copy the live grains to a MAP_READ
                // staging buffer — without it copy_buffer_to_buffer is a (silent, async) WebGPU validation
                // error and the readback reads all zeros.
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_DST
                    | wgpu::BufferUsages::COPY_SRC,
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
                readback_staging: None,
                readback_count: 0,
                readback_ready: std::rc::Rc::new(std::cell::Cell::new(false)),
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

        /// Phase 1 of the non-blocking de-resolution readback (`docs/22`): copy the live PHYSICS grains
        /// into a transient `MAP_READ` staging buffer and kick off `map_async`. Its callback flips the
        /// shared `readback_ready` flag when the GPU has finished (on WebGPU that lands during the JS
        /// event loop between frames — we can NOT block for it, unlike native `tools/gpu-verify`). A
        /// no-op if the buffer is empty or a readback is already in flight.
        fn begin_readback(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
            if self.count == 0 || self.readback_staging.is_some() {
                return;
            }
            let size = self.count as u64 * std::mem::size_of::<GpuParticle>() as u64;
            let staging = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("gpu-particles-readback"),
                size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let mut enc =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            enc.copy_buffer_to_buffer(&self.buf, 0, &staging, 0, size);
            queue.submit(std::iter::once(enc.finish()));
            self.readback_ready.set(false);
            let flag = self.readback_ready.clone();
            staging.slice(..).map_async(wgpu::MapMode::Read, move |res| {
                if res.is_ok() {
                    flag.set(true);
                }
            });
            self.readback_count = self.count;
            self.readback_staging = Some(staging);
        }

        /// Phase 2: if the in-flight readback has completed, return the snapshotted grains (a `Vec` of the
        /// `readback_count` grains as they were at copy time) and clear the in-flight state. Returns
        /// `None` while still pending or when nothing is in flight. If the live buffer was appended to
        /// since the copy (a new meteor), the snapshot no longer aligns with the buffer, so the caller
        /// must NOT compact against it — `take_readback` reports the snapshot count via `readback_count`
        /// so the caller can detect the mismatch.
        fn take_readback(&mut self) -> Option<Vec<GpuParticle>> {
            if !self.readback_ready.get() {
                return None;
            }
            let staging = self.readback_staging.take()?;
            let data = staging.slice(..).get_mapped_range();
            let out = bytemuck::cast_slice::<u8, GpuParticle>(&data).to_vec();
            drop(data);
            staging.unmap();
            self.readback_ready.set(false);
            Some(out)
        }

        /// Replace the buffer contents with `survivors` and set `count` to their number — the compaction
        /// half of de-resolution. Grains that settled back into voxels (CPU-side) are simply not in
        /// `survivors`, so `count` drops; the tail past the new count is left as-is (never stepped/drawn
        /// because every pass bounds itself by `count`). Matter is NOT destroyed here — the caller has
        /// already turned each removed grain into a voxel; this only shrinks the live GPU set.
        fn replace(&mut self, queue: &wgpu::Queue, survivors: &[GpuParticle]) {
            let take = survivors.len().min(self.capacity as usize);
            if take > 0 {
                queue.write_buffer(&self.buf, 0, bytemuck::cast_slice(&survivors[..take]));
            }
            self.count = take as u32;
        }
    }

    /// A compute-only GPU probe for **cross-device** verification (JOURNAL 2026-07-19).
    ///
    /// WHY. Two blind spots meet here. (1) `Engine::create` acquires its adapter with
    /// `request_adapter(HighPerformance)` and never reports what it got, so a browser run is silent
    /// about which GPU produced it — the same ambiguity `pick_adapter` fixes natively in
    /// `tools/gpu-verify`. (2) `GpuParticles::dispatch` splits its four stages into four separate
    /// compute passes precisely because fusing them "happened to work on desktop Vulkan (the 2070) but
    /// can RACE on other backends (e.g. Metal / the M4)" — and that mitigation has never been exercised
    /// ON Metal. This probe answers both on any device with a browser: which adapter, how fast, and
    /// whether energy stays bounded (a race injects energy).
    ///
    /// It drives the REAL `GpuParticles`, hence the real `shaders/particle_step.wgsl` — not a
    /// reimplementation — so a result here is a statement about shipping code. Compute only: no canvas,
    /// no surface. Material properties are read from the material DB, not invented (see `probe_params`).
    ///
    /// ASYNC SHAPE. Browser buffer mapping cannot block (`Maintain::Wait` is a no-op there), so this
    /// uses the same two-phase pattern as `begin_readback`/`take_readback`: `start_run` records and
    /// submits, returning immediately; JS polls `poll()` until it flips true, then reads
    /// `result_json()`. JS brackets that with `performance.now()`. Run enough frames that the poll
    /// granularity is a small fraction of the total — a single frame is not measurable this way.
    #[wasm_bindgen]
    pub struct GpuProbe {
        device: wgpu::Device,
        queue: wgpu::Queue,
        info: wgpu::AdapterInfo,
        max_buffer_size: u64,
        max_wg_per_dim: u32,
        parts: Option<GpuParticles>,
        snapshot: Vec<GpuParticle>,
        n: u32,
        frames: u32,
        gravity: f32,
    }

    #[wasm_bindgen]
    impl GpuProbe {
        /// Acquire a compute-only device. `compatible_surface: None` — nothing is drawn.
        pub async fn create() -> Result<GpuProbe, JsValue> {
            console_error_panic_hook::set_once();
            let _ = console_log::init_with_level(log::Level::Info);
            let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
                backends: wgpu::Backends::BROWSER_WEBGPU,
                ..Default::default()
            });
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    force_fallback_adapter: false,
                    compatible_surface: None,
                })
                .await
                .ok_or_else(|| JsValue::from_str("no GPU adapter (is WebGPU enabled?)"))?;
            let info = adapter.get_info();
            let limits = adapter.limits();
            let (device, queue) = adapter
                .request_device(
                    &wgpu::DeviceDescriptor {
                        label: Some("gpu-probe"),
                        required_features: wgpu::Features::empty(),
                        required_limits: limits.clone(),
                        ..Default::default()
                    },
                    None,
                )
                .await
                .map_err(|e| JsValue::from_str(&format!("request_device failed: {e}")))?;
            Ok(GpuProbe {
                device,
                queue,
                info,
                max_buffer_size: limits.max_buffer_size,
                max_wg_per_dim: limits.max_compute_workgroups_per_dimension,
                parts: None,
                snapshot: Vec::new(),
                n: 0,
                frames: 0,
                gravity: 9.81,
            })
        }

        /// Adapter provenance. On iPadOS this is what proves the backend is Metal; everywhere it stops a
        /// result from being ambiguous about the hardware that produced it.
        pub fn gpu_adapter_json(&self) -> String {
            let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
            format!(
                "{{\"name\":\"{}\",\"backend\":\"{:?}\",\"device_type\":\"{:?}\",\"driver\":\"{}\",\"driver_info\":\"{}\",\"vendor\":{},\"device\":{},\"max_buffer_size\":{},\"max_workgroups_per_dim\":{}}}",
                esc(&self.info.name),
                self.info.backend,
                self.info.device_type,
                esc(&self.info.driver),
                esc(&self.info.driver_info),
                self.info.vendor,
                self.info.device,
                self.max_buffer_size,
                self.max_wg_per_dim,
            )
        }

        /// Phase 1: seed `n` grains and submit `frames × DEBRIS_SUBSTEPS` substeps, then start a
        /// readback that fences the whole batch. Returns as soon as the work is queued.
        pub fn start_run(&mut self, n: u32, frames: u32) {
            let n = n.clamp(1, MAX_PARTICLES as u32);
            self.n = n;
            self.frames = frames.max(1);
            self.snapshot.clear();

            let mut parts = GpuParticles::new(&self.device, n, PROBE_W * PROBE_W);
            // Flat floor at voxel 0 — the probe measures the granular step, not terrain shape.
            parts.upload_heightfield(&self.queue, &vec![0i32; (PROBE_W * PROBE_W) as usize]);

            // ρ₀ from the REAL material (basalt), matching `probe_params` and the spawn path — the
            // grain carries density as Tillotson input (docs/38), so it must not be invented.
            let rho0 = {
                let mats = materials::load();
                mats[materials::index_of(&mats, "basalt")].density
            };

            // A cube of grains on the 1 m lattice, jittered for the same reason gpu-verify jitters: a
            // perfect lattice is metastable and will not flow, so an unjittered pile is not a
            // representative contact workload.
            let side = (n as f64).cbrt().ceil() as u32;
            let mut grains = Vec::with_capacity(n as usize);
            for i in 0..n {
                let (x, y, z) = (i % side, (i / side) % side, i / (side * side));
                let j = |salt: u32| {
                    let h = (i.wrapping_add(salt).wrapping_mul(2654435761)) ^ 0x9e37_79b9;
                    (((h >> 8) & 0xffff) as f32 / 32768.0 - 1.0) * 0.1
                };
                grains.push(GpuParticle {
                    offset: [x as f32 + j(1), 8.0 + y as f32 + j(2), z as f32 + j(3)],
                    // docs/38: the grain's thermodynamic state is specific internal energy, not
                    // temperature — temp = u/c is derived. 300 K ambient, same as the spawn path.
                    u: GRAIN_SPECIFIC_HEAT * 300.0,
                    vel: [0.0; 3],
                    resting: 0.0,
                    color: [0.5, 0.5, 0.5],
                    material: 0.0,
                    emission: [0.0; 3],
                    rho: rho0,
                    // docs/47 §1: size travels WITH the grain. Uniform today (every debris grain is
                    // the same 1 m ejecta scale); the hierarchical grid is what makes mixed sizes correct.
                    radius: CONTACT_RADIUS,
                    _p0: 0.0,
                    _p1: 0.0,
                    _p2: 0.0, // ρ₀ at spawn, from the real material (docs/38 4b.2 will compute it)
                });
            }
            parts.append(&self.queue, &grains);
            parts.set_params(&self.queue, &self.probe_params());

            // ONE encoder for every substep of every frame — mirrors `Engine::step_physics`, which
            // records all DEBRIS_SUBSTEPS into one encoder and submits once. Timing a
            // submit-per-substep shape would measure driver launch overhead instead of the shader
            // (JOURNAL 2026-07-19: that mistake made a 2.5× hardware gap look like 17%).
            let mut enc = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("gpu-probe") });
            for _ in 0..self.frames {
                for _ in 0..DEBRIS_SUBSTEPS {
                    parts.dispatch(&mut enc);
                }
            }
            self.queue.submit(std::iter::once(enc.finish()));
            // Fences the batch: the map callback cannot fire until the GPU has drained the queue.
            parts.begin_readback(&self.device, &self.queue);
            self.parts = Some(parts);
        }

        /// Phase 2: true once the GPU has finished and the grains are read back. Poll from JS.
        pub fn poll(&mut self) -> bool {
            let Some(parts) = self.parts.as_mut() else {
                return false;
            };
            match parts.take_readback() {
                Some(snap) => {
                    self.snapshot = snap;
                    true
                }
                None => false,
            }
        }

        /// Energy + motion summary of the settled grains. `"null"` before the first completed run.
        ///
        /// UNIT GRAIN MASS: the shader carries no per-grain mass, so these are per-unit-mass figures.
        /// That is deliberate — the check is the INVARIANT (`tot` must never rise between runs of
        /// increasing `frames`), not an absolute energy claim. A backend race shows up here as rising
        /// total energy, which is exactly how gpu-verify's scene I detects fudges natively.
        pub fn result_json(&self) -> String {
            if self.snapshot.is_empty() {
                return String::from("null");
            }
            let (mut ke, mut pe, mut vmax) = (0.0f64, 0.0f64, 0.0f64);
            for p in &self.snapshot {
                let v2 = (p.vel[0] * p.vel[0] + p.vel[1] * p.vel[1] + p.vel[2] * p.vel[2]) as f64;
                ke += 0.5 * v2;
                pe += (self.gravity * p.offset[1]) as f64;
                vmax = vmax.max(v2.sqrt());
            }
            format!(
                "{{\"n\":{},\"frames\":{},\"substeps\":{},\"grains\":{},\"ke\":{:.6e},\"pe\":{:.6e},\"tot\":{:.6e},\"vmax\":{:.4}}}",
                self.n,
                self.frames,
                DEBRIS_SUBSTEPS,
                self.snapshot.len(),
                ke,
                pe,
                ke + pe,
                vmax,
            )
        }
    }

    /// Probe world footprint in cells. Only needs to comfortably contain the seeded cube (the largest,
    /// at MAX_PARTICLES, is ~40 cells on a side).
    const PROBE_W: u32 = 256;

    impl GpuProbe {
        /// Step params for the probe. Friction, restitution-derived normal damping and cohesion are read
        /// from REAL basalt in the material DB, mirroring `Engine::gpu_step_params` — a probe that
        /// invented these would be exercising a shader configuration the engine never actually runs, and
        /// its timings would not transfer. (docs/24; same representative-material approximation, flagged
        /// there.)
        fn probe_params(&self) -> GpuStepParams {
            let mats = materials::load();
            let bulk = &mats[materials::index_of(&mats, "basalt")];
            let normal_damp = crate::granular::damping_for_restitution(
                bulk.restitution as f64,
                CONTACT_STIFFNESS as f64,
            ) as f32;
            let grain_area = std::f32::consts::PI * CONTACT_RADIUS * CONTACT_RADIUS;
            const GRANULAR_COHESION_CEIL: f32 = 5.0e4; // Pa — loose-debris adhesion ceiling (docs/24)
            let c_cohesion =
                bulk.cohesion.min(GRANULAR_COHESION_CEIL) * grain_area / bulk.density.max(1.0);
            GpuStepParams {
                gravity: [0.0, -self.gravity, 0.0],
                dt: (1.0 / 60.0) / DEBRIS_SUBSTEPS as f32,
                center: [0.0, 0.0, 0.0], // grains are already in voxel coords ⇒ ground sits at y = 0
                c_cohesion,
                // AIR: density derived from the planet's own declared atmosphere mass (docs/48). One
                // value for the patch — the barometric profile varies 1.1% over 96 m, so resolving it
                // here buys nothing (docs/44). `matter::DRAG` is gone: it was a velocity multiply.
                // Same air the engine runs in — the probe exercises SHIPPING code, so it must not
                // measure a shader configuration the engine never uses. `mats` and `self.gravity` are
                // this fn's own; the Engine's `self.mats`/`self.surface_g` do not exist on GpuProbe.
                air_rho: crate::atmosphere::air_density_at(
                    crate::planet::earth().surface_pressure(),
                    &mats[materials::index_of(&mats, "air")],
                    AIR_TEMP_K,
                    self.gravity as f64,
                    0.0,
                ) as f32,
                contact_damp: matter::CONTACT_DAMP,
                settle_speed: 0.0,
                part_half: DEBRIS_PART_HALF,
                cool_rate: 0.4,
                count: self.n,
                world_w: PROBE_W,
                world_d: PROBE_W,
                cell_size: 2.0 * CONTACT_RADIUS,
                table_mask: GRID_TABLE_SIZE - 1,
                bucket_k: GRID_BUCKET_K,
                c_radius: CONTACT_RADIUS,
                c_stiffness: CONTACT_STIFFNESS,
                c_normal_damp: normal_damp,
                c_friction: bulk.friction_coefficient,
                c_tangent_damp: CONTACT_TANGENT_DAMP,
                // docs/38: the grain carries u = c·T, so the shader needs c to derive temperature.
                // Same constant the production path passes (`gpu_step_params`) — the probe must not
                // run a different thermodynamic conversion than the engine it is measuring.
                specific_heat: GRAIN_SPECIFIC_HEAT,
                drag_cd: DRAG_CD_CUBE,
                _hp1: 0.0,
                _hp2: 0.0,
            }
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
        // Damping DERIVED from iron's own coefficient of restitution, not chosen: ζ = −ln(e)/√(π²+ln²e)
        // is the standard spring-dashpot inversion of e = exp(−ζπ/√(1−ζ²)), and it is the SAME
        // `zeta_for_restitution` the granular contact law uses — so a bond and a grain contact agree on
        // what "iron is this bouncy" means. `critically_damped` then supplies the units: √(k·m) with the
        // coordination correction that fixed the old detonation bug (√(k·m) alone over-damped each
        // particle ~√(bonds)× past critical — note the danger there was OVER-damping, docs/23).
        //
        // This REPLACES a hardcoded ζ = 0.4, picked so the ball "settles rigidly" — a behavioural target,
        // and one that implied a restitution of e ≈ 0.254, i.e. it modelled iron as ~2.4× less bouncy
        // than the material data says it is. The honest ζ is LOWER (iron e = 0.6 ⇒ ζ ≈ 0.16).
        //
        // MEASURED, not assumed (rig `probe_traction.mjs`): this change costs essentially NO extra settle
        // time — 35.0 s → 35.5 s, inside the rig's 0.5 s sampling interval. The first rebound is actually
        // LOWER (4.5 m → 3.9 m): a springier lattice stores impact energy in internal modes instead of
        // returning it as one coherent bounce. The long settle comes almost entirely from removing the
        // TANGENTIAL FUDGE in `collide_probe_with_terrain` (6.5 s → 35.0 s at unchanged ζ), which was
        // crushing the bounce flat. So ring-down is not a damping dial to tune away — bounding its
        // compute cost belongs to the demotion criterion (docs/44), not here.
        probe.damping = probe
            .critically_damped(crate::granular::zeta_for_restitution(mats[iron].restitution as f64));
        // Surface gravity is the field of the WHOLE planet below (matter all the way down), ~uniform over
        // this small patch — passed in, computed from planet::earth(), not a hardcoded constant.
        probe.with_gravity(glam::DVec3::new(0.0, -surface_g, 0.0))
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
    /// Visual scale for the GPU SPH impact particles (docs/33 stage 5): the sub-Earth proto-bodies (~5000 km)
    /// are much smaller than the Earth–Moon frame, so the particle field is drawn at an enlarged scale (Earth's
    /// ~5000 km radius → a few display units) and the camera zooms in — a scene-framing choice, the physics is
    /// unchanged (positions stay Earth-relative metres; only this render multiplier differs from DISPLAY_SCALE).
    const SPH_VIS_SCALE: f64 = 7.0e-7;
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

    /// Setup phase of the GPU SPH impact (docs/35): relax the two bodies on the GPU (placed far apart so each
    /// settles under its own gravity), read them back, assemble the collision, then step the dynamics.
    #[derive(Clone, Copy)]
    enum SphPhase {
        Relaxing(u32), // GPU `cs_relax` steps completed so far
        Assembling,    // relax done; awaiting the async read-back to compute the collision geometry
        Dynamics,      // colliding — KDK substeps + read-back
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
        // --- GPU SPH deformable-Earth impact in the browser (docs/33 stage 4c.4) ---
        /// The GPU SPH particle system (built + relaxed on the CPU at `start_gpu_impact`, then stepped on the
        /// GPU each frame via the verified `sph_step.wgsl` kernels). `None` until triggered.
        gpu_sph: Option<crate::gpu_sph::GpuSph>,
        sph_pipeline: wgpu::RenderPipeline, // instanced billboard particles (sph_render.wgsl)
        sph_cam: UniformSlot,               // view-proj + Earth display origin + scale for the particle shader
        sph_active: bool,
        sph_dt: f32, // fixed integration timestep (chosen at build; WebGPU forbids the adaptive read-back)
        sph_soft: f64, // gravitational softening (for the energy diagnostic's PE term)
        /// docs/42 browser-parity — SCHEDULED shock-dt: WebGPU forbids the per-step adaptive read-back, so the
        /// dt is stepped by SIM TIME instead — the small shock dt (`sph_dt`) resolves the collision, then after
        /// `SPH_SHOCK_WINDOW_S` we switch to the larger `sph_dt_aftermath` for the slow disk evolution (restores
        /// playback). `sph_sim_t` is the physical time integrated since the collision started.
        sph_sim_t: f64,
        sph_dt_aftermath: f32,
        /// docs/42 — ADAPTIVE GPU load: substeps (relax steps) encoded per frame, scaled to a wall-clock frame
        /// budget so the sim never monopolizes the GPU / freezes the tab or OS. Grows when there's headroom,
        /// shrinks (down to 1) when frames run long. The direct-sum O(N²) step is heavy, so this self-limits.
        sph_substeps: u32,
        /// Latest async read-back of the GPU SPH particles (one frame behind) — for the HUD/disk-stats and
        /// (later) the momentum mirror. Empty until the first read-back completes.
        sph_snapshot: Vec<crate::gpu_sph::SphParticle>,
        /// The GPU impact's setup/step phase (relax on GPU → assemble collision → dynamics). See `advance`.
        sph_phase: SphPhase,
        /// docs/42 render-layer blend: 0 = the PRETTY render (sphere/atmosphere), 1 = the raw PHYSICS particles.
        /// Cross-fades by size (grains × (1−blend), billboards × blend), so no alpha-sort. Only meaningful while
        /// `sph_active`. Default 0 (pretty first — the slider reveals the physics).
        render_blend: f64,
        /// docs/42 Phase 2: the giant-impact crater on the pretty sphere. The impact site (an EARTH-RELATIVE
        /// unit direction, captured from the GPU field at first Theia contact) and how open the bowl is (0→1,
        /// grows as the shock excavates). `None` until contact. Persists after (bake-back — Robin's call).
        gpu_impact_site: Option<glam::DVec3>,
        gpu_crater_frac: f64,
    }

    // Moon-shot Stage A constants.
    // scene impact resolution uses SCENE_DEBRIS_N/SCENE_CAP_N (module consts), not the test-facing const.
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
            let debris_unis: Vec<UniformSlot> = (0..SCENE_IMPACT_N)
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
            // GPU SPH deformable-Earth impact (stage 4c.4): its instanced-particle pipeline + a camera
            // uniform (reuses the uniform-only `bind_layout`; the buffer is sized for `SphCam`).
            let sph_pipeline = build_sph_pipeline(&device, &bind_layout, config.format);
            let sph_cam = {
                let buf = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("sph-cam"),
                    size: std::mem::size_of::<crate::gpu_sph::SphCam>() as u64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("sph-cam-bind"),
                    layout: &bind_layout,
                    entries: &[wgpu::BindGroupEntry { binding: 0, resource: buf.as_entire_binding() }],
                });
                UniformSlot { buf, bind }
            };

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
                gpu_sph: None,
                sph_pipeline,
                sph_cam,
                sph_active: false,
                sph_dt: 0.0,
                sph_soft: 1.0,
                sph_sim_t: 0.0,
                sph_dt_aftermath: 0.0,
                sph_substeps: 6,
                sph_snapshot: Vec::new(),
                sph_phase: SphPhase::Dynamics,
                render_blend: 0.0, // pretty by default (docs/42)
                gpu_impact_site: None,
                gpu_crater_frac: 0.0,
            })
        }

        /// docs/42: set the pretty⇄physics render blend (0 = pretty sphere, 1 = raw physics particles).
        pub fn set_render_blend(&mut self, blend: f32) {
            self.render_blend = (blend as f64).clamp(0.0, 1.0);
        }

        /// docs/43 — load a "system" world (Sun/Earth/Moon initial conditions) from JSON, replacing the built-in
        /// constants with declared DATA. `create(canvas, num_moons)` must have been called with the world's moon
        /// count first (the GPU per-moon uniforms are sized there); this sets the physical initial conditions
        /// (positions/velocities/masses), the planet's spin, the composition-derived tints, the time scale, the
        /// frame-of-reference focus, and the orbit-camera framing. The deorbit stays a user control
        /// (`brake_moon`/`drop_moon`) — no scripted outcome. (The planet's render radius still uses the
        /// `EARTH_RADIUS_M` constant in v1; per-body render radii from data is a flagged follow-up.)
        pub fn load_world(&mut self, world_json: &str) -> Result<(), JsValue> {
            use crate::terra::world_def::{BodyDef, World};
            let w = World::parse(world_json).map_err(|e| JsValue::from_str(&e))?;
            let defs = w
                .bodies
                .as_ref()
                .ok_or_else(|| JsValue::from_str("system world is missing a `bodies` array"))?;

            // Mass/radius resolve from an explicit field or a named profile (declared, not fudged). The Sun's mass
            // EMERGES from its composition (`planet::sun`), like the current hardcoded path.
            let body_mass = |d: &BodyDef| -> f64 {
                d.mass_kg.unwrap_or_else(|| match d.profile.as_deref() {
                    Some("sun") => crate::planet::sun().total_mass(),
                    Some("earth") => EARTH_MASS,
                    Some("moon") => MOON_MASS,
                    _ => 0.0,
                })
            };
            let body_radius = |d: &BodyDef| -> f64 {
                d.radius_m.unwrap_or_else(|| match d.profile.as_deref() {
                    Some("earth") => EARTH_RADIUS_M,
                    Some("moon") => MOON_RADIUS_M,
                    _ => 0.0,
                })
            };

            let mut bodies = Vec::with_capacity(defs.len());
            let mut planet_idx = 1usize;
            let mut moon_count = 0usize;
            for (i, d) in defs.iter().enumerate() {
                bodies.push(crate::orbit::Body {
                    pos: glam::DVec3::from_array(d.pos_m),
                    vel: glam::DVec3::from_array(d.vel_ms),
                    mass: body_mass(d),
                });
                // Tint: explicit override, else aggregated from the profile's real composition (docs/17) — the
                // borrow of `self.mats` is confined to this block, released before we mutate the tint fields.
                let tint = |profile: Option<&str>, mats: &[materials::Material]| -> [f32; 4] {
                    if let Some(t) = d.tint {
                        return [t[0], t[1], t[2], 1.0];
                    }
                    let comp: Vec<(usize, f32)> = match profile {
                        Some("earth") => vec![
                            (materials::index_of(mats, "water"), 0.71),
                            (materials::index_of(mats, "granite"), 0.24),
                            (materials::index_of(mats, "ice"), 0.05),
                        ],
                        Some("moon") => vec![(materials::index_of(mats, "basalt"), 1.0)],
                        _ => vec![(materials::index_of(mats, "granite"), 1.0)],
                    };
                    let a = materials::aggregate_albedo(&comp, mats);
                    [a[0], a[1], a[2], 1.0]
                };
                match d.role.as_str() {
                    "planet" => {
                        planet_idx = i;
                        self.earth_tint = tint(d.profile.as_deref(), &self.mats);
                        if let Some(p) = d.spin_period_s {
                            self.spin_l = glam::DVec3::new(0.0, 0.0, 1.0)
                                * (crate::tides::moment_of_inertia(body_mass(d), body_radius(d))
                                    * (2.0 * std::f64::consts::PI / p));
                            self.initial_spin_l = self.spin_l;
                        }
                    }
                    "moon" => {
                        moon_count += 1;
                        self.moon_tint = tint(d.profile.as_deref(), &self.mats);
                        self.impactor_radius = body_radius(d);
                        self.impactor_mass = body_mass(d);
                    }
                    _ => {}
                }
            }
            // `moon_unis` is a fixed render pool (drawn per moon body); guard only that we don't exceed it.
            if moon_count > self.moon_unis.len() {
                return Err(JsValue::from_str(&format!(
                    "world declares {moon_count} moon(s), exceeding the render pool of {}",
                    self.moon_unis.len()
                )));
            }

            self.bodies = bodies;
            self.acc = crate::orbit::accelerations(&self.bodies);
            self.initial_bodies = self.bodies.clone();
            // Per-moon impact-hit flags sized to this world (the physics state; `moon_unis` is just the pool).
            self.moon_hit = vec![false; moon_count];

            if let Some(t) = w.time.as_ref() {
                self.time_scale = t.scale.clamp(1.0, 2_000_000.0);
            }

            // Orbit camera: frame-of-reference focus body + framing.
            self.focus = planet_idx;
            if let Some(c) = w.camera.as_ref() {
                if let Some(f) = c.focus.as_deref() {
                    if let Some(idx) = defs.iter().position(|d| d.name == f) {
                        self.focus = idx;
                    }
                }
                if let Some(y) = c.yaw {
                    self.camera.yaw = y as f32;
                }
                if let Some(p) = c.pitch {
                    self.camera.pitch = p as f32;
                }
                if let Some(z) = c.zoom {
                    self.camera.zoom = z as f32;
                }
            }
            // Frame the view on the planet→moon separation (fall back to the current base distance).
            if let Some(moon) = self.bodies.get(planet_idx + 1) {
                let sep = (moon.pos - self.bodies[planet_idx].pos).length();
                if sep > 0.0 {
                    self.camera.base_distance = (sep * DISPLAY_SCALE) as f32 * 1.7;
                }
            }

            log::info!(
                "orbit demo: loaded system world '{}' — {} bodies, {moon_count} moon(s), {:.0}x time",
                w.name,
                self.bodies.len(),
                self.time_scale,
            );
            Ok(())
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
            // GPU-path hand-off (docs/35 stage 5, 2c): if the GPU SPH impact is running, promote its orbiting
            // disk's bound clumps to moonlets around the real Earth, retire the GPU sim, and go geologic — the
            // GPU replacement for the Aggregate hand-off below.
            if self.sph_active {
                let moonlets = crate::gpu_sph::disk_moonlets(&self.sph_snapshot, EARTH_RADIUS_M);
                if moonlets.is_empty() {
                    return; // no orbiting disk yet — keep the impact running rather than blanking the scene
                }
                self.geo_moonlets = moonlets;
                self.sph_active = false;
                self.gpu_sph = None;
                self.sph_phase = SphPhase::Dynamics;
                self.camera.zoom = 1.0; // back out from the impact framing to the Earth–Moon geologic view
                self.geologic = true;
                return;
            }
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

        /// Start the GPU deformable-Earth giant impact (docs/33 stage 4c.4): build + relax two differentiated
        /// EOS bodies on the CPU, place them on the oblique giant-impact geometry, and hand the per-frame
        /// dynamics to the GPU SPH stepper (the verified `sph_step.wgsl` kernels — same physics as the offline
        /// `tools/impact-run`). The scene then renders the live particle field instead of the rigid-Earth
        /// debris model. Call from JS on the `OrbitDemo` handle, like `drop_moon()`.
        pub fn start_gpu_impact(&mut self) {
            // Build the two bodies UNRELAXED and FAR APART, and RELAX them on the GPU (`cs_relax`, fast — the
            // measured cure for the dispersal was proper relaxation, docs/35). `advance` runs the relax steps,
            // reads back, assembles the collision, then steps the dynamics. N is higher than the old CPU-relax
            // path could afford (GPU relax + stepping is cheap).
            let eos = [crate::gpu_sph::SphEos::basalt(), crate::gpu_sph::SphEos::iron()];
            let (particles, softening, relax_dt) = crate::gpu_sph::build_far_apart(2400, 400);
            self.sph_soft = softening as f64;
            let cap = particles.len() as u32;
            let mut sph = crate::gpu_sph::GpuSph::new(&self.device, cap);
            sph.upload(&self.queue, &particles, &eos, softening);
            sph.set_dt(&self.queue, relax_dt, 0.94); // damped relaxation toward hydrostatic equilibrium
            sph.set_av(&self.queue, 0.0, 0.0); // no artificial viscosity during relax (matches the CPU relax)
            self.gpu_sph = Some(sph);
            self.sph_dt = relax_dt;
            self.sph_active = true;
            self.sph_snapshot.clear();
            self.sph_phase = SphPhase::Relaxing(0);
            self.gpu_impact_site = None; // no crater until Theia makes contact (docs/42 Phase 2)
            self.gpu_crater_frac = 0.0;
            self.sph_substeps = 6; // start conservative; the frame-budget controller adapts up (docs/42)
            self.focus = 1; // centre on Earth (the particle system sits at the display origin)
            self.camera.zoom = 0.4; // frame the impact (the Earth–Moon default zoom shows it as a speck)
        }

        /// Disk-provenance stats of the live GPU SPH impact (docs/33 stage 5), computed from the latest
        /// read-back: orbiting-disk mass (M☾), its Earth %, remnant radius, escaped mass, and the largest
        /// self-bound clump (Moon candidate). `"null"` before the first read-back. JS reads this for the HUD.
        pub fn gpu_disk_stats_json(&self) -> String {
            if !self.sph_active {
                return String::from("null");
            }
            crate::gpu_sph::disk_stats_json(&self.sph_snapshot)
        }

        /// docs/42 escape-check: the largest proto-Moon clump's orbit about the remnant — distance (km), speed
        /// (km/s), whether it is BOUND (specific orbital energy < 0), and semi-major axis (km). Tracks whether
        /// the accreted Moon is receding / unbinding. `"null"` if there's no clump yet.
        pub fn gpu_moon_track_json(&self) -> String {
            if !self.sph_active {
                return String::from("null");
            }
            match crate::gpu_sph::largest_moonlet_orbit(&self.sph_snapshot) {
                Some((r, v, e, a, mass, ecc, theta)) => format!(
                    "{{\"dist_km\":{:.0},\"v_kms\":{:.3},\"bound\":{},\"a_km\":{},\"ecc\":{:.3},\"theta_deg\":{:.0},\"mass_moon\":{:.3}}}",
                    r / 1e3, v / 1e3, e < 0.0,
                    if a.is_finite() { format!("{:.0}", a / 1e3) } else { "\"unbound\"".to_string() },
                    ecc, theta.to_degrees(), mass / 7.342e22,
                ),
                None => String::from("null"),
            }
        }

        /// Energy diagnostic of the live GPU impact (docs/35): kinetic / internal / gravitational-PE / total
        /// (J), from the latest read-back. A steadily rising total = the integrator is injecting energy (the
        /// remnant then puffs apart instead of orbiting). `"null"` before the first read-back.
        pub fn gpu_energy_json(&self) -> String {
            if !self.sph_active || self.sph_snapshot.is_empty() {
                return String::from("null");
            }
            let (ke, ie, pe) = crate::gpu_sph::total_energy(&self.sph_snapshot, self.sph_soft);
            format!("{{\"ke\":{:.4e},\"ie\":{:.4e},\"pe\":{:.4e},\"tot\":{:.4e}}}", ke, ie, pe, ke + ie + pe)
        }

        /// Advance the PHYSICS by `real_dt` wall-clock seconds. Fixed sim-timestep substeps whose
        /// COUNT (not size) varies with the wall clock — so the physics rate is independent of the
        /// display frame rate (a 30 fps client and a 120 fps client simulate the same world), and the
        /// physics NEVER depends on rendering: the render only samples what this produced, RENDER_LAG_S
        /// later. Under overload the observable clock dilates (we drop backlog) rather than corrupting
        /// the physics with an oversized step — time slows before truth breaks.
        pub fn advance(&mut self, real_dt: f64) {
            let real_dt = real_dt.clamp(0.0, 0.25); // tab-sleep / hiccup guard
            // docs/42 — ADAPTIVE GPU-load control: keep each frame's encoded work inside a wall-clock budget so
            // the sim can never monopolize the GPU and freeze the tab / OS. `real_dt` is the previous frame's
            // total time; a slow frame shrinks the substep count (multiplicative, down to 1), headroom grows it
            // by one (additive, capped). The heavy direct-sum O(N²) step is exactly what this throttles.
            if self.sph_active {
                if real_dt > 0.060 {
                    self.sph_substeps = (self.sph_substeps * 3 / 4).max(1);
                } else if real_dt < 0.028 {
                    self.sph_substeps = (self.sph_substeps + 1).min(24);
                }
            }
            // GPU SPH deformable-Earth impact owns the frame while active (docs/33 stage 4c.4): encode a batch
            // of KDK substeps on the GPU and skip the CPU orbital physics. Fixed dt (WebGPU forbids the
            // adaptive read-back); ~8 substeps/frame plays the ~10 h aftermath out over a few seconds.
            if self.sph_active {
                match self.sph_phase {
                    // RELAX (on the GPU): the two bodies sit far apart and settle under their own gravity via
                    // `cs_relax`. Fast enough to run many steps/frame; on completion, kick off the read-back.
                    SphPhase::Relaxing(steps) => {
                        // Relax steps/frame ride the same adaptive budget (a relax step ≈ half a KDK substep, so
                        // ~2× the count) — bounded so even the relax phase can't stall the device.
                        let chunk: u32 = (2 * self.sph_substeps).clamp(2, 48);
                        const TARGET: u32 = 2400; // AV-free relax is stable at the normal Courant dt ⇒ few steps
                        if let Some(sph) = self.gpu_sph.as_mut() {
                            let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("sph-relax") });
                            sph.encode_relax(&mut enc, chunk);
                            self.queue.submit(std::iter::once(enc.finish()));
                            let done = steps + chunk >= TARGET;
                            if done {
                                sph.begin_readback(&self.device, &self.queue);
                                self.sph_phase = SphPhase::Assembling;
                            } else {
                                self.sph_phase = SphPhase::Relaxing(steps + chunk);
                            }
                        }
                        return;
                    }
                    // ASSEMBLE: once the relaxed bodies are read back, compute the collision geometry from the
                    // ACTUAL relaxed radii, place them on the impact, and switch to the shock-safe dynamics dt.
                    SphPhase::Assembling => {
                        let relaxed = self.gpu_sph.as_mut().and_then(|s| s.take_readback());
                        if let Some(relaxed) = relaxed {
                            let (particles, eos, softening, dt) = crate::gpu_sph::assemble_from_relaxed(&relaxed);
                            self.sph_soft = softening as f64;
                            self.sph_dt = dt; // the SMALL shock dt (resolves the collision)
                            self.sph_dt_aftermath = dt * 5.0; // switch to this once the shock has passed
                            self.sph_sim_t = 0.0;
                            self.sph_snapshot.clear();
                            if let Some(sph) = self.gpu_sph.as_mut() {
                                sph.upload(&self.queue, &particles, &eos, softening);
                                sph.set_dt(&self.queue, dt, 1.0);
                                sph.set_av(&self.queue, 1.0, 2.0); // restore shock-capture AV for the impact
                            }
                            self.sph_phase = SphPhase::Dynamics;
                        }
                        return;
                    }
                    // DYNAMICS: KDK substeps on the GPU + async read-back for the HUD/disk-stats/energy. The dt
                    // is the shock-safe FIXED value from `assemble_from_relaxed` — MEASURED to conserve total
                    // energy to ~0.01 % (KE→IE shock heating), so the well-relaxed bodies form a bound remnant +
                    // disk rather than dispersing (docs/35). An in-kernel per-substep adaptive dt (to trim the
                    // residual escape) is the next refinement.
                    SphPhase::Dynamics => {
                        let substeps = self.sph_substeps; // adaptive (frame-budget controlled) — never a fixed 100
                        const SPH_SHOCK_WINDOW_S: f64 = 5400.0; // ~1.5 h — cover the collision + excavation, then coarsen
                        if let Some(sph) = self.gpu_sph.as_mut() {
                            let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("sph-step") });
                            sph.encode_kdk(&mut enc, substeps);
                            self.queue.submit(std::iter::once(enc.finish()));
                            if let Some(snap) = sph.take_readback() {
                                self.sph_snapshot = snap;
                            }
                            sph.begin_readback(&self.device, &self.queue);
                        }
                        // Scheduled dt (docs/42): once the shock window has passed, coarsen the dt for the slow
                        // disk aftermath (WebGPU can't read back the adaptive Courant dt, so we schedule by time).
                        self.sph_sim_t += substeps as f64 * self.sph_dt as f64;
                        if self.sph_dt < self.sph_dt_aftermath && self.sph_sim_t > SPH_SHOCK_WINDOW_S {
                            self.sph_dt = self.sph_dt_aftermath;
                            if let Some(sph) = self.gpu_sph.as_mut() {
                                sph.set_dt(&self.queue, self.sph_dt, 1.0);
                            }
                        }
                        return;
                    }
                }
            }
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
                    let (_merged, shed) = crate::tides::secular_step(
                        &mut self.geo_moonlets,
                        &mut self.spin_l,
                        self.bodies[1].mass,
                        EARTH_RADIUS_M,
                        crate::tides::EARTH_K2_OVER_Q,
                        step * year_s,
                    );
                    // A moonlet that decayed inside the Roche limit was shredded: its mass rains onto Earth
                    // (angular momentum already added to the spin in secular_step). Mass is conserved.
                    self.bodies[1].mass += shed;
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
                    // Proto-Earth's pre-impact spin (docs/31): its excavated mantle is born co-rotating,
                    // so a fast primordial spin flings Earth material into the disk (the isotopic-crisis
                    // lever). `self.spin_l` is the ANGULAR MOMENTUM; convert to angular velocity ω = L/I
                    // with the solid-sphere I = 2/5 M R² before the cap materialises (the impact then
                    // adds its own spin to Earth on top).
                    let earth_i = 0.4 * self.bodies[1].mass * EARTH_RADIUS_M * EARTH_RADIUS_M;
                    let earth_omega =
                        if earth_i > 0.0 { self.spin_l / earth_i } else { glam::DVec3::ZERO };
                    let (agg, acc0) = crate::impact::build_impact_debris_scaled(
                        &self.mats, site, earth_pos, earth_vel, moon_mass, v_contact,
                        &impactor_profile, &crate::planet::earth(), EARTH_MASS, EARTH_RADIUS_M,
                        SCENE_DEBRIS_N, SCENE_CAP_N, earth_omega,
                    );
                    self.debris_acc = acc0;
                    self.impact_site_rel = Some(site - earth_pos); // crater mask, in Earth's frame
                    // The materialized cap LEFT Earth's bulk: move its mass from the summary body to the
                    // particles (else double-counted). Use the ACTUAL materialized target mass — summing the
                    // SOURCE_TARGET grains — now that the cap is physical ρ·V (docs/28 item 4); the old
                    // moon_mass·CAP_N/DEBRIS_N formula assumed the fudged 2×-impactor cap and over-subtracts
                    // ~6.5×, under-massing Earth on screen.
                    let cap_mass: f64 = agg
                        .particles
                        .iter()
                        .zip(agg.source.iter())
                        .filter(|(_, &s)| s == crate::aggregate::SOURCE_TARGET)
                        .map(|(p, _)| p.mass)
                        .sum();
                    self.bodies[1].mass -= cap_mass;
                    self.moon_debris = Some(agg);
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
                // BLOCK-TIMESTEP advance (docs/30 stage 3): the quiescent orbiting disk coasts at the base
                // dt while the violent shocked/vapor core sub-steps internally — so the high-N debris swarm
                // evolves faster (the win grows with the base dt the time-LOD hands us under load). Verified
                // to reproduce the global-dt disk (impact::birth_impact_with_step_block_reproduces_the_disk)
                // and conserve energy; the per-substep force/heat physics is identical, just scheduled.
                agg.step_block(dt, 0.1);
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
            let frag0 = (self.impactor_mass / SCENE_DEBRIS_N as f64).max(1.0);
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
                let frag0 = (self.impactor_mass / SCENE_DEBRIS_N as f64).max(1.0);
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

            // GPU SPH impact (docs/33 stage 4c.4): push the particle-shader camera uniform. The particle
            // system lives in an Earth-relative f32 frame, so its display origin is Earth's position in the
            // focused frame; the shader maps each Earth-relative position through DISPLAY_SCALE and view_proj.
            if self.sph_active {
                let origin = ((r_bodies[1] - focus) * SPH_VIS_SCALE).as_vec3();
                let cam = crate::gpu_sph::SphCam {
                    view_proj: view_proj.to_cols_array_2d(),
                    origin: [origin.x, origin.y, origin.z, 0.0],
                    // billboard half-size fades with the render blend (docs/42): 0 at the pretty end, full at the
                    // physics end. z/w (Phase 3): matter beyond ~6.5e6 m (just past the sub-scale remnant) is
                    // EJECTA — it keeps a glowing mote size (0.006) even at the pretty end, so the sphere wears a
                    // real ejecta plume.
                    params: [SPH_VIS_SCALE as f32, 0.013 * self.render_blend as f32, 6.5e6, 0.006],
                };
                self.queue.write_buffer(&self.sph_cam.buf, 0, bytemuck::bytes_of(&cam));
            }

            // Light direction = TO the real Sun from each body (per-body; the Sun is the illuminant,
            // not a hardcoded direction). So the lit hemisphere and the phases come from the geometry.
            let earth_light = (sun - r_bodies[1]).as_vec3().normalize();
            // EARTH AS PARTICLES (docs/15): the planet renders as a shell of coarse grains — the honest
            // low-res visualization of the un-materialized bulk (whose PHYSICS is the boundary + gravity
            // source). A smooth sphere would hide excavation; grains can be missing. Shell points inside
            // the materialized impact region are hidden — the real (moving, glowing) cap particles are
            // the matter there now, and the void they leave IS the crater.
            let earth_center = r_bodies[1];
            // docs/42 render layer: while the GPU impact is live, the PRETTY Earth-shell must overlay the SPH
            // particle field — so it is sized to the sub-scale (5000 km) SPH body and rendered at SPH_VIS_SCALE
            // (not the real 6371 km at DISPLAY_SCALE), and its grains fade out by `1−render_blend` (the slider
            // cross-fades to the raw physics billboards). Otherwise (CPU scene) it's the real Earth as before.
            let (pretty_scale, pretty_r_surf) = if self.sph_active {
                (SPH_VIS_SCALE, 5.0e6_f64)
            } else {
                (DISPLAY_SCALE, EARTH_RADIUS_M)
            };
            let pretty_fade = if self.sph_active { (1.0 - self.render_blend) as f32 } else { 1.0 };
            // docs/42 Phase 3 — atmosphere MIST: the giant impact vaporizes rock into a thick, shocked vapor
            // atmosphere, so the Rayleigh veil is boosted while the impact is live → a hazy, glowing limb.
            let atm_tau_eff = if self.sph_active {
                [self.atm_tau[0] * 2.6, self.atm_tau[1] * 2.6, self.atm_tau[2] * 2.6]
            } else {
                self.atm_tau
            };
            let shell_spacing = pretty_r_surf * (4.0 * std::f64::consts::PI / SHELL_N as f64).sqrt();
            // Grains overlap MORE while the GPU impact is live (0.90 vs 0.62 of the spacing) so the crust reads
            // as opaque — the glowing interior then shows ONLY through the actual crater hole, not every crevice.
            let grain_overlap = if self.sph_active { 0.90 } else { 0.62 };
            let shell_grain_r = ((grain_overlap * shell_spacing) * pretty_scale) as f32 * pretty_fade;
            // docs/42 Phase 2 — capture the giant-impact crater site from the GPU field: at first Theia (prov 1)
            // contact with Earth (prov 0) freeze the impact DIRECTION (Earth-relative), then open the bowl over
            // ~1 s. Persists after (bake-back). The bowl radius grows with `gpu_crater_frac` (set in the crater
            // block below). `earth_center + dir·pretty_r_surf` lands it on the sub-scale surface, same frame as
            // the shell grains, so the `hidden` test carves the crust exactly where Theia struck.
            if self.sph_active && !self.sph_snapshot.is_empty() {
                let (mut ec, mut me, mut tc, mut mt) = (glam::DVec3::ZERO, 0.0f64, glam::DVec3::ZERO, 0.0f64);
                for p in &self.sph_snapshot {
                    let pos = glam::DVec3::new(p.pos[0] as f64, p.pos[1] as f64, p.pos[2] as f64);
                    let m = p.mass as f64;
                    if p.prov == 0 { ec += pos * m; me += m; } else { tc += pos * m; mt += m; }
                }
                if me > 0.0 && mt > 0.0 {
                    let (ec, tc) = (ec / me, tc / mt);
                    if self.gpu_impact_site.is_none() && (tc - ec).length() < 1.3e7 {
                        self.gpu_impact_site = (tc - ec).try_normalize(); // contact ≈ r_e + r_t (sub-scale)
                    }
                    if self.gpu_impact_site.is_some() {
                        self.gpu_crater_frac = (self.gpu_crater_frac + 0.03).min(1.0);
                    }
                }
            }
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
            // The crater opens once the RENDERED clock reaches the shatter. It is punched into the CRUST, so
            // it must CO-ROTATE with the surface — apply `spin_rot`, exactly like the shell grains below.
            // Leaving it as the inertial `earth_center + rel` let the hole slide through the rotating
            // material once the impact spun Earth up — a render-truth frame mismatch (the crater and the
            // matter it's cut from must share one frame). `impact_site_rel` was captured at spin_angle≈0, so
            // `spin_rot·rel` carries it forward with the crust.
            let (crater_site, crater_r) = if self.sph_active {
                // GPU impact (docs/42 Phase 2): the frozen site on the sub-scale surface; the bowl opens with the shock.
                match self.gpu_impact_site {
                    Some(dir) => (Some(earth_center + dir * pretty_r_surf), self.gpu_crater_frac * 0.72 * pretty_r_surf),
                    None => (None, 0.0),
                }
            } else if r_shattered {
                (self.impact_site_rel.map(|rel| earth_center + spin_rot * rel), 1.1 * self.hole_radius())
            } else {
                (None, 0.0)
            };
            // OBLATE figure: the spin flattens the planet (Radau–Darwin) — equator bulges (+f/3),
            // poles sink (−2f/3), volume-preserving to first order. At today's day it's 1/298
            // (imperceptible); at the post-impact 3.8-h day it's ~13% — a visibly squashed world.
            let spin_omega_r = self.spin_l.length()
                / crate::tides::moment_of_inertia(self.bodies[1].mass, EARTH_RADIUS_M);
            let flat = crate::tides::flattening_from_spin(
                spin_omega_r, self.bodies[1].mass, EARTH_RADIUS_M,
            );
            for (i, uni) in self.shell_unis.iter().enumerate() {
                let body_dir = crate::impact::fib_dir(i, SHELL_N); // this grain's fixed BODY direction
                let dir = spin_rot * body_dir; // its current WORLD direction (rotated by the spin)
                let u = dir.dot(spin_axis);
                let r_oblate = (pretty_r_surf - 0.62 * shell_spacing)
                    * (1.0 + flat * (1.0 / 3.0 - u * u)); // +f/3 equator, −2f/3 poles
                let pos_w = earth_center + dir * r_oblate;
                let hidden = crater_site.map_or(false, |s| (pos_w - s).length() < crater_r);
                let scale = if hidden { 0.0 } else { shell_grain_r }; // zero-scale ⇒ not drawn
                let spos = ((pos_w - focus) * pretty_scale).as_vec3();
                // Continents & oceans (docs/25): each grain samples the landmask at its fixed BODY direction
                // — so a continent is a property of the CRUST and CO-ROTATES with the planet (and with the
                // crater), rather than being painted world-fixed while the grains slide underneath. "Average
                // area particles": the grain is the mean of its ~10°×10° patch, nothing painted.
                let surf = crate::planet::earth_surface_material(body_dir);
                let m = &self.mats[materials::index_of(&self.mats, surf)];
                // RAYLEIGH (docs/26): the declared air scatters sunlight over this patch — a blue
                // veil (into the emissive channel: it IS added light) whose ground shows through
                // slightly reddened (two-way transmittance). All from the emergent pressure; an
                // airless world renders colorless by the same code.
                let v_dir = (eye_disp - (pos_w - focus) * DISPLAY_SCALE).normalize_or_zero();
                let mu_v = dir.dot(v_dir);
                let mu_s = dir.dot(sun_dir_earth);
                let cos_th = v_dir.dot(sun_dir_earth);
                let veil = crate::atmosphere::rayleigh_veil(mu_v, mu_s, cos_th, atm_tau_eff, 22.0);
                let tr = crate::atmosphere::rayleigh_transmit(mu_v, mu_s, atm_tau_eff);
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
                let ipos = ((earth_center - focus) * pretty_scale).as_vec3();
                // The interior must wear the SAME oblate figure as the shell, else at the post-impact
                // ~13% flattening the poles sink below a perfect 0.985 R sphere and the interior pokes
                // OUT through the crust at both poles (a render-truth bug). Ellipsoid: equator +f/3,
                // poles −2f/3 about the spin axis — one non-uniform scale, oriented to the spin axis.
                // docs/42: sized to the sub-scale body + faded with the blend while the GPU impact is live.
                let ir = (pretty_r_surf * 0.985) * pretty_scale * pretty_fade as f64;
                let ir_eq = (ir * (1.0 + flat / 3.0)) as f32;
                let ir_pol = (ir * (1.0 - 2.0 * flat / 3.0)) as f32;
                let align = glam::DQuat::from_rotation_arc(glam::DVec3::Z, spin_axis);
                // During the GPU giant impact the exposed interior is a MAGMA ocean (docs/42 Phase 2): a hot
                // self-lit orange, ramping up as the crater opens — so the crater (and the melt showing between
                // crust grains) reads as a molten post-impact Earth rather than the CPU scene's cool interior.
                let (itint, iglow) = if self.sph_active {
                    let g = 0.6 + 2.4 * self.gpu_crater_frac as f32; // brighter as the shock excavates
                    ([0.20, 0.09, 0.05, 1.0], [1.0, 0.42, 0.12, g])
                } else {
                    (self.interior_tint, self.interior_glow)
                };
                write_space_uniform(
                    &self.queue,
                    &self.interior_uni,
                    view_proj,
                    Mat4::from_translation(ipos)
                        * Mat4::from_quat(align.as_quat())
                        * Mat4::from_scale(Vec3::new(ir_eq, ir_eq, ir_pol)),
                    earth_light,
                    itint,
                    iglow, // outer-core iron: self-lit at its real temperature (magma while impacting)
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
            // docs/42 Phase 4 — accreting MOONLET spheres: self-bound disk clumps resolve out of the ejecta into
            // growing rock spheres (borrowing the debris uni pool, unused while the GPU impact runs). Warm-tinted
            // — freshly accreted, still cooling. They grow as the clump gathers mass; the largest is the Moon.
            let n_moonlets = if self.sph_active && pretty_fade > 0.0 && !self.sph_snapshot.is_empty() {
                let bodies = crate::gpu_sph::moonlet_bodies(&self.sph_snapshot);
                let n = bodies.len().min(self.debris_unis.len());
                for (uni, &(com_pos, radius, _mass)) in self.debris_unis.iter().zip(bodies.iter()).take(n) {
                    let spos = ((earth_center + com_pos - focus) * pretty_scale).as_vec3();
                    let r_disp = (radius * pretty_scale * 1.6) as f32 * pretty_fade;
                    write_space_uniform(
                        &self.queue,
                        uni,
                        view_proj,
                        Mat4::from_translation(spos) * Mat4::from_scale(Vec3::splat(r_disp)),
                        earth_light,
                        [0.45, 0.34, 0.28, 1.0], // cooling rock
                        [1.0, 0.55, 0.25, 0.5],  // a faint warm glow — recently molten
                    );
                }
                n
            } else {
                0
            };
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
                let frag_r = moon_r / (SCENE_DEBRIS_N as f32).cbrt(); // N fragments ≈ the Moon's volume
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
                // The rigid-Earth + sphere-debris model draws only when the GPU SPH impact is NOT running
                // (docs/33 stage 4c.4): with the deformable impact active, the particle field IS the planet.
                if !self.sph_active {
                    for uni in self.wall_unis.iter() {
                        draw(&mut pass, uni, &self.sphere_gpu); // crater bowl wall (zero-scale when intact)
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
                // The pretty Earth shell (docs/42): the CPU scene always; the GPU-impact scene whenever the blend
                // isn't fully at the physics end (its grains were sized to the SPH body + faded by 1−blend above,
                // so they overlay the particle field and cross-fade to it).
                if !self.sph_active || self.render_blend < 1.0 {
                    // the glowing deep interior first (shows through the crater), then the crust shell over it
                    draw(&mut pass, &self.interior_uni, &self.sphere_gpu);
                    for uni in self.shell_unis.iter() {
                        draw(&mut pass, uni, &self.sphere_gpu); // Earth: a shell of coarse grains
                    }
                    // accreting moonlet spheres (docs/42 Phase 4), from the disk's self-bound clumps
                    for uni in self.debris_unis.iter().take(n_moonlets) {
                        draw(&mut pass, uni, &self.sphere_gpu);
                    }
                }
                // GPU SPH particles: instanced billboards straight from the physics buffer (zero-copy).
                if self.sph_active {
                    if let Some(sph) = self.gpu_sph.as_ref() {
                        if sph.count() > 0 {
                            pass.set_pipeline(&self.sph_pipeline);
                            pass.set_bind_group(0, &self.sph_cam.bind, &[]);
                            pass.set_vertex_buffer(0, sph.particle_buffer().slice(..));
                            pass.draw(0..6, 0..sph.count());
                        }
                    }
                }
            }
            self.queue.submit(std::iter::once(encoder.finish()));
            output.present();
            Ok(())
        }

        /// World metres spanned by one screen pixel at the focus body (the look target sits at the
        /// display origin, so the focal distance is exactly `base_distance·zoom` display units).
        /// Display units are metres·DISPLAY_SCALE, so divide back out to report a true metres/pixel —
        /// which the HUD renders as a km/AU scale bar. Honest live read of camera state; feeds the
        /// same scale bar as the terrain scene through `metres_per_pixel_at`.
        pub fn meters_per_pixel(&self) -> f64 {
            let dist_disp = (self.camera.base_distance * self.camera.zoom) as f64;
            let dist_m = dist_disp / DISPLAY_SCALE; // display units → metres
            crate::metres_per_pixel_at(dist_m, 0.9, self.config.height.max(1) as f64)
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

    /// docs/43 Phase 3 — the Terra globe pipeline: the same vertex layout + bind layout as the space pipeline,
    /// but `globe.wgsl` (per-vertex biome colour + a cheap atmospheric limb) instead of the flat-tint shader.
    /// `blend` is REPLACE for the opaque globe and alpha-blending for the ground cap's cross-fade (Phase 5).
    fn build_globe_pipeline(
        device: &wgpu::Device,
        bind_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
        blend: wgpu::BlendState,
    ) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("globe-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../../shaders/globe.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("globe-pipeline-layout"),
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
            label: Some("globe-pipeline"),
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
                // docs/43: NO culling — the fly camera looking down saw the front-facing globe triangles
                // culled (a growing black VOID at nadir on descent, the ~250 km bug). Convex globe → depth
                // alone occludes correctly; robust regardless of winding, extra fragments are cheap.
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
            multisample: wgpu::MultisampleState { count: 1, mask: !0, alpha_to_coverage_enabled: false },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(blend),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        })
    }

    /// A GpuMesh whose vertex buffer is writable (VERTEX | COPY_DST) and pre-sized for `vert_capacity` vertices,
    /// with a fixed index buffer. For geometry rebuilt every frame (the ground cap) — write vertices, don't
    /// reallocate.
    fn make_dynamic_mesh(device: &wgpu::Device, label: &str, vert_capacity: usize, indices: &[u32]) -> GpuMesh {
        let vertex_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: (vert_capacity * std::mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let index_buf = make_buffer(device, label, bytemuck::cast_slice(indices), wgpu::BufferUsages::INDEX);
        GpuMesh { vertex_buf, index_buf, index_count: indices.len() as u32 }
    }

    /// The instanced particle pipeline for the GPU SPH impact (docs/33 stage 4c.4). One camera-facing
    /// billboard quad per particle, generated in the vertex shader; the instance buffer is the `sph_step.wgsl`
    /// particle buffer itself (48-byte stride, pos at offset 0, provenance u32 at offset 44). No mesh, no
    /// per-vertex buffer — the quad corners come from the vertex index.
    fn build_sph_pipeline(
        device: &wgpu::Device,
        bind_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
    ) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sph-render-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../../shaders/sph_render.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sph-render-pipeline-layout"),
            bind_group_layouts: &[bind_layout],
            push_constant_ranges: &[],
        });
        // Instance-step layout over the SPH particle buffer: pos (vec3 @ 0) + provenance (u32 @ 44).
        const ATTRS: [wgpu::VertexAttribute; 2] = [
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint32, offset: 44, shader_location: 1 },
        ];
        let instance_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<crate::gpu_sph::SphParticle>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &ATTRS,
        };
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sph-render-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[instance_layout],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None, // billboards always face the camera
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
            multisample: wgpu::MultisampleState { count: 1, mask: !0, alpha_to_coverage_enabled: false },
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

    // ---------------------------------------------------------------------------------------------
    // docs/43 — Terra: a planet/terrain scene built from a DATA "world" (the first worlds-as-data scene).
    // Phase 1 renders the Earth as the reused grain shell, recolored by the loaded world + its declared
    // atmosphere; later phases add the raster surface sampler, the displaced globe mesh, and the fly camera.
    // ---------------------------------------------------------------------------------------------
    /// Default relief exaggeration if a world doesn't declare one (`surface.relief_exaggeration`). 1.0 = true
    /// scale. The globe mesh, ground cap, and camera floor all read the world's value so they stay one surface.
    const TERRA_RELIEF_EXAG: f64 = 1.0;
    /// Ground-cap grid resolution per side (Phase 5). The vertex buffer is rebuilt each frame; the index buffer
    /// (fixed topology) is built once.
    const TERRA_CAP_RES: usize = 192;

    #[wasm_bindgen]
    pub struct Terra {
        surface: wgpu::Surface<'static>,
        device: wgpu::Device,
        queue: wgpu::Queue,
        config: wgpu::SurfaceConfiguration,
        depth_view: wgpu::TextureView,
        pipeline: wgpu::RenderPipeline,
        sphere_gpu: GpuMesh,
        shell_unis: Vec<UniformSlot>,
        shell_count: usize,
        // docs/43 Phase 3 — the displaced cube-sphere globe. Once a world with surface rasters loads, this
        // smooth mesh (land lifted by real elevation + biome-coloured, ocean cells at sea level with the water
        // material) replaces the grain shell for the scene. `None` until then (falls back to the grain shell).
        globe_pipeline: wgpu::RenderPipeline,
        globe_mesh: Option<GpuMesh>,
        globe_uni: UniformSlot,
        // docs/43 Phase 5 — the fine, camera-relative ground cap (rebuilt each frame under the camera) + its
        // alpha-blend pipeline, and a reused CPU vertex scratch buffer. Cross-faded with the globe by altitude.
        cap_pipeline: wgpu::RenderPipeline,
        cap_gpu: GpuMesh,
        cap_uni: UniformSlot,
        cap_verts: Vec<Vertex>,
        relief_exag: f64,
        mats: Vec<materials::Material>,
        fly: crate::terra::fly_camera::FlyCamera,
        planet_radius: f64,
        atm_tau: [f64; 3],
        world_name: String,
        // docs/43 Phase 2 — the baked surface rasters (land mask, elevation+bathymetry, land-cover biome) and
        // the biome-index → material-index map. `None` until a world with surface rasters is loaded.
        landmask: Option<crate::terra::raster::Raster>,
        elevation: Option<crate::terra::raster::Raster>,
        landcover: Option<crate::terra::raster::Raster>,
        elev_range: [f64; 2],
        biome_mats: Vec<usize>, // biome index → index into `mats`
    }

    #[wasm_bindgen]
    impl Terra {
        pub async fn create(canvas: HtmlCanvasElement) -> Result<Terra, JsValue> {
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
                        label: Some("greenfield-terra"),
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
            // A LOW-poly grain sphere: with a fine shell (thousands of grains) each grain is tiny, so a coarse
            // sphere keeps the triangle + draw budget sane (the smooth displaced globe mesh arrives in Phase 3).
            let sphere_gpu = upload_mesh(
                &device,
                "terra-grain",
                &mesher::build_uv_sphere(1.0, 0, [1.0, 1.0, 1.0], 10, 14),
            );
            let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("terra-bind-layout"),
                entries: &[uniform_entry(0, wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT)],
            });
            let pipeline = build_space_pipeline(&device, &bind_layout, config.format);
            let globe_pipeline =
                build_globe_pipeline(&device, &bind_layout, config.format, wgpu::BlendState::REPLACE);
            let globe_uni = make_space_uniform(&device, &bind_layout);
            // The ground cap: same shader, alpha-blended for the cross-fade; a writable vertex buffer rebuilt each
            // frame, fixed index topology.
            let cap_pipeline =
                build_globe_pipeline(&device, &bind_layout, config.format, wgpu::BlendState::ALPHA_BLENDING);
            let cap_gpu = make_dynamic_mesh(
                &device,
                "terra-cap",
                TERRA_CAP_RES * TERRA_CAP_RES,
                &crate::terra::ground_cap::cap_indices(TERRA_CAP_RES),
            );
            let cap_uni = make_space_uniform(&device, &bind_layout);
            let shell_count = 4096; // ~2.8° grain spacing — resolves continents/biomes (Phase 2, grain shell)
            let shell_unis: Vec<UniformSlot> =
                (0..shell_count).map(|_| make_space_uniform(&device, &bind_layout)).collect();
            let atm_tau = crate::atmosphere::rayleigh_tau(crate::planet::earth().surface_pressure() / 101_325.0);
            let mats = materials::load();
            // Default fly camera: orbital over the equator (a world file overrides this in `load_world`).
            let fly = crate::terra::fly_camera::FlyCamera::new(
                20.0, 0.0, 12_000_000.0, 0.0, -1.2, 2.0, 40_000_000.0,
            );
            Ok(Terra {
                surface,
                device,
                queue,
                config,
                depth_view,
                pipeline,
                sphere_gpu,
                shell_unis,
                shell_count,
                globe_pipeline,
                globe_mesh: None,
                globe_uni,
                cap_pipeline,
                cap_gpu,
                cap_uni,
                cap_verts: Vec::new(),
                relief_exag: TERRA_RELIEF_EXAG,
                mats,
                fly,
                planet_radius: EARTH_RADIUS_M,
                atm_tau,
                world_name: String::new(),
                landmask: None,
                elevation: None,
                landcover: None,
                elev_range: [-11000.0, 9000.0],
                biome_mats: Vec::new(),
            })
        }

        /// docs/43: load a world from JSON + its decoded surface rasters. The JS host decodes each PNG to raw
        /// RGBA (4 channels) via ImageBitmap and passes the bytes + dims here. Any raster may be empty (`len 0`)
        /// → treated as absent (falls back to the built-in ASCII landmask / no displacement).
        #[allow(clippy::too_many_arguments)]
        pub fn load_world(
            &mut self,
            world_json: &str,
            landmask: &[u8],
            lm_w: u32,
            lm_h: u32,
            elevation: &[u8],
            ev_w: u32,
            ev_h: u32,
            landcover: &[u8],
            lc_w: u32,
            lc_h: u32,
        ) -> Result<(), JsValue> {
            let w = crate::terra::world_def::World::parse(world_json).map_err(|e| JsValue::from_str(&e))?;
            let planet = w
                .planet
                .as_ref()
                .ok_or_else(|| JsValue::from_str("Terra world is missing a `planet` section"))?;
            self.planet_radius = planet.radius_m;
            // ONE SOURCE for surface pressure: the declared atmosphere MASS, weighed. Reading a declared
            // `surface_pressure_pa` here was a docs/46 violation with a measured cost — Earth's world file
            // said 101,325 Pa while the emergent value is 99,049 Pa, so Terra's sky was a 2.2%-different
            // atmosphere from the one the terrain and orbit scenes render. Same planet, two airs.
            let g_surface = crate::planet::earth().gravity_at(planet.radius_m);
            let p_ratio = w
                .atmosphere
                .as_ref()
                .and_then(|a| a.surface_pressure(planet.radius_m, g_surface))
                .unwrap_or_else(|| crate::planet::earth().surface_pressure())
                / 101_325.0;
            self.atm_tau = crate::atmosphere::rayleigh_tau(p_ratio);
            self.world_name = w.name.clone();

            // docs/43 Phase 4 — seed the fly camera from the world's declared camera (default: orbital over 20°N).
            if let Some(c) = w.camera.as_ref() {
                let look = c.look.clone().unwrap_or_default();
                self.fly = crate::terra::fly_camera::FlyCamera::new(
                    c.lat,
                    c.lon,
                    if c.alt_m > 0.0 { c.alt_m } else { 12_000_000.0 },
                    look.yaw,
                    look.pitch,
                    c.min_alt_m.unwrap_or(2.0),
                    c.max_alt_m.unwrap_or(40_000_000.0),
                );
            }

            use crate::terra::raster::Raster;
            let mk = |bytes: &[u8], rw: u32, rh: u32| -> Option<Raster> {
                if bytes.is_empty() {
                    return None;
                }
                Raster::new(rw as usize, rh as usize, 4, bytes.to_vec()).ok()
            };
            self.landmask = mk(landmask, lm_w, lm_h);
            self.elevation = mk(elevation, ev_w, ev_h);
            self.landcover = mk(landcover, lc_w, lc_h);

            // Biome index → material index. `biomes` maps a string index → material id in data/materials.json.
            self.biome_mats.clear();
            self.elev_range = [-11000.0, 9000.0];
            self.relief_exag = TERRA_RELIEF_EXAG;
            if let Some(s) = w.surface.as_ref() {
                if let Some(r) = s.elevation_range_m {
                    self.elev_range = r;
                }
                if let Some(x) = s.relief_exaggeration {
                    self.relief_exag = x.max(0.0);
                }
                let max_idx = s.biomes.keys().filter_map(|k| k.parse::<usize>().ok()).max().unwrap_or(0);
                self.biome_mats = (0..=max_idx)
                    .map(|i| {
                        let mat_id = s.biomes.get(&i.to_string()).map(String::as_str).unwrap_or("granite");
                        materials::index_of(&self.mats, mat_id)
                    })
                    .collect();
            }
            // docs/43 Phase 3 — build the smooth displaced globe from the loaded rasters (retires the grain
            // shell for this scene). Built once here; the fly-camera LOD refinement comes in Phase 5.
            let mesh = self.build_surface_mesh();
            let tri = mesh.indices.len() / 3;
            self.globe_mesh = Some(upload_mesh(&self.device, "terra-globe", &mesh));

            let land_frac = self.landmask.as_ref().map(|r| r.land_fraction());
            log::info!("Terra: globe mesh built — {} triangles", tri);
            log::info!(
                "Terra: loaded '{}' — radius {:.0} km, rasters land={} elev={} cover={}, land fraction {:?}",
                w.name,
                self.planet_radius / 1e3,
                self.landmask.is_some(),
                self.elevation.is_some(),
                self.landcover.is_some(),
                land_frac,
            );
            Ok(())
        }

        pub fn world_name(&self) -> String {
            self.world_name.clone()
        }

        // docs/43 Phase 4 — the continuous fly-camera API (WASD + zoom(=altitude) + mouse-look). The JS host
        // maps input to these; the camera itself blends orbit⇄ground by altitude (see `terra::fly_camera`).

        /// Set the camera outright (lat/lon degrees, altitude metres, look yaw/pitch radians).
        pub fn set_fly(&mut self, lat: f64, lon: f64, alt_m: f64, yaw: f64, pitch: f64) {
            self.fly.lat = lat;
            self.fly.lon = lon;
            self.fly.alt_m = alt_m.clamp(self.fly.min_alt, self.fly.max_alt);
            self.fly.yaw = yaw;
            self.fly.pitch = pitch;
        }

        /// WASD: move across the surface. `forward`/`right` are −1/0/+1 intents; the step scales with altitude
        /// (fast from orbit, metres-per-frame on the ground) so a keypress feels the same at every scale.
        pub fn move_tangent(&mut self, forward: f64, right: f64) {
            // Step ≈ a small fraction of the current altitude per frame, floored so ground movement still works.
            let step = (self.fly.alt_m * 0.02).max(2.0);
            self.fly.move_tangent(forward * step, right * step, self.planet_radius);
        }

        /// Zoom = altitude change. `notches` is the wheel delta (or +/−1); positive climbs, negative descends.
        pub fn zoom_alt(&mut self, notches: f64) {
            self.fly.zoom_alt((notches * 0.12).exp());
        }

        /// A pointer drag (pixel deltas): orbit high up, free-look near the ground (altitude-blended).
        pub fn drag_look(&mut self, dx: f64, dy: f64) {
            self.fly.drag(dx, dy);
        }

        pub fn altitude_m(&self) -> f64 {
            self.fly.alt_m
        }
        pub fn latitude(&self) -> f64 {
            self.fly.lat
        }
        pub fn longitude(&self) -> f64 {
            self.fly.lon
        }

        /// docs/43 Phase 6 — the surface type directly under the camera (for the HUD): the biome material id on
        /// land ("grass", "sand", "snow", …) or "ocean" over water.
        pub fn ground_biome(&self) -> String {
            let (lat, lon) = (self.fly.lat, self.fly.lon);
            let is_land = self.landmask.as_ref().map(|r| r.land_at(lat, lon)).unwrap_or(false);
            if !is_land {
                return "ocean".to_string();
            }
            let biome = self.landcover.as_ref().map_or(1, |r| r.biome_at(lat, lon) as usize);
            let mi = self.biome_mats.get(biome).copied().unwrap_or(0);
            self.mats.get(mi).map(|m| m.id.clone()).unwrap_or_default()
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

        pub fn render(&mut self) -> Result<(), JsValue> {
            let r_disp = self.planet_radius * DISPLAY_SCALE; // = 1.0 for Earth
            // docs/43 Phase 4/5 — the fly camera builds the frame (absolute + camera-relative view·projection, the
            // f64 eye, and the tangent frame). The terrain height under the camera keeps "altitude" above the
            // local ground (not sea level).
            let aspect = self.config.width as f64 / self.config.height.max(1) as f64;
            let ground_disp = self.ground_disp_at(self.fly.lat, self.fly.lon);
            let view = self.fly.view(r_disp, DISPLAY_SCALE, aspect, ground_disp);
            let view_proj = view.vp_abs;
            let eye = view.eye;
            // Fixed direction TO the sun → a pleasant ¾ lighting; the day/night terminator is emergent.
            let sun_dir = glam::DVec3::new(1.0, 0.45, 0.6).normalize();
            let sun_light = Vec3::new(sun_dir.x as f32, sun_dir.y as f32, sun_dir.z as f32);

            // docs/43 Phase 5 — build the fine ground cap under the camera and cross-fade it in as we descend.
            // `cap_fade`: 0 above ~40 km, 1 below ~15 km (smoothstep). Only build when it will show and a surface
            // is loaded.
            let alt_m = self.fly.alt_m;
            let cap_fade = {
                let (hi, lo) = (40_000.0, 15_000.0);
                let t = ((alt_m - lo) / (hi - lo)).clamp(0.0, 1.0);
                (1.0 - t * t * (3.0 - 2.0 * t)) as f32
            };
            if cap_fade > 0.0 && self.globe_mesh.is_some() {
                self.build_cap(&view, sun_light, cap_fade);
            }

            if self.globe_mesh.is_some() {
                // docs/43 Phase 3 — the displaced globe: one draw. Identity model (the mesh is already in
                // display units, Earth-centred at the origin); white tint (the mesh carries the per-vertex biome
                // colour); emissive.xyz = camera eye (display units), .w = atmosphere strength (the globe.wgsl
                // Rayleigh limb). The per-vertex Rayleigh ground veil is a Phase-5 refinement.
                write_space_uniform(
                    &self.queue,
                    &self.globe_uni,
                    view_proj,
                    Mat4::IDENTITY,
                    sun_light,
                    [1.0, 1.0, 1.0, 1.0],
                    [eye.x as f32, eye.y as f32, eye.z as f32, 0.8],
                );
            } else {
                // Fallback: the Phase-2 grain shell (used until a world's surface rasters build the globe mesh).
                let shell_spacing =
                    self.planet_radius * (4.0 * std::f64::consts::PI / self.shell_count as f64).sqrt();
                let grain_r = ((0.62 * shell_spacing) * DISPLAY_SCALE) as f32;
                const EXAG: f64 = TERRA_RELIEF_EXAG;
                let water_idx = materials::index_of(&self.mats, "water");
                for (i, uni) in self.shell_unis.iter().enumerate() {
                    let dir = crate::impact::fib_dir(i, self.shell_count);
                    let lat = dir.y.asin().to_degrees();
                    let lon = dir.z.atan2(dir.x).to_degrees();
                    // Land/ocean from the real Natural Earth mask (fallback: the built-in ASCII mask).
                    let is_land = self
                        .landmask
                        .as_ref()
                        .map(|r| r.land_at(lat, lon))
                        .unwrap_or_else(|| crate::planet::earth_surface_material(dir) == "granite");
                    // Land: biome material (land-cover) + real elevation displacement. Ocean: water at sea level.
                    let (mat_idx, elev_m) = if is_land {
                        let biome = self.landcover.as_ref().map_or(1, |r| r.biome_at(lat, lon) as usize);
                        let mi = self.biome_mats.get(biome).copied().unwrap_or(water_idx);
                        let e = self
                            .elevation
                            .as_ref()
                            .map_or(0.0, |r| r.elevation_m_at(lat, lon, self.elev_range[0], self.elev_range[1]))
                            .max(0.0);
                        (mi, e)
                    } else {
                        (water_idx, 0.0)
                    };
                    let m = &self.mats[mat_idx];
                    let pos = dir * (r_disp + elev_m * DISPLAY_SCALE * EXAG);
                    let spos = pos.as_vec3();
                    // Rayleigh atmosphere (docs/26): blue veil (added light) + two-way transmittance on the ground.
                    let v_dir = (eye - pos).normalize_or_zero();
                    let mu_v = dir.dot(v_dir);
                    let mu_s = dir.dot(sun_dir);
                    let cos_th = v_dir.dot(sun_dir);
                    let veil = crate::atmosphere::rayleigh_veil(mu_v, mu_s, cos_th, self.atm_tau, 22.0);
                    let tr = crate::atmosphere::rayleigh_transmit(mu_v, mu_s, self.atm_tau);
                    let tint = [m.albedo[0] * tr[0], m.albedo[1] * tr[1], m.albedo[2] * tr[2], 1.0];
                    write_space_uniform(
                        &self.queue,
                        uni,
                        view_proj,
                        Mat4::from_translation(spos) * Mat4::from_scale(Vec3::splat(grain_r)),
                        sun_light,
                        tint,
                        [veil[0], veil[1], veil[2], 1.0],
                    );
                }
            }
            let output = self
                .surface
                .get_current_texture()
                .map_err(|e| JsValue::from_str(&format!("get_current_texture failed: {e}")))?;
            let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("terra-frame") });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("terra-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.01, g: 0.01, b: 0.03, a: 1.0 }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.depth_view,
                        depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                if let Some(globe) = &self.globe_mesh {
                    pass.set_pipeline(&self.globe_pipeline);
                    draw(&mut pass, &self.globe_uni, globe);
                    // docs/43 Phase 5 — the fine ground cap over the globe (alpha-blended cross-fade). Drawn only
                    // when it was built this frame (cap_fade > 0); it covers the foreground out past the horizon.
                    if cap_fade > 0.0 {
                        pass.set_pipeline(&self.cap_pipeline);
                        draw(&mut pass, &self.cap_uni, &self.cap_gpu);
                    }
                } else {
                    pass.set_pipeline(&self.pipeline);
                    for uni in self.shell_unis.iter() {
                        draw(&mut pass, uni, &self.sphere_gpu);
                    }
                }
            }
            self.queue.submit(std::iter::once(encoder.finish()));
            output.present();
            Ok(())
        }

        /// docs/43 Phase 3 — build the displaced cube-sphere globe surface from the loaded rasters. The ocean is
        /// integrated into the same mesh (ocean cells sit at exactly sea level with the water material), so there
        /// is no separate ocean shell and no coast z-fighting. `EXAG` exaggerates relief so it reads on a radius-1
        /// globe (Everest is only ~0.05% of Earth's radius); the true ratio returns with the ground LOD (Phase 5).
        /// Terrain height (display units, above the sea-level sphere) as a clearance floor for the fly camera at a
        /// lat/lon. The fly camera adds this to `r_disp` so "altitude" means height above the local ground, not
        /// sea level — otherwise the ×30 exaggerated mountains would swallow the eye at low altitude.
        ///
        /// It returns the MAX over a small neighbourhood (roughly the coarse mesh cell), not a point sample, so
        /// the eye clears the terrain *envelope* around it and can never end up inside a neighbouring exaggerated
        /// peak (the camera must never pass through solid ground). Ocean → 0 (the flat sea surface). The proper
        /// per-triangle collision against the real-ratio ground surface arrives with the ground LOD (Phase 5).
        ///
        /// NOTE (architecture): this is a HEIGHTFIELD floor — single-valued along the radial. It cannot represent
        /// caves (void below the surface) or arches (solid above void). Those need a VOLUMETRIC "is this point in
        /// solid matter?" test against the material field (voxel/SDF/particle), which is where camera collision
        /// must move once terrain is a real matter field (docs/39/42). Kept as a heightfield only as a stand-in.
        fn ground_disp_at(&self, lat: f64, lon: f64) -> f64 {
            let Some(elev) = self.elevation.as_ref() else { return 0.0 };
            let land = self.landmask.as_ref();
            // ±0.2° (~22 km) 3×3 max = the local terrain envelope: enough to clear any terrain the camera could
            // reach before the floor rises (forced-up look-ahead), without floating far above a plain that merely
            // has a distant peak. The whole visible ground cap at low altitude fits inside this radius.
            let mut peak = 0.0f64;
            for dlat in [-0.2, 0.0, 0.2] {
                for dlon in [-0.2, 0.0, 0.2] {
                    let (la, lo) = (lat + dlat, lon + dlon);
                    let is_land = land.map(|r| r.land_at(la, lo)).unwrap_or(false);
                    if !is_land {
                        continue;
                    }
                    let e = elev.elevation_m_at(la, lo, self.elev_range[0], self.elev_range[1]).max(0.0);
                    peak = peak.max(e);
                }
            }
            peak * DISPLAY_SCALE * self.relief_exag
        }

        fn build_surface_mesh(&self) -> Mesh {
            let r_disp = self.planet_radius * DISPLAY_SCALE; // = 1.0 for Earth
            let exag = self.relief_exag;
            let ds = DISPLAY_SCALE;
            let water_idx = materials::index_of(&self.mats, "water");
            let water_alb = self.mats[water_idx].albedo;
            crate::terra::globe_mesh::build_globe(256, r_disp, |dir| {
                let lat = dir.y.asin().to_degrees();
                let lon = dir.z.atan2(dir.x).to_degrees();
                let is_land = self
                    .landmask
                    .as_ref()
                    .map(|r| r.land_at(lat, lon))
                    .unwrap_or_else(|| crate::planet::earth_surface_material(dir) == "granite");
                if is_land {
                    let biome = self.landcover.as_ref().map_or(1, |r| r.biome_at(lat, lon) as usize);
                    let mi = self.biome_mats.get(biome).copied().unwrap_or(water_idx);
                    let e = self
                        .elevation
                        .as_ref()
                        .map_or(0.0, |r| r.elevation_m_at(lat, lon, self.elev_range[0], self.elev_range[1]));
                    // Land above sea level; below-sea-level land (Dead Sea etc.) clamps to the shore.
                    (self.mats[mi].albedo, e.max(0.0) * ds * exag)
                } else {
                    // Ocean surface: flat at sea level with the water albedo (bathymetry is hidden, so unused).
                    (water_alb, 0.0)
                }
            })
        }

        /// docs/43 Phase 5 — rebuild the camera-relative ground cap under the camera and upload it (vertices only;
        /// the index topology is fixed). It samples the SAME surface as the globe (real elevation × the world's
        /// declared exaggeration, biome albedo) at high resolution, curving to a true horizon, emitted relative to
        /// the eye for ground-scale precision. `cap_fade` is the cross-fade alpha, carried in tint.a.
        fn build_cap(&mut self, view: &crate::terra::fly_camera::View, sun_light: Vec3, cap_fade: f32) {
            let r_disp = self.planet_radius * DISPLAY_SCALE;
            let exag = self.relief_exag;
            let ds = DISPLAY_SCALE;
            let res = TERRA_CAP_RES;
            // Cover ~1.3× the horizon angle so the patch reaches past the visible horizon (its far edge then sits
            // below the horizon / is occluded — no visible cap boundary).
            let cap_angle = (1.3 * view.horizon / r_disp).clamp(1e-4, 0.6);
            let lift = 20.0 * ds; // a few metres toward the camera so the fine cap sits in front of the coarse globe
            let water_idx = materials::index_of(&self.mats, "water");
            let water_alb = self.mats[water_idx].albedo;

            let mut verts = std::mem::take(&mut self.cap_verts);
            {
                let sample = |dir: glam::DVec3| -> ([f32; 3], f64) {
                    let lat = dir.y.asin().to_degrees();
                    let lon = dir.z.atan2(dir.x).to_degrees();
                    let is_land = self
                        .landmask
                        .as_ref()
                        .map(|r| r.land_at(lat, lon))
                        .unwrap_or_else(|| crate::planet::earth_surface_material(dir) == "granite");
                    if is_land {
                        let biome = self.landcover.as_ref().map_or(1, |r| r.biome_at(lat, lon) as usize);
                        let mi = self.biome_mats.get(biome).copied().unwrap_or(water_idx);
                        let e = self
                            .elevation
                            .as_ref()
                            .map_or(0.0, |r| r.elevation_m_at(lat, lon, self.elev_range[0], self.elev_range[1]));
                        (self.mats[mi].albedo, e.max(0.0) * ds * exag + lift)
                    } else {
                        (water_alb, lift)
                    }
                };
                crate::terra::ground_cap::fill_ground_cap(
                    &mut verts, view.up, view.east, view.north, view.eye, r_disp, cap_angle, res, sample,
                );
            }
            self.queue.write_buffer(&self.cap_gpu.vertex_buf, 0, bytemuck::cast_slice(&verts));
            self.cap_verts = verts;
            // Camera-relative draw: identity model, eye at the ORIGIN (emissive.xyz = 0 → globe.wgsl's view = the
            // direction from the surface back to the eye). tint.a = the cross-fade alpha.
            write_space_uniform(
                &self.queue,
                &self.cap_uni,
                view.vp_rel,
                Mat4::IDENTITY,
                sun_light,
                [1.0, 1.0, 1.0, cap_fade],
                [0.0, 0.0, 0.0, 0.8],
            );
        }

    }
}

#[cfg(test)]
mod tests {
    use crate::{body, gravity, materials, mesher, world};

    #[test]
    fn metres_per_pixel_matches_frustum_geometry() {
        // The visible slice of the world at the focal plane is 2·d·tan(fov/2) metres tall; one pixel
        // is that divided by the viewport height. Check the closed form and its scaling behaviour —
        // this is the pure math behind the HUD scale bar (same on terrain and in space).
        let fov = 0.9_f64;
        let vh = 1000.0_f64;
        let d = 100.0_f64;
        let mpp = crate::metres_per_pixel_at(d, fov, vh);
        let expected = 2.0 * d * (fov * 0.5).tan() / vh;
        assert!((mpp - expected).abs() < 1e-12, "closed form: {mpp} vs {expected}");
        // Linear in distance: twice as far away ⇒ twice the metres per pixel (zooming out coarsens).
        assert!(
            (crate::metres_per_pixel_at(2.0 * d, fov, vh) - 2.0 * mpp).abs() < 1e-12,
            "scale must be linear in focal distance"
        );
        // Inverse in viewport height: a taller viewport packs the same slice into more pixels.
        assert!(
            (crate::metres_per_pixel_at(d, fov, 2.0 * vh) - 0.5 * mpp).abs() < 1e-12,
            "scale must be inverse in viewport height"
        );
        // Degenerate viewport is guarded (no divide-by-zero into the HUD).
        assert_eq!(crate::metres_per_pixel_at(d, fov, 0.0), 0.0);
    }

    #[test]
    fn material_database_loads() {
        let mats = materials::load();
        assert_eq!(mats.len(), 24, "seed database should have 24 materials");
        // `rubber` (2026-07-19) — the tyre compound; the go-kart's grip, damping and hysteresis all live
        // in this datum. Deliberately carries NO `thermal` block: rubber does not melt, it pyrolyses, so
        // melt_point/latent_fusion have no honest value and the schema's optional thermal is how it says
        // "not characterised" (oak, concrete and ice do the same). `damage.rs` then returns Fractured
        // rather than ever claiming melt — the guard tested at damage.rs:190.
        assert!(mats[materials::index_of(&mats, "rubber")].thermal.is_none());
        for id in ["granite", "dirt", "grass", "iron", "nickel", "rubber"] {
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
