# Impact thermodynamics — fracture, melt, vaporize (one data-driven response)

> Design note. Fragmentation, melting, and vaporization are **not three effects** — they are one
> response read at three energy thresholds. An impact deposits **energy density** (J/m³, which *is*
> pressure) into matter; each parcel's fate is decided by comparing that density to its own material
> thresholds: **fracture strength → melt energy → vaporization energy**. Because the deposited density
> falls with distance from the impact, *one* event produces *all three* at once — near-field vaporizes,
> the shell around it melts, farther out it fractures, farther still it's intact. That makes a
> planetary impact both a physics test and a **scale-of-detail test** (`docs/13`/`19`): the same event
> demands vapor, melt, rubble, and solid at different radii. Status: **model + data + tests landed;
> integration into the impact operator and the visual display staged.**

## The thresholds (all energy densities, J/m³)

For a parcel of a material, from a reference temperature `REF_TEMP_K` (≈300 K):

- **Fracture:** `σ` — the material's yield/fracture strength (already used by `matter::impact`).
- **Melt:** `ρ · (c·(T_melt − T₀) + L_fusion)` — heat it to melting, then the latent heat of fusion.
- **Vaporize:** melt + `ρ · (c·(T_boil − T_melt) + L_vaporization)` — then boil it off.

`damage::classify(energy_density, material)` returns `Intact | Fractured | Melted | Vaporized`. It is
the **same "energy density vs threshold"** logic as fracture, just with higher thresholds — so the
whole response is one data-driven rule, not a pile of special cases.

**Data:** `Material.thermal` (optional) carries `specific_heat`, `melt_point`, `latent_fusion`,
`boil_point`, `latent_vaporization`. Cited values added for **basalt** (the Moon), **granite** (Earth's
crust), **iron**, **water**. Materials without thermal data can still fracture, but we **do not claim**
to know their melt/boil behaviour — `classify` returns at most `Fractured` (honesty).

**Verified:** `damage::impact_fractures_then_melts_then_vaporizes_by_energy_density` — the thresholds
order correctly (`σ < melt < vapor`), each band classifies right, a giant-impact energy density
vaporizes rock, and a material with no thermal data never melts/vaporizes.

## Honest first-model caveats

- Uses the **solid** specific heat across all phases and a fixed reference temperature; real `c` varies
  with phase and temperature. Ignores **pressure** (shock impacts vaporize at different thresholds under
  pressure). Good enough to classify the regime; not a shock-physics EOS.
- "Energy density" is an energy-per-volume proxy, not a full shock-Hugoniot treatment.
- No **latent heat is subtracted** as the material transforms yet — `classify` reports the *fate*, it
  doesn't yet route the energy through the phase change and conserve it into the products.

## Staged (the visible, integrated version)

1. **Integrate into `matter::impact`:** classify each affected voxel by its local energy density —
   vaporized voxels become gas/plasma (removed or a vapor field), melted voxels become a **molten**
   material (a hot liquid parcel that flows and cools), fractured voxels are ejecta (current), intact
   remain. Conserve mass; account energy through the transitions.
2. **Display beyond text — glowing melt: LANDED (terrain scene).** Impact ejecta carry a `temp_k`;
   `emission::incandescence(temp_k)` gives a black-body glow (dull red → orange → yellow → white) that
   is *added* in the particle shader, so molten debris **self-illuminates** even on the dark side — it
   emits because it is hot, the honest analogue of illumination × reflectance. Fire it with the
   **Meteor** button / `m` key in the terrain slice: a high-energy `impact` whose core melts and glows
   while the rim is cold rubble. Still staged: a **vapor plume**, and the **celestial → voxel fly-in**
   (materialising the Moon-crash crater to fly into, `docs/19`).
3. **Cooling / solidification:** molten rock radiates and re-solidifies (magma → rock), closing the
   loop with the material model.

## Why it's the right planetary-scale test

A giant impact (the Moon onto the Earth, `docs/19`) should, honestly: **vaporize** rock near the
contact, leave a **magma ocean** of melt around it, **fracture** and eject a shell beyond, and — since
`E ≪ Earth's binding energy` — leave the planet **intact but resurfaced**. Every one of those is the
*same* `classify` call at a different radius. Getting that right, at every level of detail from the
celestial summary down to a glowing voxel, is the whole engine's thesis in one event.
