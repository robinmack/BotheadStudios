# Material taxonomy, finishes, and object composition

> Design note. Materials are not a flat list — they form an **inheritance taxonomy**, are refined by
> orthogonal **finishes**, and are **composed with shapes into objects**. We start with primitive
> materials and grow sophistication over time. Status: **design**. Extends
> [`04-materials-model.md`](04-materials-model.md), [`05-data-pipeline.md`](05-data-pipeline.md),
> [`06-material-modules.md`](06-material-modules.md).

## 1. Material taxonomy (inheritance)

Materials form a tree of increasing specificity:

```
wood            (abstract base — generic properties + generic assets)
├── oak
├── mahogany
└── pine
metal
├── iron / steel
├── copper
└── aluminium
rock
├── granite
├── basalt
└── …
```

- A child **inherits everything** from its ancestors and stores **only its deltas** (the fields it
  overrides + any new assets). This keeps thousands of nodes DRY: `mahogany` need only record what
  makes it different from `wood`.
- **Abstract bases** (`wood`, `metal`) may be non-instantiable categories that supply defaults, or
  usable "generic" materials in their own right.

### Resolution rule (the crux)

To resolve **any** property or asset for a material, walk from the node **up to the root** and take
the **first defined value** (most-specific wins, with fallback):

```
resolve(mahogany, "density")      → mahogany.density ?? wood.density ?? default
resolve(mahogany, sound:"crunch") → mahogany.sounds.crunch ?? wood.sounds.crunch ?? generic
```

This is exactly the requested behavior: today `mahogany` splinters with the **generic wood** crunch
(inherited); when someone later adds a **mahogany crunch** sample to the `mahogany` node, the
resolver picks it up automatically — no engine change, no rewiring. Same for shaders (a mahogany
surface shader overrides the wood default) and any numeric property.

## 2. Finishes (orthogonal modifiers)

A **finish** is not a subclass — it's a composable modifier applied on top of a resolved material,
transforming its properties:

| Finish | Typical effect (property deltas) |
|---|---|
| `rough` | roughness↑, friction↑ |
| `smooth` | roughness↓ |
| `varnished` | roughness↓, sheen/specular↑, thin translucent layer, minor moisture resistance |
| `painted` | albedo→paint color, roughness set by paint, hides grain |
| `weathered` | albedo shift, roughness↑, strength↓, color_variance↑ |

- Finishes **stack** (`oak + varnished + weathered`) and apply as ordered deltas over the resolved
  property set at object-build time.
- Finishes can also swap/alter event assets (a varnished surface scrapes differently) via the same
  resolution chain, layered above the material.

## 3. Objects = shape + material(s) + size (+ finishes)

An object is **authored** declaratively and **compiled** into a simulated body:

```
Object "crate":
  shape:     <3D model / mesh / SDF>       # geometry only
  size:      1.2 m
  parts:                                    # one or many material assignments
    - region: all        material: wood/mahogany   finish: [varnished]
  # multi-material example — a hammer:
  # - region: handle     material: wood/oak
  # - region: head       material: metal/steel
```

**Compilation** (build-time) turns this into a concrete simulated object:

1. **Voxelize/fill** the shape at the requested size with each part's material → the object gets the
   correct **mass** (Σ density × voxel volume → gravity, inertia), and each voxel carries a
   **material identity**.
2. Material properties drive behavior: the crate is rigid until stress exceeds mahogany's
   thresholds, then **fractures like wood** (splinters, low ductility) into debris that *still carry
   the mahogany identity* — so the shards still look, sound, and behave like mahogany.
3. **Appearance** from resolved optical props + finishes (mahogany albedo/grain, varnish sheen).
4. **Sounds** from the resolved event assets (impact/crunch/scrape), most-specific-wins.

Multi-component objects simply assign different materials per region; the compiler welds them into
one body with per-voxel material identity, so a hammer's steel head and oak handle behave and sound
differently and can separate under stress.

## 4. Progressive sophistication (build order)

- **v0 (now):** primitive **flat** materials (the ~20 seed materials with cited properties).
- **v1:** introduce the **taxonomy** — reparent flat materials under bases (`oak`,`mahogany`,`pine`
  under `wood`), sparse overrides + fallback resolver.
- **v2:** **finishes** as stackable property/asset modifiers.
- **v3:** **object composition** — shape + material(s) + size + finishes → compiled bodies;
  multi-material objects; debris that retains material identity.
- Throughout: assets (sounds/shaders) are added incrementally and **automatically supersede**
  inherited fallbacks via the resolution chain.

## 5. Data-model implications

- **Postgres:** `materials` gains `parent_id TEXT REFERENCES materials(id)` (self-referential tree);
  nodes store only overridden fields (existing nullable columns already express sparse overrides).
  New tables: `finishes` (named property deltas), `objects` (shape ref, size, metadata),
  `object_components` (object_id, region, material_id, finish_ids[]).
- **Modules (`06`):** a material module is a **node** in this tree; its manifest names its `parent`.
  Asset resolution (shaders/sounds) uses the same up-the-tree fallback. The über-shader remains the
  default leaf; a node's custom shader overrides for it and its descendants unless they override again.
- **Export/runtime:** ship the tree + a resolver (lazy, cached), or export a flattened resolved view
  per material for the engine's hot path — likely both (tree for tooling, flattened cache for sim).

## 6. Open questions

1. **Flatten-on-export vs. resolve-at-runtime** — precompute resolved properties per leaf (fast, but
   larger export) vs. keep the tree and resolve lazily (compact, small runtime cost). Likely cache
   resolved leaves, keep tree for tooling.
2. **Voxelization fidelity** — resolution/scheme for filling arbitrary meshes with material at
   object-build time, and how it meshes back for rendering (ties into Phase 1 surface-nets work).
3. **Debris identity** — how finely fractured debris retains material + finish identity without
   exploding memory.
4. **Finish as layer vs. bulk** — some finishes are surface-only (varnish, paint), others alter bulk
   (weathering can be skin-deep or through-body); model a surface layer separately from bulk?
