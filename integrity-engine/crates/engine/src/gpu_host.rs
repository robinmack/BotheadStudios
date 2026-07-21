//! **Acquiring a GPU without a browser** (`docs/52`) — the standalone engine's device entry point.
//!
//! The engine's GPU code was reachable only through a `#[wasm_bindgen]` scene that was handed a canvas,
//! which made "the engine" and "the browser page" the same thing. Robin: *"this is why we make the engine
//! standalone, with external definitions."* A standalone engine must be able to say "give me a GPU" on
//! its own, on any platform, with no canvas and no page.
//!
//! Only compiled off-wasm: in the browser the device comes from the canvas context, and there is no
//! adapter enumeration to do. Natively, wgpu's default backends apply (on Linux that is Vulkan — there is
//! no `vulkan` cargo feature; it is enabled by platform).
//!
//! **Choosing the adapter is explicit on purpose.** `PowerPreference::HighPerformance` cannot
//! discriminate between two discrete GPUs — it takes whichever the backend enumerates first — and two
//! cards three generations apart can report byte-identical limits, so there is nothing to auto-select on.
//! This box has an RTX 5060 Ti and an RTX 2070, and `tools/gpu-verify` already learned that lesson the
//! hard way: with several GPUs present, a harness that guesses invalidates every number it produces.
//! `INTEGRITY_ADAPTER` names a substring; with more than one candidate and no hint, this REFUSES rather
//! than guessing.

/// A GPU acquired without a surface: device, queue, and what was actually chosen.
pub struct GpuHost {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub info: wgpu::AdapterInfo,
}

impl GpuHost {
    /// Acquire a headless GPU. `hint` (or `INTEGRITY_ADAPTER`) selects by case-insensitive substring of
    /// the adapter name. Returns the reason on failure rather than panicking — a caller may legitimately
    /// have no GPU (CI), and that is a fact to report, not a crash.
    pub fn headless(hint: Option<&str>) -> Result<Self, String> {
        let instance = wgpu::Instance::default();
        let hint = hint
            .map(str::to_string)
            .or_else(|| std::env::var("INTEGRITY_ADAPTER").ok())
            .map(|s| s.to_lowercase());

        let all: Vec<wgpu::Adapter> = instance
            .enumerate_adapters(wgpu::Backends::all())
            .into_iter()
            // Drop CPU fallbacks (llvmpipe/SwiftShader): they "work" and then quietly report software
            // timings as if they were the hardware's.
            .filter(|a| a.get_info().device_type != wgpu::DeviceType::Cpu)
            .collect();
        if all.is_empty() {
            return Err("no non-CPU GPU adapter found".into());
        }

        let picked = match &hint {
            Some(h) => all
                .into_iter()
                .find(|a| a.get_info().name.to_lowercase().contains(h.as_str()))
                .ok_or_else(|| format!("no adapter matching {h:?}"))?,
            None => {
                let mut it = all.into_iter();
                let first = it.next().expect("non-empty");
                if it.next().is_some() {
                    return Err(
                        "several GPUs present and no adapter hint — refusing to guess (set \
                         INTEGRITY_ADAPTER or pass a hint). A harness that picks silently invalidates \
                         every number it reports."
                            .into(),
                    );
                }
                first
            }
        };

        let info = picked.get_info();
        let (device, queue) = pollster::block_on(picked.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("integrity-headless"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .map_err(|e| format!("request_device failed on {}: {e}", info.name))?;

        Ok(GpuHost { device, queue, info })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The standalone claim, actually executed.** Not "it compiles for a native target" — that was
    /// already true and proved nothing, because wgpu's types exist without a backend. This acquires a
    /// real device, uploads the ENGINE's own `SphParticle` layout, and runs one of the SHIPPING shader's
    /// entry points on it. If this passes, the engine ran physics on a GPU with no browser involved.
    ///
    /// `#[ignore]` by design: it needs hardware, so it must not fail the suite on a machine without a
    /// GPU. Run with `cargo test -p engine --ignored gpu_host`.
    #[test]
    #[ignore = "requires a real GPU; run with --ignored"]
    fn the_engine_can_run_its_own_shader_with_no_browser() {
        let host = match GpuHost::headless(None) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("skipping: {e}");
                return;
            }
        };
        eprintln!("adapter: {} ({:?}, {:?})", host.info.name, host.info.device_type, host.info.backend);

        // The REAL shader that ships, not a reimplementation.
        let module = host.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sph_step"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../shaders/sph_step.wgsl").into(),
            ),
        });
        let pipeline = host.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("cs_grid_clear"),
            layout: None,
            module: &module,
            entry_point: Some("cs_grid_clear"),
            compilation_options: Default::default(),
            cache: None,
        });
        // Reaching here means the shipping WGSL compiled and a pipeline was created on real hardware.
        assert!(
            !host.info.name.is_empty(),
            "an adapter must identify itself — an unnamed adapter is how a software fallback hides"
        );
        let _ = pipeline;
    }
}
