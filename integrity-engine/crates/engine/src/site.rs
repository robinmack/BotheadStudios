//! **The declared site: camera-driven materialization of ground zero** (docs/59 order-of-work
//! item 2, the trigger half, plus the entry point of item 3).
//!
//! The first production trigger for resolution-by-necessity (docs/44) driven by the CAMERA:
//! descending past the view-necessity threshold materializes the site a ground world declares (the
//! ball and a terrain patch), through the ONE conserved refinement rung (`crate::refine`,
//! split-relax-release), with the conservation ledger surfaced rather than implied. It
//! deliberately mirrors the moon-drop's resolution-distance idiom (`live_resolution_crossing` /
//! `accretion::resolution_distance`): one derived distance, one crossing check per step, so the
//! engine has one materialization pattern, not two.
//!
//! Law IV bounds the whole module: the camera changes REPRESENTATION, never existence. The site's
//! matter is declared in the world definition whether or not anyone looks; crossing the threshold
//! only changes which representation carries it, and the ledger proves the change conserved.
//!
//! # The threshold, derived (never a declared constant)
//!
//! A coarse SPH particle of mass `m` at in-situ density `rho` answers for a cube of matter
//! `s = (m/rho)^(1/3)` on a side - its share of the body, the representation's own quantum. Seen
//! from distance `d` that quantum subtends `s/d` radians. The camera's angular budget is
//! `ResolutionController::angular_resolution` (docs/49, THE one declared fidelity dial - a viewing
//! tolerance, not a physical quantity; no second dial is introduced here). The coarse
//! representation stops being able to serve the view when its quantum subtends more than the
//! budget:
//!
//! ```text
//! s / d > theta   ⟺   d < s / theta = view_resolution_distance(s, theta)
//! ```
//!
//! This is the inversion of docs/49's camera-granularity law (`camera_grain_radius = d * theta`):
//! the crossing is exactly where the grain the camera is owed becomes finer than the grain the
//! coarse field carries. For the shipped Earth statement (total mass over the same 2400-particle
//! celestial resolution the space band spends, at the outermost layer's in-situ density) the
//! threshold lands at ~9.5e8 m - inside the space band's own camera envelope, so the out-and-back
//! demo arc (fork issue: the camera path is out-AND-back) crosses it in both directions.
//!
//! When a LIVE celestial SPH field exists, the quantum is MEASURED from the field's own particles
//! (max share, biased toward resolving early per docs/44 §5); the declared statement is only the
//! answer while no field exists to measure.
//!
//! # Bidirectional, by demand
//!
//! [`SiteTrigger`] is a two-state machine fed the camera distance every frame. Below the threshold and
//! coarse: it demands Materialize. Above the threshold and resolved: it demands Deresolve. A
//! demand STANDS until the caller confirms it executed - a refused materialization (mid-event
//! field) or a refused fold (site not settled) leaves the demand in place and the refusal on
//! screen, honestly, rather than disarming the trigger.
//!
//! The downward crossing goes through the docs/61 criterion: the site folds back into the world
//! definition's summary only once its own field has been quiet for one sustained `t_q`
//! (`recohere::SettleGauge`, the ONE settling gauge). Un-settled matter stays resolved however far
//! the camera pulls back, and the HUD says so.
//!
//! # What materializes today, and what is refused for later (flagged IOUs, Law V)
//!
//! - The declared ball plus a terrain patch of the site's own strata (the body's
//!   `surface_strata` at the declared lat/lon - one Earth, no private column) split one rung
//!   through `refine::refine_patch` and released under its stated density bound. The patch is
//!   PARTICLE-form matter; meshed, standable ground at the site is docs/59 item 4.
//! - The patch budget is a declared compute statement (docs/59 names this IOU): a bowl of
//!   ground under the site, fine region 2.2 parent-spacings across (the geometry the rung's own
//!   tests verify), guard shell one interaction reach beyond, one rung of 13 - sized so the
//!   one-shot relax stays around a second of compute, far inside the 2400-particle celestial
//!   statement it rides.
//! - This site's relief stalls the strict relax at a measured ~4.8e-2 plateau; the rung now
//!   RELEASES it under the field's own measured scale-mismatch bound (~9.1e-2 here) with the
//!   residual stated (`refine`, "Release on a stalled plateau") - so the patch ENTERS DYNAMICS
//!   ([`SiteDynamics`], the module section below). A patch the rung still refuses carries the
//!   exact split with the residual stated ([`SiteRelease::Unreleased`]) and the release gate
//!   keeps it OUT of dynamics.
//! - The 1 m grass skin is SUB-QUANTUM at this rung (a parent is ~4.5 m of basalt) and does not
//!   materialize; it needs the next rung down, exactly as recohere's sub-quantum remainder stays
//!   particles. Stated, not smoothed over.
//! - The ball materializes at its DECLARED initial position and then LIVES: under
//!   [`SiteDynamics`] it falls, rests on the ground through the one terrain-contact law, and an
//!   event's arriving energy decides its fate through the same door and `damage::classify` the
//!   ground scene's ball answers to, verdict word included.
//! - The ball's children are split EXACTLY but not relaxed, and the status says so: an isolated
//!   sub-resolution body has no uniform coarse environment to relax against (relaxing its 13
//!   children toward its own kernel smear in vacuum was measured divergent - the target decays
//!   to nothing as the children chase it). Its bonds forge at the stencil's own rest lengths, so
//!   the split alone conserves by construction, is audited, and enters the bond dynamics with no
//!   stored prestress.
//! - Energy hand-down from a live celestial field lands as the smallest honest version: a
//!   QUIESCENT field's specific internal energy is sampled at the site (mass-weighted over the
//!   particles whose support covers it) and carried into the patch; a MID-EVENT field refuses
//!   with the measured speeds stated ([`SiteRefusal::MidEvent`]) - the full mid-event hand-down
//!   (velocities, gradients, the pi-scaling gate against the live crater) is the next issue's
//!   work. Quiescence is the docs/61 law reused (`recohere::quiescent_speed`) at the coarse
//!   field's own quantum: motion that cannot buy a one-quantum rise is sub-resolution for the
//!   representation being sampled.

use crate::gpu_sph::SphParticle;
use crate::materials::Material;
use crate::recohere::SettleGauge;
use crate::refine::{self, Refusal};
use crate::terra::world_def::{GroundSurface, World};
use glam::{DVec3, Vec3};
use std::fmt;

/// The celestial resolution statement the site's coarse quantum is derived from - the SAME
/// 2400-particle budget `start_gpu_impact` and the live-drop hand-off spend on a planet, so "one
/// coarse particle" means the same thing to the trigger as to the field it predicts. A compute
/// statement, not physics (resolution is the engine's call; docs/44).
pub const CELESTIAL_STATEMENT: usize = 2400;

/// Fine particles the materialized site may spend, riding the same statement (docs/59's open
/// patch-budget question, answered provisionally: no separate budget).
pub const SITE_PATCH_BUDGET: usize = CELESTIAL_STATEMENT;

/// The fine region's radius in units of the parent spacing: a half-ball of ground under the real
/// free surface. 2.2 lattice units is the refinement geometry the rung's own tests verify
/// (`refine::tests` use exactly this region on their lattices), reused rather than re-derived. A
/// declared compute statement (module doc).
const FINE_RADIUS_DX: f64 = 2.2;

/// The coarse guard shell extends one full interaction reach beyond the fine region: 2h = 4 dx
/// (the engine's h = 2 dx convention), so every guard the relax consults actually exists -
/// an unguarded truncation is a refused configuration
/// (`refine::tests::an_unguarded_truncation_refuses_with_a_finite_stated_error`).
const GUARD_RADIUS_DX: f64 = FINE_RADIUS_DX + 4.0;

/// The edge of the cube of matter ONE particle answers for: `(m/rho)^(1/3)` - mass and in-situ
/// density are the particle's own, so the quantum moves with the field, never with a constant.
pub fn coarse_particle_extent_m(mass_kg: f64, density_kg_m3: f64) -> f64 {
    if mass_kg <= 0.0 || density_kg_m3 <= 0.0 {
        return 0.0;
    }
    (mass_kg / density_kg_m3).cbrt()
}

/// **The view-necessity threshold** (module doc): the camera distance at which a coarse particle
/// of extent `s` subtends exactly the angular budget `theta`. Inside it the coarse representation
/// under-resolves the view (its quantum is wider than the grain the camera is owed); outside it,
/// materializing finer matter would change nothing the camera can distinguish (Law III: necessity
/// decides). The mirror of `accretion::resolution_distance`, for the camera instead of the tides.
pub fn view_resolution_distance(coarse_extent_m: f64, angular_resolution_rad: f64) -> f64 {
    if coarse_extent_m <= 0.0 || angular_resolution_rad <= 0.0 {
        return 0.0;
    }
    coarse_extent_m / angular_resolution_rad
}

/// A crossing the trigger demands. The caller executes it and `confirm`s; a refusal leaves the
/// demand standing (module doc).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SiteCrossing {
    /// The camera descended inside the threshold while the site is coarse: materialize it.
    Materialize,
    /// The camera ascended outside the threshold while the site is resolved: fold it back down
    /// (through the docs/61 settling criterion - an unsettled site honestly stays).
    Deresolve,
}

/// The two-state bidirectional trigger (module doc). Pure state; natively tested.
#[derive(Clone, Copy, Debug, Default)]
pub struct SiteTrigger {
    resolved: bool,
}

impl SiteTrigger {
    pub fn new() -> Self {
        SiteTrigger { resolved: false }
    }

    pub fn is_resolved(&self) -> bool {
        self.resolved
    }

    /// The per-frame check: given the camera's current distance to the site and the derived
    /// threshold, what (if anything) must happen? Pure - call as often as the camera moves.
    pub fn observe(&self, distance_m: f64, resolve_at_m: f64) -> Option<SiteCrossing> {
        match (self.resolved, distance_m <= resolve_at_m) {
            (false, true) => Some(SiteCrossing::Materialize),
            (true, false) => Some(SiteCrossing::Deresolve),
            _ => None,
        }
    }

    /// The caller confirms a demanded crossing actually executed (materialization released /
    /// fold conserved). Refusals do NOT confirm; the demand stands.
    pub fn confirm(&mut self, crossing: SiteCrossing) {
        self.resolved = matches!(crossing, SiteCrossing::Materialize);
    }
}

/// The declared solid body at the site (the ground world's ball), as the site needs it.
#[derive(Clone, Debug)]
pub struct SiteBall {
    pub material: String,
    pub radius_m: f64,
    /// Centred ground-world coordinates (the `GroundBody::at_m` convention), metres.
    pub at_m: [f32; 3],
}

/// Everything the trigger and the materialization need about the declared site, extracted from
/// the world definition and the one shared body (docs/59 "one Earth": the strata, gravity and
/// temperature all derive from the body the world names, never from a private copy).
#[derive(Clone, Debug)]
pub struct SiteSpec {
    pub lat_deg: f64,
    pub lon_deg: f64,
    /// The declared surface with its strata RESOLVED from the named body at the site.
    pub surface: GroundSurface,
    pub ball: Option<SiteBall>,
    /// The world's declared grain scale (m) - sizes the rung when no ball declares finer matter.
    pub grain_m: f64,
    /// Surface gravity (m/s^2), emergent from the body (g = GM/R^2).
    pub g_ms2: f64,
    /// The body's declared surface temperature (K) - the definition-path thermal state.
    pub surface_t_k: f64,
    pub body_radius_m: f64,
    /// One coarse celestial particle's mass under [`CELESTIAL_STATEMENT`].
    pub coarse_mass_kg: f64,
    /// The body's outermost layer's in-situ density - the density a coarse particle summarizing
    /// the site carries.
    pub coarse_density: f64,
}

impl SiteSpec {
    /// Build the spec from a parsed `"ground"` world. The strata resolve from the named planet at
    /// the declared (lat, lon) exactly as the ground scene resolves them.
    pub fn from_ground_world(w: &World) -> Result<SiteSpec, String> {
        let g = w
            .ground
            .as_ref()
            .ok_or_else(|| format!("world {:?} declares no ground block; no site to arm", w.name))?;
        let body = crate::planet::body(&g.planet);
        let mut surface = g.surface.clone();
        surface.resolve_strata(&body, g.lat, g.lon);
        let r = body.radius();
        let coarse_density = body
            .layers
            .last()
            .map(|l| l.density)
            .ok_or_else(|| format!("body {:?} declares no layers", g.planet))?;
        Ok(SiteSpec {
            lat_deg: g.lat,
            lon_deg: g.lon,
            surface,
            ball: g.bodies.first().map(|b| SiteBall {
                material: b.material.clone(),
                radius_m: b.radius_m,
                at_m: b.at_m,
            }),
            grain_m: g.grain_size_m as f64,
            g_ms2: body.gravity_at(r),
            surface_t_k: body.temperature_at(r),
            body_radius_m: r,
            coarse_mass_kg: body.total_mass() / CELESTIAL_STATEMENT as f64,
            coarse_density,
        })
    }

    /// The declared-statement coarse quantum at this site - the answer while no live field exists
    /// to measure one.
    pub fn declared_coarse_extent_m(&self) -> f64 {
        coarse_particle_extent_m(self.coarse_mass_kg, self.coarse_density)
    }

    /// The FINEST quantum the site's one materialization rung produces (m): the declared ball's
    /// one-rung child (`m_ball / 13` at the ball material's density, the same rung-sizing law
    /// `materialize_site` uses), or the declared grain when no ball exists (children land exactly
    /// at the grain there). This is what the demo arc's floor derives from: the camera may
    /// honestly descend to the view-resolution distance of THIS quantum and no further. `None`
    /// when the ball's material is not in the catalogue (the materialization would refuse too).
    pub fn finest_child_extent_m(&self, mats: &[Material]) -> Option<f64> {
        match self.ball.as_ref() {
            Some(b) => {
                let rho = mats.iter().find(|m| m.id == b.material)?.density as f64;
                let m_ball = rho * 4.0 / 3.0 * std::f64::consts::PI * b.radius_m.powi(3);
                Some(coarse_particle_extent_m(m_ball / refine::LEVEL_MASS_RATIO, rho))
            }
            None => Some(self.grain_m),
        }
    }
}

/// The measured coarse quantum of a LIVE field: the largest matter share any particle carries -
/// the widest thing one particle answers for, biased toward resolving early (docs/44 §5).
pub fn measured_coarse_extent_m(field: &[SphParticle]) -> f64 {
    field
        .iter()
        .map(|p| coarse_particle_extent_m(p.mass as f64, p.rho.max(1.0) as f64))
        .fold(0.0, f64::max)
}

/// Where the fine patch's thermal state comes from (module doc: the smallest honest hand-down).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HandDown {
    /// No live celestial field: the definition answers (u = c * T at the body's declared surface
    /// temperature, per material - the same u = c*T law `HydroBody::particalize` uses).
    Declared,
    /// A quiescent live field was sampled at the site: this specific internal energy carries
    /// down, and the measured speeds that justified the sample are kept for the ledger.
    Sampled { u_j_kg: f64, peak_speed_ms: f64, quiescent_speed_ms: f64 },
}

/// Why the site said no. Every refusal renders to the HUD with its reason - a hidden
/// smoothing-over at a representation crossing would be a fudge (Law V).
#[derive(Clone, Debug, PartialEq)]
pub enum SiteRefusal {
    /// The live celestial field is mid-event at the site: sampling one instant of it would freeze
    /// a shock into the ground. The full mid-event hand-down is the flagged next step.
    MidEvent { peak_speed_ms: f64, quiescent_speed_ms: f64 },
    /// A live field exists but no particle's support covers the site - the coarse field holds no
    /// matter there to hand down (an excavated site), and the definition would be stale.
    Uncovered,
    /// A site material has no sourced specific heat: its thermal state cannot be honestly set
    /// (an unknown stays unknown at the boundary; CLAUDE.md Law VII SOP).
    UnsourcedHeat { material: String },
    /// A site material is not in the catalogue at all.
    UnknownMaterial { material: String },
    /// The refinement rung itself refused (contamination, interface ratio, non-convergence).
    Refine(Refusal),
    /// The fold was demanded but the site has not been quiet for one sustained t_q (docs/61):
    /// it stays resolved, honestly.
    NotSettled { quiet_s: f32, needed_s: f32 },
}

impl fmt::Display for SiteRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SiteRefusal::MidEvent { peak_speed_ms, quiescent_speed_ms } => write!(
                f,
                "site hand-down refused: the celestial field is mid-event here (peak speed \
                 {peak_speed_ms:.0} m/s over the {quiescent_speed_ms:.0} m/s quiescent bound); \
                 the mid-event hand-down is deferred"
            ),
            SiteRefusal::Uncovered => write!(
                f,
                "site hand-down refused: no live particle's support covers the site - the coarse \
                 field holds no matter here to hand down"
            ),
            SiteRefusal::UnsourcedHeat { material } => write!(
                f,
                "site refused: material {material:?} has no sourced specific heat; its thermal \
                 state would be an invention"
            ),
            SiteRefusal::UnknownMaterial { material } => write!(
                f,
                "site refused: material {material:?} is not in the catalogue"
            ),
            SiteRefusal::Refine(r) => write!(f, "site {r}"),
            SiteRefusal::NotSettled { quiet_s, needed_s } => write!(
                f,
                "site stays resolved: quiet {quiet_s:.2} s of the sustained {needed_s:.2} s the \
                 fold needs (docs/61)"
            ),
        }
    }
}

/// Sample the live celestial field at the site (Earth-relative frame, metres): the smallest
/// honest hand-down (module doc). Quiescent → the mass-weighted specific internal energy of the
/// particles whose support covers the site; mid-event → refusal with the measured speeds.
pub fn sample_hand_down(
    field: &[SphParticle],
    site_rel_m: DVec3,
    g_ms2: f64,
) -> Result<HandDown, SiteRefusal> {
    let site = Vec3::new(site_rel_m.x as f32, site_rel_m.y as f32, site_rel_m.z as f32);
    let mut cover: Vec<&SphParticle> = Vec::new();
    for p in field {
        if (Vec3::from(p.pos) - site).length() < p.h {
            cover.push(p);
        }
    }
    if cover.is_empty() {
        return Err(SiteRefusal::Uncovered);
    }
    let m_tot: f64 = cover.iter().map(|p| p.mass as f64).sum();
    let v_bulk = cover
        .iter()
        .fold(DVec3::ZERO, |a, p| a + DVec3::from(Vec3::from(p.vel).as_dvec3()) * p.mass as f64)
        / m_tot;
    let extent = cover
        .iter()
        .map(|p| coarse_particle_extent_m(p.mass as f64, p.rho.max(1.0) as f64))
        .fold(0.0, f64::max);
    let v_q = crate::recohere::quiescent_speed(g_ms2 as f32, extent as f32) as f64;
    let peak = cover
        .iter()
        .map(|p| (Vec3::from(p.vel).as_dvec3() - v_bulk).length())
        .fold(0.0, f64::max);
    if peak >= v_q {
        return Err(SiteRefusal::MidEvent { peak_speed_ms: peak, quiescent_speed_ms: v_q });
    }
    let u = cover.iter().map(|p| p.mass as f64 * p.u as f64).sum::<f64>() / m_tot;
    Ok(HandDown::Sampled { u_j_kg: u, peak_speed_ms: peak, quiescent_speed_ms: v_q })
}

/// How the terrain patch left the rung: released under the stated density bound, or split-only
/// with the relax's measured plateau REPORTED (a relief surface stalls the shifting - the
/// refine-level test documents it - and the release only gates entry into dynamics, which the
/// site defers; the residual is fidelity stated, never conservation lost).
#[derive(Clone, Copy, Debug)]
pub enum SiteRelease {
    Released(refine::RelaxReport),
    /// The relax stalled at `achieved` (over `bound`) after `iterations`; the patch is the EXACT
    /// split, its density blip unreduced and stated. Releasing relief surfaces is the named
    /// deferred computation (a fringe-corrected density estimate in the rung).
    Unreleased { achieved: f64, bound: f64, iterations: usize },
}

/// The materialized site: the released fine patch, its guards, and the honest paperwork.
/// Positions are in the SITE frame: metres, origin at the ball column's ground top, x east,
/// y up, z north. `mat` indexes the CATALOGUE slice the caller passed (not any GPU EOS table).
#[derive(Clone)]
pub struct MaterializedSite {
    pub particles: Vec<SphParticle>,
    /// Particles before this index are coarse guards (the buffer shell); children follow.
    pub fine_start: usize,
    pub ledger: refine::Ledger,
    /// Whether the terrain patch passed the relax's release bound, or carries a stated residual.
    pub release: SiteRelease,
    /// Children of the declared ball occupy the LAST `ball_children` entries (the ball splits
    /// after the terrain patch: an exact conserving split, unrelaxed and stated as such).
    pub ball_children: usize,
    /// Total declared mass the parents carried in (kg) - what the fold must give back.
    pub declared_mass_kg: f64,
    pub hand_down: HandDown,
    /// Radius (m) of the site's own footprint about the origin - the contamination-check region.
    pub extent_m: f64,
    /// Radius (m) of the FINE region (the bowl the children fill); beyond it, below the surface,
    /// is coarse rock - the boundary the dynamics treat as immovable crust.
    pub fine_radius_m: f64,
    /// The ball column's position in the declared surface's voxel coordinates (x, z) and its
    /// ground-top height (voxel y) - the site frame's anchor, kept so the dynamics ask the SAME
    /// height law the builder used for the ground outside the bowl.
    pub col_voxel: [f64; 3],
}

/// The fold's paperwork: what went back into the summary, measured.
#[derive(Clone, Copy, Debug)]
pub struct FoldReport {
    pub folded: usize,
    pub audit: refine::Audit,
    pub declared_mass_kg: f64,
    pub mass_drift_kg: f64,
}

/// **Materialize the declared site** through the one refinement rung (module doc): build the
/// coarse parents from the world definition (the ball, then the strata columns, equal parent
/// mass so the interior is one rung), split-relax-release via [`refine::refine_patch`], and
/// return the released patch with the full conservation ledger.
pub fn materialize_site(
    spec: &SiteSpec,
    hand: &HandDown,
    mats: &[Material],
) -> Result<MaterializedSite, SiteRefusal> {
    // Material lookup with refusal, never a silent fallback: an unknown material or an unsourced
    // specific heat is a stated boundary, not an invented number.
    let lookup = |id: &str| -> Result<(usize, f64, f64), SiteRefusal> {
        let idx = mats
            .iter()
            .position(|m| m.id == id)
            .ok_or_else(|| SiteRefusal::UnknownMaterial { material: id.into() })?;
        let c = mats[idx]
            .specific_heat()
            .ok_or_else(|| SiteRefusal::UnsourcedHeat { material: id.into() })?;
        Ok((idx, mats[idx].density as f64, c))
    };
    // Thermal state per material: the definition's u = c * T (the same law `particalize` uses),
    // unless a quiescent live field handed one down - the site then carries the FIELD's state,
    // because the coarse particle covering it is the current truth about this ground.
    let u_of = |c: f64| -> f32 {
        match hand {
            HandDown::Declared => (c * spec.surface_t_k) as f32,
            HandDown::Sampled { u_j_kg, .. } => *u_j_kg as f32,
        }
    };

    // THE RUNG'S QUANTUM: the declared ball is the finest declared matter at the site, so its
    // real mass sets the parent mass and every parent is one rung (equal mass, the same
    // discipline as the celestial builder). A ball-less world sizes the rung from its own grain
    // statement instead: children land exactly at the declared grain, so parents are 13 grains.
    let ball = spec.ball.as_ref();
    let (m_q, ball_mat) = match ball {
        Some(b) => {
            let (idx, rho, c) = lookup(&b.material)?;
            let m = rho * 4.0 / 3.0 * std::f64::consts::PI * b.radius_m.powi(3);
            (m, Some((idx, rho, c)))
        }
        None => {
            let top = spec.surface.strata.first().map(|s| s.material.as_str()).unwrap_or("basalt");
            let (_, rho, _) = lookup(top)?;
            (refine::LEVEL_MASS_RATIO * rho * spec.grain_m.powi(3), None)
        }
    };

    // Geometry in the ground world's own conventions: the height law is `world::terrain_height_with`
    // over voxel coordinates; `at_m` is centred, with the same centring `world::World::center()`
    // applies (x,z by half the patch, y by half the max terrain top). Mirrored, and pinned by the
    // `the_site_column_agrees_with_the_generated_ground` test so the two cannot drift apart.
    let (w, h, d) = (
        spec.surface.size_voxels[0] as f64,
        spec.surface.size_voxels[1] as f64,
        spec.surface.size_voxels[2] as f64,
    );
    let skin = spec.surface.strata.first().and_then(|s| s.thickness_m).unwrap_or(1) as f64;
    let mut max_top = 0.0f64;
    for zi in 0..spec.surface.size_voxels[2] {
        for xi in 0..spec.surface.size_voxels[0] {
            let t = crate::world::terrain_height_with(&spec.surface, xi as f32, zi as f32)
                .round()
                .clamp((skin + 1.0) as f32, (h - 1.0) as f32) as f64;
            max_top = max_top.max(t);
        }
    }
    // The ball's column anchors the site frame: origin at that column's ground top.
    let (bx, by, bz) = match ball {
        Some(b) => (b.at_m[0] as f64, b.at_m[1] as f64, b.at_m[2] as f64),
        None => (0.0, 0.0, 0.0),
    };
    let (vx_b, vz_b) = (bx + 0.5 * w, bz + 0.5 * d);
    let top_b =
        crate::world::terrain_height_with(&spec.surface, vx_b as f32, vz_b as f32) as f64;
    let ball_local_y = match ball {
        Some(_) => by + 0.5 * max_top - top_b, // centred y -> height above this column's ground
        None => 0.0,
    };

    // STAGE 1 - the terrain patch: a BOWL of ground under ground zero. Coarse parents (equal
    // mass, one rung) fill a half-ball of the local strata; the inner [`FINE_RADIUS_DX`] sphere
    // split-relax-releases against the coarse field while the shell out to [`GUARD_RADIUS_DX`]
    // is held as the buffer band - the exact configuration the rung's tests verify, with the
    // one real free surface (the vacuum above the ground) open. The ball is NOT in this field:
    // at celestial coarseness the ball is sub-quantum - the coarse field never contained it;
    // only the definition does.
    //
    // The lattice spacing derives from the material just under the skin at the ball column
    // (equal parent mass => the spacing is that material's own quantum edge).
    let mat_under_skin = stratum_at(&spec.surface, top_b, top_b - skin - 0.1);
    let (_, rho_col, _) = lookup(&spec.surface.strata[mat_under_skin].material)?;
    let dx_col = coarse_particle_extent_m(m_q, rho_col);
    let r_guard = GUARD_RADIUS_DX * dx_col;
    let n_side = GUARD_RADIUS_DX.ceil() as i64;
    let mut parents: Vec<SphParticle> = Vec::new();
    for i in -n_side..=n_side {
        for j in -n_side..=n_side {
            let lx = i as f64 * dx_col;
            let lz = j as f64 * dx_col;
            if lx * lx + lz * lz > r_guard * r_guard {
                continue;
            }
            let (vx, vz) = (vx_b + lx, vz_b + lz);
            let top = crate::world::terrain_height_with(&spec.surface, vx as f32, vz as f32) as f64;
            let mut dep = 0.0f64;
            loop {
                // Material at the parent's centre depth; its density sizes this parent's extent.
                // One re-evaluation closes the (material -> extent -> centre) loop.
                let guess = stratum_at(&spec.surface, top, top - dep - 0.5 * dx_col);
                let (_, rho_g, _) = lookup(&spec.surface.strata[guess].material)?;
                let dx_g = coarse_particle_extent_m(m_q, rho_g);
                let s = stratum_at(&spec.surface, top, top - dep - 0.5 * dx_g);
                let (idx, rho, c) = lookup(&spec.surface.strata[s].material)?;
                let dx = coarse_particle_extent_m(m_q, rho);
                let cy = top - dep - 0.5 * dx;
                let p = DVec3::new(lx, cy - top_b, lz);
                if p.length() > r_guard {
                    break; // below the bowl
                }
                parents.push(SphParticle {
                    pos: [p.x as f32, p.y as f32, p.z as f32],
                    h: (2.0 * dx) as f32,
                    vel: [0.0; 3],
                    u: u_of(c),
                    mass: m_q as f32,
                    mat: idx as u32,
                    rho: rho as f32,
                    prov: 0,
                });
                dep += dx;
            }
        }
    }
    let region = refine::Region {
        center: Vec3::ZERO,
        radius: (FINE_RADIUS_DX * dx_col) as f32,
    };
    // The full rung first. A stalled relief plateau now RELEASES under the field's own measured
    // scale-mismatch bound with the residual stated (refine, "Release on a stalled plateau" -
    // this site: 4.8e-2 under ~9.1e-2); if the rung still refuses (a genuine failure), the site
    // falls back to the EXACT conserving split with the unreleased residual REPORTED
    // ([`SiteRelease::Unreleased`]), and the release gate keeps that patch out of dynamics -
    // what is lost is fidelity that is stated, never conservation. Every other refusal refuses
    // the materialization.
    let (particles, fine_start, mut ledger, release) =
        match refine::refine_patch(&parents, &region) {
            Ok(t) => {
                let release = SiteRelease::Released(t.relax);
                (t.particles, t.fine_start, t.ledger, release)
            }
            Err(Refusal::NotConverged { achieved, bound, iterations }) => {
                let s = refine::split_patch(&parents, &region).map_err(SiteRefusal::Refine)?;
                let ledger = refine::Ledger {
                    before: s.before,
                    after_split: s.after,
                    after_relax: s.after,
                    relax_am_bound: 0.0,
                };
                (s.particles, s.fine_start, ledger, SiteRelease::Unreleased {
                    achieved,
                    bound,
                    iterations,
                })
            }
            Err(other) => return Err(SiteRefusal::Refine(other)),
        };
    let terrain = (particles, fine_start);

    // STAGE 2 - the declared ball: the exact conserving split, UNRELAXED, and stated as such
    // (module doc): an isolated sub-resolution body has no uniform coarse environment to relax
    // against - relaxing its children toward its own kernel smear in vacuum was measured
    // divergent. The split alone is conservation by construction, audited; its bonds forge at
    // the stencil's own rest lengths, so it enters the dynamics with no stored prestress.
    let (mut particles, fine_start) = terrain;
    let mut ball_children = 0usize;
    if ball.is_some() {
        let (idx, rho, c) = ball_mat.expect("looked up above");
        let ext = coarse_particle_extent_m(m_q, rho);
        let parent = SphParticle {
            pos: [0.0, ball_local_y as f32, 0.0],
            h: (2.0 * ext) as f32,
            vel: [0.0; 3],
            u: u_of(c),
            mass: m_q as f32,
            mat: idx as u32,
            rho: rho as f32,
            prov: 0,
        };
        let b_region = refine::Region { center: Vec3::from(parent.pos), radius: parent.h };
        let split = refine::split_patch(&[parent], &b_region).map_err(SiteRefusal::Refine)?;
        ball_children = split.particles.len();
        // Fold the ball's audit into the combined ledger (audits are sums over disjoint sets).
        let add = |a: &mut refine::Audit, b: &refine::Audit| {
            a.mass += b.mass;
            a.momentum += b.momentum;
            a.angular_momentum += b.angular_momentum;
            a.kinetic += b.kinetic;
            a.internal += b.internal;
        };
        add(&mut ledger.before, &split.before);
        add(&mut ledger.after_split, &split.after);
        add(&mut ledger.after_relax, &split.after); // unrelaxed: the split IS its final state
        particles.extend_from_slice(&split.particles);
    }

    let declared_mass_kg = ledger.before.mass;
    let extent_m = particles
        .iter()
        .map(|p| Vec3::from(p.pos).length() + p.h)
        .fold(0.0f32, f32::max) as f64;
    Ok(MaterializedSite {
        particles,
        fine_start,
        ledger,
        release,
        ball_children,
        declared_mass_kg,
        hand_down: *hand,
        extent_m,
        fine_radius_m: region.radius as f64,
        col_voxel: [vx_b, top_b, vz_b],
    })
}

/// The stratum index at voxel height `y_v` in a column whose ground top is `top_v` - the SAME
/// column law as `world::generate_from` (skin follows the top; deeper band bottoms are level
/// planes walked down from the valley floor), mirrored here because the space band has no voxel
/// store to ask. Pinned against the generated world by
/// `the_site_column_agrees_with_the_generated_ground`.
fn stratum_at(surface: &GroundSurface, top_v: f64, y_v: f64) -> usize {
    let n = surface.strata.len();
    if n <= 1 {
        return 0;
    }
    let skin = surface.strata.first().and_then(|s| s.thickness_m).unwrap_or(1) as f64;
    if y_v >= top_v - skin {
        return 0;
    }
    let valley_floor = (surface.base_top_m - surface.amplitude_m) as f64;
    let mut bottom = valley_floor;
    for (k, st) in surface.strata.iter().enumerate().skip(1) {
        match st.thickness_m {
            Some(t) => {
                bottom -= t as f64;
                if y_v >= bottom {
                    return k;
                }
            }
            None => return k,
        }
    }
    n - 1
}

/// The site field's own peak speed (m/s) - what the caller feeds the settle gauge each step.
pub fn site_peak_speed(site: &MaterializedSite) -> f32 {
    site.particles
        .iter()
        .map(|p| Vec3::from(p.vel).length())
        .fold(0.0, f32::max)
}

/// **Fold the site back into the world definition's summary** - the ascending crossing, gated by
/// the docs/61 criterion: the ONE settle gauge must show a sustained quiet t_q, or the site
/// honestly stays resolved. On success the caller drops the particles; the report carries the
/// measured totals so the crossing is audited, not assumed.
pub fn fold_site(
    site: &MaterializedSite,
    gauge: &SettleGauge,
    g_ms2: f32,
) -> Result<FoldReport, SiteRefusal> {
    if !gauge.settled(g_ms2) {
        // 1.0 m is recohere's own binning cell (its private CELL_M): the interval quoted in the
        // refusal is the same one the gauge is waiting out.
        return Err(SiteRefusal::NotSettled {
            quiet_s: gauge.quiet_seconds(),
            needed_s: crate::recohere::quiescent_interval_s(g_ms2, 1.0),
        });
    }
    let audit = refine::audit(&site.particles);
    Ok(FoldReport {
        folded: site.particles.len(),
        audit,
        declared_mass_kg: site.declared_mass_kg,
        mass_drift_kg: audit.mass - site.declared_mass_kg,
    })
}

// -----------------------------------------------------------------------------------------------
// The mid-event hand-down (docs/59 "The hand-down, made concrete", decision 2): during a live
// event the guard band re-samples the coarse field each coarse step. The guards ARE the coarse
// field at the boundary, so the impact's energy arrives at the fine patch as real boundary state,
// not as a formula. Ownership stays single: a guard mirrors the field's STATE (velocity, specific
// internal energy, density) and never its matter - positions and masses are untouched, so no
// parcel is counted twice - and the event window books what arrived so the crossing is audited.
// -----------------------------------------------------------------------------------------------

/// The window ledger's stated bound: the booked guard-band energy against an independent f64
/// audit of the same guards. The resample interpolates in f64 and stores f32 state, so the drift
/// is f32 rounding accumulated over the band - bounded well inside 1e-6 relative.
pub const WINDOW_BOOK_BOUND: f64 = 1.0e-6;

/// Places the site's local coordinates (metres, origin at the ball column's ground top, x east,
/// y up, z north) into the Earth-relative frame the coarse field lives in - the same tangent
/// frame the renderer uses to seat the site on the crust.
#[derive(Clone, Copy, Debug)]
pub struct SiteFrame {
    /// The site origin in Earth-relative metres (`site_dir * body_radius`).
    pub origin_rel_m: DVec3,
    pub east: DVec3,
    pub up: DVec3,
    pub north: DVec3,
}

impl SiteFrame {
    /// A site-local position in the field's frame.
    pub fn to_field_m(&self, local: [f32; 3]) -> DVec3 {
        self.origin_rel_m
            + self.east * local[0] as f64
            + self.up * local[1] as f64
            + self.north * local[2] as f64
    }

    /// A field-frame vector expressed on the site's basis (a rotation: length-preserving).
    pub fn field_vec_to_site(&self, v: DVec3) -> [f32; 3] {
        [v.dot(self.east) as f32, v.dot(self.up) as f32, v.dot(self.north) as f32]
    }
}

/// What one boundary resample measured: the guard band's state totals after the guards became
/// the coarse field. Energies are in the site frame; `uncovered` guards (no live particle's
/// support reaches them) kept their previous state and are counted, not smoothed over.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BoundaryState {
    pub covered: usize,
    pub uncovered: usize,
    /// Guard-band kinetic energy (J), site frame.
    pub kinetic_j: f64,
    /// Guard-band internal energy (J).
    pub internal_j: f64,
    /// Fastest guard speed (m/s) after the resample.
    pub peak_speed_ms: f64,
    /// Mass-weighted guard-band bulk velocity (m/s), site frame - the direction and magnitude of
    /// the ground's motion the coarse field reports at the boundary.
    pub bulk_vel_ms: [f64; 3],
    /// Total guard-band mass (kg) - the denominator that turns the booked totals into the
    /// per-kilogram state the dynamics deliver to the fine matter.
    pub mass_kg: f64,
}

/// **Re-sample the guard band from the live coarse field** (module section doc). Each guard's
/// velocity, specific internal energy and density become the SPH interpolation of the coarse
/// field at the guard's own position (Shepard-normalized volume weights over the particles whose
/// support covers it, the scatter convention `sample_hand_down` uses); positions and masses are
/// never touched. Fidelity IOU, stated where the HUD can see it (`EventWindow::hud_line`): the
/// coarse field carries its collapsed EOS material set, so the sampled state inherits that
/// collapse until the N-material upload reaches the guards (docs/59).
pub fn resample_guards(
    site: &mut MaterializedSite,
    field: &[SphParticle],
    frame: &SiteFrame,
) -> BoundaryState {
    // Contributors whose support can reach ANY part of the site: one pass over the field so the
    // per-guard loop touches only the handful of coarse particles that matter.
    let near: Vec<&SphParticle> = field
        .iter()
        .filter(|p| {
            let d = DVec3::new(p.pos[0] as f64, p.pos[1] as f64, p.pos[2] as f64)
                - frame.origin_rel_m;
            d.length() < p.h as f64 + site.extent_m
        })
        .collect();
    let mut out = BoundaryState::default();
    for g in &mut site.particles[..site.fine_start] {
        let x = frame.to_field_m(g.pos);
        let (mut w_sum, mut u_w, mut rho_w) = (0.0f64, 0.0f64, 0.0f64);
        let mut v_w = DVec3::ZERO;
        for p in &near {
            let r = (DVec3::new(p.pos[0] as f64, p.pos[1] as f64, p.pos[2] as f64) - x).length();
            let h = p.h as f64;
            if r < h {
                // Volume-weighted Shepard interpolation: w = (m/rho) W(r, h), the field's own
                // scatter-convention estimate of its state at this site.
                let w = p.mass as f64 / (p.rho.max(1.0) as f64) * crate::atmosphere::sph_w(r, h);
                w_sum += w;
                u_w += w * p.u as f64;
                rho_w += w * p.rho as f64;
                v_w += w * DVec3::new(p.vel[0] as f64, p.vel[1] as f64, p.vel[2] as f64);
            }
        }
        if w_sum > 0.0 {
            g.u = (u_w / w_sum) as f32;
            g.rho = (rho_w / w_sum) as f32;
            g.vel = frame.field_vec_to_site(v_w / w_sum);
            out.covered += 1;
        } else {
            out.uncovered += 1;
        }
        let sp = Vec3::from(g.vel).length() as f64;
        out.kinetic_j += 0.5 * g.mass as f64 * sp * sp;
        out.internal_j += g.mass as f64 * g.u as f64;
        out.peak_speed_ms = out.peak_speed_ms.max(sp);
        out.mass_kg += g.mass as f64;
        let mv = DVec3::new(g.vel[0] as f64, g.vel[1] as f64, g.vel[2] as f64) * g.mass as f64;
        out.bulk_vel_ms[0] += mv.x;
        out.bulk_vel_ms[1] += mv.y;
        out.bulk_vel_ms[2] += mv.z;
    }
    if out.mass_kg > 0.0 {
        for c in &mut out.bulk_vel_ms {
            *c /= out.mass_kg;
        }
    }
    out
}

/// **The event window the audit ledger books** (decision 2): opened by the first boundary
/// resample of a live event, it carries the boundary state at open, the latest, and the booked
/// peak, so what arrived (and what left again) is a measured delta, never an assumption. The
/// drift of the book against the guards themselves is bounded by [`WINDOW_BOOK_BOUND`].
#[derive(Clone, Copy, Debug, Default)]
pub struct EventWindow {
    steps: usize,
    opened: Option<BoundaryState>,
    last: BoundaryState,
    peak_speed_ms: f64,
}

impl EventWindow {
    /// A new event: forget the previous window entirely.
    pub fn reset(&mut self) {
        *self = EventWindow::default();
    }

    /// Book one coarse step's boundary state. The first book opens the window.
    pub fn book(&mut self, s: BoundaryState) {
        if self.opened.is_none() {
            self.opened = Some(s);
        }
        self.last = s;
        self.steps += 1;
        self.peak_speed_ms = self.peak_speed_ms.max(s.peak_speed_ms);
    }

    pub fn is_open(&self) -> bool {
        self.opened.is_some()
    }

    /// The latest booked boundary state - what the caller diffs against the next resample to
    /// hand this coarse step's ARRIVAL to the fine matter ([`SiteDynamics::deliver_boundary`]).
    pub fn last_state(&self) -> BoundaryState {
        self.last
    }

    pub fn steps(&self) -> usize {
        self.steps
    }

    /// Kinetic energy that arrived at the boundary since the window opened (J; negative when
    /// the field took energy back out - both are the honest book).
    pub fn arrived_kinetic_j(&self) -> f64 {
        self.last.kinetic_j - self.opened.map_or(0.0, |o| o.kinetic_j)
    }

    /// Internal energy that arrived at the boundary since the window opened (J).
    pub fn arrived_internal_j(&self) -> f64 {
        self.last.internal_j - self.opened.map_or(0.0, |o| o.internal_j)
    }

    /// The fastest boundary speed booked anywhere in the window (m/s).
    pub fn peak_speed_ms(&self) -> f64 {
        self.peak_speed_ms
    }

    /// The honest one-line readout the HUD renders, IOU included: the guard state inherits the
    /// coarse field's collapsed EOS material set (docs/59 names this the hand-down's fidelity
    /// limit until the N-material upload reaches the guards).
    pub fn hud_line(&self) -> String {
        format!(
            "boundary: {} guards on the coarse field ({} uncovered) · window {} steps · \
             arrived KE {:+.2e} J, IE {:+.2e} J · peak {:.0} m/s · state rides the coarse \
             field's collapsed EOS set (IOU)",
            self.last.covered,
            self.last.uncovered,
            self.steps,
            self.arrived_kinetic_j(),
            self.arrived_internal_j(),
            self.peak_speed_ms
        )
    }
}

// -----------------------------------------------------------------------------------------------
// The site in DYNAMICS (docs/59, the remaining core of the hand-down): once the rung RELEASES the
// patch, the fine parcels stop being a static exhibit and evolve under the engine's existing laws,
// with the ground scene's machinery as the precedent - cohesive bonds from the material's own
// modulus, the one granular contact law, the one deposition door, damage::classify deciding fate,
// the docs/61 settle gauge, and the fold as re-coherence. NO NEW LAWS: the ball at the site is
// simulation::CohesiveBody - the SAME type, builder and verdict word as the ground scene's ball -
// and the terrain parcels are the same cohesive matter per stratum material.
//
// What drives it: the guard band's boundary state. During a live event the guards mirror the
// coarse field each coarse step (resample_guards), and the STEP-TO-STEP DELTA of that booked
// state - specific internal energy, specific kinetic energy, bulk velocity - is delivered into
// the fine matter through the one door operator (`Aggregate::deposit_impact`, the same call the
// ground scene's ball receives a meteor through), entering from the up-shock side. At the coarse
// quantum (~1e6 m) the whole 30 m site is sub-resolution, so the field's per-kilogram state delta
// IS the honest statement of what arrived at this ground; the door then spreads it, classifies
// each parcel against its own material's thresholds, breaks the bonds fate ends, and converts
// superheat to expansion - exactly as it does for the ground scene. Quiescent otherwise: with no
// arrivals the site just settles under gravity, contact and its own bonds.
//
// Boundary geometry: below the surface, beyond the fine bowl, is coarse rock. A fine parcel
// reaching that interface is resolved by THE terrain-contact law (`granular::terrain_contact_
// resolve`) evaluated in the local frame of the bowl wall - the same non-injecting projection
// the ground scene's ball uses against its voxel ground; parcels thrown clear of the bowl land
// on the declared surface through the same law via the same height function the builder used.
//
// Honest limits, stated: (a) energy that LEAVES the boundary between books stays booked at the
// window but cannot be un-deposited from the fine matter - the door only deposits arrivals; (b)
// the door's kernel is isotropic (the standing deposit_event IOU, inherited); (c) in the browser
// the site integrates in real seconds while the celestial clock runs scaled - the site clock is
// reported, and the lag is a compute statement, not physics (verdicts are deposit-driven and land
// at the coarse step; only the visible scatter unfolds on the site clock).
// -----------------------------------------------------------------------------------------------

/// Per-substep position-projection cap (m) at the bowl wall and the outside ground - the same
/// bound `simulation::Simulation` uses for a body particle against the terrain (its
/// BODY_MAX_SURFACE_CORRECTION), for the same stack-safety reason.
const SITE_MAX_SURFACE_CORRECTION: f64 = 0.01;

/// Substep ceiling per `step` call - the ground scene's own clamp. When the stability need
/// exceeds it, the site advances less time than asked (never understeps stability) and the
/// shortfall shows on the site clock.
const SITE_MAX_SUBSTEPS: usize = 256;

/// One cohesive body of the site's fine matter: the parcels of ONE material, with the map back
/// into `MaterializedSite::particles` so the rendered/audited field stays the same matter.
struct SiteBody {
    body: crate::simulation::CohesiveBody,
    /// Catalogue material index.
    mat: usize,
    /// site.particles index per aggregate particle.
    map: Vec<usize>,
}

/// **The released site, stepping** (module section above). Owns the fine parcels as cohesive
/// matter; the guards stay in `MaterializedSite` (they are the coarse field's, not the site's).
pub struct SiteDynamics {
    /// Terrain-patch matter, one body per stratum material present among the children.
    patch: Vec<SiteBody>,
    /// The declared ball - the ground scene's own type, so `verdict()` is the same word.
    ball: Option<SiteBody>,
    surface: GroundSurface,
    /// Ball column anchor in surface voxel coordinates: [x, ground-top y, z].
    col_voxel: [f64; 3],
    fine_radius_m: f64,
    /// J the boundary has delivered through the door since the window opened.
    pub delivered_j: f64,
    /// Coarse steps that actually deposited (arrivals, not departures).
    pub delivered_steps: usize,
    /// Site seconds actually integrated - the site's own clock (module section, limit c).
    pub clock_s: f64,
}

impl SiteDynamics {
    /// Build the dynamics from a RELEASED site. The release gate stands: an unreleased patch
    /// refuses with the rung's own stated residual - fidelity that may not enter stiff dynamics
    /// unbounded.
    pub fn new(
        site: &MaterializedSite,
        spec: &SiteSpec,
        mats: &[Material],
    ) -> Result<SiteDynamics, SiteRefusal> {
        if let SiteRelease::Unreleased { achieved, bound, iterations } = site.release {
            return Err(SiteRefusal::Refine(Refusal::NotConverged {
                achieved,
                bound,
                iterations,
            }));
        }
        let fine_end = site.particles.len() - site.ball_children;
        let build = |idxs: &[usize], spacing: f64| -> Result<SiteBody, SiteRefusal> {
            let mat = site.particles[idxs[0]].mat as usize;
            let m = mats
                .get(mat)
                .ok_or_else(|| SiteRefusal::UnknownMaterial { material: format!("#{mat}") })?;
            let c_heat = m
                .specific_heat()
                .ok_or_else(|| SiteRefusal::UnsourcedHeat { material: m.id.clone() })?;
            let parcels: Vec<crate::orbit::Body> = idxs
                .iter()
                .map(|&i| {
                    let p = &site.particles[i];
                    crate::orbit::Body {
                        pos: DVec3::new(p.pos[0] as f64, p.pos[1] as f64, p.pos[2] as f64),
                        vel: DVec3::new(p.vel[0] as f64, p.vel[1] as f64, p.vel[2] as f64),
                        mass: p.mass as f64,
                    }
                })
                .collect();
            let mut body = crate::simulation::CohesiveBody::from_parcels(
                parcels,
                mat,
                spacing,
                mats,
                spec.g_ms2 as f32,
            )
            .map_err(|_| SiteRefusal::UnknownMaterial { material: m.id.clone() })?;
            // Thermal state carried across the representation: the parcel's u = c*T, so T = u/c
            // (the same law materialize_site wrote it with) - nothing invented at the crossing.
            for (k, &i) in idxs.iter().enumerate() {
                body.agg.temps[k] = (site.particles[i].u as f64 / c_heat) as f32;
            }
            Ok(SiteBody { body, mat, map: idxs.to_vec() })
        };

        // Terrain parcels, grouped by material; spacing is each material's own child extent
        // (equal child mass, the rung's law).
        let mut groups: Vec<(u32, Vec<usize>)> = Vec::new();
        for i in site.fine_start..fine_end {
            let mat = site.particles[i].mat;
            match groups.iter_mut().find(|(g, _)| *g == mat) {
                Some((_, v)) => v.push(i),
                None => groups.push((mat, vec![i])),
            }
        }
        let mut patch = Vec::new();
        for (mat, idxs) in &groups {
            let rho = mats
                .get(*mat as usize)
                .map(|m| m.density as f64)
                .unwrap_or(1.0)
                .max(1.0);
            let child_mass = site.particles[idxs[0]].mass as f64;
            patch.push(build(idxs, coarse_particle_extent_m(child_mass, rho))?);
        }
        // The ball: its children sit exactly on the split stencil, so its bond spacing is the
        // stencil separation the split placed them at (derived from the child h, no new number).
        let ball = if site.ball_children > 0 {
            let idxs: Vec<usize> = (fine_end..site.particles.len()).collect();
            let h_c = site.particles[idxs[0]].h as f64;
            let sep = h_c * (refine::SPLIT_SEPARATION_OVER_H / refine::SPLIT_CHILD_H_OVER_H) as f64;
            Some(build(&idxs, sep)?)
        } else {
            None
        };
        Ok(SiteDynamics {
            patch,
            ball,
            surface: spec.surface.clone(),
            col_voxel: site.col_voxel,
            fine_radius_m: site.fine_radius_m,
            delivered_j: 0.0,
            delivered_steps: 0,
            clock_s: 0.0,
        })
    }

    /// The ball's one-word structural verdict - literally `CohesiveBody::verdict()`, the ground
    /// scene's own word from the same bond state.
    pub fn ball_verdict(&self) -> Option<&'static str> {
        self.ball.as_ref().map(|b| b.body.verdict())
    }

    /// **Deliver one coarse step's boundary arrival through the one door.** `prev` and `now` are
    /// consecutive booked states of the SAME guards; their per-kilogram delta is the coarse
    /// field's statement of what arrived at this sub-resolution ground. Positive deltas deposit
    /// (`Aggregate::deposit_impact` per body, entering from the up-shock side); departures stay
    /// booked at the window (module section, limit a). Returns the J delivered.
    pub fn deliver_boundary(
        &mut self,
        prev: &BoundaryState,
        now: &BoundaryState,
        mats: &[Material],
    ) -> f64 {
        if now.mass_kg <= 0.0 {
            return 0.0;
        }
        let d_ie = (now.internal_j - prev.internal_j) / now.mass_kg;
        let d_ke = (now.kinetic_j - prev.kinetic_j) / now.mass_kg;
        let d_v = DVec3::from(now.bulk_vel_ms) - DVec3::from(prev.bulk_vel_ms);
        let e_per_kg = d_ie.max(0.0) + d_ke.max(0.0);
        if e_per_kg <= 0.0 && d_v.length_squared() <= 0.0 {
            return 0.0;
        }
        // The shock's propagation direction: the way the boundary's bulk motion jumped; a pure
        // heat arrival (no momentum) enters from below - through the crust the guards are.
        let dir = d_v.try_normalize().unwrap_or(DVec3::Y);
        let mut delivered = 0.0;
        for b in self.patch.iter_mut().chain(self.ball.iter_mut()) {
            let agg = &mut b.body.agg;
            if agg.particles.is_empty() {
                continue;
            }
            let m_a: f64 = agg.particles.iter().map(|p| p.mass).sum();
            let com = agg.com();
            let r = agg
                .particles
                .iter()
                .map(|p| (p.pos - com).length())
                .fold(0.0, f64::max)
                + b.body.part_half;
            let entry = com - dir * r;
            let energy = e_per_kg * m_a;
            agg.deposit_impact(mats, entry, d_v * m_a, energy);
            delivered += energy;
        }
        if delivered > 0.0 {
            self.delivered_j += delivered;
            self.delivered_steps += 1;
        }
        delivered
    }

    /// **One frame of site time.** Substepped to the stiffest body's own stability need (the
    /// ground scene's clamp); when the need exceeds the ceiling the site advances what the
    /// budget affords and the clock says so. Cross-body contact and the bowl/ground boundary run
    /// per substep through the same laws as everything else; the synced `site.particles` stay
    /// the rendered, audited, foldable matter. Returns the seconds actually advanced.
    pub fn step(&mut self, site: &mut MaterializedSite, dt: f64, mats: &[Material]) -> f64 {
        if dt <= 0.0 {
            return 0.0;
        }
        let need = self
            .patch
            .iter()
            .chain(self.ball.iter())
            .map(|b| b.body.agg.stable_substeps(dt))
            .max()
            .unwrap_or(1)
            .max(1);
        let sub = need.min(SITE_MAX_SUBSTEPS);
        let eff = dt * sub as f64 / need as f64;
        let pdt = eff / sub as f64;
        let mu_of = |mat: usize| -> f64 {
            mats.get(mat).map(|m| m.friction_coefficient as f64).unwrap_or(0.0)
        };
        for _ in 0..sub {
            for b in self.patch.iter_mut().chain(self.ball.iter_mut()) {
                b.body.substep(pdt);
            }
            // The boundary, through THE terrain-contact law (the same non-injecting, capped
            // projection the ground scene's ball uses against its voxel ground; it cannot do
            // work, so a declared configuration that already touches is resolved gently, never
            // detonated). Two floors, one law:
            //  - PATCH parcels are the ground itself: inside the footprint their interface is
            //    the bowl wall (coarse rock beyond the fine region), resolved in the wall's
            //    local frame; parcels thrown clear of the bowl land on the declared surface
            //    through the same height law the builder used.
            //  - BALL parcels stand ON the ground: their floor is the declared surface law
            //    everywhere, exactly as the ground scene's ball rests on its terrain. The
            //    site-local crater changing this floor is a named IOU (module section): the
            //    parcels themselves carry the crater's truth; the floor stays the declared
            //    surface until site-local excavation of the height law lands.
            let r_fine = self.fine_radius_m;
            let surface = &self.surface;
            let col = self.col_voxel;
            let n_patch = self.patch.len();
            for (bi, b) in self.patch.iter_mut().chain(self.ball.iter_mut()).enumerate() {
                let is_ball = bi >= n_patch;
                let half = b.body.part_half;
                let mu = mu_of(b.mat);
                for p in b.body.agg.particles.iter_mut() {
                    let h = |x: f64, z: f64| -> f64 {
                        crate::world::terrain_height_with(surface, x as f32, z as f32) as f64
                            - col[1]
                    };
                    let (vx, vz) = (col[0] + p.pos.x, col[2] + p.pos.z);
                    let horiz = (p.pos.x * p.pos.x + p.pos.z * p.pos.z).sqrt();
                    // PATCH matter inside the footprint is the ground itself: its only interface
                    // is the bowl wall (the surface floor would fight the very parcels that ARE
                    // the surface there). Everything else - the ball standing on the ground, and
                    // ejecta thrown clear of the bowl - meets the declared surface floor.
                    if !is_ball && horiz <= r_fine {
                        // The bowl wall, in its local frame (up = back toward the interior).
                        let r = p.pos.length();
                        if r > 1.0e-9 && r + half > r_fine {
                            let up = -p.pos / r;
                            let e1 = up.any_orthonormal_vector();
                            let e2 = up.cross(e1);
                            let pos_l = DVec3::new(0.0, r_fine - r, 0.0);
                            let vel_l = DVec3::new(p.vel.dot(e1), p.vel.dot(up), p.vel.dot(e2));
                            let hit = crate::granular::terrain_contact_resolve(
                                pos_l,
                                vel_l,
                                0.0,
                                0.0,
                                0.0,
                                half,
                                mu,
                                SITE_MAX_SURFACE_CORRECTION,
                                f64::INFINITY,
                            );
                            if hit.hit {
                                p.vel = e1 * hit.vel.x + up * hit.vel.y + e2 * hit.vel.z;
                                p.pos += e1 * hit.dpos.x + up * hit.dpos.y + e2 * hit.dpos.z;
                            }
                        }
                    } else {
                        // The declared ground, through the same height law the builder used
                        // (one column law, docs/59).
                        let h0 = h(vx, vz);
                        let dhdx = h(vx + 0.5, vz) - h(vx - 0.5, vz);
                        let dhdz = h(vx, vz + 0.5) - h(vx, vz - 0.5);
                        let hit = crate::granular::terrain_contact_resolve(
                            p.pos,
                            p.vel,
                            h0,
                            dhdx,
                            dhdz,
                            half,
                            mu,
                            SITE_MAX_SURFACE_CORRECTION,
                            f64::INFINITY,
                        );
                        if hit.hit {
                            p.vel = hit.vel;
                            p.pos += hit.dpos;
                        }
                    }
                }
            }
        }
        self.clock_s += eff;
        self.sync(site, mats);
        eff
    }

    /// Write the dynamic state back into the site's particle field - positions, velocities, and
    /// u = c*T (the same relation the state entered with), so the render, the settle gauge, the
    /// window audit and the fold all read the ONE matter.
    fn sync(&self, site: &mut MaterializedSite, mats: &[Material]) {
        for b in self.patch.iter().chain(self.ball.iter()) {
            let c_heat = mats
                .get(b.mat)
                .and_then(|m| m.specific_heat())
                .unwrap_or(1000.0);
            for (k, &i) in b.map.iter().enumerate() {
                let p = &b.body.agg.particles[k];
                let sp = &mut site.particles[i];
                sp.pos = [p.pos.x as f32, p.pos.y as f32, p.pos.z as f32];
                sp.vel = [p.vel.x as f32, p.vel.y as f32, p.vel.z as f32];
                sp.u = (c_heat * b.body.agg.temps[k] as f64) as f32;
            }
        }
    }

    /// Per-parcel FATE COUNTS through `damage::classify` - the readout the demo asserts on: each
    /// parcel's thermal energy density (rho c (T - ref), the same J/m3 = Pa stand-in the door
    /// uses) against its own material's thresholds. Returns (solid, fractured, melted, vaporized,
    /// decomposed) over all fine parcels.
    pub fn fate_counts(&self, mats: &[Material]) -> (usize, usize, usize, usize, usize) {
        let mut n = (0, 0, 0, 0, 0);
        for b in self.patch.iter().chain(self.ball.iter()) {
            let Some(m) = mats.get(b.mat) else { continue };
            let rho = (m.density as f64).max(1.0);
            let c = m.specific_heat().unwrap_or(1000.0) as f64;
            for &t in &b.body.agg.temps {
                let e = (rho * c * (t as f64 - crate::matter::REF_TEMP_K as f64)).max(0.0);
                match crate::damage::classify(e, m, e) {
                    crate::damage::PhaseChange::Intact => n.0 += 1,
                    crate::damage::PhaseChange::Fractured => n.1 += 1,
                    crate::damage::PhaseChange::Melted => n.2 += 1,
                    crate::damage::PhaseChange::Vaporized => n.3 += 1,
                    crate::damage::PhaseChange::Decomposed => n.4 += 1,
                }
            }
        }
        n
    }

    /// The one-line dynamics readout for the HUD: the site clock, what the boundary delivered,
    /// the ball's verdict word and the classify fate mix.
    pub fn hud_line(&self, mats: &[Material]) -> String {
        let (solid, frac, melt, vap, dec) = self.fate_counts(mats);
        let ball = match self.ball.as_ref() {
            Some(b) => format!(
                "ball {} ({}/{} bonds)",
                b.body.verdict().to_uppercase(),
                b.body.agg.active_bonds(),
                b.body.agg.bonds.len()
            ),
            None => String::from("no ball declared"),
        };
        format!(
            "site dynamics: {ball} · parcels {solid} solid / {frac} fractured / {melt} molten / \
             {vap} vapor{} · boundary delivered {:.2e} J over {} steps · site clock {:.1} s \
             (real-rate; the celestial clock runs scaled)",
            if dec > 0 { format!(" / {dec} decomposed") } else { String::new() },
            self.delivered_j,
            self.delivered_steps,
            self.clock_s
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::materials;

    fn shipped_ground_world() -> World {
        let json = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../web/public/worlds/ground/world.json"
        ))
        .expect("shipped ground world");
        World::parse(&json).expect("parses")
    }

    /// **The threshold is a derivation, not a dial.** One coarse particle's matter share `s`
    /// subtends the budget exactly at the returned distance (`d * theta = s`); a finer field
    /// resolves closer; degenerate inputs return 0 (no threshold, never a NaN).
    #[test]
    fn the_view_threshold_is_where_one_coarse_particle_subtends_the_budget() {
        // Hand-computed: Earth's total mass over the 2400 statement is 5.972e24/2400 = 2.488e21
        // kg; at the outer layer's 2900 kg/m^3 that is 8.58e17 m^3, cube edge 9.50e5 m.
        let s = coarse_particle_extent_m(5.972e24 / 2400.0, 2900.0);
        assert!((s - 9.50e5).abs() < 5.0e3, "hand-computed quantum: got {s:.3e}");

        let theta = crate::resolution::ResolutionController::default().angular_resolution;
        let d = view_resolution_distance(s, theta);
        assert!(
            (d * theta - s).abs() <= s * 1.0e-12,
            "at the threshold the quantum subtends exactly the budget"
        );
        // 9.5e5 m over the 1 mrad budget: ~9.5e8 m - inside the space band's camera envelope.
        assert!((8.0e8..1.2e9).contains(&d), "threshold ~9.5e8 m, got {d:.3e}");

        // One rung finer (mass / 13) resolves closer by 13^(1/3).
        let s_fine = coarse_particle_extent_m(5.972e24 / 2400.0 / 13.0, 2900.0);
        let ratio = d / view_resolution_distance(s_fine, theta);
        assert!((ratio - 13.0f64.cbrt()).abs() < 1.0e-9, "one rung = 13^(1/3) closer");

        // Degenerate inputs are a stated no-threshold, never NaN.
        assert_eq!(view_resolution_distance(0.0, theta), 0.0);
        assert_eq!(view_resolution_distance(s, 0.0), 0.0);
        assert_eq!(coarse_particle_extent_m(1.0, 0.0), 0.0);

        // The spec derives the same quantum from the shipped world's own body.
        let spec = SiteSpec::from_ground_world(&shipped_ground_world()).expect("spec");
        let earth = crate::planet::earth();
        let expect = coarse_particle_extent_m(
            earth.total_mass() / CELESTIAL_STATEMENT as f64,
            earth.layers.last().unwrap().density,
        );
        assert_eq!(spec.declared_coarse_extent_m(), expect, "the definition answers");

        // A live field's quantum is MEASURED: the largest matter share any particle carries.
        let field = vec![
            SphParticle { pos: [0.0; 3], h: 1.0, vel: [0.0; 3], u: 0.0, mass: 2700.0, mat: 0, rho: 2700.0, prov: 0 },
            SphParticle { pos: [5.0, 0.0, 0.0], h: 1.0, vel: [0.0; 3], u: 0.0, mass: 8.0 * 2700.0, mat: 0, rho: 2700.0, prov: 0 },
        ];
        assert!((measured_coarse_extent_m(&field) - 2.0).abs() < 1.0e-6, "max share, not mean");
    }

    /// **The trigger fires descending, re-arms ascending, and a demand stands until confirmed.**
    /// The bidirectional contract the out-and-back demo arc needs.
    #[test]
    fn the_trigger_fires_descending_and_rearms_ascending() {
        let at = 1.0e9;
        let mut t = SiteTrigger::new();

        // Approaching but still outside: silent.
        assert_eq!(t.observe(2.0e9, at), None);
        // Descending inside: Materialize demanded.
        assert_eq!(t.observe(0.9e9, at), Some(SiteCrossing::Materialize));
        // A REFUSED materialization leaves the demand standing (no confirm): still demanded.
        assert_eq!(t.observe(0.8e9, at), Some(SiteCrossing::Materialize));
        // Executed and confirmed: the demand clears while the camera stays inside.
        t.confirm(SiteCrossing::Materialize);
        assert!(t.is_resolved());
        assert_eq!(t.observe(0.8e9, at), None);

        // Ascending outside: Deresolve demanded; an unsettled site refuses, the demand stands.
        assert_eq!(t.observe(1.1e9, at), Some(SiteCrossing::Deresolve));
        assert_eq!(t.observe(1.2e9, at), Some(SiteCrossing::Deresolve));
        t.confirm(SiteCrossing::Deresolve);
        assert!(!t.is_resolved());
        assert_eq!(t.observe(1.2e9, at), None, "folded and outside: armed, silent");

        // Re-armed: the second descent fires again - the out-and-back arc.
        assert_eq!(t.observe(0.9e9, at), Some(SiteCrossing::Materialize));
    }

    /// **The materialized site conserves through the refine audit.** The shipped ground world's
    /// ball and patch, built from the definition alone, split-relax-released with the ledger
    /// checked - mass, momentum, kinetic and internal energy to f32 rounding, angular momentum
    /// within the relax's own stated bound, density released under the stated bound, and the
    /// budget respected.
    #[test]
    fn the_materialized_site_conserves_through_the_refine_audit() {
        let spec = SiteSpec::from_ground_world(&shipped_ground_world()).expect("spec");
        let mats = materials::load();
        let site = materialize_site(&spec, &HandDown::Declared, &mats)
            .expect("the declared site must materialize");

        assert!(
            site.particles.len() <= SITE_PATCH_BUDGET,
            "the patch rides the declared budget: {} > {}",
            site.particles.len(),
            SITE_PATCH_BUDGET
        );

        // The ball: 13 iron children (the first parent split) whose masses sum to the declared
        // ball's real mass, clustered at the declared position.
        let ball = spec.ball.as_ref().expect("the shipped world declares the ball");
        let rho_iron = mats[materials::index_of(&mats, "iron")].density as f64;
        let m_ball = rho_iron * 4.0 / 3.0 * std::f64::consts::PI * ball.radius_m.powi(3);
        assert_eq!(site.ball_children, 13, "one rung: 13 children");
        let kids = &site.particles[site.particles.len() - site.ball_children..];
        let iron_idx = materials::index_of(&mats, "iron") as u32;
        let m_kids: f64 = kids.iter().map(|p| p.mass as f64).sum();
        assert!(
            (m_kids - m_ball).abs() <= m_ball * 1.0e-5,
            "ball children carry exactly the declared ball's mass: {m_kids:.6e} vs {m_ball:.6e}"
        );
        for k in kids {
            assert_eq!(k.mat, iron_idx, "the ball is iron, its children are iron");
        }
        let spread = kids
            .iter()
            .map(|p| Vec3::from(p.pos))
            .fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), p| {
                (lo.min(p), hi.max(p))
            });
        assert!(
            (spread.1 - spread.0).length() < 20.0,
            "the ball's children cluster at the site, not scattered"
        );

        // The patch children come from the site's own strata - every material is one the strata
        // declare, and at this land site the top parents are the body's own crust (basalt),
        // because the 1 m grass skin is sub-quantum at this rung (module doc).
        let strata_idx: Vec<u32> = spec
            .surface
            .strata
            .iter()
            .map(|s| materials::index_of(&mats, &s.material) as u32)
            .collect();
        let patch = &site.particles[site.fine_start..site.particles.len() - site.ball_children];
        assert!(!patch.is_empty(), "a terrain patch materializes alongside the ball");
        for p in patch {
            assert!(
                strata_idx.contains(&p.mat),
                "patch child material {} is not in the site's strata",
                p.mat
            );
        }
        let basalt_idx = materials::index_of(&mats, "basalt") as u32;
        assert!(
            patch.iter().filter(|p| p.mat == basalt_idx).count() > patch.len() / 2,
            "this site's shallow column is the body's own basalt crust"
        );

        // THE LEDGER: conserved end to end, the audit the HUD surfaces.
        let l = site.ledger;
        assert!((l.after_relax.mass - l.before.mass).abs() <= l.before.mass * 1.0e-6);
        assert!(
            (l.after_relax.momentum - l.before.momentum).length()
                <= (l.before.mass * 1.0e-6).max(1.0e-9),
            "momentum conserved (parents are at rest in the site frame)"
        );
        assert!((l.after_relax.kinetic - l.before.kinetic).abs() <= l.before.kinetic.max(1.0) * 1.0e-6);
        assert!(
            (l.after_relax.internal - l.before.internal).abs() <= l.before.internal * 1.0e-6,
            "internal energy conserved: {:.6e} vs {:.6e}",
            l.before.internal,
            l.after_relax.internal
        );
        let am_drift = (l.after_relax.angular_momentum - l.after_split.angular_momentum).length();
        assert!(am_drift <= l.relax_am_bound * (1.0 + 1.0e-6) + 1.0e-9);
        // The release state is HONEST either way: released under the bound, or the exact split
        // with the stalled residual stated (this site's relief stalls the relax today - the
        // refine-level test pins that behaviour; if the rung improves, Released is fine too).
        let release_line = match site.release {
            SiteRelease::Released(r) => {
                assert!(r.released_max_density_error <= r.release_bound);
                assert!(r.release_bound >= refine::RELEASE_DENSITY_ERROR);
                format!(
                    "released {:.3e} under bound {:.3e} in {} iterations",
                    r.released_max_density_error, r.release_bound, r.iterations
                )
            }
            SiteRelease::Unreleased { achieved, bound, iterations } => {
                assert!(achieved.is_finite() && achieved > bound, "a stated, real residual");
                assert_eq!(
                    l.relax_am_bound, 0.0,
                    "split-only: no relax ran, so no relax drift bound"
                );
                format!("UNRELEASED: residual {achieved:.3e} over {bound:.3e} after {iterations} iterations")
            }
        };
        assert!(
            (site.declared_mass_kg - l.before.mass).abs() <= l.before.mass * 1.0e-6,
            "the declared mass is what the parents carried in"
        );

        // The numbers the report quotes.
        println!(
            "site audit: mass {:.6e} kg in, {:.6e} kg out; {}; AM drift {:.3e} within bound {:.3e}; \
             {} guards + {} children",
            l.before.mass,
            l.after_relax.mass,
            release_line,
            am_drift,
            l.relax_am_bound,
            site.fine_start,
            site.particles.len() - site.fine_start,
        );
    }

    /// **The strata law is the ground scene's, not a private copy** (Law II): the site's
    /// material-at-depth mirrors `world::generate_from` on the same declared surface, checked
    /// against the generated voxels away from band boundaries.
    #[test]
    fn the_site_column_agrees_with_the_generated_ground() {
        let spec = SiteSpec::from_ground_world(&shipped_ground_world()).expect("spec");
        let mats = materials::load();
        let world = crate::world::generate_from(&spec.surface, &mats);
        let site = materialize_site(&spec, &HandDown::Declared, &mats)
            .expect("the declared site must materialize");

        // Every patch child sits in the material the generated world holds at its depth - the
        // parent's centre depth decides identity, so compare against the band the centre is in,
        // skipping children within one child-extent of a band boundary (rounding differs there).
        let patch = &site.particles[site.fine_start..site.particles.len() - site.ball_children];
        let mut compared = 0;
        for p in patch {
            let vx = (p.pos[0] + world.center().x) as i32;
            let vz = (p.pos[2] + world.center().z) as i32;
            // The site frame's y origin is the ball column's ground top; re-derive the voxel y
            // through the same height law the builder used.
            let top_ball = crate::world::terrain_height_with(
                &spec.surface,
                world.center().x + spec.ball.as_ref().unwrap().at_m[0],
                world.center().z + spec.ball.as_ref().unwrap().at_m[2],
            );
            let vy = (p.pos[1] + top_ball) as i32;
            if vx < 0 || vz < 0 || vy < 0 {
                continue;
            }
            let Some(mat) = world.material_at(vx, vy, vz) else { continue };
            // Skip the 1 m skin and band edges: the parent centre law and voxel rounding may
            // legitimately differ within one cell of a boundary.
            let above = world.material_at(vx, vy + 2, vz);
            let below = world.material_at(vx, vy - 2, vz);
            if above != Some(mat) || below != Some(mat) {
                continue;
            }
            assert_eq!(
                p.mat, mat as u32,
                "one column law: the site child at ({}, {}, {}) disagrees with the ground",
                vx, vy, vz
            );
            compared += 1;
        }
        assert!(compared > 10, "the comparison must actually bite (compared {compared})");
    }

    /// **A quiescent field hands its state down; a mid-event field refuses with the reason
    /// stated; an uncovered site refuses too.** The smallest honest hand-down (module doc).
    #[test]
    fn the_hand_down_samples_quiet_fields_and_refuses_mid_event() {
        let mk = |vel: [f32; 3], u: f32| SphParticle {
            pos: [1.0e5, 0.0, 0.0],
            h: 1.0e6,
            vel,
            u,
            mass: 2.5e21,
            mat: 0,
            rho: 2900.0,
            prov: 0,
        };
        let site = DVec3::new(0.0, 0.0, 0.0);
        let g = 9.88;

        // Quiet: residual speeds far under v_q = sqrt(2 g s) with s the measured quantum.
        let quiet = vec![mk([1.0, 0.0, 0.0], 2.0e5), mk([-1.0, 0.0, 0.0], 4.0e5)];
        match sample_hand_down(&quiet, site, g) {
            Ok(HandDown::Sampled { u_j_kg, peak_speed_ms, quiescent_speed_ms }) => {
                assert!((u_j_kg - 3.0e5).abs() < 1.0, "mass-weighted mean u");
                assert!(peak_speed_ms < quiescent_speed_ms);
                // v_q at the 9.5e5 m quantum under g: sqrt(2 * 9.88 * 9.5e5) ~ 4333 m/s.
                assert!((4000.0..4700.0).contains(&quiescent_speed_ms), "v_q from the quantum");
            }
            other => panic!("a quiet field must sample, got {other:?}"),
        }

        // Mid-event: bulk motion removed, residual 6 km/s over v_q - refused, reason stated.
        let hot = vec![mk([6000.0, 0.0, 0.0], 2.0e5), mk([-6000.0, 0.0, 0.0], 2.0e5)];
        match sample_hand_down(&hot, site, g) {
            Err(r @ SiteRefusal::MidEvent { .. }) => {
                let msg = format!("{r}");
                assert!(msg.contains("mid-event"), "the reason must be stated: {msg}");
            }
            other => panic!("a mid-event field must refuse, got {other:?}"),
        }
        // A shared BULK velocity is not an event: the same 6 km/s applied to both is quiet.
        let comoving = vec![mk([6000.0, 1.0, 0.0], 2.0e5), mk([6000.0, -1.0, 0.0], 2.0e5)];
        assert!(sample_hand_down(&comoving, site, g).is_ok(), "bulk motion is not an event");

        // Uncovered: a field whose support does not reach the site refuses.
        let far = vec![mk([0.0; 3], 2.0e5)];
        match sample_hand_down(&far, DVec3::new(5.0e6, 0.0, 0.0), g) {
            Err(SiteRefusal::Uncovered) => {}
            other => panic!("an uncovered site must refuse, got {other:?}"),
        }

        // And a sampled hand-down actually reaches the children: materialize the shipped site
        // with a sampled u and every child carries it.
        let spec = SiteSpec::from_ground_world(&shipped_ground_world()).expect("spec");
        let mats = materials::load();
        let hand = HandDown::Sampled { u_j_kg: 7.7e5, peak_speed_ms: 1.0, quiescent_speed_ms: 4000.0 };
        let site = materialize_site(&spec, &hand, &mats).expect("materializes");
        for p in &site.particles {
            assert!((p.u - 7.7e5).abs() < 1.0, "the sampled u carries down to every particle");
        }
    }

    /// **The guards ARE the coarse field at the boundary, and the window books the event.**
    /// The docs/59 hand-down made concrete, decision 2: during a live event the guard band
    /// re-samples the coarse field each coarse step, so the impact's energy arrives at the fine
    /// patch as real boundary state. Ownership stays single: guards mirror state (never mass,
    /// never position), the fine children stay bitwise untouched (the release gate still stands
    /// between the patch and any dynamics), and the window's booked energy agrees with an
    /// independent audit of the guards within the stated bound - the conservation statement
    /// across the event window.
    #[test]
    fn the_guards_carry_the_coarse_field_and_the_window_books_the_event() {
        let spec = SiteSpec::from_ground_world(&shipped_ground_world()).expect("spec");
        let mats = materials::load();
        let mut site = materialize_site(&spec, &HandDown::Declared, &mats).expect("materializes");
        let before: Vec<SphParticle> = site.particles.clone();
        let m_guard: f64 = site.particles[..site.fine_start].iter().map(|p| p.mass as f64).sum();
        assert!(site.fine_start > 0, "the site carries a guard band to be the boundary");

        // An orthonormal frame like the wired one: the site sits on +x of the field frame.
        let frame = SiteFrame {
            origin_rel_m: DVec3::new(6.371e6, 0.0, 0.0),
            east: DVec3::new(0.0, 1.0, 0.0),
            up: DVec3::new(1.0, 0.0, 0.0),
            north: DVec3::new(0.0, 0.0, 1.0),
        };
        // A coarse field in ONE uniform state whose support covers the whole site:
        // interpolating a uniform field returns that field, so every covered guard must carry
        // exactly this state - the guards ARE the coarse field at the boundary.
        let coarse = |u: f32, vel: [f32; 3]| -> Vec<SphParticle> {
            vec![
                SphParticle { pos: [6.371e6, 0.0, 0.0], h: 1.0e6, vel, u, mass: 2.5e21, mat: 0, rho: 2900.0, prov: 0 },
                SphParticle { pos: [6.0e6, 8.0e5, 0.0], h: 1.0e6, vel, u, mass: 2.5e21, mat: 0, rho: 2900.0, prov: 0 },
            ]
        };

        let mut window = EventWindow::default();
        assert!(!window.is_open(), "no event yet, no window");

        // Coarse step 1 - the quiet field before the shock.
        let s1 = resample_guards(&mut site, &coarse(3.0e5, [0.0; 3]), &frame);
        window.book(s1);
        assert!(window.is_open());
        assert_eq!(s1.uncovered, 0, "the coarse Earth covers its own ground");
        assert_eq!(s1.covered, site.fine_start);
        for g in &site.particles[..site.fine_start] {
            assert_eq!(g.u, 3.0e5, "a covered guard carries exactly the field's u");
            assert_eq!(g.vel, [0.0; 3]);
            assert_eq!(g.rho, 2900.0, "and the field's density");
        }

        // Coarse step 2 - the shock arrives at the boundary: the field is hot and moving.
        let vel = [1200.0f32, -300.0, 400.0];
        let v_len = (1200.0f64 * 1200.0 + 300.0 * 300.0 + 400.0 * 400.0).sqrt();
        let s2 = resample_guards(&mut site, &coarse(9.0e5, vel), &frame);
        window.book(s2);
        for g in &site.particles[..site.fine_start] {
            assert_eq!(g.u, 9.0e5);
            let sp = Vec3::from(g.vel).length() as f64;
            assert!(
                (sp - v_len).abs() <= v_len * 1.0e-6,
                "the basis change preserves the field speed: {sp} vs {v_len}"
            );
        }
        // The booked energy agrees with an independent audit of the guard band, within the
        // window's stated bound (f32 state rounding) - energy across the event window.
        let expect_ie = m_guard * 9.0e5;
        let expect_ke = 0.5 * m_guard * v_len * v_len;
        assert!((s2.internal_j - expect_ie).abs() <= expect_ie * WINDOW_BOOK_BOUND);
        assert!((s2.kinetic_j - expect_ke).abs() <= expect_ke * WINDOW_BOOK_BOUND);
        let d_ie = m_guard * (9.0e5 - 3.0e5);
        assert!(
            (window.arrived_internal_j() - d_ie).abs() <= d_ie * WINDOW_BOOK_BOUND,
            "the window books what arrived: {:.4e} vs {:.4e}",
            window.arrived_internal_j(),
            d_ie
        );
        assert!((window.arrived_kinetic_j() - expect_ke).abs() <= expect_ke * WINDOW_BOOK_BOUND);
        assert!(window.peak_speed_ms() >= v_len * (1.0 - 1.0e-6));

        // Coarse step 3 - the shock passes and the field returns to the quiet state: the
        // boundary returns with it. Nothing invented, nothing silently kept.
        let s3 = resample_guards(&mut site, &coarse(3.0e5, [0.0; 3]), &frame);
        window.book(s3);
        assert!(window.arrived_internal_j().abs() <= expect_ie * WINDOW_BOOK_BOUND);
        assert!(window.arrived_kinetic_j().abs() <= expect_ke * WINDOW_BOOK_BOUND);
        assert_eq!(window.steps(), 3);
        assert!(window.peak_speed_ms() >= v_len * (1.0 - 1.0e-6), "the peak stays booked");

        // OWNERSHIP STAYS SINGLE. Guards mirror state only: no particle moved, gained or lost
        // mass, or changed material; the fine children (patch + ball) are bitwise untouched.
        for (a, b) in before.iter().zip(&site.particles) {
            assert_eq!(a.pos, b.pos, "the resample never moves matter");
            assert_eq!(a.mass, b.mass, "and never re-owns it");
            assert_eq!(a.mat, b.mat);
        }
        for (a, b) in before[site.fine_start..].iter().zip(&site.particles[site.fine_start..]) {
            assert_eq!(a.vel, b.vel, "the RESAMPLE alone never steps the fine patch");
            assert_eq!(a.u, b.u);
            assert_eq!(a.rho, b.rho);
        }
        let audit = refine::audit(&site.particles);
        assert!(
            (audit.mass - site.declared_mass_kg).abs() <= site.declared_mass_kg * 1.0e-6,
            "no parcel is counted twice: the site still holds exactly the declared mass"
        );

        // The line the HUD renders names the numbers and the two-EOS collapse IOU.
        let line = window.hud_line();
        assert!(line.contains("EOS"), "the guard band's EOS collapse is a stated IOU: {line}");

        // A field whose support no longer reaches the site leaves the boundary as it was, and
        // says so: uncovered guards are counted, not smoothed over.
        let mut far = coarse(7.7e5, [5000.0, 0.0, 0.0]);
        for p in &mut far {
            p.pos[0] += 3.0e7;
        }
        let s4 = resample_guards(&mut site, &far, &frame);
        assert_eq!(s4.covered, 0);
        assert_eq!(s4.uncovered, site.fine_start);
        for g in &site.particles[..site.fine_start] {
            assert_eq!(g.u, 3.0e5, "an uncovered guard keeps its last honest state");
        }
    }

    /// **The released site enters dynamics and, quiescent, comes to rest** (docs/59, the
    /// hand-down's remaining core). No boundary arrival: the declared ball FALLS under the
    /// planet's own gravity, lands on the patch through the one contact law, and RESTS - the
    /// same fall-rest-stay contract the ground scene's ball test pins - while the patch stays
    /// put in its bowl. The gauge then settles and the fold conserves the declared mass, so the
    /// whole out-and-back arc works with dynamics live.
    #[test]
    fn the_released_site_steps_quiescently_and_the_ball_falls_to_rest() {
        let spec = SiteSpec::from_ground_world(&shipped_ground_world()).expect("spec");
        let mats = materials::load();
        let mut site = materialize_site(&spec, &HandDown::Declared, &mats).expect("materializes");
        assert!(
            matches!(site.release, SiteRelease::Released(_)),
            "the site releases under the rung's derived bound; dynamics may enter"
        );
        let mut dyn_ = SiteDynamics::new(&site, &spec, &mats).expect("released: dynamics enter");
        assert_eq!(dyn_.ball_verdict(), Some("intact"), "the declared ball starts whole");

        let ball_i0 = site.particles.len() - site.ball_children;
        let y0 = site.particles[ball_i0..]
            .iter()
            .map(|p| p.pos[1] as f64)
            .sum::<f64>()
            / site.ball_children as f64;

        // Ten seconds of site time: fall, land, ring down.
        let mut t = 0.0;
        while t < 10.0 {
            t += dyn_.step(&mut site, 1.0 / 60.0, &mats);
        }
        let ball_y = |site: &MaterializedSite| {
            site.particles[ball_i0..]
                .iter()
                .map(|p| p.pos[1] as f64)
                .sum::<f64>()
                / site.ball_children as f64
        };
        let y1 = ball_y(&site);
        assert!(y1 < y0 - 0.5, "gravity pulls the ball down without help ({y0:.2} -> {y1:.2})");
        assert!(y1 > -8.0, "and the patch/bowl stops it - it does not fall through the ground");

        // STAYS: two more seconds move it almost nowhere, and it is still intact - its own
        // landing is far under iron's thresholds (the ground scene's same assertion).
        let com1 = y1;
        let mut t2 = 0.0;
        while t2 < 2.0 {
            t2 += dyn_.step(&mut site, 1.0 / 60.0, &mats);
        }
        assert!((ball_y(&site) - com1).abs() < 0.5, "settled ({com1:.2} -> {:.2})", ball_y(&site));
        assert_eq!(dyn_.ball_verdict(), Some("intact"), "landing does not shatter iron");
        let (solid, frac, melt, vap, dec) = dyn_.fate_counts(&mats);
        assert_eq!(
            (frac, melt, vap, dec),
            (0, 0, 0, 0),
            "no event, no fate change: every parcel classifies Intact ({solid} solid)"
        );

        // Nothing delivered, and the sync kept the audited mass exactly the declared mass.
        assert_eq!(dyn_.delivered_j, 0.0);
        let audit = refine::audit(&site.particles);
        assert!(
            (audit.mass - site.declared_mass_kg).abs() <= site.declared_mass_kg * 1.0e-6,
            "dynamics never create or destroy matter"
        );

        // The gauge settles on the site's own measured peak speed, and the fold conserves.
        let g = spec.g_ms2 as f32;
        let mut gauge = SettleGauge::new();
        let mut quiet = 0.0f64;
        while quiet < 60.0 {
            let adv = dyn_.step(&mut site, 1.0 / 60.0, &mats);
            gauge.observe(site_peak_speed(&site), g, adv as f32);
            quiet += adv;
            if gauge.settled(g) {
                break;
            }
        }
        assert!(gauge.settled(g), "a quiescent site settles at the docs/61 criterion");
        let rep = fold_site(&site, &gauge, g).expect("a settled dynamic site folds");
        assert!(
            (rep.audit.mass - rep.declared_mass_kg).abs() <= rep.declared_mass_kg * 1.0e-6,
            "the fold returns exactly the declared mass after real dynamics"
        );
    }

    /// **THE DEMO BEAT AT THE SEAM: the celestial event's arriving energy shatters the site's
    /// ball** (docs/23's sentence at the site; docs/59 decision 2 completed). The guards mirror
    /// a quiet coarse field, then the field goes hot and fast at moon-impact magnitudes (the
    /// pi test's ~10 km/s contact: specific KE ~5e7 J/kg, a post-shock u jump of the same
    /// order); the step-to-step boundary delta is delivered through the ONE door
    /// (`Aggregate::deposit_impact`, the ground scene's own operator), and the verdicts are
    /// `damage::classify`'s alone - nothing here asks for destruction. At this energy density
    /// iron is far past its thresholds, so the ball SHATTERS; if the patch wholesale vaporizes,
    /// that is the honest outcome and the fate counts say so.
    #[test]
    fn a_landed_drops_boundary_energy_shatters_the_sites_ball_by_classify() {
        let spec = SiteSpec::from_ground_world(&shipped_ground_world()).expect("spec");
        let mats = materials::load();
        let mut site = materialize_site(&spec, &HandDown::Declared, &mats).expect("materializes");
        let mut dyn_ = SiteDynamics::new(&site, &spec, &mats).expect("dynamics enter");

        // Settle the ball onto the patch first, so the hit lands on a RESTING witness.
        let mut t = 0.0;
        while t < 8.0 {
            t += dyn_.step(&mut site, 1.0 / 60.0, &mats);
        }
        assert_eq!(dyn_.ball_verdict(), Some("intact"));

        // The same wired frame and coarse-field shape the window test uses.
        let frame = SiteFrame {
            origin_rel_m: DVec3::new(6.371e6, 0.0, 0.0),
            east: DVec3::new(0.0, 1.0, 0.0),
            up: DVec3::new(1.0, 0.0, 0.0),
            north: DVec3::new(0.0, 0.0, 1.0),
        };
        let coarse = |u: f32, vel: [f32; 3]| -> Vec<SphParticle> {
            vec![
                SphParticle { pos: [6.371e6, 0.0, 0.0], h: 1.0e6, vel, u, mass: 2.5e21, mat: 0, rho: 2900.0, prov: 0 },
                SphParticle { pos: [6.0e6, 8.0e5, 0.0], h: 1.0e6, vel, u, mass: 2.5e21, mat: 0, rho: 2900.0, prov: 0 },
            ]
        };
        let mut window = EventWindow::default();

        // Coarse step 1: the quiet crust before the shock (ambient u, at rest).
        let quiet = resample_guards(&mut site, &coarse(2.4e5, [0.0; 3]), &frame);
        window.book(quiet);

        // Coarse step 2: the shock arrives. Moon-drop magnitudes: the ground surges at km/s and
        // the specific internal energy jumps by ~5e6 J/kg (orders below the ~5e7 J/kg specific
        // KE of a 10 km/s contact - a CONSERVATIVE share of it reaching this ring of crust).
        let hot = resample_guards(&mut site, &coarse(5.0e6, [2000.0, -500.0, 800.0]), &frame);
        let prev = window.last_state();
        window.book(hot);
        let delivered = dyn_.deliver_boundary(&prev, &hot, &mats);

        // The delivery is the booked arrival, exactly: m_fine x (d_u + d_ke per kg).
        let m_fine: f64 = site.particles[site.fine_start..].iter().map(|p| p.mass as f64).sum();
        let d_u = (hot.internal_j - quiet.internal_j) / hot.mass_kg;
        let d_ke = (hot.kinetic_j - quiet.kinetic_j) / hot.mass_kg;
        let expect = m_fine * (d_u + d_ke);
        assert!(
            (delivered - expect).abs() <= expect * 1.0e-9,
            "the door delivers the boundary's booked arrival: {delivered:.4e} vs {expect:.4e}"
        );
        assert_eq!(dyn_.delivered_steps, 1);

        // THE VERDICTS ARE CLASSIFY'S. Iron's thresholds are exceeded: the ball shatters.
        assert_eq!(
            dyn_.ball_verdict(),
            Some("shattered"),
            "the event's energy density is past iron's catalogued thresholds"
        );
        let (solid, frac, melt, vap, dec) = dyn_.fate_counts(&mats);
        println!(
            "fate at the site: {solid} solid / {frac} fractured / {melt} molten / {vap} vapor / \
             {dec} decomposed · delivered {delivered:.3e} J"
        );
        assert!(
            solid < (site.particles.len() - site.fine_start) / 2,
            "most of the site's matter is past Intact - this is a moon landing on it"
        );
        assert!(vap + melt > 0, "at these densities some matter melts or vaporizes");

        // Scatter is emergent: a few seconds of site time and the ball is no longer a ball.
        let ball_i0 = site.particles.len() - site.ball_children;
        let spread = |site: &MaterializedSite| {
            let ps = &site.particles[ball_i0..];
            let com = ps.iter().fold(Vec3::ZERO, |a, p| a + Vec3::from(p.pos)) / ps.len() as f32;
            ps.iter().map(|p| (Vec3::from(p.pos) - com).length()).fold(0.0f32, f32::max)
        };
        let s0 = spread(&site);
        let mut t2 = 0.0;
        while t2 < 3.0 {
            t2 += dyn_.step(&mut site, 1.0 / 60.0, &mats);
        }
        assert!(
            spread(&site) > 2.0 * s0,
            "the shattered parcels scatter ({s0:.2} m -> {:.2} m)",
            spread(&site)
        );

        // Matter is conserved through all of it: the door deposits state, never mass.
        let audit = refine::audit(&site.particles);
        assert!(
            (audit.mass - site.declared_mass_kg).abs() <= site.declared_mass_kg * 1.0e-6,
            "the event changes fate, not the amount of matter"
        );

        // And a mid-event site honestly refuses to fold: it is anything but settled.
        let g = spec.g_ms2 as f32;
        let mut gauge = SettleGauge::new();
        gauge.observe(site_peak_speed(&site), g, 0.1);
        assert!(
            matches!(fold_site(&site, &gauge, g), Err(SiteRefusal::NotSettled { .. })),
            "a shattered, flying site stays resolved"
        );
    }

    /// **The release gate still stands between an unreleased site and dynamics.** A patch that
    /// carries the exact split with a stated residual (the rung refused) may render and fold,
    /// but it may NOT step: `SiteDynamics::new` refuses with the rung's own numbers.
    #[test]
    fn an_unreleased_site_is_refused_dynamics_by_the_standing_gate() {
        let spec = SiteSpec::from_ground_world(&shipped_ground_world()).expect("spec");
        let mats = materials::load();
        let mut site = materialize_site(&spec, &HandDown::Declared, &mats).expect("materializes");
        site.release = SiteRelease::Unreleased { achieved: 4.8e-2, bound: 5.0e-3, iterations: 400 };
        match SiteDynamics::new(&site, &spec, &mats) {
            Err(SiteRefusal::Refine(Refusal::NotConverged { achieved, .. })) => {
                assert_eq!(achieved, 4.8e-2, "the gate quotes the rung's own residual");
            }
            other => panic!("an unreleased site must not enter dynamics, got {:?}", other.is_ok()),
        }
    }

    /// **The fold obeys the docs/61 criterion and conserves.** An unsettled site refuses (stays
    /// resolved, reason stated); after one sustained t_q of quiet the fold releases and the
    /// audit matches the declared mass.
    #[test]
    fn an_unsettled_site_refuses_to_fold_and_a_settled_one_folds_conserving() {
        let spec = SiteSpec::from_ground_world(&shipped_ground_world()).expect("spec");
        let mats = materials::load();
        let site = materialize_site(&spec, &HandDown::Declared, &mats).expect("materializes");
        let g = spec.g_ms2 as f32;

        // Fresh gauge: no sustained quiet yet - the fold refuses, honestly.
        let mut gauge = SettleGauge::new();
        match fold_site(&site, &gauge, g) {
            Err(r @ SiteRefusal::NotSettled { .. }) => {
                let msg = format!("{r}");
                assert!(msg.contains("stays resolved"), "the reason must be stated: {msg}");
            }
            other => panic!("an unsettled site must refuse to fold, got {other:?}"),
        }

        // The un-stepped site's field is at rest, so feeding the gauge its real peak speed
        // settles it after one t_q of observed quiet (the dynamic case is its own test).
        let peak = site_peak_speed(&site);
        assert_eq!(peak, 0.0, "the un-stepped site is at rest");
        for _ in 0..200 {
            gauge.observe(peak, g, 0.01);
        }
        let rep = fold_site(&site, &gauge, g).expect("a settled site folds");
        assert_eq!(rep.folded, site.particles.len());
        assert!(
            (rep.audit.mass - rep.declared_mass_kg).abs() <= rep.declared_mass_kg * 1.0e-6,
            "the fold returns exactly the declared mass: {:.6e} vs {:.6e} (drift {:.3e})",
            rep.audit.mass,
            rep.declared_mass_kg,
            rep.mass_drift_kg
        );
        assert!((rep.mass_drift_kg - (rep.audit.mass - rep.declared_mass_kg)).abs() < 1.0e-9);
    }
}
