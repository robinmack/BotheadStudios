//! Layered planets (docs/25): a planet is DECLARED as its real composition — concentric layers of real
//! materials with their observed mean (compressed) densities and temperatures — and everything else is
//! COMPUTED:
//!
//!   • gravity g(r): Gauss's law over the enclosed layer mass (not a point);
//!   • pressure P(r): hydrostatic equilibrium, dP/dr = −ρ(r)·g(r), integrated from the surface;
//!   • PHASE (solid vs molten): the material's Simon–Glatzel melting curve T_m(P) against the local
//!     temperature — never assigned.
//!
//! The emergence test this module must pass (Robin): Earth's OUTER core is molten but its INNER core is
//! SOLID — not because we said so, but because pressure raises iron's melting point faster than the
//! geotherm rises, so the melt curve crosses the temperature profile at the real inner-core boundary.
//! A fudged composition could never produce that; the real one does.
//!
//! Honesty notes: layer mean densities are the compressed in-situ values (PREM) — the surface-condition
//! `Material::density` can't know compression, so the profile carries them (declared data, cited).
//! Temperatures are the observed geotherm/selenotherm (declared data): deriving them would need thermal
//! history, radiogenic heating and convection — future physics, flagged. Phase and pressure are computed.

use crate::materials::Material;
use glam::DVec3;

/// One concentric layer: real material + observed mean density/temperature profile across it.
#[derive(Clone, Debug)]
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layer {
    /// Material id in the catalog (`materials.json`).
    pub material: String,
    /// Outer radius of the layer (m), from the body centre.
    pub outer_r: f64,
    /// Mean in-situ density (kg/m³) — compressed (PREM), not the surface-condition material density.
    pub density: f64,
    /// Temperature at the layer's inner and outer boundary (K); linear in between (declared geotherm).
    pub t_inner: f64,
    pub t_outer: f64,
}

/// A layered body: layers ordered inside-out; `layers.last().outer_r` is the surface.
#[derive(Clone, Debug)]
#[derive(serde::Deserialize)]
pub struct LayeredBody {
    pub layers: Vec<Layer>,
    /// Total atmosphere mass (kg) — DECLARED (measured; Earth: 5.15e18 kg). The surface pressure is
    /// never declared: it EMERGES as the weight of this column, P = M·g/(4πR²) — which for Earth's
    /// declared mass comes out ≈1 atm. Zero for an airless body (the Moon): then open liquids boil off
    /// (see `surface_phase`) — water on a vacuum world is not stable, as a consequence of the model.
    #[serde(rename = "atmosphere_mass_kg")]
    pub atmosphere_mass: f64,
    /// This body's SURFACE — its continents, elevation and land cover. It belongs to the BODY, not to a
    /// scene: Earth has one set of continents no matter which world places it, so a scene may position
    /// Earth but may never redefine it. `None` for a body we have no surface data for.
    #[serde(default)]
    pub surface: Option<crate::terra::world_def::Surface>,
    #[serde(default, rename = "name")]
    pub name: String,
    #[serde(default, rename = "$comment")]
    _comment: String,
}

/// Phase of matter at a point — computed from T vs the material's pressure-dependent melting AND
/// boiling curves. `Molten` is the liquid state (a molten core and a liquid ocean are the same phase at
/// different temperatures); `Vapor` is gas (boiled off).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Solid,
    Molten,
    Vapor,
}

const G: f64 = 6.674e-11;

impl LayeredBody {
    pub fn radius(&self) -> f64 {
        self.layers.last().map_or(0.0, |l| l.outer_r)
    }

    /// The layer containing radius `r` (clamped to the surface layer).
    pub fn layer_at(&self, r: f64) -> &Layer {
        self.layers
            .iter()
            .find(|l| r <= l.outer_r)
            .unwrap_or_else(|| self.layers.last().expect("layered body has layers"))
    }

    /// Declared temperature (K) at radius `r` — linear within its layer (the observed geotherm).
    pub fn temperature_at(&self, r: f64) -> f64 {
        let mut inner_r = 0.0;
        for l in &self.layers {
            if r <= l.outer_r {
                let f = ((r - inner_r) / (l.outer_r - inner_r).max(1.0)).clamp(0.0, 1.0);
                return l.t_inner + (l.t_outer - l.t_inner) * f;
            }
            inner_r = l.outer_r;
        }
        self.layers.last().map_or(0.0, |l| l.t_outer)
    }

    /// Mass enclosed within radius `r` (kg) — the source of Gauss's-law gravity.
    pub fn enclosed_mass(&self, r: f64) -> f64 {
        let mut m = 0.0;
        let mut inner_r: f64 = 0.0;
        for l in &self.layers {
            let ro = l.outer_r.min(r);
            if ro > inner_r {
                m += l.density * (4.0 / 3.0) * std::f64::consts::PI
                    * (ro.powi(3) - inner_r.powi(3));
            }
            inner_r = l.outer_r;
            if inner_r >= r {
                break;
            }
        }
        m
    }

    pub fn total_mass(&self) -> f64 {
        self.enclosed_mass(self.radius())
    }

    /// Gravity g(r) (m/s²), COMPUTED: Gauss's law over the enclosed mass — rises through the mantle,
    /// peaks near the core boundary, falls to zero at the centre. Nothing assumed.
    pub fn gravity_at(&self, r: f64) -> f64 {
        if r < 1.0 {
            return 0.0;
        }
        G * self.enclosed_mass(r) / (r * r)
    }

    /// Gravitational acceleration (m/s²) at a world `point` for a body centred at `center` — the
    /// positioned, sample-anywhere vector form of [`gravity_at`](Self::gravity_at): `−r̂ · G·M(<r)/r²`
    /// over the real differentiated [`enclosed_mass`](Self::enclosed_mass) profile (Gauss interior → 0 at
    /// the centre; monopole `G·M_total/r²` outside, since `enclosed_mass` saturates to the total). This is
    /// the T0 **bulk** gravity a particalized region feels (docs/39): a particle at `point` is pulled
    /// toward `center` by the mass enclosed below it. Zero within 1 m of the centre (matches `gravity_at`).
    pub fn acceleration_at(&self, point: DVec3, center: DVec3) -> DVec3 {
        let d = point - center;
        let r = d.length();
        if r < 1.0 {
            return DVec3::ZERO;
        }
        -(d / r) * self.gravity_at(r)
    }

    /// Surface pressure (Pa), EMERGENT: the weight of the declared atmosphere column spread over the
    /// sphere, P = M_atm·g/(4πR²). Earth's declared 5.15e18 kg comes out ≈1 atm; an airless body is 0.
    pub fn surface_pressure(&self) -> f64 {
        let r = self.radius();
        if r <= 0.0 {
            return 0.0;
        }
        self.atmosphere_mass * self.gravity_at(r) / (4.0 * std::f64::consts::PI * r * r)
    }

    /// Pressure P(r) (Pa), COMPUTED: hydrostatic equilibrium dP/dr = −ρ·g integrated inward from the
    /// surface (whose pressure is the atmosphere's weight, `surface_pressure`). This is what "1 g of
    /// overburden, all the way down" adds up to — Earth's centre comes out ≈360 GPa from the declared
    /// densities alone.
    pub fn pressure_at(&self, r: f64) -> f64 {
        let surface = self.radius();
        let r = r.clamp(0.0, surface);
        // Integrate ρ(x)·g(x) dx from r to the surface (midpoint rule; profile is smooth).
        const STEPS: usize = 400;
        let dx = (surface - r) / STEPS as f64;
        if dx <= 0.0 {
            return self.surface_pressure();
        }
        let mut p = self.surface_pressure();
        for i in 0..STEPS {
            let x = r + (i as f64 + 0.5) * dx;
            p += self.layer_at(x).density * self.gravity_at(x) * dx;
        }
        p
    }

    /// PHASE at radius `r` — EMERGENT: the declared temperature against the material's Simon–Glatzel
    /// melting curve (and Clausius–Clapeyron boiling curve) at the computed hydrostatic pressure.
    /// Molten cores, solid inner cores, solid mantles all fall out of gravity + mass + material; none is
    /// assigned.
    pub fn phase_at(&self, r: f64, mats: &[Material]) -> Phase {
        let layer = self.layer_at(r);
        let mat = &mats[crate::materials::index_of(mats, &layer.material)];
        surface_phase(mat, self.temperature_at(r), self.pressure_at(r))
    }
}

/// Phase of a material at temperature `t` (K) under ambient pressure `p` (Pa) — the general P–T phase
/// decision, from the material's pressure-dependent melting and boiling curves. THE consequence Robin
/// called out: with `p = 0` (vacuum) the boiling curve collapses to 0 K, so any liquid — water above
/// all — flashes to vapor. Open oceans exist only under an atmosphere's weight; nothing enforces this,
/// it falls out of Clausius–Clapeyron.
pub fn surface_phase(mat: &Material, t: f64, p: f64) -> Phase {
    let Some(th) = mat.thermal.as_ref() else {
        return Phase::Solid; // uncharacterized: we don't claim to know its melt/boil (honesty)
    };
    if t >= th.boil_point_at(p) {
        Phase::Vapor
    } else if t >= th.melt_point_at(p) {
        Phase::Molten
    } else {
        Phase::Solid
    }
}

/// EARTH, declared as its real construction (PREM densities, observed geotherm):
/// solid-iron inner core, iron outer core, peridotite mantle, basalt crust. The ocean/continents live on
/// the surface columns (rendering + impact sampling), not as a layer here (mean depth ~3.7 km ≪ grain).
/// Load a BODY DEFINITION by id. Bodies are **fixtures, not engine code**: Earth is a thing the engine
/// is pointed at, not a behaviour it implements. Keeping them in `assets/bodies/*.json` means there is
/// exactly ONE Earth — every scene that names `earth` gets the same layers, the same air, the same
/// emergent surface pressure — and improving Earth improves every world at once, with no recompile of
/// the physics. The engine knows how to turn *a* layered body into gravity, pressure and phase; it does
/// not know which body you meant until you say.
///
/// FLAGGED (IOU): these are `include_str!`d at build time, so shipping a NEW body still needs a rebuild.
/// Fetching them like `worlds/*.json` (so anyone can define a planet without touching the engine) is the
/// next step, and the reason they already live in their own directory rather than inside a world file.
pub fn body(id: &str) -> LayeredBody {
    let src = match id {
        "earth" => include_str!("../../../assets/bodies/earth.json"),
        "proto-earth" => include_str!("../../../assets/bodies/proto-earth.json"),
        "moon" => include_str!("../../../assets/bodies/moon.json"),
        "sun" => include_str!("../../../assets/bodies/sun.json"),
        "theia" => include_str!("../../../assets/bodies/theia.json"),
        other => panic!("unknown body definition {other:?} (add assets/bodies/{other}.json)"),
    };
    serde_json::from_str(src).unwrap_or_else(|e| panic!("body {id:?} is malformed: {e}"))
}

pub fn earth() -> LayeredBody {
    body("earth")
}


/// The MOON, declared as its real construction (GRAIL/Apollo seismology): small solid inner core,
/// fluid outer core (the "hot molten core"), thick peridotite mantle, basalt crust. HONESTY NOTE: the
/// real lunar core is an Fe–S alloy whose eutectic melts well BELOW pure iron — that's the physical
/// reason the outer core is molten at only ~1,900–2,000 K. Our catalog models pure iron, so we use the
/// upper range of the published selenotherm (Weber et al. 2011: partially molten outer core,
/// ~1,900–2,000 K); an Fe–S material entry is the honest refinement (flagged).
pub fn moon() -> LayeredBody {
    body("moon")
}


/// Earth's land/ocean mask at 10°×10° — matched to the ~9° angular spacing of the 512-grain render
/// shell, so each grain samples one cell ("average area particles"). '#' = land, '.' = ocean. Rows from
/// 85°N to 85°S, columns from 180°W to 180°E. HONESTY: a coarse hand-digitized summary of the real
/// continents (flagged; a cited high-res landmask dataset is the refinement), same spirit as
/// `aggregate_albedo` — the best average of what's really there at this resolution.
const LANDMASK: [&str; 18] = [
    "....................................", // 85N arctic ocean
    "......###...##..........####........", // 75N canadian arctic·greenland·taymyr
    ".###########.##.#.################..", // 65N alaska+canada·greenland·iceland·eurasia
    "..##########......###############...", // 55N canada·uk·eurasia
    ".....######.......##############....", // 45N usa·europe·asia
    ".....#####.......#.############.....", // 35N usa·med·asia
    ".......###......########.#####......", // 25N mexico·sahara-arabia·india·se-asia
    ".......###......#####....###..#.....", // 15N c-america·sahel·india·philippines
    "..........###....######....###......", // 5N colombia·africa·indonesia
    "..........####....####.....##..##...", // 5S brazil·africa·java·new-guinea
    "..........####....####........##....", // 15S brazil·angola-mozambique·n-australia
    "..........####.....####......####...", // 25S chile-brazil·s-africa+madagascar·australia
    "..........###......##..........##..#", // 35S chile-argentina·s-africa·se-australia·nz
    "..........##......................##", // 45S patagonia·new-zealand
    "..........##........................", // 55S patagonia tip
    "...........##...........####........", // 65S antarctic peninsula·e-antarctic coast
    "...#########...###################..", // 75S antarctica (weddell + ross seas open)
    "####################################", // 85S antarctica
];

/// Is the surface land (true) or ocean (false) at this latitude/longitude (degrees)? NOTE: the model
/// has no planetary rotation yet, so the mask's orientation is arbitrary but consistent (flagged).
pub fn is_land(lat_deg: f64, lon_deg: f64) -> bool {
    let row = (((90.0 - lat_deg) / 10.0).floor() as isize).clamp(0, 17) as usize;
    let col = (((lon_deg + 180.0) / 10.0).floor() as isize).rem_euclid(36) as usize;
    LANDMASK[row].as_bytes()[col] == b'#'
}

/// The surface material seen at a unit direction from Earth's centre: granite continents or water ocean
/// (the ocean's mean depth ~3.7 km is far below one shell grain, so at this LOD water is the material of
/// the grain's top — "filled with water" at the resolution we can honestly claim).
pub fn earth_surface_material(dir: glam::DVec3) -> &'static str {
    let (lat, lon) = crate::geo::lat_lon_from_dir(dir);
    if is_land(lat, lon) {
        "granite"
    } else {
        "water"
    }
}

/// THE SUN, declared as its real construction (standard solar model, coarse 3-layer binning): H–He
/// plasma whose layer mean densities carry the enormous compression (150,000 kg/m³ core → ~0.1 near
/// the surface), exactly as PREM's do for Earth. Its mass — and therefore every orbit in the system —
/// EMERGES from the declared composition; SUN_MASS-the-constant retires (docs/27: "calculate the sun's
/// mass as a material so the gravity manifests naturally", Robin).
pub fn sun() -> LayeredBody {
    body("sun")
}


/// THEIA — the Mars-sized impactor of the giant-impact hypothesis (docs/27: the birth of the Moon).
/// Declared as a differentiated Mars-like body: iron core, peridotite mantle, hot from accretion.
/// Layer masses integrate to ~6.5e23 kg (Mars-scale, the theorized impactor class).
pub fn theia() -> LayeredBody {
    body("theia")
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::materials;

    #[test]
    fn declared_earth_composition_reproduces_the_real_mass_and_surface_gravity() {
        let e = earth();
        let m = e.total_mass();
        assert!(
            (m - 5.972e24).abs() / 5.972e24 < 0.03,
            "PREM layer densities integrate to Earth's real mass (got {m:.3e} kg)"
        );
        let g = e.gravity_at(e.radius());
        assert!((g - 9.81).abs() < 0.3, "surface gravity ≈ 9.8 m/s² (got {g:.2})");
    }

    #[test]
    fn positioned_gravity_matches_the_radial_profile_and_the_gauss_limits() {
        // docs/39 39a: the vector `acceleration_at(point, center)` is the T0 bulk gravity a particalized
        // region feels — it must equal the scalar `gravity_at(r)` in magnitude, point at the centre, be a
        // monopole outside, and fall toward zero at the centre (Gauss interior).
        let e = earth();
        let c = DVec3::new(1.0e6, -2.0e6, 3.0e6); // arbitrary body centre
        let big_r = e.radius();
        let dir = DVec3::new(0.3, -0.5, 0.81).normalize();
        for &r in &[0.05 * big_r, 0.2 * big_r, 0.5 * big_r, 0.9 * big_r, big_r, 1.5 * big_r, 5.0 * big_r] {
            let a = e.acceleration_at(c + dir * r, c);
            let g = e.gravity_at(r);
            assert!((a.length() - g).abs() <= 1.0e-6 * g.max(1.0e-9), "|a| == gravity_at(r) at r={r:.3e}");
            assert!(a.normalize().dot(-dir) > 1.0 - 1.0e-9, "gravity points toward the centre at r={r:.3e}");
        }
        // Exterior → the monopole G·M_total/r².
        let r_ext = 3.0 * big_r;
        let g_mono = G * e.total_mass() / (r_ext * r_ext);
        let a_ext = e.acceleration_at(c + dir * r_ext, c);
        assert!((a_ext.length() - g_mono).abs() < 1.0e-6 * g_mono, "exterior is a monopole");
        // Centre → exactly zero; and g falls toward the centre (Gauss interior, ∝ r in a uniform core).
        assert_eq!(e.acceleration_at(c, c), DVec3::ZERO);
        let deep = e.acceleration_at(c + dir * (0.04 * big_r), c).length();
        let mid = e.acceleration_at(c + dir * (0.10 * big_r), c).length();
        assert!(deep < mid, "gravity falls toward the centre (got deep {deep:.3} ≥ mid {mid:.3})");
    }

    #[test]
    fn hydrostatic_pressure_reaches_earths_real_central_pressure() {
        let e = earth();
        let p_c = e.pressure_at(0.0);
        // Real: ~364 GPa. Piecewise-constant PREM means ±10% is honest.
        assert!(
            (3.2e11..4.1e11).contains(&p_c),
            "central pressure from hydrostatics ≈ 360 GPa (got {:.1} GPa)",
            p_c / 1e9
        );
        let p_cmb = e.pressure_at(3.48e6);
        assert!(
            (1.15e11..1.55e11).contains(&p_cmb),
            "core–mantle boundary ≈ 135 GPa (got {:.1} GPa)",
            p_cmb / 1e9
        );
    }

    #[test]
    fn earths_molten_outer_core_and_solid_inner_core_emerge_from_pressure() {
        // THE emergence test: nobody assigns phases. The inner core is HOTTER than the outer core, yet
        // it must come out SOLID — because the computed pressure pushes iron's melting curve above the
        // geotherm there. The outer core must come out MOLTEN, the mantle and crust SOLID.
        let mats = materials::load();
        let e = earth();
        assert_eq!(e.phase_at(6.0e5, &mats), Phase::Solid, "inner core: solid (pressure-frozen iron)");
        assert_eq!(e.phase_at(2.4e6, &mats), Phase::Molten, "outer core: molten iron");
        assert_eq!(e.phase_at(5.0e6, &mats), Phase::Solid, "mantle: solid rock");
        assert_eq!(e.phase_at(6.36e6, &mats), Phase::Solid, "crust: solid");
    }

    #[test]
    fn the_declared_atmosphere_mass_weighs_in_at_one_atmosphere() {
        // Nobody declares "1 atm": the measured MASS of the air column (5.15e18 kg) is declared, and
        // its weight over the sphere comes out ≈101 kPa. The Moon declares no atmosphere ⇒ 0 Pa.
        let p = earth().surface_pressure();
        assert!(
            (9.5e4..1.07e5).contains(&p),
            "Earth's atmosphere mass ⇒ ~1 atm at the surface (got {p:.0} Pa)"
        );
        assert_eq!(moon().surface_pressure(), 0.0, "the Moon is airless");
    }

    #[test]
    fn liquid_oceans_exist_under_an_atmosphere_and_boil_off_in_vacuum() {
        // Robin: "we need an atmosphere to keep the water from boiling off into space — naturally, as a
        // consequence of the model." Clausius–Clapeyron delivers exactly that: at Earth's emergent
        // surface pressure, 288 K water is LIQUID; expose the same water to vacuum (the Moon) and its
        // boiling point collapses to 0 K, so it flashes to VAPOR at any temperature. Cold water under
        // pressure freezes. None of this is scripted — it's the material's phase physics.
        let mats = materials::load();
        let water = &mats[materials::index_of(&mats, "water")];
        let p_earth = earth().surface_pressure();
        assert_eq!(surface_phase(water, 288.0, p_earth), Phase::Molten, "ocean: liquid under 1 atm");
        assert_eq!(surface_phase(water, 288.0, 0.0), Phase::Vapor, "vacuum: boils off at any temp");
        assert_eq!(surface_phase(water, 250.0, p_earth), Phase::Solid, "cold: ice");
        // Below the triple-point pressure (~611 Pa) liquid water cannot exist even when warm.
        assert_eq!(
            surface_phase(water, 285.0, 300.0),
            Phase::Vapor,
            "sub-triple-point pressure: no liquid regime"
        );
        // And the boiling point at altitude drops (why water boils cooler on a mountain): ~0.7 atm.
        let tb = water.thermal.as_ref().unwrap().boil_point_at(0.7 * 101_325.0);
        assert!((360.0..371.0).contains(&tb), "boils below 373 K at 0.7 atm (got {tb:.1} K)");
    }

    #[test]
    fn the_landmask_places_the_major_continents_and_oceans() {
        assert!(is_land(5.0, 20.0), "central Africa");
        assert!(is_land(45.0, -100.0), "north America");
        assert!(is_land(-10.0, -55.0), "Brazil");
        assert!(is_land(55.0, 40.0), "Russia");
        assert!(is_land(-25.0, 135.0), "Australia");
        assert!(is_land(-80.0, 0.0), "Antarctica");
        assert!(!is_land(0.0, -150.0), "equatorial Pacific");
        assert!(!is_land(30.0, -45.0), "north Atlantic");
        assert!(!is_land(-30.0, 80.0), "Indian ocean");
        assert!(!is_land(85.0, 0.0), "Arctic ocean");
        // ~29% of Earth's surface is land. AREA-weighted (a 10° cell shrinks as cos(lat)); the coarse
        // hand mask over-represents land somewhat (~0.37 — flagged; a cited landmask dataset is the
        // refinement) but must stay in the honest neighbourhood.
        let mut land = 0.0f64;
        let mut total = 0.0f64;
        for row in 0..18 {
            let lat = (85 - 10 * row) as f64;
            let w = lat.to_radians().cos(); // area weight
            for col in 0..36 {
                let lon = -175.0 + 10.0 * col as f64;
                total += w;
                if is_land(lat, lon) {
                    land += w;
                }
            }
        }
        let frac = land / total;
        assert!((0.25..0.42).contains(&frac), "land fraction plausible (got {frac:.2})");
    }

    #[test]
    fn the_declared_solar_composition_yields_the_real_solar_mass_and_gravity() {
        let s = sun();
        let m = s.total_mass();
        assert!(
            (m - 1.989e30).abs() / 1.989e30 < 0.03,
            "solar layers integrate to the real solar mass (got {m:.3e} kg)"
        );
        let g = s.gravity_at(s.radius());
        assert!((g - 274.0).abs() < 12.0, "photospheric gravity ≈ 274 m/s² (got {g:.0})");
    }

    #[test]
    fn the_moons_outer_core_is_molten_but_its_mantle_is_not() {
        let mats = materials::load();
        let m = moon();
        assert_eq!(m.phase_at(3.0e5, &mats), Phase::Molten, "lunar outer core: molten (hot, low P)");
        assert_eq!(m.phase_at(1.0e6, &mats), Phase::Solid, "lunar mantle: solid");
        let mass = m.total_mass();
        assert!(
            (mass - 7.342e22).abs() / 7.342e22 < 0.05,
            "lunar layers integrate to the Moon's real mass (got {mass:.3e})"
        );
    }
}

#[cfg(test)]
mod body_definition_tests {
    /// Every body definition on disk must LOAD, and every id the engine offers must RESOLVE. This exists
    /// because the first cut of the extraction shipped `theia.json` without its match arm: the file was
    /// right there and the engine still panicked "unknown body". A body is data, so the data directory is
    /// the source of truth — if these two ever disagree again, this fails instead of a scene.
    #[test]
    fn every_body_definition_loads_and_every_id_resolves() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/bodies");
        let mut on_disk: Vec<String> = std::fs::read_dir(dir)
            .expect("assets/bodies must exist")
            .filter_map(|e| {
                let p = e.ok()?.path();
                (p.extension()? == "json").then(|| p.file_stem()?.to_str().map(str::to_owned))?
            })
            .collect();
        on_disk.sort();
        assert!(!on_disk.is_empty(), "no body definitions found in {dir}");

        for id in &on_disk {
            let b = super::body(id);
            assert!(!b.layers.is_empty(), "{id} declares no layers");
            // Layers must be ordered outward from the centre — the pressure integral walks them inward.
            for w in b.layers.windows(2) {
                assert!(w[1].outer_r > w[0].outer_r, "{id}: layers must be ordered outward");
            }
            assert!(b.atmosphere_mass >= 0.0, "{id}: negative atmosphere mass");
        }
        // Earth must be Earth, from the file — the single definition every scene now shares.
        let e = super::earth();
        assert!((e.radius() - 6.371e6).abs() < 1e3, "earth.json radius {} m", e.radius());
        assert!((e.surface_pressure() / 101_325.0 - 1.0).abs() < 0.06,
            "Earth's declared air must still weigh ~1 atm at the surface (got {:.0} Pa)", e.surface_pressure());
    }
}

#[cfg(test)]
mod star_appearance_tests {
    /// **The Sun's apparent size is not drawn, it is a consequence.** Its declared radius at Earth's real
    /// orbital distance must subtend the ~0.53° every observer measures. Nothing in the renderer sets an
    /// angle: it draws a sphere of this radius at that distance, so the disk grows on approach and shrinks
    /// from Mars for free. If this drifts, someone has started painting the sky.
    #[test]
    fn the_declared_sun_subtends_half_a_degree_from_earth() {
        let r = super::body("sun").radius();
        assert!((r - 6.96e8).abs() < 1e7, "declared solar radius {r:.3e} m");
        const AU: f64 = 1.495_978_707e11;
        let angular_deg = 2.0 * (r / AU).atan().to_degrees();
        assert!(
            (angular_deg - 0.533).abs() < 0.01,
            "the Sun must subtend ~0.53° from Earth, got {angular_deg:.4}°"
        );
        // And ~2× that from Mercury's perihelion (4.6e10 m) — the same sphere, no special case.
        let from_mercury = 2.0 * (r / 4.6e10).atan().to_degrees();
        assert!(from_mercury > 1.6, "closer ⇒ bigger: {from_mercury:.2}° from Mercury");
    }

    /// The Sun's COLOUR comes from its declared photosphere temperature, not from a hardcoded white. At
    /// 5,772 K that is white — which is what the Sun looks like from space; the "yellow sun" is what the
    /// atmosphere does to it, seen from underneath. The point of deriving it is that a cooler star must
    /// come out red without anyone writing new code.
    #[test]
    fn a_stars_colour_follows_its_declared_temperature() {
        let photosphere = super::body("sun").layers.last().unwrap().t_outer;
        assert!(
            (photosphere - 5772.0).abs() < 1.0,
            "the Sun's declared photosphere is 5,772 K, got {photosphere}"
        );
        // The renderer's incandescence law is wasm-gated, so reproduce its ramp here rather than
        // duplicating the constants: white above ~3200 K, red-dominant just above the 800 K glow floor.
        let ramp = |t: f64| ((t - 800.0) / (3200.0 - 800.0)).clamp(0.0, 1.0);
        assert!(ramp(photosphere) >= 1.0, "a 5,772 K photosphere saturates the ramp ⇒ white");
        assert!(ramp(1200.0) < 0.25, "a cool body stays down the red end of the ramp");
    }
}

#[cfg(test)]
mod giant_impact_budget_tests {
    /// **Mass has to add up.** The giant-impact hypothesis is a claim about a mass budget: proto-Earth
    /// plus Theia becomes modern Earth, plus the Moon, plus whatever escaped. So the bodies the scene
    /// places must satisfy it — otherwise the simulation is running an impact that could not have made
    /// the world we live on.
    ///
    /// This test exists because the scene was pointed at `earth.json`: adding a Mars-sized body to a
    /// planet that ALREADY contains it gave a post-impact Earth of 1.115 M⊕ — 11% too heavy, an entire
    /// Mars of surplus. Modern Earth is the RESULT of this collision, never its input.
    #[test]
    fn proto_earth_plus_theia_makes_the_earth_and_moon_we_have() {
        const M_EARTH: f64 = 5.972e24;
        const M_MOON: f64 = 7.342e22;
        let m = |id: &str| super::body(id).total_mass();
        let (proto, theia, earth) = (m("proto-earth"), m("theia"), m("earth"));

        // Theia is Mars-class: the impactor of the hypothesis, ~0.1 M⊕.
        let theia_frac = theia / M_EARTH;
        assert!((0.08..0.15).contains(&theia_frac), "Theia is Mars-class, got {theia_frac:.3} M⊕");

        // Proto-Earth is ~0.9 M⊕ — the canonical value, and independently what the budget demands.
        let proto_frac = proto / M_EARTH;
        assert!((0.85..0.95).contains(&proto_frac), "proto-Earth is ~0.9 M⊕, got {proto_frac:.3}");

        // THE BUDGET: what goes in must account for what comes out, within the mass that escapes (a few
        // percent in the published simulations) and our own layer-density approximation.
        let before = proto + theia;
        let after = earth + M_MOON;
        let err = (before - after).abs() / after;
        assert!(
            err < 0.03,
            "proto-Earth + Theia ({before:.4e}) must account for Earth + Moon ({after:.4e}); off by {:.1}%",
            100.0 * err
        );

        // And the error must be in the physical direction: at least as much goes in as comes out, since
        // some fraction escapes the system entirely.
        assert!(before >= after * 0.99, "the impact cannot create mass");

        // The target must actually be the smaller, pre-impact planet — not modern Earth wearing its name.
        assert!(proto < earth, "proto-Earth is lighter than the Earth it becomes");
        assert!(super::body("proto-earth").radius() < super::body("earth").radius(), "and smaller");
        // It carries a great deal MORE atmosphere than modern Earth, not less — a magma ocean degasses a
        // ~100 bar steam/CO₂ envelope. This assertion used to read "carries no declared air", which was
        // the same mistake in miniature: "not modern air" is not "no air".
        assert!(
            super::body("proto-earth").atmosphere_mass > super::body("earth").atmosphere_mass,
            "a degassing magma ocean has more air than the modern planet, not none"
        );
    }
}

#[cfg(test)]
mod hadean_surface_tests {
    /// **What did the continents look like at the giant impact? There weren't any.**
    ///
    /// The scene was previously going to show Theia striking a planet wearing modern Earth's coastlines,
    /// 4.5 billion years before either the continents or the oceans existed. There is no paleogeographic
    /// map to assign instead: the oldest continental material is ~4.4 Ga zircon, the oldest surviving
    /// crust ~4.0 Ga, and reconstructions do not meaningfully reach past ~1 Ga. At 4.51 Ga Earth was a
    /// magma ocean over a freshly differentiated core.
    ///
    /// So proto-Earth carries no surface map at all, and its molten state is DERIVED: the declared
    /// geotherm is compared against the material's own melt point. Nothing here says "draw it glowing".
    #[test]
    fn proto_earth_is_a_magma_ocean_with_no_continents_to_draw() {
        let proto = super::body("proto-earth");
        let mats = crate::materials::load();

        // No surface map, and none to be had — this is the answer, not an omission.
        assert!(proto.surface.is_none(), "proto-Earth must carry no continents: there were none");

        // The surface is mantle rock, not crust. Continental and oceanic crust are both later products.
        let top = proto.layers.last().expect("layers");
        assert_eq!(top.material, "peridotite", "no crust existed yet — the surface IS the mantle");

        // MOLTEN, and emergent: the declared temperature exceeds the declared melt point. If either
        // number changes, this conclusion changes with it, which is the point.
        let rock = &mats[crate::materials::index_of(&mats, &top.material)];
        let melt = rock.thermal.as_ref().expect("peridotite declares thermal properties").melt_point as f64;
        assert!(
            top.t_outer > melt,
            "proto-Earth's surface ({} K) must be above peridotite's melt point ({melt} K) — molten is \
             derived from the geotherm, never asserted",
            top.t_outer
        );
        // ...and modern Earth's surface is not, by the same comparison. One rule, two answers, both right.
        let modern = super::body("earth");
        let modern_top = modern.layers.last().unwrap();
        let modern_rock = &mats[crate::materials::index_of(&mats, &modern_top.material)];
        assert!(
            modern_top.t_outer < modern_rock.thermal.as_ref().unwrap().melt_point as f64,
            "modern Earth's surface is solid rock, by the same test"
        );

        // Hot enough to glow visibly, which is what makes it read as a magma ocean without any art:
        // the incandescence law starts at ~800 K.
        assert!(top.t_outer > 800.0, "a magma ocean glows: {} K", top.t_outer);
    }
}

#[cfg(test)]
mod hadean_atmosphere_tests {
    /// **A magma ocean is not an airless world — it is the opposite.** Proto-Earth's atmosphere was
    /// declared as ZERO on the reasoning that modern air is later outgassing and the impact would strip
    /// it. Half right: it is certainly not modern air. But a magma ocean DEGASSES, and the Hadean surface
    /// carried a massive steam/CO₂ envelope — the literature puts it at the ~100 bar scale over the
    /// ocean, and independently at >27 bar with a ~503 K surface once the ocean had solidified, which is
    /// the cooled end state. Zero was further from the truth than modern air would have been.
    ///
    /// Nothing here declares a pressure: the engine weighs the declared mass, as it does for every body.
    #[test]
    fn proto_earth_carries_a_massive_hot_steam_atmosphere() {
        let proto = super::body("proto-earth");
        let modern = super::body("earth");

        let p_bar = proto.surface_pressure() / 1.0e5;
        assert!(
            (60.0..200.0).contains(&p_bar),
            "a magma-ocean steam envelope is ~100 bar, got {p_bar:.1} bar"
        );
        // Emphatically heavier than today's, not lighter.
        assert!(
            proto.atmosphere_mass > 50.0 * modern.atmosphere_mass,
            "proto-Earth's air far outweighs modern air ({:.2e} vs {:.2e} kg)",
            proto.atmosphere_mass, modern.atmosphere_mass
        );
        // And the modern planet still weighs in at ~1 atm — the same rule, two bodies, two answers.
        let modern_bar = modern.surface_pressure() / 1.0e5;
        assert!((0.9..1.1).contains(&modern_bar), "modern Earth is ~1 bar, got {modern_bar:.2}");

        // The surface underneath it is molten, and hot enough to GLOW — which is what the renderer needs
        // to know, and all it needs to know.
        let top = proto.layers.last().unwrap();
        assert!(top.t_outer > 1800.0, "a magma ocean surface, got {} K", top.t_outer);
        let glow = crate::blackbody::blackbody_srgb(top.t_outer);
        assert!(glow[0] > glow[2], "at {} K it glows red-orange, got {glow:?}", top.t_outer);
    }
}
