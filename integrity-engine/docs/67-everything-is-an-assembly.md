# 67 — Everything is an assembly

> **Robin, 2026-08-05:** *"To make the architectural model work, planets must be assemblies. Possibly a
> different class of assembly with its own morphology, etc, but an assembly."*
>
> And, when asked what would resist: *"A planet is an accretion of debris bound by its own gravitational
> effects which we've worked hard to model."*

Status: **design, agreed in conversation, nothing built.** This document exists so the work is executed
once rather than re-derived, and so the migration order is decided before any of it starts.

Supersedes nothing. It is the concrete data model `docs/65` (scene/assembly/engine) assumes and
`docs/58` (the generic body) started toward.

---

## 1. The finding: there are two formats for "a thing that exists"

| | `assembly::Assembly` | `planet::LayeredBody` |
|---|---|---|
| made of | `Vec<Part>` — material, `Shape`, position, orientation, packing | `Vec<Layer>` — concentric shells |
| holds | `Connection`s; nests without limit (in principle) | an atmosphere mass; surface rasters |
| bulk quantities | `Derived` — a **cache** of mass, volumes, centre of mass | computed on demand: `total_mass`, `moment_of_inertia`, `enclosed_mass` |
| instances | 6: cannon, charge, shot, oak, spruce, grass tuft | every planet |

Nothing in the repo recorded this split. `docs/65` §4 is titled *"Where it stands, honestly"*, enumerates
79 scene-called methods and three scene structs, and **never says that no planet is an assembly**. The
architecture page shipped to integrity.bothead.net says *"adding a species, a vehicle or a planet is
adding an assembly"* — in the present tense, and untrue for planets. Two days of work were described in
assembly language on top of that silence. Recorded as `docs/46` row 45.

## 2. Why it is one thing and not two — the accretion argument

The split reads as principled and is not. **A planet is an accretion of debris bound by its own
gravity**, and this engine models exactly that — it is the whole of the birth-of-the-Moon scene. The
round trip is already built and tested, in both directions:

```
hydrostatic::HydroBody::particalize(&LayeredBody, resolution)   layers  → particles
accretion::sample_layers(pos, mass, rho, material, temp_k)      particles → layers
```

So `LayeredBody` is not a different KIND of object. **It is a de-resolved summary of an assembly of
matter** — precisely the relationship `Derived` already has to `Assembly::parts`: bulk quantities cached
over components, with the components winning if the two disagree. Two names for one idea, sharing no
type.

The split is **lineage, not principle**. `LayeredBody` came from the giant impact, where a planet needed
layers for hydrostatic equilibrium, EOS and particalization. `Assembly` came from the cannon (`docs/64`),
for made things. Neither was wrong; they never met. That is the same shape as `docs/46` row 1
(`Aggregate` vs the voxel `World`), which was closed by unifying rather than by justifying.

★ It is also the **fifth** arrival of the substance-versus-assembly distinction. `docs/46` row 35 counted
four — rainforest canopy, black powder, Irish albedo, canopy darkening — and said it *"wants to be a
first-class idea rather than a lesson relearned."* This is that idea.

## 3. What an assembly must be

Robin's decomposition, stated 2026-08-05, with what exists against each:

| property | today |
|---|---|
| describes itself, **or the portion of itself that is visible**, to the engine | `mesh()` describes ALL of it, at one detail |
| has an **attitude** | no — `Part::along` orients a part within an assembly; the assembly has none |
| has **mass**, is assembled of **materials** | ✅ `mass_kg`, `Part::material` |
| **contains other assemblies**, each describing itself to its container as the view requires | ✗ — `Connection` exists, containment does not |
| has **momentum** and **heading** | no — those live on `orbit::Body` / `Drawn`, not on an assembly |
| is **destructible** — matter the engine can damage, move, or collide through | `damage.rs` exists; no assembly instance for damage to live in |
| **time signals** propagate engine → assembly → contained assemblies, all the way down | `solar::RespondsToTime` exists with one implementor and no propagation |

### What that list is missing

1. **Extent before detail.** "Describe what is visible" is only affordable if *where are you and how
   big* is answerable without a description. `Assembly::reach_m` (2026-08-05) is the cheap half.
2. ★★★ **THE SCALABILITY LAW — identical until damaged.** Robin: *"Oaks can be handled as identical to
   their construction until they are damaged, at which point they become unique. This will help for
   limited compute."* Stronger than copy-on-write, and it is what makes 10¹² trees affordable: **a
   pristine instance need not exist.** Its placement comes from its container's own rule and every other
   observable is a question the TYPE answers, so there is nothing left for storage to hold. A world
   stores **divergences, not individuals**, and `Damage::is_pristine` is the predicate deciding whether
   an instance has earned its bytes. The consequence for everything built on this: **never materialise
   an instance in order to read it** — ask the rule; materialise when something happens to it, and then
   it is unique for good, because that is what damage means. Checkable rather than aspirational:
   `instance::divergence_tests` asserts two pristine instances of one type at one place are the same
   object to every consumer, and goes red the day something is added to `Instance` that a type cannot
   answer — i.e. the day 10¹² trees quietly become 10¹² allocations.
3. **Containment must be DERIVABLE, not enumerated.** A planet contains ~10¹² trees. So containment is
   *a rule answering "what is in this region"* **plus a set of exceptions the assembly remembers**.
   `terra::flora::scatter` already does the rule half — a position hash, deliberately stateless, so an
   unwatched meadow keeps its tufts. The exception half is what a bus crushing a tree creates. This is
   the load-bearing idea of the whole model.
4. **Type versus instance.** `broadleaf-tree-oak.json` is a species; the tree at 53.1°N is an individual
   with its own lean, damage and season. `terra::flora::Sited` carries lat/lon/kind/yaw/scale and **no
   state**, so destructibility has nowhere to live.
5. **Containment transfers at runtime.** The shot is contained by the gun and then is not. The tree is
   contained by the planet and then is debris on a bus. `assembly.rs`'s own header already says this —
   *"containment is a relationship with state rather than a static parent-child link — an assembly GRAPH,
   because a tree cannot express reloading"* — as a comment, not a type.
6. **Cadence belongs to the actor**, or the time signal wakes 10¹² trees per frame. A leaf answers in
   days, a bear in minutes, a rock never (`docs/46` row 38, written and unbuilt).
7. **The signal is bidirectional.** The bus must be able to say *I struck something, here, this hard*.
   `docs/65` already names the missing verb and counts five bespoke spellings of it (`fire_cannon`,
   `throw_meteor`, `drop_moon`, `brake_moon`, `launch_swarm`).
8. **Answers must agree across resolutions**, or the bus crushes a different tree than the one you saw.
   `docs/63` states this as the convergence invariant for footprints; it generalises.
9. **Temperature and energy state**, and **frame**. Attitude and momentum relative to *what* — a bus on a
   spinning planet has both in the planet's frame, and containers must compose them.

## 4. The collision engine, per assembly

> **Robin:** *"let's look at embedding copies of the collision engine into each assembly. The collision
> engine works at all scales depending on the amount of energy in the reaction… each assembly can then
> determine the impact of the energy/materials on itself. These can then bubble up to the renderer."*

The law here is **already scale-invariant** — `granular::Contact` derives from material properties,
`terrain_contact_resolve` is energy-monotone and hardware-verified, `eos.rs`'s Tillotson is pinned to
Benz & Asphaug 1999. What this proposes is not a new law: it is making the **instance local** instead of
the solver global. That is why it scales — 10¹² trees run zero solver steps, and the one being crushed
resolves because energy says so, not because a camera does.

★★ **ONE IMPLEMENTATION, MANY INSTANCES.** The word "copies" is the whole risk. Per-assembly solver
STATE is the idea. Per-assembly-type solver CODE is `docs/46` at maximum multiplicity, and this repo's
measured history says that is the default outcome, not a hypothetical: two grain-interaction paths, two
ways to get ground height, two incandescence curves, four integrators. **Gate it by counting
implementations**, the way `biome_mixtures` is already gated at exactly one definition.

Three things expected to bite, in order:

1. **Energy at the boundary.** Every handoff between a container's solver and a contained one can create
   energy. Already paid for once: the terrain penalty spring released ½k·pen² as launch KE (the settling
   storm), fixed by a non-injecting constraint. Nest N solvers and there are N² such seams. Each needs a
   **checked conservation ledger**, the way `site::MaterializedSite` carries mass-in/out and
   angular-momentum drift against its own bound.
2. **Multirate time.** A tree splintering is milliseconds; an orbit is 10⁷ s. Per-assembly resolution
   implies per-assembly timestep, and the coupling between rates is where this is genuinely hard rather
   than laborious.
3. **What decides resolution.** Energy in the reaction — never visibility. The unwatched bus still
   crushes the tree (Law IV/VI).

**Already built and unwired**, which is most of it: `granular::Contact`, `terrain_contact_resolve`, the
Tillotson EOS, `docs/39`'s JIT particalization (conserving to <1e-12, wired at planetary scale only),
`damage.rs`, `Connection`, and `gpu_sph::promote_ground_cap` — surface becoming matter — built, tested,
**zero consumers**.

## 5. What resists, concretely

Three gaps, all of which assemblies need anyway — **and a fourth this document did not predict, found
by attempting it**:

0. ★★ **A part could not say its matter was COMPRESSED.** `mass = envelope × packing × density` reads
   the catalogue's *surface-condition* density, and a planet is mostly not at the surface: Earth's
   lower mantle is peridotite at 4500 kg/m³ against a reference near 3300. `packing` cannot express it
   — it is clamped to 1, because it means VOID.
   ★ Robin pushed back — *"We know the difference between sand and gravel. Or sand and sandstone."* —
   and that is the settling argument, in the opposite direction to the one it looks like. Random close
   packing is ~0.6 for **both** sand and gravel; what differs is grain size. Sand and sandstone are the
   same grains at nearly the same packing, and one flows while the other holds a cliff, because in
   sandstone they are cemented. So **packing is a lossy summary of an ASSEMBLY** — resolved by grains
   with a size and with or without `Connection`s — while **compression is a STATE of a substance**,
   resolved by the EOS at the local pressure. Different resolved counterparts, therefore different
   fields. `Part::in_situ_density` (flagged, PREM-measured, with `pressure_at` + Tillotson named as
   what it defers to), and `Part::packing`'s doc now says what it cannot hold.

1. ✅ **`Shape::Shell`, built 2026-08-05.** `Tube` was already the cylindrical version of it, so this
   was a missing variant rather than a missing idea. Nested `Sphere`s counted the core five times over.
2. **A planet's surface is a raster, not parts.** This looks like a difference in kind and is not:
   `docs/63` already argues a raster is a statistical description of the matter that is there,
   integrated over a footprint — a *resolution* of the same object. Planets are simply the first
   assembly big enough to force property 1.
3. **Containment at 10¹² cannot be a `Vec`.** See §3.2.

## 6. Order of the work, and the guard rail

★ **The guard rail already exists and it is why this is safe to attempt.**
`laws::one_earth_tests::the_three_scenes_read_one_earth` asserts **digit-identity** across all three
scenes reading Earth. Any migration that moves a number fails it. `LayeredBody` carries fourteen methods
of verified physics — `enclosed_mass`, `moment_of_inertia`, `gravity_at`, `pressure_at`, `phase_at`,
`surface_strata`, `surface_pressure` — and every one must keep answering identically.

1. **Fix the record.** `docs/46` row 45, `docs/65` §4, the architecture page. Nothing is built on a page
   that is lying. *(Done with this document.)*
2. **Instance versus type.** ✅ **BUILT 2026-08-05, unwired** (`instance.rs`, `docs/46` row 46).
   `Instance` holds only STATE — identity, placement (position + attitude, in a container's frame),
   motion, thermal energy, damage — while mass, extent, temperature and strength are all asked of the
   TYPE, so ten thousand oaks do not carry ten thousand copies of one oak's mass. It is deliberately not
   a fifth sibling of `orbit::Body` / `accretion::Body` / `interaction::BodyState` / `render::Drawn`:
   those become **projections of it**, and `Instance::body_state` lives here so a view cannot quietly
   disagree with the instance. Damage is measured in matter — halving every part halves the mass through
   the same envelope × packing × density the definition uses, with no damage-to-mass curve anywhere.
   ★ Nothing holds one yet, and the reason is this list: the first natural consumer is the cannon, whose
   placement is geographic, which needs Earth to be a container — step 5.
3. **Extent, attitude, momentum on the instance.** `reach_m` exists; the rest is state.
4. **Containment as a derivable rule + exceptions**, proven on the case that already exists: a planet
   containing its flora. `scatter` is the rule; the exception set is new.
5. **Earth expressed as parts.** ✅ **HALF DONE 2026-08-05**: `LayeredBody::as_assembly` exists and
   `an_earth_described_as_an_assembly_weighs_the_same_earth` pins it — mass agrees to better than
   1e-12 relative, the air part carries exactly the declared atmosphere mass, the shells tile the
   sphere with no gap or overlap, the centre of mass is the centre, and the assembly reaches ~97 km
   past the rock because its outermost component is its air. The Moon takes the same call and gets no
   atmosphere part, with no branch. **What remains is the direction of travel**: this converts, it does
   not yet REPLACE — nothing reads Earth as an assembly, and the surface rasters are not parts.
6. **The signal**, bidirectional, replacing the five bespoke spellings.
7. **The per-assembly solver**, last, because it needs 2–4 to exist.

### The smallest thing that would falsify it

**One assembly hitting another, resolving through the struck assembly's own parts and `Connection`s,
with an energy ledger across the single boundary** — the cannon's shot into an oak. Plus the negative
control: an identical hit at lower energy that must leave the tree intact. Depth 1, two assemblies, one
seam, fully measurable. If energy conserves across that seam and the failure comes from the tree's own
materials rather than from anything written for trees, the recursion is proven and the rest is
repetition. If it does not, that is found at depth 1 instead of depth 5.
