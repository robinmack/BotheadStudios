//! The voxel matter store and the Phase 1 layered-world generator.
//!
//! Each voxel holds a material index (0 = empty/air, else `material_index + 1`). This is the
//! authoritative "matter store" — later phases attach per-voxel density = material.density (so
//! summed mass drives gravity) and activate voxels into MPM particles under stress. The generator
//! lays a surface patch of the REAL layered Earth — grass skin, basalt crust, peridotite mantle, iron
//! core — as a declared vertical LOD (real materials/order, compressed depths; docs/25/28).

use crate::materials::{index_of, Material};
use glam::{IVec3, Vec3};
use std::collections::VecDeque;

/// Width (X), height (Y, up), depth (Z) of the world in voxels. 1 voxel = 1 metre.
pub const W: usize = 96;
pub const H: usize = 96;
pub const D: usize = 96;

const GRASS_THICKNESS: usize = 1; // thin fragile biosphere skin over the crust

/// Highest possible surface top (voxel-y ≈ metres): leaves 8 voxels of headroom above the terrain.
const BASE_TOP: f32 = (H - 8) as f32;
/// Peak-to-valley relief of the procedural heightfield (voxels ≈ metres): real rolling hills, not a slab.
const AMPLITUDE: f32 = 34.0;

/// Sea-level datum (voxel-y, metres in the patch's own frame): the reference height the oceans fill
/// water BELOW (real `water` matter above the seabed, up to this level; land is ABOVE it). This is the
/// waterline where the ONE continuous hydrostatic column switches from air (pressure decreasing upward,
/// the declared atmosphere) to water (pressure increasing downward, [`ocean_pressure`]) — exactly
/// parallel to the atmosphere, P = P_atm at depth 0.
///
/// DEMONSTRATION SEA LEVEL (flagged): the coarse 10° landmask calls this cell all-land, so there is no
/// real bathymetry to pin the datum yet. It is chosen to sit WITHIN the terrain's relief band
/// (`BASE_TOP - AMPLITUDE` = 54 .. `BASE_TOP` = 88, procedural tops ~54..81) so the low basins are
/// genuinely submerged and the sea is visible, while hills stay dry. When real elevation/bathymetry
/// (ETOPO) drops into `terrain_height`, the true 0 m geoid replaces this demonstration value.
pub const SEA_LEVEL_Y: f32 = 64.0;

/// Hydrostatic pressure (Pa) in the ocean at `depth_below_sea` metres beneath [`SEA_LEVEL_Y`], for a
/// surface gravity `g` (m/s²). This is the SAME hydrostatic law the air column obeys, continued
/// DOWNWARD through the water: `P(depth) = P_atm + ρ_water · g · depth`. It is continuous with the
/// atmosphere at the waterline — at `depth = 0` it returns exactly `P_atm`, the declared atmosphere's
/// weight (`planet::earth().surface_pressure()`) — so air-above and water-below are ONE column, not two.
///
/// DERIVED, not dialed: `ρ_water` is the DB `water` material's density and `P_atm` is the emergent
/// surface pressure of the declared atmosphere. Above the waterline (`depth_below_sea < 0`) there is no
/// water, so it clamps to `P_atm` (the atmosphere takes over there). Approximation (flagged): constant
/// ρ (water's ~0.05%/km compressibility is ignored) and constant g over the shallow column.
pub fn ocean_pressure(depth_below_sea: f64, g: f64) -> f64 {
    let p_atm = crate::planet::earth().surface_pressure();
    let mats = crate::materials::load();
    let rho_water = mats[index_of(&mats, "water")].density as f64;
    p_atm + rho_water * g * depth_below_sea.max(0.0)
}

pub struct World {
    pub w: usize,
    pub h: usize,
    pub d: usize,
    /// `voxels[idx] == 0` is air; otherwise the material index is `voxels[idx] - 1`.
    pub voxels: Vec<u16>,
    /// Tallest column, for centering the camera on the terrain.
    pub max_top: usize,
    /// Material index treated as LIQUID water (the sea), if the world has one. Water voxels are real
    /// matter (they carry mass — see gravity — and render), but they are NOT load-bearing SOLID: they
    /// are excluded from [`Self::is_solid`], so the structural-support model, the camera/land-surface
    /// queries, and the terrain strata all continue to mean the solid ground beneath the sea. `None`
    /// for hand-built worlds (tests) and any world without an ocean.
    pub water_mat: Option<usize>,
    /// **T0 — the persistent bulk field** (`w × d`, metres, added to the procedural relief).
    ///
    /// [`terrain_height`] is a PURE FUNCTION of position — procedural fbm, no state. That made T0
    /// unwritable, which is why de-resolution had nowhere to put its result and `patch_resolved` could
    /// only ever go one way: dig once and the whole 96 m patch stays voxels for the session
    /// (docs/46 ledger item 6). docs/39 requires the opposite — *"T0 is a renderable persistent field —
    /// bake-back writes real displacement/normals; a crater stays a crater."*
    ///
    /// This raster is that field: `bulk_height = terrain_height + displacement`. A crater baked back
    /// here PERSISTS in the cheap representation after its voxels are freed, so the ground a body stands
    /// on is the same ground whether or not that region is currently resolved.
    ///
    /// Zero-initialised, so an untouched world is exactly the procedural surface as before.
    pub displacement: Vec<f32>,
    /// **Which columns keep their surface in T0 rather than in voxels** (`w × d`).
    ///
    /// Needed because "this column has no voxels" is ambiguous on its own, and the two cases want
    /// opposite answers: a column DEMOTED by [`Self::demote_column_to_field`] still has ground — its
    /// height moved into [`Self::displacement`] — while a column excavated to nothing genuinely has
    /// none. Without this flag, [`Self::ground_top_voxel`] would have to read a zero displacement as
    /// "pristine procedural relief" and pop dug-out ground back up to the untouched surface.
    ///
    /// Set only by demotion, cleared by any [`Self::set_voxel`] that puts matter back in the column
    /// (re-resolving hands authority back to the voxels). Untouched worlds are all-`false`.
    pub demoted: Vec<bool>,
    /// **Cached surface top per column** (`w × d`), `-1` where [`Self::surface_top_voxel`] would say
    /// `None`. Pure acceleration — it holds exactly what the old top-down scan returned.
    ///
    /// That scan was O(height) and walked every air voxel above the surface on EVERY call. Measured on
    /// the terrain scene (CDP profile, 2026-07-21): `surface_top_voxel` was **16.7% of frame time**, the
    /// single largest cost, because the probe's terrain contact queries it per particle per frame and
    /// `surface_bilinear_grad` asks for FOUR columns per query.
    ///
    /// **Invalidation is by recompute, not by reasoning.** Every [`Self::set_voxel`] rescans that one
    /// column. Writes are rare (a dig, a deposit); reads are per-particle per-frame. This is deliberately
    /// the dumb version: incremental "the top only moves if you wrote above/at it" logic has to get water
    /// (excluded from `is_solid`), demotion and mid-column removal all right, and a wrong cached top is a
    /// silent physics error — bodies resting at the wrong height. `tops_match_a_fresh_scan` pins it.
    tops: Vec<i32>,
}

impl World {
    /// Assemble a world from an already-built voxel array, with T0 flat (an unmodified procedural
    /// surface). Exists so that adding a field to `World` — as the T0 `displacement` raster was — does
    /// not break every construction site; prefer this over a struct literal.
    pub fn from_voxels(
        w: usize,
        h: usize,
        d: usize,
        voxels: Vec<u16>,
        max_top: usize,
        water_mat: Option<usize>,
    ) -> Self {
        let mut world = World { w, h, d, voxels, max_top, water_mat,
            displacement: vec![0.0; w * d], demoted: vec![false; w * d], tops: vec![-1; w * d] };
        world.rebuild_tops();
        world
    }

    #[inline]
    pub fn idx(&self, x: usize, y: usize, z: usize) -> usize {
        (y * self.d + z) * self.w + x
    }

    /// Material index at a voxel, or `None` for air / out of bounds.
    #[inline]
    pub fn material_at(&self, x: i32, y: i32, z: i32) -> Option<usize> {
        if x < 0
            || y < 0
            || z < 0
            || x as usize >= self.w
            || y as usize >= self.h
            || z as usize >= self.d
        {
            return None;
        }
        let v = self.voxels[self.idx(x as usize, y as usize, z as usize)];
        if v == 0 {
            None
        } else {
            Some((v - 1) as usize)
        }
    }

    /// Is this voxel LIQUID water (the sea)? Water is matter but not solid ground.
    #[inline]
    pub fn is_water(&self, x: i32, y: i32, z: i32) -> bool {
        self.water_mat.is_some() && self.material_at(x, y, z) == self.water_mat
    }

    /// Is this voxel SOLID (load-bearing) ground — matter that is NOT liquid water? The structural
    /// support model, the land-surface/camera queries, and the strata all key off this, so the sea does
    /// not masquerade as ground. Use [`Self::material_at`] (`is_some()`) when you want "any matter
    /// present" (e.g. gravity mass, rendering), which INCLUDES the water.
    #[inline]
    pub fn is_solid(&self, x: i32, y: i32, z: i32) -> bool {
        match self.material_at(x, y, z) {
            Some(m) => Some(m) != self.water_mat,
            None => false,
        }
    }

    /// The offset used to center the world on the origin (shared by the mesher, gravity, and
    /// physics so geometry and forces live in the same coordinate frame).
    pub fn center(&self) -> Vec3 {
        Vec3::new(
            self.w as f32 * 0.5,
            self.max_top as f32 * 0.5,
            self.d as f32 * 0.5,
        )
    }

    /// The Y (in voxel units) where air begins above column `(x, z)` — i.e. the surface top.
    /// `None` if the column is empty or out of bounds.
    ///
    /// O(1): reads the [`Self::tops`] cache, which every write keeps equal to [`Self::scan_top`].
    pub fn surface_top_voxel(&self, x: i32, z: i32) -> Option<i32> {
        if x < 0 || z < 0 || x as usize >= self.w || z as usize >= self.d {
            return None;
        }
        match self.tops[z as usize * self.w + x as usize] {
            t if t >= 0 => Some(t),
            _ => None,
        }
    }

    /// The authoritative top-down scan — the ONE definition of "where does this column's surface sit".
    /// The cache stores its result; nothing else may compute a column top independently.
    fn scan_top(&self, x: i32, z: i32) -> i32 {
        for y in (0..self.h as i32).rev() {
            if self.is_solid(x, y, z) {
                return y + 1;
            }
        }
        -1
    }

    /// Recompute ONE column's cached top. Called by every write that can move a surface.
    fn recompute_top(&mut self, x: i32, z: i32) {
        if x < 0 || z < 0 || x as usize >= self.w || z as usize >= self.d {
            return;
        }
        self.tops[z as usize * self.w + x as usize] = self.scan_top(x, z);
    }

    /// Fill the whole cache. Constructors only — O(w·d·h).
    fn rebuild_tops(&mut self) {
        for z in 0..self.d as i32 {
            for x in 0..self.w as i32 {
                let t = self.scan_top(x, z);
                self.tops[z as usize * self.w + x as usize] = t;
            }
        }
    }

    /// The smooth (bilinear) terrain surface height at a world position, returned in CENTERED coords —
    /// the SAME surface the GPU debris step collides grains against (`particle_step.wgsl::terrain_h`):
    /// the four surrounding column tops (edge-clamped to the patch, `-1` for an empty column, then
    /// `-0.5` for the mesh iso-surface) bilinearly interpolated. Debris comes to rest ON this bilinear
    /// surface, NOT on a single column's top. The CPU de-resolution readback must judge "grounded"
    /// against this SAME surface — otherwise a grain resting on a SLOPE (binned into the lower column of
    /// its cell, but physically held up by the higher corner) reads as airborne against the single lower
    /// column top and never de-resolves, so the pile stacked on it can never peel down to voxels either
    /// (the "rubble that never returns to the grid" stall). Mirrors the shader by construction.
    pub fn surface_height_bilinear(&self, pos: Vec3) -> f32 {
        self.surface_bilinear_grad(pos).0
    }

    /// The bilinear surface height AND its horizontal gradient `(h, ∂h/∂x, ∂h/∂z)` at `pos`.
    ///
    /// Same field as [`Self::surface_height_bilinear`] (which now delegates here, so there is exactly ONE
    /// bilinear implementation on the CPU — the codebase's forked-path failure mode is what we are
    /// avoiding). Mirrors `particle_step.wgsl::terrain_surface` by construction, including its
    /// `mix(h10-h00, h11-h01, fz)` gradient form.
    ///
    /// The gradient is what makes an honest contact possible: the outward surface normal is
    /// `(−∂h/∂x, 1, −∂h/∂z)` normalised, which is continuous across voxel edges (it never flips), so a
    /// body on a SLOPE gets a real normal to resolve against instead of being treated as sitting on a
    /// flat floor. Without it there is no normal load to bound Coulomb friction with — i.e. no traction.
    pub fn surface_bilinear_grad(&self, pos: Vec3) -> (f32, f32, f32) {
        let c = self.center();
        let vx = pos.x + c.x;
        let vz = pos.z + c.z;
        let cx = vx.floor() as i32;
        let cz = vz.floor() as i32;
        let top = |x: i32, z: i32| -> f32 {
            let xc = x.clamp(0, self.w as i32 - 1);
            let zc = z.clamp(0, self.d as i32 - 1);
            // `ground_top_voxel`, not `surface_top_voxel`: a demoted column still HAS ground, and
            // reading the freed voxels would report -1 and open a hole under anything resting there.
            self.ground_top_voxel(xc, zc).unwrap_or(-1) as f32
        };
        let h00 = top(cx, cz);
        let h10 = top(cx + 1, cz);
        let h01 = top(cx, cz + 1);
        let h11 = top(cx + 1, cz + 1);
        let fx = vx - cx as f32;
        let fz = vz - cz as f32;
        let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
        let h = lerp(lerp(h00, h10, fx), lerp(h01, h11, fx), fz);
        // Cell spacing is 1 voxel, so the finite difference IS the derivative (no division).
        let dhdx = lerp(h10 - h00, h11 - h01, fz);
        let dhdz = lerp(h01 - h00, h11 - h10, fx);
        (h - c.y - 0.5, dhdx, dhdz)
    }

    /// The BULK terrain surface height (centered coords) at horizontal `(x, z)` — the DEFAULT ground
    /// everywhere, resolved or not (Robin: "the default terrain is the bulk heightmap everywhere"). It is
    /// the shared continuous [`terrain_height`] (the SAME field the distant Earth cap and the resolved
    /// voxels sample), converted from the patch's voxel frame into centered coords. Unlike
    /// [`Self::surface_top_voxel`] it is defined over the WHOLE plane — off the finite voxel footprint the
    /// bulk ground continues, so a probe or grain out there rests on real terrain instead of falling into
    /// the void. Smooth (no voxel rounding); the on-demand voxels only refine it locally around an impact.
    pub fn bulk_height(&self, x: f32, z: f32) -> f32 {
        let c = self.center();
        let (vx, vz) = (x + c.x, z + c.z);
        terrain_height(vx, vz) + self.displacement_at(vx, vz) - c.y
    }

    /// The T0 displacement at a VOXEL-frame position, bilinearly sampled so the baked field is as smooth
    /// as the procedural relief it adds to (a nearest-neighbour lookup would put a 1 m step at every cell
    /// edge and the contact normal would flip across it). Zero outside the patch: the bulk continues
    /// unmodified beyond the finite footprint.
    pub fn displacement_at(&self, vx: f32, vz: f32) -> f32 {
        if self.displacement.is_empty() {
            return 0.0;
        }
        // Samples live at CELL CENTRES: entry (x,z) is the displacement at (x+0.5, z+0.5). Shift by half
        // a cell before flooring so a lookup exactly at a cell centre returns that cell's value alone.
        // Without this, sampling the centre of a freshly-baked column blends it 50/50 with a neighbour
        // still at zero, and the ground drops by half the bake — a silent terrain shift.
        let (sx, sz) = (vx - 0.5, vz - 0.5);
        let (cx, cz) = (sx.floor() as i32, sz.floor() as i32);
        let at = |x: i32, z: i32| -> f32 {
            if x < 0 || z < 0 || x as usize >= self.w || z as usize >= self.d {
                return 0.0;
            }
            self.displacement[z as usize * self.w + x as usize]
        };
        let (fx, fz) = (sx - cx as f32, sz - cz as f32);
        let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
        lerp(
            lerp(at(cx, cz), at(cx + 1, cz), fx),
            lerp(at(cx, cz + 1), at(cx + 1, cz + 1), fx),
            fz,
        )
    }

    /// **THE column top — the single authoritative answer to "where is the ground here?"**
    ///
    /// Returns the same voxel-frame top [`Self::surface_top_voxel`] does while a column is resolved, and
    /// keeps returning it after that column has been demoted to T0, when the voxels are gone and only
    /// the field remembers. `None` means there is genuinely no ground (an unbaked empty column, or out
    /// of bounds).
    ///
    /// **Why this can be exact, and why no f32 heightfield is needed to make demotion safe:**
    /// [`Self::demote_column_to_field`] preserves the surface *exactly*, and the surface it preserves is
    /// already voxel-quantised (`top − 0.5`). So the field can hand back the identical integer top it
    /// was given — demotion becomes invisible to every consumer that asks this question, with no change
    /// to the `array<i32>` the GPU grain step samples. (Sub-voxel terrain is a separate, deferred piece
    /// of work — `docs/45`'s `SLOPE_QUANTUM_M` IOU — and it is deliberately not entangled with this.)
    ///
    /// Consumers must prefer this over `surface_top_voxel`; reading the voxels directly is what makes a
    /// demoted column read as a hole and drops whatever was standing on it through the floor.
    pub fn ground_top_voxel(&self, x: i32, z: i32) -> Option<i32> {
        if x < 0 || z < 0 || x as usize >= self.w || z as usize >= self.d {
            return None;
        }
        if self.demoted[z as usize * self.w + x as usize] {
            // Invert the bake: field surface = procedural + displacement = top − 0.5.
            let surface = terrain_height(x as f32 + 0.5, z as f32 + 0.5)
                + self.displacement[z as usize * self.w + x as usize];
            return Some((surface + 0.5).round() as i32);
        }
        self.surface_top_voxel(x, z)
    }

    /// **Demote a column from voxels (T1) to the persistent field (T0).**
    ///
    /// Writes the column's current surface into `displacement` so `bulk_height` reproduces it EXACTLY,
    /// then clears the voxels. The crater stays a crater; the compute does not.
    ///
    /// Returns the number of voxels freed, or `None` if the column is not bakeable (see
    /// [`Self::column_is_bakeable`]) — a caller must not treat a refusal as "nothing to do", because the
    /// column is still resolved and still costing.
    ///
    /// THE INVARIANT: the surface must not move. `displacement` is set to exactly the difference between
    /// the voxel surface and the procedural relief, so a body resting on this ground before demotion
    /// rests at the same height after it. Demotion is a change of REPRESENTATION, never of state — the
    /// same discipline as `deposit_resting_grain`, which never deletes matter to lower a count.
    pub fn demote_column_to_field(&mut self, x: i32, z: i32) -> Option<usize> {
        if !self.column_is_bakeable(x, z) {
            return None;
        }
        if x < 0 || z < 0 || x as usize >= self.w || z as usize >= self.d {
            return None;
        }
        let top = self.surface_top_voxel(x, z).unwrap_or(0);
        // The surface the voxels present (the same `-0.5` iso the mesher and the contact use).
        let voxel_surface = top as f32 - 0.5;
        // What the procedural relief alone would say here. The residual IS the displacement.
        let procedural = terrain_height(x as f32 + 0.5, z as f32 + 0.5);
        self.displacement[z as usize * self.w + x as usize] = voxel_surface - procedural;
        // The field is now the record for this column: `ground_top_voxel` must read it, not the voxels
        // we are about to free. Set BEFORE clearing so the column is never momentarily groundless.
        self.demoted[z as usize * self.w + x as usize] = true;
        let mut freed = 0usize;
        for y in 0..top {
            if self.material_at(x, y, z).is_some() {
                self.set_voxel(x, y, z, None);
                freed += 1;
            }
        }
        Some(freed)
    }

    /// **Demote the WHOLE patch to T0 — all columns or none.** Returns the voxels freed, or `None` if
    /// any column refuses (see [`Self::column_is_bakeable`]), having changed nothing.
    ///
    /// All-or-nothing is not timidity, it is what the current representation can express. `patch_resolved`
    /// is ONE bool for the entire 96 m footprint — it decides whether the voxel mesh is drawn at all and
    /// whether the bulk cap leaves a hole — so a half-demoted patch has no consistent rendering: the
    /// demoted columns produce no surface-nets geometry while the cap still holds its hole open over
    /// them, and you would see straight through the ground. Demoting atomically keeps the one flag
    /// meaningful (`docs/47` §6).
    ///
    /// **The honest cost, and it is real:** a single unbakeable column — one cave, one undercut crater
    /// lip — pins the entire patch resolved. Per-column demotion needs `patch_resolved` to become a
    /// per-column mask and the cap's hole to follow it; that is the next increment, not this one.
    pub fn demote_patch_to_field(&mut self) -> Option<usize> {
        for z in 0..self.d as i32 {
            for x in 0..self.w as i32 {
                if !self.column_is_bakeable(x, z) {
                    return None; // checked BEFORE mutating: a refusal must leave the patch untouched
                }
            }
        }
        let mut freed = 0usize;
        for z in 0..self.d as i32 {
            for x in 0..self.w as i32 {
                freed += self.demote_column_to_field(x, z).unwrap_or(0);
            }
        }
        Some(freed)
    }

    /// Can this column be DEMOTED to T0 — i.e. can the cheap heightfield represent its state?
    ///
    /// The mirror of docs/44's promotion test. Promotion asks "does the cheap model provably DIFFER from
    /// the honest one?"; demotion asks "can the cheap model represent this WITHIN the bound?" A single
    /// surface per column can be baked exactly. A column with a void beneath its top — a cave, an
    /// overhang, an undercut crater lip — CANNOT: a heightfield has one height per column, so collapsing
    /// it would silently delete the void and the matter bounding it. Those columns stay resolved, which
    /// is the honest answer rather than a lossy one.
    pub fn column_is_bakeable(&self, x: i32, z: i32) -> bool {
        let Some(top) = self.surface_top_voxel(x, z) else {
            return true; // empty column: nothing to bake, trivially representable
        };
        // WATER refuses for the same reason a void does: the field stores ONE height per column, and a
        // sea column has two surfaces — the seabed and the waterline. `surface_top_voxel` deliberately
        // means SOLID ground (water is matter but not load-bearing), so baking such a column would record
        // the seabed, free the ground beneath, and leave the sea sitting on nothing.
        if let Some(wm) = self.water_mat {
            if (0..self.h as i32).any(|y| self.material_at(x, y, z) == Some(wm)) {
                return false;
            }
        }
        // Solid from the base up to `top` with no gaps ⇒ one surface ⇒ a heightfield can hold it.
        (0..top).all(|y| self.is_solid(x, y, z))
    }

    /// Is a camera `eye` (in CENTERED coordinates, the frame `center()` maps to voxel space) in free
    /// air, at least `clearance` metres above the ground beneath it? "Free" means the voxel the eye sits
    /// in is not solid AND, where a ground column exists under the eye, the eye is `clearance` above that
    /// column's surface top. Off the terrain footprint (out-of-bounds column) there is no ground beneath,
    /// so the eye is free — the patch is a finite chunk of matter in vacuum, not walled in.
    pub fn eye_is_free(&self, eye: Vec3, clearance: f32) -> bool {
        let p = eye + self.center();
        let (xi, yi, zi) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        if self.is_solid(xi, yi, zi) {
            return false;
        }
        match self.surface_top_voxel(xi, zi) {
            Some(top) => p.y >= top as f32 + clearance,
            None => true, // no ground column here (beside/beyond the patch)
        }
    }

    /// Clamp a third-person orbit camera eye (CENTERED coords) so it never sits inside solid matter or
    /// below the terrain surface. If the requested eye is already free air a `clearance` above the
    /// ground, it is returned UNCHANGED (normal orbit/zoom stays smooth). Otherwise the eye is pulled
    /// back along its own radial direction (a third-person "boom", preserving yaw/pitch) in half-voxel
    /// steps until it reaches free air — the honest fix for zoom-into-the-ground. For a degenerate
    /// near-straight-down aim (little horizontal spread, or a radial pull-back that can only burrow
    /// deeper), it falls back to lifting the eye straight up until it clears the surface.
    pub fn clamp_eye_outside(&self, eye: Vec3, clearance: f32) -> Vec3 {
        if self.eye_is_free(eye, clearance) {
            return eye;
        }
        let dir = eye.normalize_or_zero();
        let horiz = (dir.x * dir.x + dir.z * dir.z).sqrt();
        // World bounding-sphere radius (centered): any point past it is guaranteed outside the matter.
        let bound = ((self.w * self.w + self.h * self.h + self.d * self.d) as f32).sqrt()
            + clearance
            + 4.0;

        // Radial boom pull-back: only meaningful when the ray has real horizontal spread AND is not
        // aimed down into the ground (a down-pointing ray only burrows deeper as it lengthens).
        if dir != Vec3::ZERO && horiz >= 0.1 && dir.y >= -0.05 {
            let mut d = eye.length();
            while d <= bound {
                let e = dir * d;
                if self.eye_is_free(e, clearance) {
                    return e;
                }
                d += 0.5;
            }
        }

        // Vertical-lift fallback: raise the eye straight up until it clears the surface above it.
        let mut e = eye;
        let ceiling = self.h as f32 - self.center().y + clearance + 1.0;
        while e.y <= ceiling {
            if self.eye_is_free(e, clearance) {
                return e;
            }
            e.y += 0.5;
        }
        e
    }

    /// Clamp a terrain camera eye (CENTERED coords) so it can NEVER go below the real Earth surface —
    /// anywhere, not just over the resolved 96 m voxel patch. The bulk planet is a sphere of `radius`
    /// centred at `earth_center` (in the same centered frame; ≈ a full Earth radius straight down under
    /// the uniform surface gravity). Two constraints, both enforced:
    ///   1. The eye stays at least `clearance` ABOVE the Earth sphere: its distance from `earth_center`
    ///      is ≥ `radius + clearance`. If it dips inside, it is pushed radially back out from the centre
    ///      (so off the patch, over the summarized bulk surface, the horizon still walls the eye in —
    ///      closing the old "off-footprint is free" hole that only the fake flat plane made safe).
    ///   2. The eye stays out of the resolved voxel hills (the existing local clamp), which can rise
    ///      ABOVE the smooth sphere where the patch relief pokes up.
    /// Computed in f64: `radius` is ~6.4e6 m, so the metre-scale curvature drop is below f32 precision.
    pub fn clamp_eye_above_earth(
        &self,
        eye: Vec3,
        earth_center: Vec3,
        radius: f32,
        clearance: f32,
    ) -> Vec3 {
        let c = earth_center.as_dvec3();
        let rel = eye.as_dvec3() - c;
        let d = rel.length();
        let min_d = radius as f64 + clearance as f64;
        let lifted = if d > 1e-9 && d < min_d {
            (c + rel * (min_d / d)).as_vec3()
        } else {
            eye
        };
        // Then keep it clear of the resolved voxel patch (lifting only ever increases the distance from
        // the Earth centre, so the sphere constraint from above still holds).
        self.clamp_eye_outside(lifted, clearance)
    }

    /// Set a voxel's material (`None` = air). Out-of-bounds writes are ignored.
    pub fn set_voxel(&mut self, x: i32, y: i32, z: i32, material: Option<usize>) {
        if x < 0
            || y < 0
            || z < 0
            || x as usize >= self.w
            || y as usize >= self.h
            || z as usize >= self.d
        {
            return;
        }
        let i = self.idx(x as usize, y as usize, z as usize);
        self.voxels[i] = material.map(|m| m as u16 + 1).unwrap_or(0);
        // Keep the column-top cache exact by RECOMPUTING this column, not by reasoning about which way
        // the surface moved. Writes are rare; reads are per-particle per-frame.
        self.recompute_top(x, z);
        // Putting matter back into a demoted column hands authority back to the voxels. Without this the
        // column keeps answering `ground_top_voxel` from its stale baked height and the new matter is
        // invisible to everything that asks where the ground is. (Demotion re-sets the flag itself, and
        // does so before it clears, so its own clearing pass cannot trip this.)
        if material.is_some() {
            self.demoted[z as usize * self.w + x as usize] = false;
        }
    }

    /// Total number of solid voxels — used for matter-conservation checks (tests).
    #[allow(dead_code)]
    pub fn solid_count(&self) -> usize {
        self.voxels.iter().filter(|&&v| v != 0).count()
    }

    /// March a ray (given in centered coordinates) through the grid; return the first solid voxel it
    /// hits and the centered hit position. Amanatides–Woo DDA — used for click-to-dig picking.
    pub fn raycast(&self, origin: Vec3, dir: Vec3, max_dist: f32) -> Option<(i32, i32, i32, Vec3)> {
        let d = dir.normalize_or_zero();
        if d == Vec3::ZERO {
            return None;
        }
        let o = origin + self.center(); // ray origin in voxel space

        let mut v = IVec3::new(o.x.floor() as i32, o.y.floor() as i32, o.z.floor() as i32);
        let step = IVec3::new(sign(d.x), sign(d.y), sign(d.z));

        // Parametric distance to the first voxel boundary on each axis, and per-voxel increments.
        let t_max = |oc: f32, dc: f32, s: i32| -> f32 {
            if dc == 0.0 {
                f32::INFINITY
            } else if s > 0 {
                (oc.floor() + 1.0 - oc) / dc
            } else {
                (oc.floor() - oc) / dc
            }
        };
        let mut tmx = t_max(o.x, d.x, step.x);
        let mut tmy = t_max(o.y, d.y, step.y);
        let mut tmz = t_max(o.z, d.z, step.z);
        let tdx = if d.x != 0.0 {
            (1.0 / d.x).abs()
        } else {
            f32::INFINITY
        };
        let tdy = if d.y != 0.0 {
            (1.0 / d.y).abs()
        } else {
            f32::INFINITY
        };
        let tdz = if d.z != 0.0 {
            (1.0 / d.z).abs()
        } else {
            f32::INFINITY
        };

        let mut t = 0.0f32;
        for _ in 0..8192 {
            if self.is_solid(v.x, v.y, v.z) {
                return Some((v.x, v.y, v.z, origin + d * t));
            }
            if tmx <= tmy && tmx <= tmz {
                v.x += step.x;
                t = tmx;
                tmx += tdx;
            } else if tmy <= tmz {
                v.y += step.y;
                t = tmy;
                tmy += tdy;
            } else {
                v.z += step.z;
                t = tmz;
                tmz += tdz;
            }
            if t > max_dist {
                break;
            }
        }
        None
    }

    /// Solid voxels **not** connected (6-connectivity, through solid) to the anchored base (the
    /// `y = 0` layer). These are unsupported and should collapse. A flood-fill from the base marks
    /// everything supported; the rest is returned. O(number of voxels).
    pub fn find_unsupported(&self) -> Vec<(i32, i32, i32)> {
        const NEIGHBORS: [(i32, i32, i32); 6] = [
            (1, 0, 0),
            (-1, 0, 0),
            (0, 1, 0),
            (0, -1, 0),
            (0, 0, 1),
            (0, 0, -1),
        ];
        let mut supported = vec![false; self.w * self.h * self.d];
        let mut stack: Vec<usize> = Vec::new();

        // Seed with every solid voxel in the base layer.
        for z in 0..self.d {
            for x in 0..self.w {
                if self.is_solid(x as i32, 0, z as i32) {
                    let i = self.idx(x, 0, z);
                    if !supported[i] {
                        supported[i] = true;
                        stack.push(i);
                    }
                }
            }
        }

        // Flood-fill through connected solid voxels.
        while let Some(i) = stack.pop() {
            let x = i % self.w;
            let rem = i / self.w;
            let z = rem % self.d;
            let y = rem / self.d;
            for (dx, dy, dz) in NEIGHBORS {
                let (nx, ny, nz) = (x as i32 + dx, y as i32 + dy, z as i32 + dz);
                if self.is_solid(nx, ny, nz) {
                    let j = self.idx(nx as usize, ny as usize, nz as usize);
                    if !supported[j] {
                        supported[j] = true;
                        stack.push(j);
                    }
                }
            }
        }

        // Collect solid voxels the fill never reached.
        let mut out = Vec::new();
        for y in 0..self.h {
            for z in 0..self.d {
                for x in 0..self.w {
                    if self.is_solid(x as i32, y as i32, z as i32) && !supported[self.idx(x, y, z)]
                    {
                        out.push((x as i32, y as i32, z as i32));
                    }
                }
            }
        }
        out
    }

    /// Solid voxels that gravity cannot hold up — the honest structural-support model (`docs/28`).
    ///
    /// The old [`Self::find_unsupported`] called a voxel "supported" if ANY 6-connected path of solid
    /// reached the base, so an overhanging crater lip attached SIDEWAYS to the rim counted as supported
    /// even with nothing beneath it. That is unphysical: matter is held against gravity by support FROM
    /// BELOW. Here support propagates from the base UPWARD, with a material-strength-limited cantilever:
    ///
    /// 1. **DIRECTLY SUPPORTED** — a column to the base: `(x, y, z)` is directly supported iff it is
    ///    solid AND (`y == 0` OR the voxel directly below `(x, y-1, z)` is directly supported). A single
    ///    bottom-up sweep resolves the whole column.
    /// 2. **BRACED** (cantilever) — a solid voxel that is NOT directly supported is still held if it lies
    ///    within its material's CANTILEVER REACH, laterally at its own `y`-level (through solid), of a
    ///    directly-supported voxel. The reach is DERIVED from the material, not tuned: a ~1-voxel-thick
    ///    cantilever beam of length `L` carrying its own weight develops a root bending stress that grows
    ///    as `σ ≈ ρ·g·L² / t` (Euler–Bernoulli beam under a uniform self-weight load). It fails when that
    ///    reaches the material's tensile strength `σ_t`, giving
    ///        `L_max ≈ sqrt(σ_t · t / (ρ · g))`     (`t` = voxel size = 1 m)
    ///    FLAGGED as a first-order STRUCTURAL APPROXIMATION — a declared model, derived from real material
    ///    properties (an order-1 geometric constant is dropped). With the real DB values it gives basalt
    ///    σ_t≈1.45e7 → L≈22 m (competent rock holds a real crater lip) and grass/soil σ_t≈1.5e4 → L≈1 m
    ///    (loose soil barely overhangs) — physically right. Lateral graph distance (unit steps through
    ///    solid) stands in for the beam's arc length.
    /// 3. **UNSUPPORTED** — solid AND neither directly supported nor braced. These collapse (see
    ///    [`crate::matter::MatterSim::collapse`]): an undercut overhang past its material's reach falls.
    ///
    /// `g` is the terrain's surface gravity (m/s²; the emergent ~9.88, the Engine's `surface_g`). This
    /// also subsumes pure disconnection — a floating chunk has no directly-supported voxel to brace from,
    /// so it is returned too. O(voxels).
    pub fn find_structurally_unsupported(&self, materials: &[Material], g: f32) -> Vec<(i32, i32, i32)> {
        let n = self.w * self.h * self.d;
        // Cantilever reach per material, in voxels (≈ metres): L_max = sqrt(σ_t · t / (ρ · g)), t = 1 m.
        let reach: Vec<f32> = materials
            .iter()
            .map(|m| {
                let rho = m.density.max(1.0);
                let gg = g.max(1.0e-6);
                (m.fracture_strength / (rho * gg)).max(0.0).sqrt()
            })
            .collect();

        // 1. Directly supported: bottom-up per-column sweep (a voxel stands on a directly-supported one).
        let mut directly = vec![false; n];
        for z in 0..self.d {
            for x in 0..self.w {
                for y in 0..self.h {
                    if !self.is_solid(x as i32, y as i32, z as i32) {
                        continue;
                    }
                    let i = self.idx(x, y, z);
                    directly[i] = y == 0 || directly[self.idx(x, y - 1, z)];
                }
            }
        }

        // 2. Braced: per y-level multi-source BFS from the directly-supported voxels, flooding laterally
        //    (4-connectivity in x,z) through solid. Each solid voxel's shortest lateral distance to a
        //    directly-supported voxel is its cantilever length; it is braced iff that ≤ its material reach.
        let mut dist = vec![-1i32; self.w * self.d]; // reused per level
        let mut queue: VecDeque<(usize, usize)> = VecDeque::new();
        let mut out = Vec::new();
        for y in 0..self.h {
            dist.iter_mut().for_each(|d| *d = -1);
            queue.clear();
            for z in 0..self.d {
                for x in 0..self.w {
                    if directly[self.idx(x, y, z)] {
                        dist[z * self.w + x] = 0;
                        queue.push_back((x, z));
                    }
                }
            }
            while let Some((x, z)) = queue.pop_front() {
                let d0 = dist[z * self.w + x];
                for (dx, dz) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
                    let (nx, nz) = (x as i32 + dx, z as i32 + dz);
                    if nx < 0 || nz < 0 || nx as usize >= self.w || nz as usize >= self.d {
                        continue;
                    }
                    let (nx, nz) = (nx as usize, nz as usize);
                    if dist[nz * self.w + nx] >= 0 {
                        continue; // already reached at an equal-or-shorter distance
                    }
                    if self.is_solid(nx as i32, y as i32, nz as i32) {
                        dist[nz * self.w + nx] = d0 + 1;
                        queue.push_back((nx, nz));
                    }
                }
            }
            for z in 0..self.d {
                for x in 0..self.w {
                    let i = self.idx(x, y, z);
                    if directly[i] {
                        continue; // supported from below already
                    }
                    // Only SOLID ground is subject to structural (cantilever) support. Liquid water is
                    // matter but not load-bearing — it is held by hydrostatic pressure, not by bracing —
                    // so it is never "collapsed" by this model (excluding it here stops the whole sea from
                    // being flagged unsupported and shattering into debris). Air is skipped too.
                    if !self.is_solid(x as i32, y as i32, z as i32) {
                        continue;
                    }
                    let mat = self
                        .material_at(x as i32, y as i32, z as i32)
                        .expect("is_solid ⇒ material present");
                    let d = dist[z * self.w + x];
                    let braced = d >= 0 && (d as f32) <= reach[mat];
                    if !braced {
                        out.push((x as i32, y as i32, z as i32));
                    }
                }
            }
        }
        out
    }
}

fn sign(x: f32) -> i32 {
    if x > 0.0 {
        1
    } else if x < 0.0 {
        -1
    } else {
        0
    }
}

/// Continuous surface elevation of the Earth at patch coordinates `(world_x, world_z)` — the SINGLE
/// heightfield that BOTH the resolved voxel patch and the distant curved cap sample, so they are ONE
/// surface (killing the old flat-cap-above-a-valley step where rubble read as hovering). Coordinates and
/// the returned height are in the patch's own frame (voxel units; 1 voxel = 1 m). It is deterministic and
/// seedless (multi-octave value noise) and defined over the WHOLE plane, so the cap can sample it
/// arbitrarily far out — near the patch it equals the patch surface; far out it is the same field, just
/// sampled coarsely by the cap's ring spacing (that coarse far sampling is OPTIMIZATION, not a fudge).
///
/// TODO(ETOPO): this is procedural relief only. A real Earth elevation map drops in HERE, behind this
/// exact interface — add the map's elevation at the (lat, lon) for `(world_x, world_z)` to (or in place
/// of) the procedural term. At the 96 m patch scale `planet::is_land`'s 10° cell is uniform, so
/// map-driven land/ocean contrast isn't visible locally until that finer dataset arrives (docs/28). Do
/// not fake continents in the meantime.
pub fn terrain_height(world_x: f32, world_z: f32) -> f32 {
    let n = fbm(world_x, world_z); // 0..1 procedural relief
    let top = BASE_TOP - AMPLITUDE * (1.0 - n);
    top.clamp(GRASS_THICKNESS as f32 + 1.0, (H - 1) as f32)
}

/// Generate the world as a surface patch of the REAL layered Earth (planet::earth()): a grass skin over
/// basalt crust, peridotite mantle, iron core — Earth's true radial column as a declared VERTICAL LOD
/// (material order real; layer thicknesses compressed into the patch so the strata are visible when a
/// dig or impact excavates). The grassy surface top follows [`terrain_height`] — the SAME continuous
/// heightfield the distant Earth cap samples — giving real rolling relief (hills and valleys) that joins
/// the cap without a step.
pub fn generate(materials: &[Material]) -> World {
    // Real Earth column (planet::earth(), docs/25/28): a biosphere skin over basalt CRUST, peridotite
    // MANTLE, iron CORE. This is a DECLARED VERTICAL LOD: the material order is Earth's real radial
    // structure, but the layer THICKNESSES are rebalanced into the ~88-voxel patch (real crust is 0.4%
    // of the radius — invisible at true scale), so a dig or a giant impact exposes honest strata from
    // this surface frame (Robin: "see Theia impact from this perspective"). Depths are compressed —
    // flagged; 1 voxel = 1 m holds only for the near-surface probe/dig physics.
    let grass = index_of(materials, "grass") as u16 + 1;
    let crust = index_of(materials, "basalt") as u16 + 1;
    let mantle = index_of(materials, "peridotite") as u16 + 1;
    let core = index_of(materials, "iron") as u16 + 1;
    let water_idx = index_of(materials, "water");
    let water = water_idx as u16 + 1;

    let mut voxels = vec![0u16; W * H * D];
    let base_top = BASE_TOP as i32; // highest possible surface; leaves headroom above the terrain
    let valley_floor = base_top - AMPLITUDE as i32; // the LOWEST any surface top can reach

    // Flat strata boundaries (real geology is horizontal), anchored BENEATH the deepest valley so every
    // column — hilltop or valley bottom — carries the full grass → crust → mantle → core column. The
    // grass skin follows the undulating terrain top; the crust/mantle/core boundaries are level planes,
    // so a dig anywhere hits the same deep layer at the same absolute depth.
    const CRUST_VOX: i32 = 12; // basalt crust band (LOD-inflated from ~25 km)
    const MANTLE_VOX: i32 = 22; // peridotite mantle band
    let crust_bottom = valley_floor - CRUST_VOX;
    let mantle_bottom = crust_bottom - MANTLE_VOX;

    let mut max_top = 0usize;
    for z in 0..D {
        for x in 0..W {
            // Fill up to the SHARED continuous heightfield (the same function the Earth cap samples).
            let top = (terrain_height(x as f32, z as f32).round() as i32)
                .clamp(GRASS_THICKNESS as i32 + 1, H as i32 - 1);
            let grass_start = top - GRASS_THICKNESS as i32;
            for y in 0..top {
                let v = if y >= grass_start {
                    grass
                } else if y >= crust_bottom {
                    crust
                } else if y >= mantle_bottom {
                    mantle
                } else {
                    core
                };
                let i = (y as usize * D + z) * W + x;
                voxels[i] = v;
            }
            max_top = max_top.max(top as usize);
        }
    }

    // OCEAN PASS — water as real matter (docs/28; the sea, parallel to the atmosphere). Fill every AIR
    // voxel that lies below the sea-level datum and ABOVE the solid land top with the DB `water`
    // material, so the terrain's below-sea-level basins become genuine water bodies (filled voxels that
    // carry mass and render), never a decorative plane. The solid strata beneath the seabed are left
    // untouched — water sits in the air space above the grass, up to SEA_LEVEL_Y. STATIC filled sea for
    // now: no flow/waves/splash yet (that dynamic step — water resolving into flowing particles when a
    // meteor/dig disturbs it — is deferred and must NOT be faked). The hydrostatic pressure of this
    // column is [`ocean_pressure`], continuous with the atmosphere at the waterline.
    let sea_level = SEA_LEVEL_Y.round() as i32;
    for z in 0..D {
        for x in 0..W {
            for y in 0..sea_level.min(H as i32) {
                let i = (y as usize * D + z) * W + x;
                if voxels[i] == 0 {
                    voxels[i] = water; // air below the datum, above the land → sea
                }
            }
        }
    }

    let mut world = World {
        w: W,
        h: H,
        d: D,
        voxels,
        max_top,
        water_mat: Some(water_idx),
        // T0 starts flat: a fresh world IS the procedural relief, unmodified. Every non-zero entry
        // hereafter is a real, persisted deformation baked back from voxels.
        displacement: vec![0.0; W * D],
        demoted: vec![false; W * D],
        tops: vec![-1; W * D],
    };
    world.rebuild_tops();
    world
}

// --- deterministic value noise (no RNG; stable across runs/clients) ---

fn hash2(x: i32, z: i32) -> f32 {
    let mut h = (x.wrapping_mul(374_761_393)).wrapping_add(z.wrapping_mul(668_265_263)) as u32;
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    ((h ^ (h >> 16)) & 0xffff) as f32 / 65535.0
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t) // smoothstep
}

/// Bilinearly-interpolated value noise at lattice frequency `freq`.
fn value_noise(x: f32, z: f32, freq: f32) -> f32 {
    let fx = x * freq;
    let fz = z * freq;
    let x0 = fx.floor() as i32;
    let z0 = fz.floor() as i32;
    let tx = smooth(fx - x0 as f32);
    let tz = smooth(fz - z0 as f32);
    let a = hash2(x0, z0);
    let b = hash2(x0 + 1, z0);
    let c = hash2(x0, z0 + 1);
    let d = hash2(x0 + 1, z0 + 1);
    let top = a + (b - a) * tx;
    let bot = c + (d - c) * tx;
    top + (bot - top) * tz
}

/// Three-octave fractal noise in 0..1 (deterministic; no RNG, stable across runs/clients).
///
/// A broad low-frequency octave carries the large rolling hills and valleys across the map, a mid band
/// shapes individual slopes, and a fine octave adds surface texture. The weights sum to 1.0 so the
/// result stays in 0..1; the low-frequency term is weighted heaviest to give genuine map-wide relief
/// (not a flat plateau) rather than only local bumps.
fn fbm(x: f32, z: f32) -> f32 {
    let n = 0.55 * value_noise(x, z, 0.026)
        + 0.30 * value_noise(x, z, 0.062)
        + 0.15 * value_noise(x, z, 0.13);
    n.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    /// The column-top CACHE must always equal what the authoritative top-down scan would return.
    ///
    /// `surface_top_voxel` went from an O(height) scan to an O(1) lookup because it was 16.7% of the
    /// terrain frame (CDP profile, 2026-07-21). That is only sound while the cache is exact, and a stale
    /// top is a SILENT physics error — bodies rest at the wrong height, grains de-resolve against the
    /// wrong ground — not a crash. So compare the whole grid against a fresh scan after every kind of
    /// mutation the world supports.
    #[test]
    fn tops_match_a_fresh_scan_after_every_kind_of_mutation() {
        let mats = crate::materials::load();
        let mut w = super::generate(&mats);
        let check = |w: &super::World, when: &str| {
            for z in 0..w.d as i32 {
                for x in 0..w.w as i32 {
                    let cached = w.surface_top_voxel(x, z).unwrap_or(-1);
                    let fresh = w.scan_top(x, z);
                    assert_eq!(cached, fresh, "column ({x},{z}) stale {when}: cached {cached}, real {fresh}");
                }
            }
        };
        check(&w, "on a fresh world");

        let (cx, cz) = (w.w as i32 / 2, w.d as i32 / 2);
        let top = w.surface_top_voxel(cx, cz).expect("solid column at centre");

        // Dig down through the surface: the top must fall.
        for y in (top - 5).max(0)..top {
            w.set_voxel(cx, y, cz, None);
        }
        check(&w, "after digging the surface away");
        assert!(w.surface_top_voxel(cx, cz).unwrap_or(-1) < top, "digging must lower the top");

        // Put matter back ABOVE the old surface: the top must rise.
        let rock = 0usize;
        w.set_voxel(cx, top + 3, cz, Some(rock));
        check(&w, "after depositing above the surface");
        assert_eq!(w.surface_top_voxel(cx, cz), Some(top + 4), "deposit must raise the top");

        // Remove a voxel in the MIDDLE of a column (top unchanged) — the case incremental logic gets wrong.
        w.set_voxel(cx, (top / 2).max(1), cz, None);
        check(&w, "after removing a mid-column voxel");

        // Demotion clears a whole column through set_voxel.
        w.demote_column_to_field(cx + 1, cz);
        check(&w, "after demoting a column to the field");

        // Excavate a column to nothing: cached must become None, not a stale height.
        for y in 0..w.h as i32 {
            w.set_voxel(cx + 2, y, cz, None);
        }
        check(&w, "after excavating a column to nothing");
        assert_eq!(w.surface_top_voxel(cx + 2, cz), None, "an empty column has no top");
    }

    /// The guard must be ABLE to fail: a comparison that cannot detect a stale cache is worse than none,
    /// because it converts an unchecked risk into a believed-checked one.
    #[test]
    fn the_top_cache_guard_detects_staleness() {
        let mats = crate::materials::load();
        let mut w = super::generate(&mats);
        let (cx, cz) = (w.w as i32 / 2, w.d as i32 / 2);
        let before = w.surface_top_voxel(cx, cz).unwrap_or(-1);
        // Write voxels DIRECTLY, bypassing set_voxel — exactly how a future edit would break the cache.
        for y in 0..w.h {
            let i = w.idx(cx as usize, y, cz as usize);
            w.voxels[i] = 0;
        }
        assert_eq!(w.surface_top_voxel(cx, cz).unwrap_or(-1), before, "cache is stale by construction here");
        assert_ne!(w.scan_top(cx, cz), before, "a fresh scan must disagree — otherwise the guard is blind");
    }

    use super::*;
    use crate::materials;

    /// DEMOTION MUST NOT MOVE THE GROUND. Bake a column into T0, free its voxels, and the bulk field
    /// must report the SAME surface it did as voxels — otherwise a body standing there would step or
    /// sink the instant its patch de-resolved, and "a world is a world" would be false across the
    /// resolution boundary (docs/46).
    #[test]
    fn demoting_a_column_preserves_the_surface_it_presented() {
        let mats = materials::load();
        let mut w = dry_world(&mats);
        let c = w.center();
        for &(x, z) in &[(30i32, 30i32), (48, 48), (12, 70), (80, 20)] {
            let before_top = w.surface_top_voxel(x, z).expect("column has ground");
            let voxel_surface = before_top as f32 - 0.5 - c.y;
            let freed = w.demote_column_to_field(x, z).expect("a generated column is bakeable");
            assert!(freed > 0, "demotion should free the column's voxels");
            assert!(w.surface_top_voxel(x, z).is_none(), "voxels must be gone after demotion");
            // Sample the BULK field at the column centre, in centered coords.
            let after = w.bulk_height(x as f32 + 0.5 - c.x, z as f32 + 0.5 - c.z);
            assert!(
                (after - voxel_surface).abs() < 1.0e-3,
                "surface moved at ({x},{z}): voxels said {voxel_surface:.4}, field says {after:.4}"
            );
        }
    }


    /// A patch with no sea, for demotion tests. Water columns are legitimately unbakeable (a sea column
    /// has two surfaces, the field stores one), so a generated world — 7.4% of whose columns are wet,
    /// scattered across a bounding box spanning almost the whole 96 m patch — cannot exercise the
    /// demotion path at all. Drying it isolates the property under test from that separate limitation.
    fn dry_world(mats: &[Material]) -> World {
        let mut w = generate(mats);
        if let Some(wm) = w.water_mat {
            for z in 0..w.d as i32 {
                for x in 0..w.w as i32 {
                    for y in 0..w.h as i32 {
                        if w.material_at(x, y, z) == Some(wm) {
                            w.set_voxel(x, y, z, None);
                        }
                    }
                }
            }
        }
        w.water_mat = None;
        w
    }

    /// **Demotion must be INVISIBLE to whoever asks where the ground is** (`docs/47` §5). The surface
    /// is preserved exactly by `demote_column_to_field`, and that surface is already voxel-quantised —
    /// so the authoritative query hands back the IDENTICAL integer top after the voxels are gone. This
    /// is what lets the GPU grain step keep its `array<i32>` heightfield: demotion needs no sub-voxel
    /// surface, and must not be entangled with that separate deferred work.
    #[test]
    fn ground_top_survives_demotion_exactly() {
        let mats = materials::load();
        let mut w = dry_world(&mats);
        for &(x, z) in &[(30i32, 30i32), (48, 48), (12, 70), (80, 20), (5, 5)] {
            let before = w.ground_top_voxel(x, z).expect("column has ground");
            assert_eq!(before, w.surface_top_voxel(x, z).unwrap(), "resolved: voxels answer");
            w.demote_column_to_field(x, z).expect("a generated column is bakeable");
            assert!(w.surface_top_voxel(x, z).is_none(), "the voxels really are gone");
            assert_eq!(
                w.ground_top_voxel(x, z),
                Some(before),
                "the ground moved at ({x},{z}) when its representation changed — a body standing \
                 there would drop through the floor the instant its patch de-resolved"
            );
        }
    }

    /// The ambiguity the `demoted` flag exists to resolve. A column with no voxels means two opposite
    /// things: DEMOTED (its ground moved into the field) or EXCAVATED (there is genuinely no ground).
    /// Inferring from a zero displacement would pop dug-out ground back up to the pristine surface.
    #[test]
    fn an_excavated_column_has_no_ground_but_a_demoted_one_does() {
        let mats = materials::load();
        let mut w = dry_world(&mats);
        let (dug, kept) = ((33i32, 44i32), (34i32, 44i32));
        let top = w.surface_top_voxel(dug.0, dug.1).unwrap();
        for y in 0..top {
            w.set_voxel(dug.0, y, dug.1, None); // excavate the whole column, never demoting it
        }
        assert_eq!(
            w.ground_top_voxel(dug.0, dug.1),
            None,
            "an excavated column has no ground — it must not report the relief it used to have"
        );
        let before = w.ground_top_voxel(kept.0, kept.1).unwrap();
        w.demote_column_to_field(kept.0, kept.1).unwrap();
        assert_eq!(w.ground_top_voxel(kept.0, kept.1), Some(before), "a demoted column still has ground");
    }

    /// Re-resolving hands authority back to the voxels. Putting matter into a demoted column must clear
    /// the flag, or the column keeps answering from its stale baked height and the new matter is
    /// invisible to every ground query — including the one the GPU grains collide against.
    #[test]
    fn putting_matter_back_re_resolves_a_demoted_column() {
        let mats = materials::load();
        let mut w = dry_world(&mats);
        let (x, z) = (60, 60);
        let grass = index_of(&mats, "grass");
        let before = w.ground_top_voxel(x, z).unwrap();
        w.demote_column_to_field(x, z).unwrap();
        assert_eq!(w.ground_top_voxel(x, z), Some(before));
        // A grain de-resolves back into this column (deposit_resting_grain's job).
        w.set_voxel(x, 3, z, Some(grass));
        assert_eq!(
            w.ground_top_voxel(x, z),
            Some(4),
            "the voxels are authoritative again — the query must not keep reading the field"
        );
    }

    /// Demoting the whole patch must leave every ground query saying exactly what it said before — that
    /// is the entire promise of a representation change (docs/46: a world is a world, across the
    /// resolution boundary too). Checked over the whole footprint, not a sample.
    #[test]
    fn demoting_the_whole_patch_moves_no_ground_anywhere() {
        let mats = materials::load();
        let mut w = dry_world(&mats);
        let before: Vec<Option<i32>> = (0..w.d as i32)
            .flat_map(|z| (0..w.w as i32).map(move |x| (x, z)))
            .map(|(x, z)| w.ground_top_voxel(x, z))
            .collect();
        let freed = w.demote_patch_to_field().expect("a pristine patch is bakeable");
        assert!(freed > 0, "demotion should free the patch's voxels");
        assert_eq!(w.solid_count(), 0, "every column's voxels are gone");
        let after: Vec<Option<i32>> = (0..w.d as i32)
            .flat_map(|z| (0..w.w as i32).map(move |x| (x, z)))
            .map(|(x, z)| w.ground_top_voxel(x, z))
            .collect();
        assert_eq!(before, after, "the ground moved somewhere in the patch when it de-resolved");
    }

    /// All-or-nothing, and it must be atomic: a refusal has to leave the patch EXACTLY as it was. A
    /// partial demotion is unrenderable — `patch_resolved` is one bool for the whole footprint, so the
    /// demoted columns would have no mesh while the cap still holds its hole open over them.
    #[test]
    fn one_unbakeable_column_refuses_the_whole_patch_without_touching_it() {
        let mats = materials::load();
        let mut w = dry_world(&mats);
        let (x, z) = (40, 40);
        let top = w.surface_top_voxel(x, z).unwrap();
        w.set_voxel(x, top - 4, z, None); // one cave, anywhere in the 96 m patch
        let solid_before = w.solid_count();
        assert!(w.demote_patch_to_field().is_none(), "an unbakeable column must refuse the patch");
        assert_eq!(
            w.solid_count(),
            solid_before,
            "a refused demotion freed voxels anyway — the patch is now half-resolved and unrenderable"
        );
        assert!(w.demoted.iter().all(|&d| !d), "and no column may be left flagged as demoted");
    }


    /// **The sea refuses to demote, and that is the finding that blocks the whole-patch trigger.** A
    /// column under water has TWO surfaces — seabed and waterline — and the field stores one per column.
    /// Baking it would record the seabed, free the ground beneath, and leave the sea resting on nothing.
    /// Measured on the generated world: **680 of 9,216 columns are wet (7.4%), in a bounding box
    /// spanning x[0..78] z[0..87] of a 96×96 patch** — scattered, not pooled in a corner — so an
    /// all-or-nothing patch demotion is pinned essentially anywhere. Per-column demotion is required,
    /// not a refinement (docs/47 §6).
    #[test]
    fn a_sea_column_refuses_to_demote_and_pins_the_patch() {
        let mats = materials::load();
        let mut w = generate(&mats);
        let wm = w.water_mat.expect("the generated world has a sea");
        let wet = (0..w.d as i32)
            .flat_map(|z| (0..w.w as i32).map(move |x| (x, z)))
            .find(|&(x, z)| (0..w.h as i32).any(|y| w.material_at(x, y, z) == Some(wm)))
            .expect("some column is under water");
        assert!(!w.column_is_bakeable(wet.0, wet.1), "a column under the sea is not bakeable");
        assert!(w.demote_column_to_field(wet.0, wet.1).is_none(), "it must refuse, not strand the sea");
        assert!(
            w.demote_patch_to_field().is_none(),
            "and one wet column pins the whole patch — the reason the trigger needs per-column demotion"
        );
    }

    /// A column the heightfield CANNOT represent — one with a void under its top (a cave, an undercut
    /// crater lip) — must refuse to demote. Collapsing it would silently delete the void and the matter
    /// bounding it, which is a lossy change of STATE masquerading as a change of representation.
    #[test]
    fn a_column_with_a_void_refuses_to_demote() {
        let mats = materials::load();
        let mut w = generate(&mats);
        let (x, z) = (40, 40);
        let top = w.surface_top_voxel(x, z).unwrap();
        assert!(w.column_is_bakeable(x, z), "an intact generated column is bakeable");
        // Hollow out a cell well below the surface -> a void the single-height field cannot express.
        w.set_voxel(x, top - 4, z, None);
        assert!(!w.column_is_bakeable(x, z), "a column with a void is NOT bakeable");
        assert!(w.demote_column_to_field(x, z).is_none(), "demotion must refuse, not silently flatten");
        assert!(w.surface_top_voxel(x, z).is_some(), "the refused column keeps its voxels");
    }

    /// T0 starts flat: an untouched world is EXACTLY the procedural relief, so adding the field changed
    /// nothing until something is baked into it.
    #[test]
    fn untouched_world_bulk_equals_procedural_relief() {
        let mats = materials::load();
        let w = generate(&mats);
        let c = w.center();
        for i in 0..25 {
            let (x, z) = (-30.0 + i as f32 * 2.4, 20.0 - i as f32 * 1.7);
            let expect = terrain_height(x + c.x, z + c.z) - c.y;
            assert_eq!(w.bulk_height(x, z), expect, "T0 must start flat at ({x},{z})");
        }
    }

    /// A flat test patch with one raised step, so the surface has a KNOWN gradient to check against.
    fn stepped_world(mats: &[Material]) -> World {
        let (w, h, d) = (8usize, 8usize, 8usize);
        let rock = materials::index_of(mats, "basalt") as u16 + 1;
        let mut voxels = vec![0u16; w * h * d];
        for z in 0..d {
            for x in 0..w {
                // Left half 2 voxels deep, right half 3 — a single step at x == 4.
                let top = if x < 4 { 2 } else { 3 };
                for y in 0..top {
                    voxels[y * w * d + z * w + x] = rock;
                }
            }
        }
        World::from_voxels(w, h, d, voxels, 3, None)
    }

    /// The refactor that added `surface_bilinear_grad` must not have moved the surface: the height it
    /// returns is byte-identical to what `surface_height_bilinear` reports (which now delegates to it).
    /// Grains collide against this field on the GPU, so a shift here would silently move the ground.
    #[test]
    fn surface_grad_height_agrees_with_surface_height_bilinear() {
        let mats = materials::load();
        let w = stepped_world(&mats);
        for i in 0..40 {
            let p = Vec3::new(-3.0 + i as f32 * 0.17, 0.0, -2.0 + i as f32 * 0.11);
            let (h, _, _) = w.surface_bilinear_grad(p);
            assert_eq!(h, w.surface_height_bilinear(p), "height moved at {p:?}");
        }
    }

    /// The gradient must actually differentiate the height field it is paired with — otherwise the
    /// surface normal `(−∂h/∂x, 1, −∂h/∂z)` is wrong and every contact resolved against it is wrong.
    /// Checked against a central difference of the SAME function (spacing 1 voxel ⇒ the finite
    /// difference IS the derivative), away from the cell seams where the bilinear gradient is
    /// piecewise-constant and a centred stencil would straddle two cells.
    #[test]
    fn surface_grad_matches_finite_difference_of_its_own_height() {
        let mats = materials::load();
        let w = stepped_world(&mats);
        let hq = |x: f32, z: f32| w.surface_bilinear_grad(Vec3::new(x, 0.0, z)).0;
        for &(x, z) in &[(-2.5f32, -1.5f32), (0.3, 0.25), (1.5, -0.75), (-0.4, 1.2)] {
            let (_, dhdx, dhdz) = w.surface_bilinear_grad(Vec3::new(x, 0.0, z));
            const E: f32 = 0.02;
            let fd_x = (hq(x + E, z) - hq(x - E, z)) / (2.0 * E);
            let fd_z = (hq(x, z + E) - hq(x, z - E)) / (2.0 * E);
            assert!((dhdx - fd_x).abs() < 1e-3, "∂h/∂x {dhdx} vs fd {fd_x} at ({x},{z})");
            assert!((dhdz - fd_z).abs() < 1e-3, "∂h/∂z {dhdz} vs fd {fd_z} at ({x},{z})");
        }
    }

    /// A flat surface must report ZERO gradient, so the normal is straight up and a body resting on
    /// level ground gets no spurious sideways push from the contact.
    #[test]
    fn flat_surface_has_zero_gradient() {
        let mats = materials::load();
        let w = stepped_world(&mats);
        // Well inside the left (uniformly 2-deep) half, away from the step at x == 4.
        let (_, dhdx, dhdz) = w.surface_bilinear_grad(Vec3::new(-2.5, 0.0, -1.5));
        assert!(dhdx.abs() < 1e-6 && dhdz.abs() < 1e-6, "flat ground sloped: {dhdx}, {dhdz}");
    }

    /// TRACTION — the property the old `vel.x *= 0.5` fudge could not express at any μ.
    /// Same slope, same approach velocity, two materials: ice (μ=0.05) must retain far more downslope
    /// speed than basalt (μ=0.7), because Coulomb friction is bounded by μ·jn. A velocity multiply is
    /// blind to μ and would give both the identical answer.
    #[test]
    fn friction_depends_on_material_mu_not_a_fixed_multiplier() {
        let mats = materials::load();
        let mu_ice = mats[materials::index_of(&mats, "ice")].friction_coefficient as f64;
        let mu_rock = mats[materials::index_of(&mats, "basalt")].friction_coefficient as f64;
        assert!(mu_ice < mu_rock, "test premise: ice must be slipperier than basalt");

        // A body sliding along a 1:1 slope, driven into it (so there IS a normal impulse to bound
        // friction with). dhdx = 1 ⇒ the surface climbs with +x.
        let run = |mu: f64| {
            let c = crate::granular::terrain_contact_resolve(
                glam::DVec3::new(0.0, 0.0, 0.0),
                glam::DVec3::new(2.0, -1.0, 0.0), // moving downslope-ish and into the surface
                0.05, // surface just above the body's base ⇒ penetrating
                1.0,
                0.0,
                0.0,
                mu,
                0.01,
                f64::INFINITY,
            );
            assert!(c.hit, "test premise: the body must actually be in contact");
            c.vel.length()
        };
        let v_ice = run(mu_ice);
        let v_rock = run(mu_rock);
        assert!(
            v_ice > v_rock,
            "ice (μ={mu_ice}) should keep more speed than basalt (μ={mu_rock}): {v_ice} vs {v_rock}"
        );
    }

    /// The declared cantilever reach for a material at gravity `g` (voxels ≈ m): the SAME derivation the
    /// support model uses, so the tests assert against the real physics, not a copied literal.
    fn reach_of(m: &Material, g: f32) -> f32 {
        (m.fracture_strength / (m.density.max(1.0) * g)).sqrt()
    }

    /// Build a bare world with a vertical support wall (column to the base at x=0, z=`z0`) and a
    /// horizontal cantilever beam of `mat`, `len` voxels long, jutting in +x at height `y0` over air.
    fn overhang_world(mat: usize, len: i32, y0: i32) -> World {
        let (w, h, d) = (48usize, 24usize, 8usize);
        let mut world = World::from_voxels(w, h, d, vec![0u16; w * h * d], y0 as usize + 1, None);
        let z0 = (d / 2) as i32;
        // Support wall: a full column to the base at x=0, so its top voxel at y0 is DIRECTLY supported.
        for y in 0..=y0 {
            world.set_voxel(0, y, z0, Some(mat));
        }
        // Cantilever beam at y0, x = 1..=len, with nothing beneath it (air below) — a pure overhang.
        for x in 1..=len {
            world.set_voxel(x, y0, z0, Some(mat));
        }
        world
    }

    #[test]
    fn overhang_longer_than_material_reach_collapses() {
        // docs/28 (a): a soil/grass overhang LONGER than its cantilever reach must fail. Grass σ_t≈1.5e4,
        // ρ≈1400 → reach ≈ 1 m at surface g, so only the first voxel off the wall can hold; everything
        // past ~1 voxel is unsupported and returned to collapse.
        let mats = materials::load();
        let g = 9.88; // emergent surface gravity (Engine::surface_g); pass g, don't hardcode a reach
        let grass = materials::index_of(&mats, "grass");
        let reach = reach_of(&mats[grass], g);
        assert!(reach < 1.5, "grass barely overhangs (reach {reach:.2} m)");

        let len = 6;
        let y0 = 15;
        let w = overhang_world(grass, len, y0);
        let unsup: std::collections::HashSet<(i32, i32, i32)> =
            w.find_structurally_unsupported(&mats, g).into_iter().collect();
        let z0 = (w.d / 2) as i32;
        // Beam voxels at lateral distance d (= x) from the wall hold iff d ≤ reach; the rest fall.
        for x in 1..=len {
            let far = (x as f32) > reach;
            assert_eq!(
                unsup.contains(&(x, y0, z0)),
                far,
                "beam voxel x={x} (dist {x} vs reach {reach:.2}) support classification"
            );
        }
        // The wall's own column (support to base) is never returned.
        assert!(!unsup.contains(&(0, y0, z0)), "the support wall must hold");
        assert!(!unsup.contains(&(0, 0, z0)), "the base voxel must hold");
    }

    #[test]
    fn overhang_shorter_than_material_reach_holds() {
        // docs/28 (b): competent rock keeps a small lip. Basalt σ_t≈1.45e7, ρ≈2900 → reach ≈ 22 m, so a
        // 6-voxel basalt overhang is well within reach and NONE of it collapses (a real crater rim holds).
        let mats = materials::load();
        let g = 9.88;
        let basalt = materials::index_of(&mats, "basalt");
        let reach = reach_of(&mats[basalt], g);
        assert!(reach > 10.0, "basalt holds a real lip (reach {reach:.1} m)");

        let len = 6;
        assert!((len as f32) < reach, "the test overhang is shorter than the reach");
        let y0 = 15;
        let w = overhang_world(basalt, len, y0);
        assert!(
            w.find_structurally_unsupported(&mats, g).is_empty(),
            "a rock overhang shorter than its cantilever reach holds — nothing collapses"
        );
    }

    #[test]
    fn a_full_base_supported_column_never_collapses() {
        // docs/28 (c): matter in a solid column to the base is directly supported and never returned.
        let mats = materials::load();
        let g = 9.88;
        let w = generate(&mats);
        assert!(
            w.find_structurally_unsupported(&mats, g).is_empty(),
            "intact terrain — every column full to y=0 — is fully supported"
        );
    }

    #[test]
    fn a_disconnected_floating_chunk_still_collapses() {
        // docs/28 (d): don't regress pure disconnection. A chunk with no directly-supported voxel (no
        // column to the base) has nothing to brace from, so every voxel of it is returned to collapse.
        let mats = materials::load();
        let g = 9.88;
        let rock = materials::index_of(&mats, "basalt");
        let mut w = generate(&mats);
        // A small floating 2×2×2 block high above the terrain, disconnected from everything.
        let fy = w.max_top as i32 + 4;
        let mut expected = Vec::new();
        for dy in 0..2 {
            for dz in 0..2 {
                for dx in 0..2 {
                    let (x, y, z) = (4 + dx, fy + dy, 4 + dz);
                    w.set_voxel(x, y, z, Some(rock));
                    expected.push((x, y, z));
                }
            }
        }
        let unsup: std::collections::HashSet<(i32, i32, i32)> =
            w.find_structurally_unsupported(&mats, g).into_iter().collect();
        for e in expected {
            assert!(unsup.contains(&e), "floating chunk voxel {e:?} must collapse");
        }
    }

    #[test]
    fn bulk_height_is_the_shared_terrain_everywhere_including_off_the_footprint() {
        // Increment 1 (dissolve the cube): the DEFAULT ground is the bulk heightmap EVERYWHERE. bulk_height
        // must (a) equal the shared terrain_height (in centered coords) so it is the SAME surface the cap
        // and the resolved voxels use, (b) match the resolved patch top within the footprint (both are
        // terrain_height, up to the ≤0.5 m voxel rounding), and (c) be defined OFF the footprint (the bulk
        // continues — a probe/grain out there rests on real terrain, not the void the old finite patch left).
        let mats = materials::load();
        let w = generate(&mats);
        let c = w.center();

        // (a) exactly terrain_height, converted to centered coords, at arbitrary (incl. fractional) points.
        for &(x, z) in &[(10.0f32, 20.0f32), (48.5, 48.5), (0.0, 95.0), (-500.0, 800.0)] {
            let expect = terrain_height(x + c.x, z + c.z) - c.y;
            assert!(
                (w.bulk_height(x, z) - expect).abs() < 1e-4,
                "bulk_height({x},{z}) != terrain_height there"
            );
        }

        // (b) within the footprint, agrees with the resolved patch top (both are terrain_height ± rounding).
        for z in (2..D as i32 - 2).step_by(7) {
            for x in (2..W as i32 - 2).step_by(7) {
                let patch = w.surface_top_voxel(x, z).unwrap() as f32 - c.y;
                // bulk_height takes CENTERED coords; convert the voxel index (x,z) → centered (x−c.x, z−c.z).
                let bulk = w.bulk_height(x as f32 - c.x, z as f32 - c.z);
                assert!(
                    (patch - bulk).abs() <= 1.0,
                    "bulk vs resolved patch top disagree at ({x},{z}): bulk {bulk:.2} patch {patch:.2}"
                );
            }
        }

        // (c) far OFF the footprint the bulk still returns finite real terrain (the old patch had none).
        let off = w.bulk_height(5000.0, -3000.0);
        assert!(off.is_finite(), "bulk terrain must extend off the footprint");
    }

    #[test]
    fn patch_surface_equals_the_shared_terrain_height() {
        // The refactor's contract: generate() fills each column up to terrain_height — the SAME function
        // the distant Earth cap samples. So the resolved patch top must equal round(terrain_height)
        // everywhere: ONE surface sampled at the fine (patch) resolution. This guards against the patch
        // and the cap ever drifting apart again (the hovering-rubble bug).
        let mats = materials::load();
        let w = generate(&mats);
        for z in 0..D as i32 {
            for x in 0..W as i32 {
                let top = w.surface_top_voxel(x, z).expect("solid column");
                let th = (terrain_height(x as f32, z as f32).round() as i32)
                    .clamp(GRASS_THICKNESS as i32 + 1, H as i32 - 1);
                assert_eq!(top, th, "patch top disagrees with terrain_height at ({x},{z})");
            }
        }
    }

    #[test]
    fn sea_fills_the_low_basins_below_the_datum_and_only_there() {
        // Water is REAL MATTER (docs/28): generate() fills every air voxel below SEA_LEVEL_Y and above the
        // solid land with the DB `water` material, so the terrain's below-sea-level basins become genuine
        // water bodies — never a decorative plane. Asserts (a) the datum sits WITHIN the relief so the sea
        // is genuinely visible (some columns submerged, some dry), (b) water appears ONLY in the air space
        // below the datum and above the land, filling [land_top, sea) so the surface is FLAT at the
        // waterline, and (c) no water leaks at or above the waterline. (This replaces the pre-ocean
        // "all land above the datum" invariant, which the sea intentionally supersedes.)
        let mats = materials::load();
        let w = generate(&mats);
        let water = materials::index_of(&mats, "water");
        let sea = SEA_LEVEL_Y.round() as i32;

        // (a) The demonstration datum is inside the terrain's relief band → genuinely part sea, part land.
        assert!(
            SEA_LEVEL_Y > BASE_TOP - AMPLITUDE && SEA_LEVEL_Y < BASE_TOP,
            "sea level {SEA_LEVEL_Y} must sit within the relief band ({}..{}) to be visible",
            BASE_TOP - AMPLITUDE,
            BASE_TOP
        );

        let (mut water_cols, mut land_cols, mut water_voxels) = (0usize, 0usize, 0usize);
        for z in 0..D as i32 {
            for x in 0..W as i32 {
                // surface_top_voxel is the SOLID land top — the sea is excluded from is_solid, so this is
                // the seabed, not the waterline.
                let land_top = w.surface_top_voxel(x, z).expect("solid land column");
                if land_top < sea {
                    water_cols += 1;
                    // (b) water fills exactly [land_top, sea): matter above the seabed, up to the waterline.
                    for y in 0..H as i32 {
                        let is_w = w.material_at(x, y, z) == Some(water);
                        let should = y >= land_top && y < sea;
                        assert_eq!(
                            is_w, should,
                            "water fill wrong at ({x},{y},{z}) with seabed top {land_top}"
                        );
                        if is_w {
                            water_voxels += 1;
                        }
                    }
                    assert!(w.is_water(x, land_top, z), "the seabed voxel is water");
                    assert!(!w.is_solid(x, land_top, z), "water is matter but NOT solid ground");
                    assert!(w.is_solid(x, land_top - 1, z), "solid seabed directly under the water");
                } else {
                    land_cols += 1;
                    // (c) a dry column carries no water anywhere.
                    for y in 0..H as i32 {
                        assert!(
                            w.material_at(x, y, z) != Some(water),
                            "water above the waterline at dry column ({x},{y},{z})"
                        );
                    }
                }
                // No water ever sits at or above the waterline datum (the surface is flat AT sea level).
                assert!(!w.is_water(x, sea, z), "no water at the waterline voxel ({x},{sea},{z})");
            }
        }
        assert!(
            water_cols > 200,
            "the sea must be visibly large, not a puddle (got {water_cols} submerged columns)"
        );
        assert!(
            land_cols > water_cols,
            "the patch is mostly land, part sea (land {land_cols} vs sea {water_cols})"
        );
        assert!(water_voxels > 0, "the sea is real filled matter");
    }

    #[test]
    fn land_strata_beneath_the_seabed_are_intact() {
        // The sea must NOT corrupt the solid column beneath it: under a submerged basin the seabed still
        // reads grass → basalt → peridotite → iron (Earth's real radial order), with water in the air
        // space ABOVE the grass, up to the waterline. Guards docs/28's "water sits above the land strata".
        let mats = materials::load();
        let w = generate(&mats);
        let id = |name| materials::index_of(&mats, name);
        let sea = SEA_LEVEL_Y.round() as i32;

        // Find a submerged column (its solid land top is below the waterline).
        let mut found = None;
        'outer: for z in 0..D as i32 {
            for x in 0..W as i32 {
                if w.surface_top_voxel(x, z).unwrap() < sea {
                    found = Some((x, z));
                    break 'outer;
                }
            }
        }
        let (x, z) = found.expect("the demonstration sea must submerge at least one basin");
        let land_top = w.surface_top_voxel(x, z).unwrap();

        // Water directly above the seabed; grass is the seabed skin just below it.
        assert!(w.is_water(x, land_top, z), "water fills the air space above the seabed");
        assert_eq!(w.material_at(x, land_top - 1, z), Some(id("grass")), "seabed skin is grass");

        // The solid strata below, top to bottom, are Earth's real radial order — unchanged by the sea.
        let mut seq: Vec<usize> = Vec::new();
        for y in (0..land_top).rev() {
            if let Some(m) = w.material_at(x, y, z) {
                if seq.last() != Some(&m) {
                    seq.push(m);
                }
            }
        }
        assert_eq!(
            seq,
            vec![id("grass"), id("basalt"), id("peridotite"), id("iron")],
            "seabed column must still be grass → crust → mantle → core"
        );
    }

    #[test]
    fn ocean_pressure_is_continuous_with_the_atmosphere_and_grows_hydrostatically() {
        // The ocean is ONE hydrostatic column with the atmosphere (docs/28): at the waterline P = P_atm
        // (the declared atmosphere's weight), and it grows DOWNWARD as P = P_atm + ρ_water·g·depth — the
        // same hydrostatic law the air obeys, continued through the water. DERIVED from the DB water
        // density and the declared atmosphere's surface pressure, not a dial.
        let mats = materials::load();
        let p_atm = crate::planet::earth().surface_pressure();
        let rho = mats[materials::index_of(&mats, "water")].density as f64;
        let g = 9.88; // emergent surface gravity (Engine::surface_g)

        // Continuity at the waterline: depth 0 → exactly the atmosphere's surface pressure.
        assert!(
            (ocean_pressure(0.0, g) - p_atm).abs() < 1e-6,
            "at the waterline the ocean pressure must equal P_atm ({p_atm} Pa)"
        );
        // Above the waterline there is no water column — it clamps to P_atm (the atmosphere takes over).
        assert_eq!(ocean_pressure(-5.0, g), ocean_pressure(0.0, g));

        // Grows linearly with depth at exactly ρ_water·g per metre.
        for &d in &[1.0f64, 5.0, 10.0, 100.0] {
            let expect = p_atm + rho * g * d;
            assert!(
                (ocean_pressure(d, g) - expect).abs() < 1e-6,
                "hydrostatic law at depth {d} m: got {} expected {expect}",
                ocean_pressure(d, g)
            );
        }
        // ≈1 atm of added pressure per ~10 m of water — the real, familiar result (magnitude sanity).
        let ten_m = ocean_pressure(10.0, g) - p_atm;
        assert!(
            ten_m > 9.0e4 && ten_m < 1.1e5,
            "≈1 atm added per 10 m of water (got {ten_m} Pa)"
        );
    }

    #[test]
    fn column_is_earths_real_layers_top_to_bottom() {
        // docs/28 A: the terrain is a surface patch of the REAL layered Earth (planet::earth()) as a
        // declared vertical LOD — grass skin, basalt CRUST, peridotite MANTLE, iron CORE, in that order
        // down a column. Asserts the strata (not game grass/dirt/granite) so a dig/impact exposes honest
        // composition. Depths are LOD-compressed; the ORDER and MATERIALS are Earth's.
        let mats = materials::load();
        let w = generate(&mats);
        let id = |name| materials::index_of(&mats, name);
        let (cx, cz) = (W as i32 / 2, D as i32 / 2);
        let top = w.surface_top_voxel(cx, cz).expect("solid column at centre");

        // Surface skin is grass; the first solid below it is basalt crust.
        assert_eq!(w.material_at(cx, top - 1, cz), Some(id("grass")), "surface skin");
        // Walk down and record the sequence of DISTINCT materials encountered.
        let mut seq: Vec<usize> = Vec::new();
        for y in (0..top).rev() {
            if let Some(m) = w.material_at(cx, y, cz) {
                if seq.last() != Some(&m) {
                    seq.push(m);
                }
            }
        }
        assert_eq!(
            seq,
            vec![id("grass"), id("basalt"), id("peridotite"), id("iron")],
            "column must be Earth's real radial order: grass → crust → mantle → core"
        );
    }

    #[test]
    fn terrain_has_varied_elevation_and_is_solid_all_the_way_down() {
        // The surface must read as REAL rolling terrain — hills and valleys — not a near-flat plateau,
        // and the relief must be genuine matter: every column solid from its surface top down to the
        // base (matter all the way down, no holes). We measure the map-wide surface-top distribution.
        let mats = materials::load();
        let w = generate(&mats);

        let mut tops: Vec<f64> = Vec::with_capacity(W * D);
        for z in 0..D as i32 {
            for x in 0..W as i32 {
                let top = w
                    .surface_top_voxel(x, z)
                    .expect("every column must be solid (matter all the way down)");
                // No holes: solid from the base up to the surface top.
                for y in 0..top {
                    assert!(
                        w.is_solid(x, y, z),
                        "hole at ({x},{y},{z}) beneath surface top {top}"
                    );
                }
                tops.push(top as f64);
            }
        }

        let n = tops.len() as f64;
        let mean = tops.iter().sum::<f64>() / n;
        let std = (tops.iter().map(|t| (t - mean).powi(2)).sum::<f64>() / n).sqrt();
        let (min, max) = tops.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &t| {
            (lo.min(t), hi.max(t))
        });
        let range = max - min;

        // Threshold justification: 1 voxel ≈ 1 m. The old amplitude-6 heightfield was a near-flat
        // plateau (surface-top std under ~1 voxel, peak-to-valley range only a few voxels). Real
        // rolling terrain over this 96×96 patch must show many metres of relief, so we require
        // surface-top std ≥ 4 voxels (≈4 m of undulation about the mean) AND a peak-to-valley range
        // ≥ 15 voxels. The current heightfield measures std ≈ 4.6 and range ≈ 27 — comfortably above
        // these floors, and far above a slab. (Deterministic/seedless, so these values are stable.)
        assert!(
            std >= 4.0,
            "surface-top std must show real relief, not a plateau (got {std:.2} voxels)"
        );
        assert!(
            range >= 15.0,
            "peak-to-valley range must be substantial (got {range:.0} voxels)"
        );
    }

    #[test]
    fn ground_is_matter_all_the_way_through() {
        // Robin: "visible ground is matter all the way through — eliminating the lies of other game
        // engines." Every column must be SOLID contiguously from its surface top down to the base
        // (y = 0): no hollow shell, no air pockets. And a downward raycast from high above must strike
        // solid matter (the surface is not a paper-thin skin over a void).
        let mats = materials::load();
        let w = generate(&mats);

        for z in 0..D as i32 {
            for x in 0..W as i32 {
                let top = w
                    .surface_top_voxel(x, z)
                    .expect("every column must be solid (matter all the way down)");
                // Contiguous solid from the very base up to the surface top — no gaps anywhere.
                for y in 0..top {
                    assert!(
                        w.is_solid(x, y, z),
                        "air pocket / hollow at ({x},{y},{z}) below surface top {top}"
                    );
                }
                // The base voxel itself is matter (solid all the way THROUGH to y = 0).
                assert!(w.is_solid(x, 0, z), "hollow base at ({x},0,{z})");
            }
        }

        // A downward raycast from well above the centre column hits solid matter (not a void skin).
        let c = w.center();
        let start = Vec3::new(0.0, H as f32 - c.y + 10.0, 0.0); // centered coords, above the terrain
        let hit = w
            .raycast(start, Vec3::new(0.0, -1.0, 0.0), 4000.0)
            .expect("downward ray must strike solid ground");
        let (hx, hy, hz, _) = hit;
        assert!(
            w.is_solid(hx, hy, hz),
            "raycast reported a non-solid hit voxel"
        );
    }

    #[test]
    fn orbit_camera_never_penetrates_terrain() {
        // Robin: "camera should never penetrate matter." The terrain orbit camera builds its eye as
        //   eye = dir * (base_distance * zoom),  dir = (cos(pitch)sin(yaw), sin(pitch), cos(pitch)cos(yaw))
        // (see the wasm-gated `Engine::view_proj` / `set_orbit` in lib.rs, which is unreachable from
        // native tests — so we replicate the EXACT construction here). Zoomed in / tilted down, the raw
        // eye ends up buried in the terrain; `World::clamp_eye_outside` must push it back to free air.
        let mats = materials::load();
        let w = generate(&mats);

        let max_dim = w.w.max(w.h).max(w.d) as f32;
        let base_distance = max_dim * 1.6; // exactly Engine::create's construction
        let clearance = 2.0;

        let eye_for = |yaw: f32, pitch: f32, zoom: f32| -> Vec3 {
            let cp = pitch.cos();
            let dir = Vec3::new(cp * yaw.sin(), pitch.sin(), cp * yaw.cos());
            dir * (base_distance * zoom)
        };
        // A centered eye penetrates iff its voxel is solid, or it sits below the surface of its column.
        let penetrates = |eye: Vec3| -> bool {
            let p = eye + w.center();
            let (xi, yi, zi) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
            if w.is_solid(xi, yi, zi) {
                return true;
            }
            match w.surface_top_voxel(xi, zi) {
                Some(top) => p.y < top as f32,
                None => false,
            }
        };

        // GUARD against a vacuous pass: a config that provably buries the raw eye in the terrain.
        let buried = eye_for(0.7, -1.4, 0.2);
        assert!(
            penetrates(buried),
            "test setup is vacuous: the raw eye must actually penetrate the terrain, got {buried:?}"
        );

        // Sweep the full yaw/pitch/zoom envelope (matching set_orbit's clamps: pitch ∈ [-1.5, 1.5],
        // zoom ∈ [0.2, 6.0]).
        let mut saw_penetration = false;
        for &yaw in &[0.0f32, 0.7, 1.5, 2.4, 3.14, 4.7, 6.0] {
            for &pitch in &[-1.5f32, -1.0, -0.5, -0.16, 0.0, 0.16, 0.6, 1.2, 1.5] {
                for &zoom in &[0.2f32, 0.4, 0.7, 1.0, 2.0, 4.0, 6.0] {
                    let raw = eye_for(yaw, pitch, zoom);
                    if penetrates(raw) {
                        saw_penetration = true;
                    }
                    let clamped = w.clamp_eye_outside(raw, clearance);
                    // The clamped eye must be free air, never inside/below matter.
                    assert!(
                        !penetrates(clamped),
                        "clamped eye still penetrates at yaw={yaw} pitch={pitch} zoom={zoom}: {clamped:?}"
                    );
                    assert!(
                        w.eye_is_free(clamped, clearance),
                        "clamped eye not clearance-above ground at yaw={yaw} pitch={pitch} zoom={zoom}"
                    );
                    // An already-free eye is returned UNCHANGED (smooth normal orbit).
                    if w.eye_is_free(raw, clearance) {
                        assert_eq!(
                            clamped, raw,
                            "free eye must be returned unchanged at yaw={yaw} pitch={pitch} zoom={zoom}"
                        );
                    }
                }
            }
        }
        assert!(
            saw_penetration,
            "sweep must exercise penetrating configs (else the clamp is untested)"
        );
    }

    #[test]
    fn eye_never_goes_below_the_earth_sphere_anywhere() {
        // Robin rejected the flat decorative ground: off the 96 m patch the OLD clamp treated the eye as
        // "free" (no ground column) — a hole that was only safe because a fake infinite plane hid it.
        // With the real curved Earth surface, the eye must be walled in by the planet EVERYWHERE — over
        // the patch AND far off it — never below the sphere at radius R centred a full Earth radius down.
        use crate::planet;
        let mats = materials::load();
        let w = generate(&mats);

        let radius = planet::earth().radius() as f32; // ≈6.371e6 m — the SAME body the space band draws
        // The patch centre's surface height in centered coords; the Earth centre is `radius` below it.
        let c = w.center();
        let surf_y = w
            .surface_top_voxel(c.x as i32, c.z as i32)
            .map(|t| t as f32 - c.y)
            .expect("solid centre column");
        let earth_center = Vec3::new(0.0, surf_y - radius, 0.0);
        let clearance = 2.0f32;

        // True below-the-surface test (f64: the metre-scale drop is below f32 precision at R≈6.4e6).
        let below_sphere = |eye: Vec3| -> bool {
            (eye.as_dvec3() - earth_center.as_dvec3()).length() < radius as f64
        };

        // GUARD (non-vacuous): an eye placed 5 km OFF the patch and 40 m below the local surface height.
        // The cap there has dropped only ~2 m, so this eye is well below the real sphere — exactly the
        // off-footprint case the old clamp let through.
        let off_patch_buried = Vec3::new(5000.0, surf_y - 40.0, 0.0);
        assert!(
            below_sphere(off_patch_buried),
            "test setup vacuous: the off-patch eye must actually start below the Earth sphere"
        );
        let fixed = w.clamp_eye_above_earth(off_patch_buried, earth_center, radius, clearance);
        assert!(
            !below_sphere(fixed),
            "clamp failed to lift an off-patch eye above the Earth sphere: {fixed:?}"
        );

        // The terrain orbit eye (exact `Engine::view_proj` construction, wasm-gated so replicated here).
        let max_dim = w.w.max(w.h).max(w.d) as f32;
        let base_distance = max_dim * 1.6;
        let eye_for = |yaw: f32, pitch: f32, zoom: f32| -> Vec3 {
            let cp = pitch.cos();
            let dir = Vec3::new(cp * yaw.sin(), pitch.sin(), cp * yaw.cos());
            dir * (base_distance * zoom)
        };

        // Sweep the full envelope; a steep downward pitch drives the raw eye below the sphere.
        let mut saw_below = false;
        let extra_lateral = [
            Vec3::new(9000.0, surf_y - 10.0, 0.0),
            Vec3::new(-12000.0, surf_y - 30.0, 6000.0),
            Vec3::new(0.0, surf_y - 200.0, 15000.0),
        ];
        for &yaw in &[0.0f32, 0.7, 1.5, 2.4, 3.14, 4.7, 6.0] {
            for &pitch in &[-1.5f32, -1.0, -0.5, 0.0, 0.5, 1.0, 1.5] {
                for &zoom in &[0.2f32, 0.7, 1.0, 2.0, 4.0, 6.0] {
                    let raw = eye_for(yaw, pitch, zoom);
                    if below_sphere(raw) {
                        saw_below = true;
                    }
                    let clamped = w.clamp_eye_above_earth(raw, earth_center, radius, clearance);
                    assert!(
                        !below_sphere(clamped),
                        "clamped eye still below the Earth sphere at yaw={yaw} pitch={pitch} zoom={zoom}: {clamped:?}"
                    );
                }
            }
        }
        for &e in &extra_lateral {
            assert!(below_sphere(e), "lateral guard {e:?} must start below the sphere");
            let clamped = w.clamp_eye_above_earth(e, earth_center, radius, clearance);
            assert!(
                !below_sphere(clamped),
                "clamped lateral eye still below the Earth sphere: {clamped:?}"
            );
        }
        assert!(
            saw_below,
            "sweep must exercise below-sphere configs (else the sphere clamp is untested)"
        );
    }

    #[test]
    fn bilinear_surface_grounds_a_grain_resting_on_a_slope() {
        // Regression: GPU debris rests on the BILINEAR terrain surface (particle_step.wgsl::terrain_h),
        // but the CPU de-resolution readback USED to test "grounded" against only the single column the
        // grain is binned into. On a slope those disagree: a grain in the LOW column of a cell is held up
        // by the cell's HIGH corner, so the single-low-column top judged it airborne and it never
        // de-resolved — and the pile stacked on it stalled as rubble that never returned to the grid
        // (the debris count plateaus at thousands instead of falling to ~0). `surface_height_bilinear`
        // mirrors the shader, so grounding now agrees with where the grain physically rests.
        const PART_HALF: f32 = 0.5; // DEBRIS_PART_HALF (lib.rs) — a grain's collision half-extent
        const MARGIN: f32 = 0.1; //   SETTLE_GROUND_MARGIN (lib.rs)

        // A pure x-slope: column x=3 tops out low (voxel top = 5), x=4 tops out high (voxel top = 10).
        let (w, h, d) = (8usize, 16usize, 8usize);
        let mut world = World::from_voxels(w, h, d, vec![0u16; w * h * d], 10, None);
        for z in 0..d as i32 {
            for y in 0..5 {
                world.set_voxel(3, y, z, Some(0)); // solid 0..=4 ⇒ surface_top_voxel = 5
            }
            for y in 0..10 {
                world.set_voxel(4, y, z, Some(0)); // solid 0..=9 ⇒ surface_top_voxel = 10
            }
        }
        let c = world.center(); // center.y = max_top/2 = 5
        assert_eq!(world.surface_top_voxel(3, 0), Some(5));
        assert_eq!(world.surface_top_voxel(4, 0), Some(10));

        // A grain binned into the LOW column (x=3) but sitting near the HIGH corner (fx = 0.9) of the
        // cell [3,4]. In centered coords that x is (3.9 - center.x).
        let vx = 3.9_f32;
        let vz = 3.5_f32;
        let pos_xz = Vec3::new(vx - c.x, 0.0, vz - c.z);

        // The bilinear surface here is ~lerp(5,10,0.9)=9.5 voxels ⇒ centered 9.5 - 5 - 0.5 = 4.0.
        let surf = world.surface_height_bilinear(pos_xz);
        assert!((surf - 4.0).abs() < 1e-4, "bilinear slope surface should be ~4.0, got {surf}");

        // The single LOW column (x=3) top in the same centered convention: 5 - 5 - 0.5 = -0.5. Far below.
        let single_low = world.surface_top_voxel(3, 0).unwrap() as f32 - c.y - 0.5;
        assert!((single_low + 0.5).abs() < 1e-4);

        // A grain resting ON the bilinear surface: its bottom touches `surf`, so its center is surf+HALF.
        let grain_y = surf + PART_HALF;
        let pos = Vec3::new(pos_xz.x, grain_y, pos_xz.z);

        // The FIX: grounded against the bilinear surface — TRUE (the grain really is on the slope).
        assert!(
            pos.y - PART_HALF <= world.surface_height_bilinear(pos) + MARGIN,
            "a grain resting on the bilinear slope must read as grounded"
        );
        // The BUG it replaces: grounded against only the low column top would be FALSE (surf 4.0 sits far
        // above single_low -0.5 + margin), so the grain — and everything piled on it — never de-resolved.
        assert!(
            !(pos.y - PART_HALF <= single_low + MARGIN),
            "the old single-low-column test wrongly judged the slope grain airborne (the stall)"
        );
    }
}
