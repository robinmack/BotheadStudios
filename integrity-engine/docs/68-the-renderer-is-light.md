# 68 — The renderer is a separate entity, and its realm is light

> **Robin, 2026-08-05:**
>
> *"It's OK if what the eye can see is an illusion, as long as when it's interacted with the
> simulation/physics govern it. If a meteor smashes through the canopy, it leaves a hole, if we can
> perceive where a treetrunk should be through that hole we render it. Think of it as 'raycasting to show
> reality' à la the ancient 3d games like Wolfenstein that reversed ray-tracing to render a realistic 3-d
> environment on pretty pitiful hardware."*
>
> *"I almost feel like the renderer should be on a separate thread, or a separate process with multiple
> threads to optimize the compute, each getting signals from the model and the engine to determine what
> it should show and how it should be shown before stitching it together to be displayed on the canvas of
> the monitor."*
>
> *"This would solve a lot of problems and also demonstrates my earlier assertion that the renderer is a
> separate entity from the core engine; perhaps its realm is light and it handles the raytracing, since
> particle physics cares little about photons (indecisive beasts that they are)."*
>
> *"In the case of looking top down, we only need to render the canopy, and that can be rendered as a
> texture I think quite cheaply, one with holes punched in it for clearings."*
>
> *"Again, renderer should make choices based on quality/fidelity of view v performance."*

Status: **design, agreed in conversation, nothing built.** Written down immediately because it changes
what several things already built are *for*, and because the argument it settles was one I had previously
argued the other way.

---

## 1. The line, and it is sharper than the one I had

**Engine owns matter and time. Renderer owns light.** Particle physics does not care about photons; a
contact law, an EOS and a gravity solver never ask what colour anything is or who is looking.

The version of Law IV in `docs/00` says *the camera changes representation, never existence*. Robin's
statement is the same law from the other side and it is more useful:

> **An illusion is legitimate exactly as long as interaction is governed by the simulation.**

A canopy drawn as a texture is honest **because the meteor does not consult it.** The meteor meets real
matter, the hole it leaves is real, and through that hole we draw the trunk that was there all along.
Wolfenstein's trick was never that the world was fake — it was that the *drawing* was cheap while the
world stayed whole. That is the test for every representational shortcut this engine will ever take:

- Does anything **interact** with the illusion? → then it is a fudge (Law V), delete it.
- Does everything interact with the matter and only the *picture* take the shortcut? → legitimate, and it
  should be flagged with the resolved version it stands in for (Law V again).

## 2. What this dissolves — an objection of mine, recorded because it was wrong

On 2026-08-04 I wrote: *"I'm beginning to think we need to separate the renderer from the engine, but
that makes things needlessly complicated IMO since so many simulation choices (math v simulation) are
made based on viewport."* That objection is dissolved rather than outweighed:

**The viewport decides RESOLUTION, and resolution is a request the renderer MAKES of the model — not a
decision it makes FOR it.** The model always knows what is true; it is asked *"at what detail?"* and
answers. Nothing about that requires the two to share a thread, a process, or a struct. It requires a
protocol:

| direction | content |
|---|---|
| renderer → model | *where I am, what I can see, and how finely I need it* — a `containment::Region`, already a type, already carrying the cone |
| model → renderer | *here is what is there, at that resolution* — instances, matter, and the quantities light needs |
| engine → model | time, collisions, ignitions — the signals of `docs/65` |
| model → engine | what it did about them |

★ Note that the renderer's question is **already** the shape of `Region::seen`, and the "a renderer asks
a cone, physics asks the ball" line already in that type is this document in miniature.

## 3. What the renderer owns, once it owns light

- **Ray casting**, including the terrain self-shadowing docs/63 already asks for and the sky march
  `docs/66` already built. `atmos.wgsl` is light scattered along a ray; it is renderer work by this
  division, and it currently sits in the engine crate.
- **Fidelity versus cost.** Robin: *"the renderer should make choices based on quality/fidelity of view
  vs performance."* Which is to say the LOD decision belongs to the side that knows the pixel budget,
  not to the side that knows the physics. Today `FLORA_ALT_M`, the flora budget and the segment's
  tessellation are all engine-side constants deciding a picture.
- **Parallelism.** Several workers, each asking the model about a slice of the view, stitched before
  present. That is only possible because the model is a thing you ASK rather than a thing you step.

## 4. What it does NOT own

- What exists. Ever.
- What anything is made of, how heavy it is, where it is, or what happened to it.
- Whether something is resolved for **interaction**. A bus crushes an unwatched tree.

## 5. What this changes about work already done

| built | under this division |
|---|---|
| `atmos.wgsl` + `SkyVeil` (docs/66) | renderer work, currently inside the engine crate — the sky is light |
| `containment::Region::seen` | already the protocol's request type |
| `instance::Instance` | model-side; the renderer receives projections, never the instance |
| flora budget by subtended angle (row 48) | **a renderer decision living in the model** — it should be the renderer saying "this is what I can resolve" |
| `render::Drawn` | already exactly the model→renderer message, for matter |

## 6. The canopy, as the worked example

Top-down, a forest is a canopy: a texture, with holes punched where the clearings are. The holes are not
decoration — they are where the cover fraction says there is no crown, which the model already answers
(`terra::appearance`, the land-cover mixture). Punch a hole with a meteor and the model's answer changes,
so the texture changes, and through it the renderer draws the trunks that were always there.

**The invariant that keeps it honest is docs/63's convergence rule, generalised:** the canopy texture's
integrated albedo must equal what the resolved trees would return over the same footprint. If flying
lower changes the colour of a forest, one of the two is lying.

## 7. Risks, named now

1. **A protocol is a place for two answers to hide.** The moment the renderer can compute anything the
   model also computes, they will disagree. Everything crossing the boundary must be *asked for*, never
   recomputed. (`docs/46` is full of this failure at smaller scale.)
2. **Latency becomes a correctness question.** A renderer a frame behind the model is drawing the past.
   Tolerable, and it must be *stated* — an interpolation is a declared stand-in like any other.
3. **WASM.** The browser target has workers and `SharedArrayBuffer`, not threads-as-usual; a separate
   process is a native-only luxury. Robin has already said native is acceptable if WASM constrains
   (memory: the three lifts got most of the way; the blocker is wgpu's webgpu-only pin).
4. **Do not extract to fix a bug.** See below.

## 8. Order — and the immediate question

The immediate question was whether the current defect (flora generated, meshed at 43,200 triangles, and
not appearing) is better solved by doing this extraction first. **No — and the reason is diagnostic, not
conservative.**

The bug is currently *narrow*: the geometry exists, so the fault lies between the mesh and the screen.
Extracting the renderer would move that boundary while the fault is still unexplained, so the first
question afterwards would be "did we carry it across, or introduce it?" — and an unexplained bug on the
seam of a new architecture is the most expensive kind there is. It is the `mod app` lesson again: a
change that makes a defect harder to attribute costs more than the defect.

Better order:

1. **Find why the flora does not draw.** It is a small hunt and it is *evidence about this boundary* —
   whatever is wrong will be either a model concern or a light concern, and knowing which sharpens where
   the seam belongs.
2. **Move the LOD decisions to the renderer's side of the protocol** (`FLORA_ALT_M`, the budget, the
   segment's tessellation) *in place*, before any process boundary. If the model stops deciding
   fidelity, the extraction is mostly mechanical afterwards.
3. **Then extract**, with the protocol above as the interface, and `render::Drawn` + `Region` as its
   first two message types because they already are.
