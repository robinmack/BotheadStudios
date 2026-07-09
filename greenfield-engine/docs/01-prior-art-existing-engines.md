# Prior Art: Physical-Property / Material Simulation Games & Engines

> Research report #1 for the greenfield browser physics engine.
> Destined for: `~/workspace/greenfield-engine/docs/01-prior-art-existing-engines.md`

Your vision has four fused pillars: (1) matter as **aggregates of particles with mass/density**, (2) **material behavior emergent from density** rather than authored per-type, (3) **destructible "all the way down"** (true volumetric, not textured shells), and (4) **aggregate mass producing real gravitational attraction** (F=ma, planetary gravity). No existing product combines all four.

## Comparison Table

| Name | 2D/3D | Open Source + License | Browser? | Relevance to vision |
|------|-------|----------------------|----------|---------------------|
| **Noita** ("Falling Everything" engine) | 2D | No — proprietary C++ | No (native) | **High.** Every pixel simulated as a material (burn, melt, freeze, flow). Closest to "material sim as the core loop," but 2D, closed, no gravity-as-attraction. |
| **The Powder Toy** | 2D | **Yes — GPL-3.0** | No (desktop; unofficial web ports) | **High.** 258 elements; air pressure, velocity, heat, gravity modes. Best OSS reference for material-interaction breadth. Grid-based, not mass/density-derived. |
| **Sandspiel** | 2D | **Yes — MIT** (Rust→WASM + WebGL) | **Yes** | **High for browser tech.** Proves Rust/WASM cellular sim runs performantly in-browser. Simple element set; density loosely modeled. |
| **Sandspiel Studio** | 2D | Yes | **Yes** | Medium. Block-based authoring of custom elements — model for user-extensible materials. |
| **Sandboxels** | 2D | Source on GitHub, custom "free to use" license (not OSI/GPL) | **Yes** (JS/Canvas) | **High for breadth.** 500+ elements with explicit **density**, heat, chemistry, electricity. Density already drives layering/settling — conceptually nearest to "behavior from density." 2D only. |
| **Teardown** | 3D | No — proprietary voxel engine | No | **High for 3D destruction.** Voxel volumes on a grid; materials carry physical type; SIMD/multithreaded. Not "down to particles/mass," no gravity field. |
| **Space Engineers** | 3D | No (proprietary; some tools/mods open) | No | **High.** Fully destructible **volumetric voxel** planets/asteroids with **natural gravitational fields**, materials, atmosphere. Two of four pillars. Voxels material-typed, not mass/density-derived. |
| **Astroneer** | 3D | No — proprietary | No | Medium-High. Deformable voxel terrain via **marching cubes**; deform = add/subtract density per chunk, re-polygonize locally. Great model for "edit density, re-mesh." |
| **Enshrouded** ("Holistic" engine) | 3D | No — proprietary voxel engine | No | Medium-High. Fully destructible voxel terrain + building; deliberately **non-physical**. Strong destruction reference, weak on real physics. |
| **Minecraft + physics mods** | 3D | Game proprietary; mods vary | No | Medium. Mods add falling blocks, structural support, per-block mass/strength, planetary-gravity presets. Blocky, bolted-on. |
| **From Dust** | 2.5D (heightmap) | No — proprietary (Ubisoft) | No | Medium-High conceptually. Real-time particle sim of lava/water/soil with erosion, cooling-to-rock, viscosity. Heightmap, not volumetric. |
| **Deep Rock Galactic** | 3D | No — proprietary | No | Medium. Fully destructible caves with per-material hardness, but **grid-less polygonal** — counter-example to voxel destruction. |
| **Planetary Annihilation** | 3D | No — proprietary | No | Low-Medium. Whole-planet smashing, but terrain meshed not volumetric (voxels **rejected** for scaling). Gravity scripted. |
| **Universe Sandbox** | 3D | No — proprietary | No | Medium (gravity pillar only). True **N-body Newtonian gravity**, collisions, material/climate. Bodies are point-masses, not excavatable. |
| **Universe Sandbox Web** (it-efrem) | 3D | Yes (GitHub) | **Yes** | Low-Medium. Browser N-body gravity demo; minimal material depth. |
| **Populous** (1989) | 2.5D | No — proprietary | No | Low (historical). Raise/lower terrain; conceptual ancestor. |
| **Outer Wilds** | 3D | No — proprietary | No | Low. Real orbital mechanics, but static-mesh terrain — no material/destruction sim. |
| **MPM-World** (Lee-abcde) | 2D/3D | **Yes — open source**, Taichi Lang | No (GPU/desktop) | **High (academic).** Material Point Method for fluid/sand/snow/elastic — genuine continuum matter from particles-with-mass. The physics you want, minus game/browser/gravity. |
| **MPM-Taichi / MLS-MPM / WindQAQ MPM** | 2D/3D | **Yes — open source**, Taichi/CUDA | No | **High (academic).** GPU MPM demos; materials differ by constitutive parameters. Reference implementations. |
| **Taichi (taichi-lang)** | N/A | **Yes — Apache-2.0** | Partial (WASM/JS backends limited) | High as tooling. Go-to framework for performant MPM/PBD granular sims. |
| **TPTBox** (Bowserinator) | 3D | Yes (GitHub) | No | Medium. 3D Powder-Toy-style falling-sand — rare 3D cellular sim in OSS. |

## Which systems come closest — and the gaps

**Closest on "material behavior as the core loop": the falling-sand lineage (Noita, The Powder Toy, Sandboxels, Sandspiel).** These make material interaction *the entire game*, and Sandboxels already treats **density** as a first-class driver of settling/layering — the closest match to "materials get their behavior from density." The Powder Toy (GPL-3.0) and Sandspiel (MIT, Rust→WASM→WebGL, runs in-browser) are the two best OSS starting points. Gap: **all 2D grid cellular automata**, where "material" is a cell type with hand-tuned transition rules, not aggregates carrying real mass; none derive behavior from density in a first-principles Newtonian way; none have gravitational attraction.

**Closest on "destructible all the way down in 3D": Teardown, Space Engineers, Astroneer, Enshrouded.** These prove volumetric (not shell) destruction is shippable. **Space Engineers is the single most-overlapping product**: destructible volumetric voxel planets/asteroids *with real gravitational fields* and typed materials. Astroneer's recipe (store density per voxel, edit by adding/subtracting density, re-run marching cubes on the dirty chunk) is a battle-tested model for "edit matter, re-mesh locally at scale." Gap: voxels are material-typed cells with hardness/health, **not particle aggregates whose mass/density is integrated into F=ma**; destruction is discrete chunk removal + rigid debris; gravity (where present) is a scripted radial field, not the emergent sum of the aggregate's own mass.

**Closest on the actual physics: the MPM / granular-continuum academic stack.** The Material Point Method *is* "matter as aggregates of particles with mass and density, whose behavior emerges from constitutive properties" — sand, snow, mud, elastic solids from the same particle-grid transfer with different parameters. Correct theoretical foundation for pillars 1–3, much of it OSS (Taichi Apache-2.0). Gaps are the inverse of the games': no gameplay, no browser deployment (GPU/desktop-bound), no self-gravity, doesn't scale to planet-sized worlds in real time.

**Net: nobody has fused all four pillars.** The two hardest gaps unique to this vision: (a) **density-derived, self-gravitating matter at planetary scale in real time** — Universe Sandbox does real N-body self-gravity but only on point-masses you can't excavate; voxel games have planet gravity but scripted, not the integral of the world's own mass. (b) **A true MPM-like continuum matter model running destructibly in a browser** — sand games are in-browser but 2D-cellular and non-physical about mass; MPM engines are physical but native/GPU. What must be built is the bridge: a 3D, streaming, chunked volumetric material store (density per voxel, à la Astroneer/Space Engineers) whose cells carry real mass and feed **both** a granular/continuum solver (MPM or PBD granular) **and** an aggregate-gravity solver (à la Universe Sandbox N-body, from summed voxel mass), all compiled to WASM/WebGPU (the Sandspiel model, scaled to 3D). Each piece exists in isolation; the novel engineering is making **density the single source of truth** that simultaneously drives material behavior, destruction, and gravity in one real-time browser loop.

## Sources

Falling-sand / cellular:
- https://en.wikipedia.org/wiki/Noita_(video_game) · https://80.lv/articles/noita-a-game-based-on-falling-sand-simulation · https://www.gdcvault.com/play/1025695/Exploring-the-Tech-and-Design
- https://en.wikipedia.org/wiki/The_Powder_Toy · https://github.com/The-Powder-Toy/The-Powder-Toy · https://powdertoy.co.uk/ · https://github.com/Bowserinator/TPTBox
- https://github.com/MaxBittker/sandspiel · https://maxbittker.com/making-sandspiel/ · https://studio.sandspiel.club/
- https://github.com/R74nCom/sandboxels/ · https://sandboxels.r74n.com/

Voxel / deformable-terrain 3D:
- https://en.wikipedia.org/wiki/Teardown_(video_game) · https://80.lv/articles/teardown-developer-breaks-down-multiplayer-and-voxel-destruction-tech
- https://en.wikipedia.org/wiki/Space_Engineers · https://spaceengineers.fandom.com/wiki/Voxel_Hands · https://spaceengineers.wiki.gg/wiki/Gravity
- https://www.gamedeveloper.com/design/what-i-astroneer-i-s-devs-learned-while-leaving-early-access · https://astroneer.wiki.gg/wiki/Terrain_Tool
- https://www.gtxgaming.co.uk/building-new-worlds-exploring-enshroudeds-voxel-based-system/
- https://deeprockgalactic.wiki.gg/wiki/Terrain
- https://en.wikipedia.org/wiki/From_Dust · https://www.gamedeveloper.com/design/the-core-of-i-from-dust-i-
- https://modrinth.com/mod/realistic-block-physics

Planetary-gravity:
- https://universesandbox.com/ · https://github.com/it-efrem/Universe-Sandbox-Web
- https://en.wikipedia.org/wiki/Planetary_Annihilation · http://mavorsrants.blogspot.com/2013/02/planetary-annihilation-engine.html

Academic / OSS continuum & granular:
- https://github.com/Lee-abcde/MPM-World · https://simulately.wiki/docs/snippets/taichi/mpm/ · https://github.com/gizemdal/MPM-Taichi · https://github.com/CzzzzH/MLS-MPM · https://github.com/WindQAQ/MPM · https://github.com/topics/material-point-method
