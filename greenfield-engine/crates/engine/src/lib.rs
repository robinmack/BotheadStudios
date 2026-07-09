//! greenfield-engine core.
//!
//! Phase 1: generate a layered voxel world (rock / dirt / grass) from the cited material database,
//! mesh it, and render it with an orbit camera and directional lighting on a browser canvas via
//! `wgpu`. Densities come straight from `data/materials.json` (the single source of truth); in
//! later phases the same per-voxel density drives self-gravity and the MLS-MPM matter solver.
//!
//! ## Structure & testing
//! The pure simulation logic — the material model, voxel store, and mesher — lives in modules that
//! compile and unit-test **natively** (`cargo test`). Only the rendering/host layer (`wgpu` +
//! `wasm-bindgen`) is gated to the wasm target. TDD is canonical for this project: keep testable
//! logic out of the wasm-only path.

// On native builds the sim modules' only non-test consumer (the wasm renderer) is compiled out, so
// their API reads as "unused" there. The wasm build still enforces dead-code detection, and tests
// exercise them. (A future `matter-core` crate split, per docs, removes the need for this.)
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

mod materials;
mod mesher;
mod world;

#[cfg(target_arch = "wasm32")]
pub use app::Engine;

/// The rendering + browser-host layer. wasm/`wgpu`-only; excluded from native builds and tests.
#[cfg(target_arch = "wasm32")]
mod app {
    use crate::mesher::{self, Vertex};
    use crate::{materials, world};
    use wasm_bindgen::prelude::*;
    use web_sys::HtmlCanvasElement;

    const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Uniforms {
        view_proj: [[f32; 4]; 4],
        light_dir: [f32; 4],
        camera_pos: [f32; 4],
    }

    /// Orbit camera state, driven from JS.
    struct Camera {
        yaw: f32,
        pitch: f32,
        zoom: f32,
        base_distance: f32,
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
        vertex_buf: wgpu::Buffer,
        index_buf: wgpu::Buffer,
        index_count: u32,
        uniform_buf: wgpu::Buffer,
        bind_group: wgpu::BindGroup,

        camera: Camera,
    }

    #[wasm_bindgen]
    impl Engine {
        /// Initialize the engine against an existing `<canvas>`: acquire the GPU, build the world,
        /// and upload its mesh. Returns a `Promise` in JS.
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

            // --- Build the world and its mesh ---
            let mats = materials::load();
            let world = world::generate(&mats);
            let mesh = mesher::build(&world, &mats);
            log::info!(
                "greenfield-engine: world {}x{}x{}, {} vertices / {} triangles",
                world.w,
                world.h,
                world.d,
                mesh.vertices.len(),
                mesh.indices.len() / 3
            );

            let vertex_buf = make_buffer(
                &device,
                "vertices",
                bytemuck::cast_slice(&mesh.vertices),
                wgpu::BufferUsages::VERTEX,
            );
            let index_buf = make_buffer(
                &device,
                "indices",
                bytemuck::cast_slice(&mesh.indices),
                wgpu::BufferUsages::INDEX,
            );
            let index_count = mesh.indices.len() as u32;

            // --- Uniforms + bind group ---
            let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("uniforms"),
                size: std::mem::size_of::<Uniforms>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("uniform-layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("uniform-bind-group"),
                layout: &bind_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buf.as_entire_binding(),
                }],
            });

            // --- Pipeline ---
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("world-shader"),
                source: wgpu::ShaderSource::Wgsl(
                    include_str!("../../../shaders/world.wgsl").into(),
                ),
            });
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("pipeline-layout"),
                bind_group_layouts: &[&bind_layout],
                push_constant_ranges: &[],
            });
            const ATTRS: [wgpu::VertexAttribute; 3] =
                wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x3];
            let vertex_layout = wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &ATTRS,
            };
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
                        format: config.format,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview: None,
                cache: None,
            });

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
                vertex_buf,
                index_buf,
                index_count,
                uniform_buf,
                bind_group,
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

        /// Render one frame.
        pub fn render(&mut self) -> Result<(), JsValue> {
            self.update_uniforms();

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
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.set_vertex_buffer(0, self.vertex_buf.slice(..));
                pass.set_index_buffer(self.index_buf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..self.index_count, 0, 0..1);
            }
            self.queue.submit(std::iter::once(encoder.finish()));
            output.present();
            Ok(())
        }

        fn update_uniforms(&self) {
            use glam::{Mat4, Vec3};
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
            let view_proj = (proj * view).to_cols_array_2d();

            let light = Vec3::new(0.45, 0.9, 0.4).normalize();
            let uniforms = Uniforms {
                view_proj,
                light_dir: [light.x, light.y, light.z, 0.0],
                camera_pos: [eye.x, eye.y, eye.z, 1.0],
            };
            self.queue
                .write_buffer(&self.uniform_buf, 0, bytemuck::bytes_of(&uniforms));
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

    /// Create a GPU buffer and fill it at creation time (no queue needed).
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
}

#[cfg(test)]
mod tests {
    use crate::{materials, mesher, world};

    #[test]
    fn material_database_loads() {
        let mats = materials::load();
        assert_eq!(mats.len(), 19, "seed database should have 19 materials");
        for id in ["granite", "dirt", "grass"] {
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

        // Scan the central column top-to-bottom: expect grass, then a dirt band, then rock, and
        // solid all the way down to y=0 ("rock all the way down").
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
}
