# Material modules — extensible, at the scale of thousands

> Design note. A material is not just a row of properties; it's a self-contained **module** that can
> bundle properties, shaders, sounds, and behavior. We expect **thousands** of them over time, so the
> system is designed for lazy loading and shader/asset deduplication from the start.
> Status: **design** (not yet implemented). Extends [`04-materials-model.md`](04-materials-model.md).

## What a material module is

A versioned, self-contained package describing one material:

```
core:granite/                     # namespaced id  (namespace:name)
├── module.json                   # manifest: id, version, category, deps, license, author
├── properties.json               # the physical properties (mechanical + optical + thermal), cited
├── shaders/                      # OPTIONAL custom WGSL (surface/procedural texture)
│   └── surface.wgsl
├── sounds/                       # OPTIONAL, event-keyed (impact, fracture, scrape, footstep)
│   ├── impact.ogg
│   └── fracture.ogg
├── behavior.wasm | behavior.json # OPTIONAL custom logic / state transitions (water↔ice, burn)
└── assets/                       # OPTIONAL icons, reference images
```

Most materials need only `module.json` + `properties.json` — they render and sound via shared,
property-driven defaults (below). Custom shaders/sounds/behavior are opt-in for special materials.

## The scaling problem, and how we avoid it

Thousands of modules must NOT mean thousands of compiled shaders, loaded sounds, or resident
records. Three mechanisms:

1. **Über-shader by default.** There is ONE parameterized surface shader driven by a material's
   optical properties (albedo, roughness, metallic, translucency, grain, color_variance). A module
   with no custom shader uses it — so 1,000 property-only materials compile **zero** extra shaders.
   Only modules that ship `shaders/surface.wgsl` get their own pipeline (and those are deduplicated
   by content hash).
2. **Default event sounds.** A small shared sound bank is synthesized/selected from properties
   (a dense-hard material rings; a soft granular one thuds). Modules override per-event only when
   they ship custom `sounds/`.
3. **Lazy, streamed loading.** A cheap global **registry index** (id + manifest only) is always
   resident. A module's heavy parts (shaders/sounds/behavior/assets) load **on demand** — when a
   material actually appears in the active region — and unload when it leaves. Worlds reference a
   small working set at any moment even if the catalog is huge.

## Registry & addressing

- **Namespaced ids:** `core:granite`, `community:alice/exotic-alloy`. `core:*` ships with the engine;
  others come from a module repository/CDN.
- **Registry index:** a manifest list (local bundled core + fetched remote catalogs). Loaded at
  startup; full modules resolved lazily by id.
- **Versioning:** each module is independently semver'd; a world pins the module versions it uses
  (consistent with the dogfooding versioning policy, `docs/03-versioning.md`).
- **Dependencies:** a module may depend on others (e.g. an alloy referencing base metals) via
  `deps` in the manifest.

## Relationship to the Postgres → JSON pipeline

- **Postgres** (`05-data-pipeline.md`) stays the source of truth for **base properties** across the
  whole catalog.
- **Base modules are generated** from that catalog: each material row → a `core:<id>` module with
  `properties.json` and no custom assets (uses the über-shader + default sounds). This is how the
  first ~thousands of materials exist cheaply without hand-authoring each.
- **Rich modules** (custom shaders/sounds/behavior) are hand- or tool-authored on top, and their
  non-property assets live with the module, not in the relational DB (Postgres keeps a pointer/blob
  reference, not the WGSL/audio bytes inline).
- The web-based creation system edits both: property rows in Postgres, and module assets in storage.

## Open questions

1. **Behavior format** — sandboxed `behavior.wasm` (powerful, needs a safe host API) vs. a
   declarative `behavior.json` state-machine (safer, less flexible). Likely start declarative.
2. **Module distribution** — a registry/CDN akin to a package index; signing/trust for community
   modules.
3. **Über-shader capability ceiling** — how far property-driven shading goes before a material
   genuinely needs custom WGSL (moving water, lava flow, foliage).
4. **Hot-loading budget** — memory/GPU caps for the resident module working set, and eviction policy.
