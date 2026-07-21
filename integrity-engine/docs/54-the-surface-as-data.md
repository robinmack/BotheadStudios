# docs/54 — the surface as data

Continues docs/53 (the engine driven by a definition). Robin: *"all scene/game/etc outside the engine,
engine ensures the world acts like a physical world."*

## What moved

docs/53 let a definition declare what HAPPENED on the ground. It could not declare what the ground WAS —
`world::generate` hardcoded the patch size, the fbm octaves, the relief band, sea level and the material
strata, so every ground world was the same 96 m patch with the same hills.

`world_def::GroundSurface` declares all of it, and `world::generate_from` builds it:

| declared | was |
|---|---|
| `size_voxels` | `W`/`H`/`D` = 96 |
| `base_top_m`, `amplitude_m` | `BASE_TOP` = H−8, `AMPLITUDE` = 34 |
| `sea_level_m` | `SEA_LEVEL_Y` = 64 |
| `octaves[]` | the three fbm terms, 0.55@0.026 / 0.30@0.062 / 0.15@0.13 |
| `strata[]` | grass 1 → basalt 12 → peridotite 22 → iron |

**The laws did not move.** How strata stack, how water fills air below the datum, how the heightfield is
sampled, what makes a column collapse — all still the engine's. The file says what this ground IS; the
engine says how ground BEHAVES.

**Named `GroundSurface`, not `TerrainDef`.** The terrain SCENE was deleted (docs/50) and must not appear
to be returning. This is the engine's voxel ground — a core capability that scene merely used. (When
terrain is rebuilt it should be a ground DEFINITION, not a scene struct.) It is also distinct from
`world_def::Surface`, which names planet-scale RASTER data for `Terra`; the two converge the day real
bathymetry feeds a patch, and that is a merge to make deliberately rather than let happen.

## Output-neutral, and proven to drive

`surface_defaults_reproduce_the_hardcoded_world` asserts the declared defaults produce a **voxel-identical**
world to `generate`. `changing_the_declared_surface_changes_the_world` asserts the converse per dial —
size, amplitude (zero ⇒ provably flat), octaves, sea level (zero ⇒ no water), and skin material (what you
stand on). Without the second, the schema could be decoration.

## A mistyped key is now an error

serde ignores unknown fields by default, so `"terrian"` — or a key the engine renamed — would silently
leave the value at its default and run a DIFFERENT world than the file describes, with nothing to see.
This bit for real during the `terrain` → `surface` rename: a test went red **only because it asserted the
world's SHAPE**, not because the key was wrong. `deny_unknown_fields` now covers the ground and impact
schema, with a test naming the failure.

## Matter accounting — what two worlds actually showed

`run-definition` reports every grain, because "0 particles" is ambiguous: de-resolution (matter
conserved) and the off-patch cull in `matter::step` (matter deleted) look identical from a particle count.

| definition | patch | created | returned | in flight | **lost** |
|---|---|---|---|---|---|
| `ejecta-ground.json` | 96 m | 260 | 260 | 0 | **0 (0.0%)** |
| `small-island.json` | 48 m | 6,328 | 3,535 | 971 | **1,822 (28.8%)** |

The first conserves matter exactly. The second loses **~29%** — an energetic impact on a half-size patch
throws ejecta past the domain boundary, where `matter::step` culls it. That is docs/46 ledger row 9
("matter leaks at the seam"), previously measured at ~2% on the big patch and never at a small one. It is
a property of the DOMAIN, not of the physics — and it is now measurable per definition, which is the
point: a user of this engine can see whether their world conserves matter before trusting it.

## Next

The definition path is still **headless**. Nothing here is visible, and a game engine earns users by
being seen working. The next step is a browser scene that renders a ground world from a definition —
which would also give the granular GPU pipeline a visible consumer again (it is currently reachable only
from `GpuProbe`). That is the rebuilt terrain: a definition, not a scene struct.
