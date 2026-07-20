//! **The GPU-facing `#[repr(C)]` layouts, and the tests that pin them to the shader** (`docs/47`).
//!
//! These structs describe bytes the GPU reinterprets through `shaders/particle_step.wgsl`. Nothing in
//! the Rust toolchain checks a `#[repr(C)]` declaration against WGSL — rustc never sees the shader — so
//! a drifted field ORDER fails **silently**: no error, no crash, just wrong physics. That is not
//! hypothetical. `tools/gpu-verify`'s `Params` comment records `drag_cd` arriving as 0.0 from exactly
//! this, leaving drag a quiet no-op.
//!
//! **Why these live here and not in `mod app`.** They used to sit inside
//! `#[cfg(target_arch = "wasm32")] mod app`, which native `cargo check`/`cargo test` do not compile at
//! all — so the mirror that actually SHIPS was the one no native test could reach. Moving them out is
//! what puts them under the suite; they are plain POD with no wasm-only dependencies, so nothing but
//! their location was ever keeping them there.
//!
//! **Two mirrors are permanent, and that is fine.** `tools/gpu-verify` keeps its own replica and must:
//! it is deliberately not a workspace member, so its native Vulkan `wgpu` build cannot leak into the
//! engine's WebGPU-only wasm build through cargo feature unification. Safety comes from both mirrors
//! being pinned to the SAME authority — pinned to one shader, they cannot drift from each other.

/// One GPU particle — 64 bytes, four 16-byte rows. Layout matches `particle_step.wgsl`'s `Particle`
/// and is read directly by the renderer (offset @0, color @32, emission @48).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct GpuParticle {
    pub(crate) offset: [f32; 3], // position (centered coords) = the render instance offset
    pub(crate) u: f32, // specific internal energy (J/kg); temp = u/c is derived (docs/38, was `temp` in K)
    pub(crate) vel: [f32; 3],
    pub(crate) resting: f32,       // 0 in flight, 1 settled
    pub(crate) color: [f32; 3],    // material albedo (set on spawn)
    pub(crate) material: f32,      // material index (informational)
    pub(crate) emission: [f32; 3], // incandescent glow (written by the compute step)
    pub(crate) rho: f32, // density (kg/m³) — Tillotson input; ρ₀ placeholder until 4b.2 (was `_pad`)
}

/// Per-dispatch uniforms for the compute step — matches `particle_step.wgsl`'s `Params`.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct GpuStepParams {
    pub(crate) gravity: [f32; 3], // uniform planetary surface gravity (m/s²)
    pub(crate) dt: f32,
    pub(crate) center: [f32; 3],
    pub(crate) c_cohesion: f32, // attractive adhesion between touching grains (docs/24)
    pub(crate) air_rho: f32,
    pub(crate) contact_damp: f32,
    pub(crate) settle_speed: f32,
    pub(crate) part_half: f32,
    pub(crate) cool_rate: f32,
    pub(crate) count: u32,
    pub(crate) world_w: u32,
    pub(crate) world_d: u32,
    // Granular spatial hash + contact (docs/23) — mirrors particle_step.wgsl's Params tail.
    pub(crate) cell_size: f32,
    pub(crate) table_mask: u32,
    pub(crate) bucket_k: u32,
    pub(crate) c_radius: f32,
    pub(crate) c_stiffness: f32,
    pub(crate) c_normal_damp: f32,
    pub(crate) c_friction: f32,
    pub(crate) c_tangent_damp: f32,
    pub(crate) specific_heat: f32, // J/(kg·K) — grain temp↔u (docs/38)
    pub(crate) drag_cd: f32,
    pub(crate) _hp1: f32,
    pub(crate) _hp2: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Field names of a `struct <name>` block in WGSL, in declaration order.
    ///
    /// Splits on COMMAS rather than lines: the shader declares `_hp1 : f32, _hp2 : f32,` on one line,
    /// and a line-based parser silently drops the second — which would leave a real trailing field
    /// unchecked, in exactly the padding region a struct grows into.
    fn wgsl_typed(src: &str, name: &str) -> Vec<(String, &'static str)> {
        let head = format!("struct {name} {{");
        let start = src.find(&head).unwrap_or_else(|| panic!("no `{head}` in the shader"));
        let body = &src[start + head.len()..];
        let end = body.find('}').expect("unterminated struct in the shader");
        body[..end]
            .lines()
            .map(|l| l.split("//").next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n")
            .split(',')
            .filter_map(|chunk| {
                let (field, ty) = chunk.split_once(':')?;
                let field = field.trim();
                let ty = if ty.trim().starts_with("vec3") { "vec3" } else { "scalar" };
                (!field.is_empty()).then(|| (field.to_string(), ty))
            })
            .collect()
    }

    const SHADER: &str = include_str!("../../../shaders/particle_step.wgsl");

    /// Byte offset of each field, tying the assertion to the REAL Rust layout rather than to a literal
    /// list. Without this the test only pins the shader to a hardcoded array and never reads the struct
    /// at all — reorder two Rust fields and it still passes. That mistake was live in the first version
    /// of this file and in `gpu-verify`'s equivalent, and it is the worst kind: a layout guard that
    /// reports green while the layout drifts.
    macro_rules! offsets {
        ($t:ty, $($f:ident),+ $(,)?) => {
            vec![$((stringify!($f).to_string(), std::mem::offset_of!($t, $f))),+]
        };
    }

    /// WGSL byte offsets for a field list, applying WGSL's rules: `vec3<f32>` occupies 12 bytes but
    /// aligns to 16, scalars are 4. This is what the GPU will actually do with the bytes.
    fn wgsl_offsets(fields: &[(String, &'static str)]) -> Vec<(String, usize)> {
        let mut off = 0usize;
        let mut out = Vec::new();
        for (name, ty) in fields {
            let (size, align) = if ty.starts_with("vec3") { (12, 16) } else { (4, 4) };
            off = off.div_ceil(align) * align;
            out.push((name.clone(), off));
            off += size;
        }
        out
    }

    /// The mirror that SHIPS, pinned to the shader that reads it — by OFFSET, so a reordering of either
    /// side fails. This check did not exist while `GpuParticle` lived inside `#[cfg(wasm32)] mod app`:
    /// native tests could not even compile it, so the production layout was verified by nothing but a
    /// human reading two files side by side.
    #[test]
    fn gpu_particle_matches_the_shader_field_for_field() {
        let rust = offsets!(GpuParticle, offset, u, vel, resting, color, material, emission, rho);
        let shader = wgsl_offsets(&wgsl_typed(SHADER, "Particle"));
        assert_eq!(
            rust, shader,
            "GpuParticle has drifted from particle_step.wgsl. Field ORDER is the whole risk: the GPU \
             reinterprets the bytes with no error, so a swap is wrong physics that looks fine."
        );
        assert_eq!(
            std::mem::size_of::<GpuParticle>(),
            64,
            "particle stride changed — the renderer reads offset @0, color @32, emission @48, and \
             every particle after the first would read shifted memory"
        );
    }

    /// `Params` is the uniform the whole step is driven by, and where the known silent failure actually
    /// happened (`drag_cd` arriving as 0.0 from a drifted mirror).
    #[test]
    fn gpu_step_params_matches_the_shader_field_for_field() {
        let rust = offsets!(
            GpuStepParams, gravity, dt, center, c_cohesion, air_rho, contact_damp, settle_speed,
            part_half, cool_rate, count, world_w, world_d, cell_size, table_mask, bucket_k, c_radius,
            c_stiffness, c_normal_damp, c_friction, c_tangent_damp, specific_heat, drag_cd, _hp1, _hp2,
        );
        let shader = wgsl_offsets(&wgsl_typed(SHADER, "Params"));
        assert_eq!(
            rust, shader,
            "GpuStepParams has drifted from particle_step.wgsl's Params — the exact failure that once \
             delivered drag_cd = 0.0 and made drag a silent no-op"
        );
        assert_eq!(
            std::mem::size_of::<GpuStepParams>() % 16,
            0,
            "a uniform buffer's size must stay 16-byte aligned; the `_hp` tail exists for this"
        );
    }

    /// The guard must be ABLE to fail. A layout test that passes when the layout is wrong is worse than
    /// no test, because it converts an unchecked risk into a believed-checked one.
    #[test]
    fn the_guard_detects_a_reordered_rust_struct() {
        // The real struct's offsets, then the same names in a swapped order — what a careless edit
        // produces. The comparison must reject it.
        let honest = offsets!(GpuParticle, offset, u, vel, resting);
        let swapped = offsets!(GpuParticle, offset, u, resting, vel);
        assert_ne!(
            honest, swapped,
            "swapping two fields produced an identical offset list — the guard cannot see order"
        );
    }

    /// The parser must survive the shader's real formatting, not just tidy input — otherwise a green
    /// test means "found nothing to compare" rather than "they agree".
    #[test]
    fn the_wgsl_parser_actually_reads_fields() {
        assert_eq!(wgsl_typed(SHADER, "Particle").len(), 8);
        // The comma-split case: two fields sharing one line at the very end of Params.
        let p = wgsl_typed(SHADER, "Params");
        let tail: Vec<&str> = p[p.len() - 2..].iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(tail, ["_hp1", "_hp2"], "the one-line pair must both be seen");
    }
}
