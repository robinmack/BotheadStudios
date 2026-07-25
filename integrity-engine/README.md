# Integrity engine

> An open-source, browser-based game engine with **real Newtonian physics at its core**.

Most game engines treat the world as textured surfaces that objects bounce off. `Integrity engine`
treats the world as **matter** — aggregates of particles with mass and density — and lets behavior
*emerge* from physical properties instead of being hand-authored.

## The goal, in four pillars

These four are the destination the engine is built toward, not a status report. What runs today
is under [Status](#status).

1. **Matter = aggregates of particles with mass & density.** A 200 km sphere of rock is rock all
   the way down, not a shell with a texture.
2. **Behavior emerges from density & material parameters.** Rock is dense, stiff, and hard to
   break; dirt separates in chunks more easily; grass is low-density and fragile — all from the
   *same* rules with different parameters, not per-material special cases.
3. **Destructible all the way down.** Enough force breaks a segment off; the hole persists. The
   ground scene does this today: impacts excavate real craters in regolith, and the craters stay.
4. **Real self-gravity from aggregate mass.** The world's own summed mass produces a gravitational
   field. A 5 kg mass above it accelerates per `F = ma`, with `g = G·M/r²`. Light is handled
   conventionally (like a normal engine) for now.

The novel bit is making **density the single source of truth** that simultaneously drives material
behavior, destruction, *and* gravity in one real-time browser loop. We know of no engine that fuses
all four (the survey is [`docs/01`](docs/01-prior-art-existing-engines.md)), but the pitch does not
rest on that claim. It rests on a discipline you can audit in five minutes: **this engine treats
fudges as bugs.** Every known deviation from the one-physics promise sits on a public conformance
ledger ([`docs/46`](docs/46-one-physics-charter.md)) with the test that closes it. `laws.rs` fails
the build on the machine-detectable class. Accelerated code is pinned to brute-force references, so
speed never changes an answer. Negative results ship in the [JOURNAL](JOURNAL.md).

## Architecture (short version)

Everything performance-critical is **one Rust crate compiled to WASM**, sharing a single
[`wgpu`](https://github.com/gfx-rs/wgpu) WebGPU device so simulation buffers *are* the render
buffers (zero-copy). TypeScript is only the thin host: canvas, input, and UI.

```
web/ (TypeScript + Vite)  ──►  Rust → WASM (single wgpu device)
                                ├─ matter / aggregate / granular : particle matter, one contact law
                                ├─ materials : sourced physical params (data/materials.json)
                                ├─ eos       : Tillotson equation of state (Benz & Asphaug 1999)
                                ├─ gravity   : Barnes-Hut self-gravity (CPU) + GPU kernels (WGSL)
                                ├─ gpu_sph   : SPH giant-impact solver (WGSL compute)
                                └─ render    : custom wgpu renderer + surface meshing + sky
```

Built on permissively-licensed OSS: `wgpu`, `glam`, `wasm-bindgen`, `fast-surface-nets`
meshing. See [`docs/02-oss-building-blocks.md`](docs/02-oss-building-blocks.md) for the full
survey and rationale.

## Status

🚧 **Pre-1.0, under heavy development.** What runs today, in a WebGPU-capable browser:

- **Birth of the Moon** (`/birth.html`): proto-Earth and Theia collide; GPU SPH with a Tillotson
  equation of state relaxes the bodies, runs the impact, and accretes a proto-lunar disk.
- **Space / Two Moons** (`/orbit.html`, `/twomoons.html`): real orbital mechanics for
  engine-owned bodies.
- **Ground** (`/ground.html`): a regolith surface with a free-fly camera; impacts excavate
  real craters.
- **Earth** (`/terra.html`): a worlds-as-data globe built from sourced elevation and landcover
  rasters.

The charter is [`docs/00-laws-of-integrity.md`](docs/00-laws-of-integrity.md): one physics at
every scale, no fudge. Current work is the law-conformance burn-down
([`docs/57`](docs/57-law-conformance-burndown.md)) on the way to the north-star demo
([`docs/23`](docs/23-everything-is-matter-north-star.md)): de-orbit the Moon onto a metal ball
and zoom from the celestial view down to find it destroyed, with no line of code that says
destroy.

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
