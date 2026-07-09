//! greenfield-engine core.
//!
//! Phase 0: bring up a `wgpu` WebGPU device against a browser canvas and clear it every frame.
//! This proves the Rust -> WASM -> wgpu -> canvas pipeline end to end. Later phases add the
//! voxel matter store, MLS-MPM solver, self-gravity, Rapier coupling, and real rendering — all
//! sharing this single `wgpu::Device`/`Queue` so simulation buffers *are* the render buffers.

use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

/// The engine handle exposed to JavaScript.
///
/// Construct with [`Engine::create`] (async — it awaits the GPU adapter/device), then drive
/// [`Engine::render`] from `requestAnimationFrame` and call [`Engine::resize`] on layout changes.
#[wasm_bindgen]
pub struct Engine {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    frame: u32,
}

#[wasm_bindgen]
impl Engine {
    /// Initialize the engine against an existing `<canvas>` element.
    ///
    /// Returns a `Promise` in JS (the fn is async). Rejects with a descriptive string if WebGPU
    /// is unavailable or adapter/device acquisition fails.
    pub async fn create(canvas: HtmlCanvasElement) -> Result<Engine, JsValue> {
        // Route Rust panics to the browser console with a readable stack, and wire up `log`.
        console_error_panic_hook::set_once();
        let _ = console_log::init_with_level(log::Level::Info);

        let width = canvas.width().max(1);
        let height = canvas.height().max(1);

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU,
            ..Default::default()
        });

        // Owned canvas -> the surface borrows nothing, so it is `Surface<'static>`.
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
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("greenfield-device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults(),
                ..Default::default()
            },
            None, // trace path (native only)
            )
            .await
            .map_err(|e| JsValue::from_str(&format!("request_device failed: {e}")))?;

        let caps = surface.get_capabilities(&adapter);
        // Prefer an sRGB format so our colors are displayed correctly.
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

        log::info!("greenfield-engine: wgpu device ready ({width}x{height}, {format:?})");

        Ok(Engine {
            surface,
            device,
            queue,
            config,
            frame: 0,
        })
    }

    /// Reconfigure the surface when the canvas backing size changes.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    /// Render one frame. Phase 0: a slowly pulsing clear color — proof the loop is live.
    pub fn render(&mut self) -> Result<(), JsValue> {
        self.frame = self.frame.wrapping_add(1);

        // A gentle pulse so we can see the loop is actually running frame to frame.
        let t = f64::from(self.frame) * 0.015;
        let pulse = 0.5 + 0.5 * t.sin();
        let clear = wgpu::Color {
            r: 0.03 + 0.05 * pulse,
            g: 0.05 + 0.08 * pulse,
            b: 0.10 + 0.15 * pulse,
            a: 1.0,
        };

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
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("clear-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }
}
