# Data pipeline — Postgres source of truth → exported JSON

> Design note. How data assets (materials first, later objects/prefabs/worlds/bodies) are stored,
> curated, and shipped. Status: **design** (not yet implemented).

## Why a database, not just files

The engine ships static JSON that anyone can use offline. But the *authoritative* asset catalog —
materials with cited properties, and eventually reusable objects (crates, houses, spaceships) and
whole worlds/solar-systems/galaxies built in the web tooling — needs a real store that supports
curation over time: editing, validation, provenance, versioning, and multi-user contribution.

So we split the two roles:

```
┌─────────────────────────────┐        export         ┌──────────────────────────────┐
│  PostgreSQL (source of truth)│ ───────────────────►  │  materials.json (bundled)     │
│  • curated + edited over time│   (per release / CI)  │  • base dataset in the repo    │
│  • per-value citations       │                       │  • zero-DB for OSS users       │
│  • web tooling writes here   │                       │  • matter-core loads it        │
└─────────────────────────────┘                        └──────────────────────────────┘
        ▲                                                          │
        │ web-based creation system (future)                       ▼ contributors can also
        └──────────────── authoring UI ───────────────  submit JSON edits via PRs
```

**Direction of authority:** Postgres → JSON is the release path. Contributors without DB access can
still propose changes by editing the exported JSON in a PR; those get reconciled back into the DB.
(One-way export keeps it simple initially; a two-way sync/import can come later if needed.)

## Postgres schema (first cut — materials)

SI units throughout. One row per material; citations in a child table so every value is sourced.

```sql
-- Canonical material catalog
CREATE TABLE materials (
    id                      TEXT PRIMARY KEY,          -- stable slug, e.g. 'granite'
    display_name            TEXT NOT NULL,
    category                TEXT NOT NULL,             -- rock|soil|granular|metal|organic|liquid|frozen|ceramic
    -- mechanical (nullable where N/A, e.g. liquids)
    density_kg_m3           DOUBLE PRECISION,
    youngs_modulus_pa       DOUBLE PRECISION,
    bulk_modulus_pa         DOUBLE PRECISION,          -- liquids
    poisson_ratio           DOUBLE PRECISION,
    compressive_strength_pa DOUBLE PRECISION,
    tensile_strength_pa     DOUBLE PRECISION,
    cohesion_pa             DOUBLE PRECISION,          -- shear strength / cohesion
    friction_angle_deg      DOUBLE PRECISION,          -- granular
    hardness_mohs           DOUBLE PRECISION,
    ductility               DOUBLE PRECISION,          -- 0=brittle .. 1=ductile
    restitution             DOUBLE PRECISION,
    friction_coefficient    DOUBLE PRECISION,
    dynamic_viscosity_pa_s  DOUBLE PRECISION,          -- liquids
    -- optical
    albedo_r                DOUBLE PRECISION,          -- linear 0..1
    albedo_g                DOUBLE PRECISION,
    albedo_b                DOUBLE PRECISION,
    roughness               DOUBLE PRECISION,
    metallic                DOUBLE PRECISION,
    translucency            DOUBLE PRECISION,
    color_variance          DOUBLE PRECISION,
    grain                   JSONB,                     -- {direction, scale, contrast, ...}
    -- reserved thermal (future)
    melting_point_k         DOUBLE PRECISION,
    specific_heat_j_kgk     DOUBLE PRECISION,
    notes                   TEXT,
    created_at              TIMESTAMPTZ DEFAULT now(),
    updated_at              TIMESTAMPTZ DEFAULT now()
);

-- Provenance: one citation per (material, field). This is what makes the DB defensible.
CREATE TABLE material_sources (
    material_id  TEXT NOT NULL REFERENCES materials(id) ON DELETE CASCADE,
    field        TEXT NOT NULL,        -- e.g. 'density_kg_m3'
    value_note   TEXT,                 -- representative value / assumed condition / range
    source_url   TEXT NOT NULL,
    source_title TEXT,
    PRIMARY KEY (material_id, field, source_url)
);
```

Later asset kinds (objects, prefabs, bodies, worlds) get their own tables and reference
`materials(id)`; the same export pattern applies.

## Export step

A small exporter (Rust binary in `crates/`, or a Node script) runs `SELECT`s and emits
`integrity-engine/data/materials.json` in the schema from `04-materials-model.md`, embedding a
`sources` object per material. Wired into a release/CI step so the bundled JSON never drifts from
the DB. The committed JSON is the artifact contributors and the engine actually consume.

## Local dev

- `docker compose` service for Postgres (so contributors can run the DB locally without a global
  install), plus a `schema.sql` + `seed` step. The **seed** is populated from the cited research
  compiled in task #4 → this is how the DB gets its first ~20 materials.
- Engine builds and tests only need the **exported JSON**, not Postgres — so the physics/render
  work is never blocked on DB setup.

## Open questions

1. **Where the DB + web tooling live** — same monorepo (e.g. `tools/asset-db/`, `tools/web-studio/`)
   vs. separate repos under BotheadStudios. Leaning monorepo for now.
2. **Exporter language** — Rust (reuse `Material` types, one toolchain) vs. Node (shares the web stack).
3. **One-way export vs. two-way sync** with contributor JSON edits.
4. **Versioning the data** — does `materials.json` get its own version/schema-version independent
   of the engine version?
