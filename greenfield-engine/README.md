# greenfield-engine

> An open-source, browser-based game engine with **real Newtonian physics at its core**.

Most game engines treat the world as textured surfaces that objects bounce off. `greenfield-engine`
treats the world as **matter** — aggregates of particles with mass and density — and lets behavior
*emerge* from physical properties instead of being hand-authored.

## The idea, in four pillars

1. **Matter = aggregates of particles with mass & density.** A 200 km sphere of rock is rock all
   the way down, not a shell with a texture.
2. **Behavior emerges from density & material parameters.** Rock is dense, stiff, and hard to
   break; dirt separates in chunks more easily; grass is low-density and fragile — all from the
   *same* rules with different parameters, not per-material special cases.
3. **Destructible all the way down.** Enough force breaks a segment off; the hole persists. Layer
   10 m of dirt and a skin of grass on top of the rock and each behaves according to its material.
4. **Real self-gravity from aggregate mass.** The world's own summed mass produces a gravitational
   field. A 5 kg mass above it accelerates per `F = ma`, with `g = G·M/r²`. Light is handled
   conventionally (like a normal engine) for now.

The novel bit is making **density the single source of truth** that simultaneously drives material
behavior, destruction, *and* gravity in one real-time browser loop. No existing engine fuses all
four pillars — see [`docs/01-prior-art-existing-engines.md`](docs/01-prior-art-existing-engines.md).

## Architecture (short version)

Everything performance-critical is **one Rust crate compiled to WASM**, sharing a single
[`wgpu`](https://github.com/gfx-rs/wgpu) WebGPU device so simulation buffers *are* the render
buffers (zero-copy). TypeScript is only the thin host: canvas, input, and UI.

```
web/ (TypeScript + Vite)  ──►  Rust → WASM (single wgpu device)
                                ├─ matter-core : chunked sparse voxel store {material, density}
                                ├─ materials   : physical params (density, stiffness, cohesion…)
                                ├─ mpm         : MLS-MPM solver (WGSL compute) — the "matter"
                                ├─ gravity     : Barnes-Hut self-gravity field
                                ├─ rapier      : rigid bodies (chunks, debris, player, dropped mass)
                                └─ render      : custom wgpu renderer + surface meshing + shading
```

Built on permissively-licensed OSS: `wgpu`, `rapier3d`, `glam`, `wasm-bindgen`, surface-nets
meshing. See [`docs/02-oss-building-blocks.md`](docs/02-oss-building-blocks.md) for the full
survey and rationale.

## Status

🚧 **Pre-alpha.** Building the first vertical slice: a small layered chunk (rock / dirt / grass)
you can dig and blast, with local gravity that pulls a 5 kg mass down per `F = ma`. Roadmap phases:

- **Phase 0** — Scaffold + `wgpu` first pixel in the browser.
- **Phase 1** — Voxel matter store, layered world, surface-nets meshing.
- **Phase 2** — Self-gravity + a Rapier 5 kg sphere that falls per `F = ma`.
- **Phase 3** — MLS-MPM: dig, fracture, chunks fall; materials differ by density alone.
- **Phase 4** — Emergent procedural texture, tools, benchmarks.

## Building

Requires the Rust toolchain (with the `wasm32-unknown-unknown` target), `wasm-pack`, and Node.js.

```bash
# once toolchain is set up:
cd web
npm install
npm run dev        # builds the Rust/WASM core and serves the host app
```

## Deploying

The public demo is live at **[integrity.bothead.net](https://integrity.bothead.net)** — a static
build served by nginx (`:8080`) behind a Cloudflare tunnel. One command builds and publishes it:

```bash
./scripts/deploy.sh   # npm run build → rsync web/dist → /var/www/integrity
```

Full pipeline, serving stack, and one-time wiring: [`docs/29-deployment.md`](docs/29-deployment.md).
For on-device LAN testing without deploying, use [`scripts/dev-lan.sh`](scripts/dev-lan.sh).

## License

[MIT](LICENSE-MIT). Part of the [BotheadStudios](https://github.com/robinmack/BotheadStudios)
monorepo.
