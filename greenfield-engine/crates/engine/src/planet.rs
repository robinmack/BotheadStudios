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
pub struct Layer {
    /// Material id in the catalog (`materials.json`).
    pub material: &'static str,
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
pub struct LayeredBody {
    pub layers: Vec<Layer>,
    /// Total atmosphere mass (kg) — DECLARED (measured; Earth: 5.15e18 kg). The surface pressure is
    /// never declared: it EMERGES as the weight of this column, P = M·g/(4πR²) — which for Earth's
    /// declared mass comes out ≈1 atm. Zero for an airless body (the Moon): then open liquids boil off
    /// (see `surface_phase`) — water on a vacuum world is not stable, as a consequence of the model.
    pub atmosphere_mass: f64,
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
        let mat = &mats[crate::materials::index_of(mats, layer.material)];
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
pub fn earth() -> LayeredBody {
    LayeredBody {
        layers: vec![
            // Inner core: iron, ρ̄ ≈ 12,900 kg/m³, ~5,700→5,900 K. SOLID — but that must EMERGE.
            Layer { material: "iron", outer_r: 1.2215e6, density: 12_900.0, t_inner: 5_900.0, t_outer: 5_800.0 },
            // Outer core: liquid iron, in TWO segments — the core adiabat is convex in radius (it hugs
            // the melting curve from just above, touching it at the ICB where the core freezes), so one
            // linear segment sags dishonestly below the melt curve mid-core. Two segments with the
            // published mid-core anchor (~5,150 K at ~240 GPa) represent the observed adiabat honestly.
            // PREM densities: ~12,160 (ICB) → ~9,900 (CMB); segment means 11,900 / 10,600.
            Layer { material: "iron", outer_r: 2.35e6, density: 11_900.0, t_inner: 5_800.0, t_outer: 5_150.0 },
            Layer { material: "iron", outer_r: 3.48e6, density: 10_600.0, t_inner: 5_150.0, t_outer: 4_150.0 },
            // Mantle: peridotite, ρ̄ ≈ 4,500 kg/m³ (compressed), 2,900 K (CMB side) → 1,600 K (top).
            Layer { material: "peridotite", outer_r: 6.346e6, density: 4_500.0, t_inner: 2_900.0, t_outer: 1_600.0 },
            // Crust: basalt (oceanic bulk; continental granite is a surface-column detail), ~25 km.
            Layer { material: "basalt", outer_r: 6.371e6, density: 2_900.0, t_inner: 900.0, t_outer: 288.0 },
        ],
        // DECLARED: the measured mass of Earth's atmosphere. Its weight ⇒ ~1 atm at the surface —
        // the pressure that keeps the oceans liquid (surface_phase). Never a declared pressure.
        atmosphere_mass: 5.15e18,
    }
}

/// The MOON, declared as its real construction (GRAIL/Apollo seismology): small solid inner core,
/// fluid outer core (the "hot molten core"), thick peridotite mantle, basalt crust. HONESTY NOTE: the
/// real lunar core is an Fe–S alloy whose eutectic melts well BELOW pure iron — that's the physical
/// reason the outer core is molten at only ~1,900–2,000 K. Our catalog models pure iron, so we use the
/// upper range of the published selenotherm (Weber et al. 2011: partially molten outer core,
/// ~1,900–2,000 K); an Fe–S material entry is the honest refinement (flagged).
pub fn moon() -> LayeredBody {
    LayeredBody {
        layers: vec![
            Layer { material: "iron", outer_r: 2.4e5, density: 7_800.0, t_inner: 1_800.0, t_outer: 1_820.0 },
            Layer { material: "iron", outer_r: 3.3e5, density: 7_000.0, t_inner: 2_000.0, t_outer: 1_960.0 },
            Layer { material: "peridotite", outer_r: 1.697e6, density: 3_350.0, t_inner: 1_800.0, t_outer: 500.0 },
            Layer { material: "basalt", outer_r: 1.737e6, density: 2_900.0, t_inner: 500.0, t_outer: 250.0 },
        ],
        atmosphere_mass: 0.0, // airless — so any exposed liquid boils off (as observed: no lunar seas)
    }
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
    let lat = dir.y.asin().to_degrees();
    let lon = dir.z.atan2(dir.x).to_degrees();
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
    LayeredBody {
        layers: vec![
            // Core (0–0.25 R☉): ~half the mass. Fusion region, ~15.7e6 K.
            Layer { material: "hh_plasma", outer_r: 1.74e8, density: 45_200.0, t_inner: 1.57e7, t_outer: 7.0e6 },
            // Radiative zone (0.25–0.7 R☉).
            Layer { material: "hh_plasma", outer_r: 4.87e8, density: 1_940.0, t_inner: 7.0e6, t_outer: 2.0e6 },
            // Convective zone to the photosphere (5,772 K).
            Layer { material: "hh_plasma", outer_r: 6.96e8, density: 97.0, t_inner: 2.0e6, t_outer: 5_772.0 },
        ],
        atmosphere_mass: 0.0, // the corona is future physics (flagged)
    }
}

/// THEIA — the Mars-sized impactor of the giant-impact hypothesis (docs/27: the birth of the Moon).
/// Declared as a differentiated Mars-like body: iron core, peridotite mantle, hot from accretion.
/// Layer masses integrate to ~6.5e23 kg (Mars-scale, the theorized impactor class).
pub fn theia() -> LayeredBody {
    LayeredBody {
        layers: vec![
            Layer { material: "iron", outer_r: 1.8e6, density: 7_200.0, t_inner: 2_600.0, t_outer: 2_400.0 },
            Layer { material: "peridotite", outer_r: 3.39e6, density: 3_400.0, t_inner: 2_200.0, t_outer: 1_200.0 },
        ],
        atmosphere_mass: 0.0,
    }
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
