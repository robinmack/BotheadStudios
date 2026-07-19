//! docs/43 — worlds-as-data + the `Terra` planet/terrain scene. Pure logic lives here (the world schema, and
//! later the raster sampler, globe mesh, and fly camera — all compile native + wasm). The wasm-bindgen `Terra`
//! scene struct itself lives in `mod app` (lib.rs) so it can reuse that module's render helpers directly.

pub mod raster;
pub mod world_def;
