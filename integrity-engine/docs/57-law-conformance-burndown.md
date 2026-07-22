# Law-conformance burn-down (2026-07-22)

A rigorous pass over the engine and its scenes against `docs/00-laws-of-integrity.md`, for a tune-up.

Findings are ranked by **what is physically wrong**, not by how hard they are to fix. Every entry names
the Law, the evidence, and **the test that turns it green** — because a finding without a test is a
conversation, and conversations do not survive the session.

Method: deterministic scans (`laws.rs`, constant-duplication counting, world-file inspection) for what
can be counted, plus `scripts/law-audit.sh` — an advisory Claude reviewer — for the class that cannot be:
**the same mechanic implemented twice**. Nothing is repeated in that case, so no grep finds it; the second
implementation looks like ordinary new code.

---

## HIGH — the physics is wrong, or one question has two answers

### 1. Ground-scene grains fall in MICROGRAVITY — measured at 1/46,000 g

**Law I, V, VII.** `matter.rs:1031` steps every grain under
`field.acceleration_point_approx(p.pos, 6.0)` — the self-gravity of the loaded surface **patch**. A patch
is a box of voxels tens of metres across. A planet is not.

Measured (`simulation::gravity_audit_tests`, and it prints the numbers):

| | |
|---|---|
| the planet's own surface gravity | **9.8808 m/s²** |
| what a grain actually falls under | **0.000214 m/s²** |
| ratio | **2.2 × 10⁻⁵** |

Everything downstream is wrong by four orders of magnitude — settling times, ejecta arcs, crater
profiles, angle of repose. A grain takes about **215× too long to fall**. `simulation.rs:139` computes the
correct `-surface_g` and hands it to the analytic effects, so the scene holds *both* answers at once and
gives the grains the wrong one. This is very likely implicated in the crater-refill behaviour that
`docs/55` attributes to missing grain–grain contact.

**Fix.** The patch is a patch OF a planet: the field a grain feels must be the planet's, with the local
voxels as a perturbation — not the lump alone in space.
**Test.** Already written and currently asserting the DEFECT. Fixing it makes it fail, by design; invert
it to assert `|a| ≈ g_planet`.

### 2. Only the first impactor is real

**Law II.** `lib.rs:2264` — `if k == 0 && shatter.is_none()`. The impact loop correctly sweeps every moon
for contact, but **only index 0 shatters or produces debris**. A second impactor adds its energy to the
total and is parked at the contact site, intact.

So *Two Moons → Drop* is not a three-body impact; it is one impact plus one silently absorbed collision.
The same event gets two different treatments depending on an array index.

**Fix.** Debris per impact; `moon_debris` becomes a collection.
**Test.** Drop two moons onto the same body; assert both produce debris and the total ejected mass scales
with the number of impactors. Fails now.

### 3. The Ground meteor bypasses the shared collision rule

**Law II.** `accretion::representation` is the engine's one answer to surface-vs-particles at any scale,
and `matter::impact` is a second, parallel answer for the same question. Ground computes ½mv² and calls
its own voxel excavation; Birth resolves an SPH body at the tidal threshold. Two paths, one mechanic.

They already agree in principle — the ground path sizes its resolved region by energy, which is right.
**Fix.** One entry point taking an interaction (energy, place, bodies) and returning how much matter to
resolve and in what form, with voxel/granular and SPH as backends beneath it.
**Test.** Assert both scenes route through the same function; assert a scale sweep (droplet → Theia)
produces monotonically growing resolved-matter counts through one API.

### 4. Settling is decided by a frame counter

**Law V, VII.** `matter.rs:47-51, 1065` — `SETTLE_SPEED = 0.02`, `SETTLE_FRAMES = 10`. The moment matter
stops being matter depends on the **timestep**, so the same world settles differently at 30 fps and 120.
De-resolution is the decision `accretion::representation` makes by measurement; here it is a tuned speed
and a frame count.

**Fix.** A physical criterion (energy below what the material's contact dissipates in one step); any timer
in seconds, derived.
**Test.** Run the same excavation at `dt` and `dt/10`; assert wall-clock settle time agrees within 10%.
Fails now (0.167 s vs 0.0167 s).

### 5. The physics clock IS the display clock

**Law VI — physics drives the render, never the reverse.** `ground_scene.rs:668` — `self.sim.step(1.0/60.0)`
inside `render()`, with no accumulator and no measured frame time. Simulated time = frames ÷ 60, so a
30 fps machine runs the world at half speed. Measured frame rates on this box span 23–354 fps, so it is
not hypothetical.

**Fix.** Wall-clock accumulator, fixed inner step, carried remainder.
**Test.** Host-loop property, not testable in-crate; `scripts/rigvideo.sh` can assert a meteor's simulated
fall time matches wall-clock time.

---

## MEDIUM — honest but duplicated, or declared where it should be derived

### 6. Two ground heights
**Law II**, and `CLAUDE.md` already lists this class as a mistake made here. A voxel-step answer
(`matter.rs:1051`, `simulation.rs:227`, `ground_scene.rs:891`) and a bilinear answer
(`ground_scene.rs:861, 885`, `matter.rs:461`), up to a metre apart on a slope — so the camera rests a
metre from where grains rest. Neither uses `World::ground_top_voxel`, which the ledger records as
authoritative. **Test:** assert a resting grain and the camera shell agree within a grain radius.

### 7. Scenes still declare bodies they only place
`worlds/earth`, `worlds/one-moon`, `worlds/two-moons` declare `mass_kg` and `radius_m` for Earth and the
Moon, which `assets/bodies/*.json` already own. The impact scene was fixed; these were not.
**Test:** extend the `laws.rs` scan to reject body physics in a world that names a defined body.

### 8. `grain_size_m` is render-only
Declared in the world and consumed only by the renderer; the physics always uses one 1 m³ voxel with
`PARTICLE_HALF = 0.45`. Set it to 0.1 and the picture shows 10 cm debris while the sim runs 1 m grains —
and the rendered 0.5 does not even match the collision 0.45. **Test:** assert `2·PARTICLE_HALF ==
grain_size_m`, or that worlds differing only in it produce different particle counts.

### 9. Meteor radius is declared, and unused by physics
`simulation.rs:57` documents `r = (3m/4πρ)^⅓` and then takes it from the caller; the repo's own tests pass
`0.5` for an 800 kg iron meteor whose real radius is 0.288 m. Contact tests use the centre.
**Test:** assert the engine-derived radius matches the formula, and that a large meteor contacts one
radius above the surface.

### 10. The in-view camera is a stale constant
`simulation.rs:137-147` resolves detail against the world's declared `camera_m`/`view_radius_m` while the
real eye moves every frame (`ground_scene::eye_and_target`). Two answers to "where is the observer" — and
Law IV hangs on that answer. **Test:** move the eye 200 m away, put an effect 5 m from it, assert it
resolves. Fails now.

### 11. Hardcoded 9.81 in slope stability
`matter.rs:700` — while `collapse()` takes `g` as a parameter precisely so it is not hardcoded, and
`simulation.rs:72` claims "there is no magic 9.81 anywhere in this path". Not live today (no production
caller), which is exactly why it will land unnoticed. `laws.rs` catches this in world files and cannot see
Rust. **Test:** same terrain under Earth and Moon gravity; assert grain counts differ.

### 12. The meteor's glow is a chosen temperature
`ground_scene.rs:723` — `incandescence(1600.0)` with a comment asserting a physical cause. The law is
shared; the number is not derived. A rock at 5 m/s glows exactly as hot as one at 900 m/s, in vacuum.
**Test:** assert `temp_k` after 1 s at 900 m/s exceeds that at 5 m/s. Fails now — the field does not exist.

---

## LOW — sourcing, naming, and stale claims

13. **The Sun's direction is an unsourced vector** (`ground_scene.rs:688`) feeding both shading and the
    derived sky. The Sun is a body; it is declared nowhere. Terra already computes the real one.
14. **Bare standard constants** — `288.0` and `101_325.0` appear as literals in three files. Per the
    Law VII SOP they belong in the catalogue with `sources`.
15. **`ground_scene` hardcodes Earth** for its sky (`planet::earth()`) while `Simulation` resolves the
    planet from the definition. A world naming another body gets its gravity and Earth's air.
16. **Particle budget declared twice** (`ground_scene.rs:571`, `simulation.rs:84`) — equal today by
    coincidence.
17. **Stale claims in headers** — `ground_scene.rs:10` says it gives the granular GPU pipeline a visible
    consumer; it does not. The kind of error that misroutes the next session.

---

## Already on the ledger — do not re-file

Seam culling (row 9) · `MAX_EJECT` in Earth gravity (row 11) · no atmosphere in any scene, hence no meteor
drag (row 12) · two incandescence curves (row 13) · scene KIND is code (row 14).

---

## Suggested burn-down order

1. **#1 microgravity** — largest physical error in the engine, and it is measured.
2. **#2 first-impactor-only** — one line, and it is the multi-body scalability test.
3. **#5 physics clock** — silently halves simulated time on a slow machine.
4. **#4 settle counter** and **#6 two ground heights** — both make results depend on how, not what.
5. **#3 one collision entry point** — the largest piece, and the one the whole premise rests on.
6. The MEDIUM sourcing/derivation items, which are mostly small.

## How this list stays honest

`scripts/law-audit.sh` regenerates it for any area. `scripts/law-review.sh` reviews a change before it
lands. Both are **advisory and optional** — they need a logged-in `claude` CLI and skip cleanly without
one, because `scripts/test.sh` is the suite that must pass for every contributor, Claude or not.

The durable half is the tests. Every finding above names one; as each is written, that finding stops
depending on anyone remembering it.
