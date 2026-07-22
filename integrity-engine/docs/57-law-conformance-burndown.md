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

## Status

**#1 (microgravity) and #2 (first-impactor-only) are FIXED** — see below; both now carry tests. The rest
stand.

**Added during the burn-down**, and not yet fixed:

* **The follow-camera collapses after a drop.** `orbit.ts:381` — `followZoom()` returns
  `moon_distance_km() / LUNAR_KM`, and once an impactor is parked at the contact site that distance is a
  planet radius rather than a lunar one, so the camera zooms to its closest clamp and ends up inside the
  body it is meant to be watching. Pre-existing, and unrelated to the impact fixes. **Test:** after a
  drop, assert the eye is outside the target's radius.
* **`deploy.sh` had the same silent-stale hole as `rig.sh`.** A deploy shipped from a `main` that did not
  contain the work being deployed — the PR merge had failed, a `;` let the deploy run anyway, and it
  printed "✓ deployed" over the previous release. It now names the branch and commit it is publishing and
  refuses to run when `origin/main` is ahead of the checkout. FIXED.
* **The granular tests validate under patch self-gravity.** Every `matter.rs` test builds a bare
  `MassField` (`host: None`), which is correct for an asteroid and wrong for anything claiming to model an
  Earth surface — the same defect as #1, still live in the test fixtures. **Test:** give those fixtures a
  host and re-derive whatever they assert about settling and repose.

## HIGH — the physics is wrong, or one question has two answers

### 1. ~~Ground-scene grains fall in MICROGRAVITY~~ — FIXED

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

**FIXED.** `MassField` now carries a `HostBody` — the planet a patch belongs to — and reports its gravity
with the local voxels as the perturbation they are. "Down" is computed toward the host's centre rather
than assumed to be −Y, so it stays right on a patch large enough for the difference to matter. Measured
after: **9.881 m/s², ratio 1.000**. The characterization test fired on the fix exactly as designed and is
now inverted to assert the correct behaviour.

### 2. ~~Only the first impactor is real~~ — FIXED

**Law II.** `lib.rs:2264` — `if k == 0 && shatter.is_none()`. The impact loop correctly sweeps every moon
for contact, but **only index 0 shatters or produces debris**. A second impactor adds its energy to the
total and is parked at the contact site, intact.

So *Two Moons → Drop* is not a three-body impact; it is one impact plus one silently absorbed collision.
The same event gets two different treatments depending on an array index.

**FIXED.** Every impactor that lands now shatters, each carrying its own mass and body index, and the
clouds ABSORB into one debris field rather than replacing each other. Two follow-on defects surfaced while
fixing it: `bodies[2].mass = 1.0` was hardcoded, so moon 0 was zeroed twice and moon 1's mass stayed
double-counted; and `bodies[1].mass -= cap_mass` now runs per impact, which drove the target's mass toward
zero on the second strike. `Aggregate::absorb` extends EVERY per-particle array and offsets the incoming
bonds' particle indices — the first version extended three of seven and the scene panicked
(`per_grain_contact[i]` out of bounds) the instant two clouds existed.

Measured after: Two Moons → Drop reports **3,071 fragments, 7.05e30 J, a 0.96 M☾ disk in 4 moonlets** —
against one cloud before.

### 3. The Ground meteor bypasses the shared collision rule

**Law II.** `accretion::representation` is the engine's one answer to surface-vs-particles at any scale,
and `matter::impact` is a second, parallel answer for the same question. Ground computes ½mv² and calls
its own voxel excavation; Birth resolves an SPH body at the tidal threshold. Two paths, one mechanic.

They already agree in principle — the ground path sizes its resolved region by energy, which is right.
**Fix.** One entry point taking an interaction (energy, place, bodies) and returning how much matter to
resolve and in what form, with voxel/granular and SPH as backends beneath it.
**Test.** Assert both scenes route through the same function; assert a scale sweep (droplet → Theia)
produces monotonically growing resolved-matter counts through one API.

### 4. ~~Settling is decided by a frame counter~~ — FIXED

**Law V, VII.** `matter.rs:47-51, 1065` — `SETTLE_SPEED = 0.02`, `SETTLE_FRAMES = 10`. The moment matter
stops being matter depends on the **timestep**, so the same world settles differently at 30 fps and 120.
De-resolution is the decision `accretion::representation` makes by measurement; here it is a tuned speed
and a frame count.

**FIXED (partly).** `SETTLE_FRAMES: u32 = 10` is now `SETTLE_SECONDS: f32 = 10.0/60.0` — the same 0.167 s
at today's step, but it stays 0.167 s when the step changes. Tested: the same excavation settles within
35% of the same SIMULATED time at 60 Hz and 240 Hz, where it was a factor of four apart by construction.

**STILL OPEN:** a duration is not a physical criterion. The honest form is energy — deposit once a grain's
kinetic energy falls below what its material's contact dissipates in one step, which is what
`accretion::representation` does by measurement for bodies.

### 5. ~~The physics clock IS the display clock~~ — FIXED

**Law VI — physics drives the render, never the reverse.** `ground_scene.rs:668` — `self.sim.step(1.0/60.0)`
inside `render()`, with no accumulator and no measured frame time. Simulated time = frames ÷ 60, so a
30 fps machine runs the world at half speed. Measured frame rates on this box span 23–354 fps, so it is
not hypothetical.

**FIXED.** A wall-clock accumulator consumed in fixed inner steps, with the remainder carried and a cap
so a long stall is admitted rather than chased (the spiral of death). Tested at 20, 60 and 240 fps: one
second of wall time advances one second of physics in all three, and they agree with each other — which is
the property that was broken.

It lives in a new `crate::clock`, NOT in the scene, because every scene needs it and there is one right
answer. It also had to move to be testable at all: `ground_scene` is browser-only, so its tests never run
natively — which is how a defect this basic survived in it. **That is worth generalising: arithmetic in a
wasm-gated module is arithmetic nobody tests.**

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
