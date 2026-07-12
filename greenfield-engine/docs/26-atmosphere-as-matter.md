# Atmosphere as matter — air is particles too (docs/25 roadmap, made concrete)

> Robin: *"we likely need an atmosphere to keep the water from boiling off into space naturally as a
> consequence of the model"* — delivered as a static pressure boundary in docs/25. This doc is the
> promotion: air as REAL matter in the dynamic simulation, one law, scale-relative.

## The one-law extension: gases resist compression by their EOS, not their elastic modulus

The canonical contact law (`granular::contact_from_material`) derives a solid's stiffness from its
declared Young's modulus — "how this matter resists compression." A gas resists compression too, but its
declared physics is an **equation of state**, not an elastic modulus: ideal gas `P = ρ·R_s·T` with the
specific gas constant `R_s = R_u/M` from the molar mass we already store. The isentropic bulk modulus is
`K = γ·P`. So the SAME `Contact` machinery applies with stiffness derived from `K(ρ, T)` instead of `E`
— matter declares what it is; the law reads the right property for its phase. No fork, one law.

## v0 model (natively testable before any rendering)

- **Air parcels**: equal-mass particles (the mass-agnostic model), material `air` (real: sea-level
  ρ 1.225 kg/m³, c_p 1005 J/(kg·K), M 0.028964 kg/mol → R_s ≈ 287, γ = 1.4, μ ≈ 1.8e-5 Pa·s).
- **Parcel force**: repulsive contact with stiffness from `K = γ·P_ref` at the parcel's reference state
  (isothermal v0, flagged); zero cohesion, tiny friction (viscosity is the later refinement).
- **Gravity**: the same extended-body source (Gauss interior / 1/r² exterior).
- **Heat**: the same dissipation→temperature accounting; shock-compressed air GLOWS (entry plasma) via
  the same incandescence path as rock — the fireball of a large impactor is mostly hot air.

## The emergence tests (TDD — these define "done" for v0)

1. **Scale height**: a column of air parcels under 1 g at 288 K must settle to an exponential density
   profile with H = R_s·T/g ≈ 8.4 km — the real atmosphere's shape, from nothing but the declared gas
   constants. (The atmosphere test analogous to the molten-core test.)
2. **Surface pressure**: the settled column's weight must reproduce `planet::surface_pressure` — the
   static boundary condition of docs/25 becomes the *limit* of the dynamic model (consistency).
3. **Free expansion**: parcels released in vacuum disperse (no cohesion, no fake containment).
4. **Drag emerges**: a solid body moving through settled parcels loses momentum to them —
   momentum-conserving (the air gains what the body loses) — and the swept parcels heat up.
5. **Entry glow**: at meteor entry speeds the shocked parcels' dissipation heats them past visible
   incandescence — the fireball emerges.

## Scale-relative deployment (client-capability sensitive, docs/13)

- **Planetary zoom**: the atmosphere stays the docs/25 boundary condition (declared mass → surface
  pressure → phase stability). Zero parcels, zero cost — and provably the limit of the dynamic model
  (test 2).
- **Where it matters**: parcels materialize around the observer or an entering body (the same
  promote/demote as terrain/impacts), budgeted by client capability like every other particle count.
- **Rayleigh blue / optics**: arrives with the parcel field (scattering needs a medium with density);
  until then Earth renders un-hazed, honestly.

## Status

- [x] `air` in the materials DB (real constants, cited-representative values)
- [ ] `atmosphere.rs`: parcel builder + EOS-derived contact params (v0 isothermal, flagged)
- [ ] Emergence tests 1–3 (column/scale-height, pressure consistency, free expansion)
- [ ] Drag + entry-glow tests 4–5 (needs solid-vs-gas parcel coupling — the same one contact law)
- [ ] Scale-relative wiring into the space band (parcels near an entering body only)
