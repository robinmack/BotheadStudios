# The scene, the assembly, and the engine

> **Robin, 2026-08-03, after a session spent watching scene-shaped routes accumulate:**
>
> *"I'm really getting frustrated… I just haven't found a way to communicate clearly enough the
> difference between a scene and an assembly and what the engine does. **Setting a scene should never
> involve changes to the engine.** Fulfilling the requirements to create a scene should call out that
> engine changes need to be made, and done carefully to ensure maximum harmony with existing engine
> systems. Same with assemblies."*
>
> *"**The scene sets the characters and the setting. The assemblies are the actors. The engine is the
> director and the stage** — it knows what should happen and gets the actors to make it happen."*
>
> *"I'm tired of scenes adding/accessing custom engine routes."*

This document exists because that had to be said more than once, and because prose is not a gate
(`AGENTS.md` §2). The rule is stated here and enforced in `laws::scene_api_tests`.

---

## 1. The three roles

### The scene sets the characters and the setting

A scene names **which assemblies are present**, **where they are**, **how fast they are going**, and
**where the observer stands**. That is the whole vocabulary.

A scene is DATA. `web/yarr.html` is the shape to copy: it loads the same `/worlds/earth/world.json`
that `terra.html` loads, and adds one attribute — `data-cannon` — naming a gun, a coordinate and a
bearing. No engine code was added for it to exist.

**Creating a scene must never require an engine change.** If a scene needs a capability the engine does
not have, that is not a licence to add a scene-shaped method: it is a finding, to be raised, designed
for harmony with what already exists, and built as a GENERAL capability that any scene could use.

### The assembly is an actor

An assembly is an entity in the universe. It carries its own **behaviour and systems**, and those are
**time-dependent**. It knows how it is put together — including the assemblies it contains, positioned
relative to itself — and it answers questions about itself when the engine asks.

> *"An assembly is an entity in our universe; it can describe its behavior, systems, etc to the engine.
> These are dependent on time. The engine keeps the time and sends signals to the model. The model sends
> information back to the engine (albedo, etc) and the engine renders the assembly, or the assemblies on
> the assemblies, ad infinitum."*

Assemblies nest without limit. A gun contains a charge and a shot. A ship carries guns. A forest is
an arrangement of trees, each an arrangement of foliage and timber — which is why a canopy is darker
than its own leaves (`docs/46` row 35) and why that darkening belongs to the ARRANGEMENT and never to
the leaf.

### The engine is the director and the stage

The engine keeps **the time** and **the state of the universe**. It signals the actors and renders what
comes back:

> *"The engine will inform the model if there is a collision from what vector and what intensity, or if
> there is an ignition source, etc… it keeps track of the universe state."*

It bounds what it asks by what is **viewable** — Law IV, the camera changing representation and never
existence. An assembly nobody is looking at still has a state; it is simply not asked for detail.

**The engine never learns what a cannon is.** It knows about matter, heat, contact, time and light. A
cannon is a thing that answers when heat arrives.

---

## 2. The worked example: firing a gun

This is Robin's own decomposition, and it is the test of whether the model is understood.

> *"Scene for cannon can create a button that tells the engine to create the heat source in the
> coordinates of the gunpowder; it can tell the engine where to place the cannon, the gunpowder (within
> the assembly; I think the engine should tell the assembly where to put the powder relative to itself,
> as well as the cannonball, generating an assembly with contained assemblies relative to each other.
> The engine places the cannon where the scene tells it to."*

| | wrong (today) | right |
|---|---|---|
| the scene says | `terra.fire_cannon(bearing, elevation)` | *apply heat, here* |
| who knows where the charge is | the engine, by name | the GUN assembly, which contains it |
| who decides it burns | `fire_cannon` | `oxidation`, because heat arrived at something combustible |
| who decides the shot leaves | `fire_cannon` | the pressure, against the shot's fit |
| what the engine's API says | `fire_cannon` | `signal(at, heat)` |

`fire_cannon` is wrong twice over. It carves an assembly's name into the engine's public surface, **and**
it makes the scene the thing that fires. In the right version the scene expresses an intent — heat,
there — and combustion, pressure, launch, recoil and smoke follow as consequences nobody asked for by
name. The same call lights a forest (`oxidation::apply_heat` was generalised for exactly this reason)
and the engine cannot tell the difference, which is the point.

★ **Containment is the engine's question to the assembly, not the scene's.** The scene says where the
GUN stands. The engine asks the gun where its charge sits relative to itself, and composes.

---

## 3. What a scene may call

The permitted surface is deliberately tiny. Four verbs:

| verb | what it means | today |
|---|---|---|
| **place** | which assemblies are present, where, how fast | `load_world` |
| **observe** | where the watcher is, and how big the picture is | `set_camera_pose`, `clear_camera_pose`, `camera_state`, `resize` |
| **step** | let time pass; draw what is | `advance`, `render` |
| **signal** | tell the universe something happened at a point | **does not exist** |

★★ **That last row is the finding.** There is no general way for a scene to say *"heat, here"* or
*"an impulse, there"*. Every scene that has ever needed one grew a bespoke method instead:
`fire_cannon`, `throw_meteor`, `drop_moon`, `brake_moon`, `launch_swarm`. Five spellings of *signal*.

A fifth verb is arguably needed — **ask**, for a scene reading back what the engine says is true
(`altitude_m`, `latitude`, `surface_material_at`). Reads change nothing and decide nothing, so they do
not break the model; but each one added by reflex is still API surface, and the general form of most of
them is one `state` query rather than forty accessors.

`add_tile` / `tiles_wanted` are legitimate and worth naming as the pattern to copy: **the engine
decides what data it needs and the host performs the I/O.** The decision stays in the engine; only the
fetching is delegated. That is what every scene/engine boundary should look like.

---

## 4. Where it stands, honestly

★★★ **AMENDED 2026-08-05 — this section was titled "honestly" and left out the biggest thing.**
It counted the scene-API debt and never said that **no planet is an assembly**. `assembly::Assembly`
describes six objects (cannon, charge, shot, oak, spruce, grass tuft); every planet is a
`planet::LayeredBody`, a different format that knows nothing about parts, containment or connections.
So the model this document states — *"adding a species, a vehicle or a planet is adding an assembly"* —
is true of species and vehicles and **false of planets**, and saying so was left to a conversation two
days later rather than to the doc that exists to say it. `docs/67` states the unified model and the
migration; `docs/46` row 45 carries the violation.

Measured 2026-08-03 by scanning `web/src/*.ts` for calls on the engine handle:

- **79 distinct engine methods are called by scene code.**
- **Three `#[wasm_bindgen]` scene structs** exist — `Terra`, `OrbitDemo`, `Ground` — where the model
  says there should be none. Adding or removing a scene edits the engine (`docs/46` row 14).
- The three answer the same questions differently. Terra flies a camera with `set_fly`; the space band
  orbits one with `set_orbit`; the Ground scene has its own. One question, three answers.

`laws::scene_api_tests` enumerates every one of those calls as **declared debt**. The gate:

1. a call that is neither ALLOWED nor declared → **FAIL** (a new violation)
2. a declared entry nothing calls any more → **FAIL** (delete the entry; the list must stay true)

So the list can only shrink, and it cannot rot. This is the same shape as
`laws::UNWIRED_MATERIAL_PROPERTIES`, which works for the same reason.

## 5. The order of the work

1. **This document and its gate.** Done first so nothing new lands while the rest is paid down.
2. **A general `signal`.** The one genuinely missing verb. `fire_cannon` and `throw_meteor` collapse
   into it, and the assembly graph answers "where is the charge".
3. **One camera.** `set_fly`, `set_orbit` and the Ground scene's variant become `set_camera_pose`,
   which already exists and is already general.
4. **One scene struct, then none.** `docs/46` row 14. A scene becomes a definition the engine reads,
   which is what `worlds-as-data` (`docs/43`) started and never finished.

Related: `docs/64` (the compiled assembly — what an actor IS), `docs/51` (scenes as data),
`docs/46` row 14 (the ledger entry this doc gives a design to).
