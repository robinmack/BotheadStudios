# The isotopic crisis, and why proto-Earth spin does not (yet) resolve it (2026-07-16)

## The problem

The canonical giant impact makes a proto-lunar disk that is **mostly Theia** — the impactor contributes
60–90% of the disk in most SPH simulations. But the real Moon is isotopically **almost identical to
Earth's mantle** (oxygen, titanium, tungsten, chromium isotopes all match Earth, not the Mars-like or
chondritic values Theia is expected to have). A Theia-dominated disk should have made a Moon that is
isotopically *distinct* from Earth. It isn't. This is the **isotopic crisis** — the sharpest quantitative
objection to the giant-impact hypothesis.

Two families of proposed resolutions:

1. **Ćuk & Stewart (2012)** — a *fast-spinning* proto-Earth (near the ~2.3 h rotational-stability limit)
   struck by a smaller, faster impactor. The high pre-impact angular momentum lets Earth's *own* mantle be
   flung into the disk, so the disk is Earth-derived. The excess angular momentum is later removed by the
   evection resonance with the Sun. (High-AM route.)
2. **Canup (2012)** — a near-equal-mass collision (two ~half-Earths). Both bodies contribute ~equally and
   the debris is well mixed, so the disk ≈ Earth composition because Earth ≈ Theia after mixing.
   (Equal-mass / mixing route.)
3. **Lock & Stewart synestia; Pahlevan & Stevenson** — a vaporized, turbulently **mixed** structure in
   which Earth and Theia material homogenize isotopically before the Moon condenses. (Mixing route.)

Our scene is the low-angular-momentum canonical case: a Mars-sized Theia (0.11 M⊕) on an oblique
near-parabolic approach. Its disk is Theia-dominated, as expected — so it reproduces the crisis. Option C
was to test whether physics can push it toward an Earth-like disk.

## What we implemented — proto-Earth spin

The excavated Earth cap is the planet's surface mantle, and it was **co-rotating with Earth before the
impact**. So it should be born with the local ground velocity `v = ω × (pos − centre)`, not at rest in
Earth's frame. Previously the cap was born at rest (proto-Earth spin was hardwired to zero, flagged
"unknown"). We added the co-rotation honestly:

- `impact::build_impact_debris_scaled` takes an `earth_omega` (angular velocity) and gives every
  `SOURCE_TARGET` cap grain its co-rotating velocity **before** the ploughing loft, so the momentum
  exchange acts on the real pre-impact velocity. `earth_omega = 0` is byte-identical to the old build.
- The scene (`lib.rs`) converts the proto-Earth spin angular momentum `spin_l` to `ω = L/I` (solid-sphere
  `I = 2/5 M R²`) and passes it in. The scene default remains **zero** (unknown initial condition,
  flagged) — the plumbing exists so a spin can be *explored*, but nothing on screen changes by default.

This is a physical initial condition, not a dial tuned to a target composition: we set a spin and let the
disk provenance emerge.

## What we measured

`impact::a_fast_spinning_protoearth_makes_the_disk_earth_derived` (`#[ignore]`, N = 256 debris + 512 cap,
3000 × 2 s aftermath), non-spinning vs a 2.3 h-day proto-Earth (surface velocity ω·R ≈ 4835 m/s):

| proto-Earth spin | Earth aloft | Theia aloft | disk Earth fraction |
|---|---|---|---|
| ω = 0            | 0.162 M☾    | 1.241 M☾    | **12 %** |
| ω = fast (2.3 h) | 0.181 M☾    | 2.412 M☾    | **7 %**  |

A fast spin lofts *slightly* more Earth material in absolute terms (0.162 → 0.181 M☾) and injects a large
amount of angular momentum, so the whole bound disk grows (1.40 → 2.59 M☾). **But it retains
proportionally more Theia** — Theia is most of the debris, and the extra angular momentum keeps more of it
bound — so the disk Earth *fraction* does not rise. It falls, 12 % → 7 %.

**Spinning the target does not resolve the isotopic crisis in our model.**

## Why — and the real lever

The result is a direct consequence of docs/28 **root cause #1: Earth is a rigid boundary.** The only Earth
material that can reach the disk is the small excavated cap (~0.16 M☾). The actual Ćuk & Stewart mechanism
is a spinning proto-Earth shedding its **bulk mantle** — and our Earth is a rigid analytic sphere that
cannot deform, crater, or shed mantle beyond the one cap. So:

- The measured Earth fraction is a **lower bound the rigid boundary imposes** — the bulk-mantle shedding
  that would enrich the disk in Earth material simply cannot happen in this model.
- Adding spin only speeds up the material that *is* free to move — overwhelmingly Theia debris — so it
  makes the disk *more* Theia-rich, the opposite of the intent.

The honest resolution of the isotopic crisis therefore does **not** run through target spin. It requires
one of:

1. **Earth as deformable matter** (docs/28 root cause #1 / ranked mechanism #1) — let the bulk mantle
   participate, so a spinning or hard-hit proto-Earth can shed its own mantle into the disk. This is the
   prerequisite for the Ćuk & Stewart route to even be expressible here.
2. **Vapor-phase Earth↔Theia mixing** (the Lock & Stewart / Pahlevan route) — now partly within reach: we
   have a real SPH vapor field (docs/26/27). If shock-vaporized Earth and Theia material mix in the vapor
   disk before condensing, the isotopic signature homogenizes regardless of the mass split. Measuring the
   *mixing* (not just the mass provenance) of the vapor disk is the next honest experiment.
3. A different **impact geometry** — a smaller, faster impactor or a near-equal-mass (Canup) collision —
   which changes the impactor/target mass ratio rather than relying on spin.

## Status

- Proto-Earth spin is plumbed end to end (`earth_omega` → co-rotating cap → scene), correct physics,
  default zero (flagged unknown IC). No behaviour change on screen.
- Measured, no-fudge: spin alone raises total retained disk mass but does **not** Earth-enrich the disk;
  the rigid boundary caps the Earth fraction. Test asserts the robust mechanics (spin ⇒ larger bound disk)
  and the measured ceiling (fraction does not rise), and prints the provenance split.
- Next lever for the crisis: Earth-as-matter (#1 above) or vapor mixing (#2). Spin is not the lever.
