//! **The one WGSL↔Rust layout checker** (test-only support for `docs/47`).
//!
//! Nothing in the Rust toolchain checks a `#[repr(C)]` mirror against the WGSL that reinterprets its
//! bytes — rustc never sees the shader. So drift fails SILENTLY: no error, no crash, just wrong physics.
//! `gpu_layout.rs` documents the case where that already bit (`drag_cd` arriving as 0.0).
//!
//! These helpers were written inside `gpu_layout`'s own `mod tests`. They live here now because a second
//! shader needed them (`gpu_sph`), and copying a parser is how one question acquires two answers — the
//! charter violation (`docs/46`, Law 2). One parser, pinned to every shader that has a Rust mirror.
//!
//! Test-only on purpose: this compiles no bytes into the engine, it only guards them.

/// Field names + coarse types of a `struct <name>` block in WGSL, in declaration order.
///
/// Splits on COMMAS rather than lines: the shaders declare `_p1: f32, _p2: f32,` on one line, and a
/// line-based parser silently drops the second — which would leave a real trailing field unchecked, in
/// exactly the padding region a struct grows into.
pub(crate) fn wgsl_typed(src: &str, name: &str) -> Vec<(String, &'static str)> {
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

/// WGSL byte offsets for a field list, applying WGSL's rules: `vec3<f32>` occupies 12 bytes but aligns to
/// 16, scalars are 4. This is what the GPU will actually do with the bytes.
pub(crate) fn wgsl_offsets(fields: &[(String, &'static str)]) -> Vec<(String, usize)> {
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

/// Byte offset of each named field, tying an assertion to the REAL Rust layout rather than to a literal
/// list. Without this a test only pins the shader to a hardcoded array and never reads the struct at all —
/// reorder two Rust fields and it still passes. That mistake was live in the first version of `gpu_layout`
/// and in `gpu-verify`'s equivalent, and it is the worst kind: a guard that reports green while the layout
/// drifts.
macro_rules! offsets {
    ($t:ty, $($f:ident),+ $(,)?) => {
        vec![$((stringify!($f).to_string(), std::mem::offset_of!($t, $f))),+]
    };
}
pub(crate) use offsets;
