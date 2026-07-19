# Materials model — physical properties as the single source of truth

> Design note. Proposed schema and data plan for the material property database.
> Status: **design** (property *values* to be filled by cited research — see "Data plan").

## Motivation

The engine's core thesis is that **behavior and appearance both emerge from physical properties**,
not from hand-authored rules or pasted textures. So the material system is a **database of
real-world material properties**, keyed by material, that feeds two consumers from one source:

- **Simulation** — density, stiffness, strength, cohesion, hardness → how matter deforms,
  fractures, separates into chunks, flows as granular/fluid, and resists digging.
- **Rendering** — color, reflectance, roughness, grain → procedural textures/shading generated
  from the properties (the "grain of the rock, differences in dirt" the project set out to get).

One material entry → correct physics *and* a plausible look, with no per-material special-casing.

## Property taxonomy & schema

Each material is a record of physically-grounded fields **in real SI units** (so `F = ma`, stress,
and gravity all compose correctly). Grouped by consumer:

### Mechanical (drives simulation)
| Field | Unit | Drives |
|---|---|---|
| `density` | kg/m³ | mass per voxel → gravity, inertia, buoyancy/settling order |
| `youngs_modulus` | Pa | elastic stiffness (how much it springs vs. deforms) |
| `poisson_ratio` | – | lateral vs. axial strain coupling |
| `compressive_strength` | Pa | crush/impact threshold before failure |
| `tensile_strength` | Pa | pull-apart threshold (usually ≪ compressive for rock) |
| `shear_strength` / `cohesion` | Pa | resistance to sliding; low cohesion → granular (sand, dirt) |
| `internal_friction_angle` | deg | granular repose/pile behavior |
| `hardness` | Mohs (+ optional Brinell/Vickers) | dig/scratch resistance, tool interaction |
| `ductility` | – (brittle↔ductile scalar) | shatter into shards vs. deform plastically |
| `restitution` | – | bounciness of rigid debris |
| `friction_coefficient` | – | surface friction for rigid coupling |

### Optical (drives rendering / procedural texture)
| Field | Unit | Drives |
|---|---|---|
| `albedo` | linear RGB | base color |
| `roughness` | – [0..1] | microfacet spread (matte ↔ polished) |
| `metallic` | – [0..1] | dielectric vs. metal response |
| `specular` / `shine` | – | highlight intensity (derived from roughness/metallic) |
| `translucency` | – [0..1] | light bleed (ice, thin grass, water) |
| `grain` | struct | anisotropy direction + scale + contrast for procedural texture noise |
| `color_variance` | – | per-grain/mineral color spread (dirt mottling, granite specks) |

### Thermal (future phases — reserved, not used yet)
`melting_point` (K), `specific_heat` (J/kg·K), `thermal_conductivity` (W/m·K),
`ignition_point` (K). Included in the schema now so save files don't break later.

### Metadata
`id` (stable string key), `display_name`, `category` (rock/soil/organic/liquid/metal/…),
and **`sources`** — every numeric value carries a citation (see data plan). A `notes` field for
caveats (e.g. "soil properties vary widely with moisture/compaction").

## How properties map to behavior (examples)

- **Rock (granite):** high density, high `youngs_modulus`, high `compressive_strength`, low
  `tensile_strength`, high `hardness`, low `ductility` → hard to dig, barely chips under small
  impulse, fractures into angular chunks under large impulse. Speckled albedo + low roughness.
- **Dirt/soil:** medium density, low stiffness, low `cohesion`, moderate `friction_angle` →
  separates in clumps, piles at repose angle, easy to dig. Mottled brown albedo, high roughness.
- **Grass/turf:** low density, very low strength, some `translucency` → fragile, shreds easily.
  Green albedo with high `color_variance`.

The same solver reads these numbers; the *difference in behavior is entirely in the data.*

## Data plan

- **Format: JSON** (decided). Chosen over RON for universal tooling and web-friendliness — the
  material data feeds a planned **web-based creation system**, and a TypeScript/web material
  browser/editor should read it with zero friction. Citations live in a structured `sources`
  field (better than free-text comments: queryable, surfaceable in tooling).
- **Source of truth: PostgreSQL** (see [`05-data-pipeline.md`](05-data-pipeline.md)). The
  authoritative asset store is a Postgres DB, edited over time via the web tooling; the repo's
  `materials.json` is an **export** of that DB, regenerated per release and bundled as the base
  dataset for OSS users/contributors (who don't need DB access to build).
- **In-repo path:** `integrity-engine/data/materials.json` (exported artifact, committed).
- **Provenance:** each property value cites a source (engineering-toolbox / materials handbooks /
  USGS / peer-reviewed refs). Values that vary by condition store a representative value + range.
- **Loading:** `matter-core` deserializes into a `Material` struct at startup; the voxel store
  references materials by `id`. Rendering derives shader params from the optical fields.
- **Extensibility:** community can add materials by adding a record — no code change (aligns with
  the "emergence over authoring" principle in CONTRIBUTING).

## Open questions

1. ~~RON vs JSON~~ **Resolved: JSON** (web tooling + contributor accessibility). Postgres is the
   source of truth; JSON is the exported, bundled artifact.
2. **Units & anisotropy** — some rocks are strongly anisotropic (bedding/grain); do we model a
   single scalar strength now and add tensor properties later?
3. **Condition dependence** — soil/dirt properties swing with moisture; do we bake one "typical"
   value now and add state (moisture/compaction) in a later phase?
4. **Starter set size** — how many materials to research first (see task).
