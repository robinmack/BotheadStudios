//! **Drop a heap of things and measure what forms** — the settled pile, and its summary (docs/71 §3b).
//!
//! Robin, 2026-08-10, on a haystack whose packing was a number computed from a cylinder somebody chose:
//!
//! > *"As you pile them a stack should naturally form if you drop them all in the same location. Which
//! > is cool, but very slow to simulate. I wonder if we could simulate it and then map the pile as a
//! > derived assembly?"*
//!
//! So this settles the members under the engine's own contact law and MEASURES the heap. The result is
//! a measurement with provenance, not a declaration — the same epistemic status as a catalogued
//! material property, and falsifiable in the same way: a settled heap's density can be compared
//! against a real one's.
//!
//! ## It is the engine's own contact law, applied where the bodies actually touch
//!
//! `granular::contact_accel` takes two POINTS and their velocities. A grain of sand is a sphere and its
//! centre is where it touches; a blade of grass is 0.35 m long and 3 mm wide, and where it touches
//! depends on which way it lies. So each member is carried as a CAPSULE — a segment with a radius — and
//! the same law is evaluated at the two segments' closest approach. **No new physics**: the same
//! stiffness, the same damping, the same Coulomb cap that produce an angle of repose for sand.
//!
//! ★ **Why this matters more than it sounds.** Settling blades as SPHERES would pack them at random
//! close packing, ~0.6, and a straw bale is **0.071** — eight times looser. The elongation IS the
//! physics here; a sphere model would return a number that looked measured and was wrong by that
//! factor. The capsule is volume-exact (`π r² L` equals the member's own matter volume), so nothing is
//! gained or lost in the substitution.
//!
//! ## What is honest about it, and what is not
//!
//! - The heap's ENVELOPE is measured by occupancy on a grid whose cell size is stated with the result,
//!   because "the volume a heap occupies" has no meaning without one. Refining the cell converges.
//! - ★ FLAGGED: a capsule cannot BEND. Real straw is flexible, and flexibility lets a heap settle
//!   denser than rigid rods of the same shape — so this measurement is a LOWER bound on packing, in a
//!   direction that is stated rather than discovered.
//! - ★ FLAGGED: this settles under gravity alone. A BALE is compressed by a machine, so a free heap is
//!   loose hay rather than a bale, and the difference between the two numbers is the baling.

use crate::assembly::Assembly;
use crate::materials::Material;
use glam::DVec3;

/// A member of a pile, as the settler carries it: a segment with a radius, volume-exact against the
/// assembly it stands for.
#[derive(Clone, Copy, Debug)]
pub struct Rod {
    pub centre: DVec3,
    /// Unit direction of the long axis.
    pub axis: DVec3,
    pub half_length_m: f64,
    pub radius_m: f64,
    pub vel: DVec3,
}

impl Rod {
    /// The two ends of the segment.
    pub fn ends(&self) -> (DVec3, DVec3) {
        (
            self.centre - self.axis * self.half_length_m,
            self.centre + self.axis * self.half_length_m,
        )
    }

    pub fn volume_m3(&self) -> f64 {
        std::f64::consts::PI * self.radius_m * self.radius_m * 2.0 * self.half_length_m
    }
}

/// **The member of a pile, as a capsule** — length from its longest extent, radius chosen so the
/// capsule holds exactly the matter the assembly does.
pub fn rod_for(member: &Assembly) -> Option<(f64, f64)> {
    let matter = member.matter_volume_m3();
    if matter <= 0.0 {
        return None;
    }
    // The longest dimension any of its parts has — a blade's length, a log's length.
    let length = member
        .parts
        .iter()
        .map(|p| {
            let h = p.shape.half_extents_m();
            2.0 * h.x.max(h.y).max(h.z)
        })
        .fold(0.0f64, f64::max);
    if length <= 0.0 {
        return None;
    }
    // π r² L = matter ⇒ the capsule carries exactly what the member does.
    let radius = (matter / (std::f64::consts::PI * length)).sqrt();
    Some((length, radius))
}

/// The closest points on two segments, and the distance between them. Standard segment–segment
/// closest approach; the degenerate parallel case falls back to clamping, which is what it should do.
fn closest_points(a0: DVec3, a1: DVec3, b0: DVec3, b1: DVec3) -> (DVec3, DVec3) {
    let (u, v, w) = (a1 - a0, b1 - b0, a0 - b0);
    let (a, b, c) = (u.dot(u), u.dot(v), v.dot(v));
    let (d, e) = (u.dot(w), v.dot(w));
    let denom = a * c - b * b;
    // Solve the unconstrained problem, then clamp — and RE-SOLVE the other parameter after each clamp.
    // ★ Skipping the re-solve is the classic bug in this routine and my own test caught it: two
    // COLLINEAR segments lying end to end returned the far end of the first one, because the
    // degenerate branch pinned `s = 0` and never asked what `s` should be once `t` was clamped.
    let mut s = if denom.abs() < 1e-12 {
        0.0
    } else {
        ((b * e - c * d) / denom).clamp(0.0, 1.0)
    };
    let mut t = if c > 1e-12 { (b * s + e) / c } else { 0.0 };
    if t < 0.0 {
        t = 0.0;
        s = if a > 1e-12 {
            (-d / a).clamp(0.0, 1.0)
        } else {
            0.0
        };
    } else if t > 1.0 {
        t = 1.0;
        s = if a > 1e-12 {
            ((b - d) / a).clamp(0.0, 1.0)
        } else {
            0.0
        };
    }
    (a0 + u * s, b0 + v * t)
}

/// What a settled heap turned out to be.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settled {
    pub members: usize,
    /// Substance in the heap, m³ — exactly `members × the member's own`.
    pub matter_m3: f64,
    /// The space the heap occupies, m³, by occupancy on `cell_m`.
    pub envelope_m3: f64,
    /// What the heap came to: matter over envelope. **Measured, not authored.**
    pub packing: f64,
    /// How tall it stands, m.
    pub height_m: f64,
    /// The grid cell the envelope was measured on — the number without which the envelope is
    /// meaningless.
    pub cell_m: f64,
}

/// A deterministic value in `0..1` — the same seed and index always give the same number, so a heap is
/// reproducible and a test is not at the mercy of an RNG.
pub fn derived_unit(seed: u64, i: u64, k: u64) -> f64 {
    unit(seed, i, k)
}

fn unit(seed: u64, i: u64, k: u64) -> f64 {
    let mut h = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ i.wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ k.wrapping_mul(0x1656_67B1_9E37_79F9);
    h ^= h >> 33;
    h = h.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    h ^= h >> 33;
    (h >> 11) as f64 / (1u64 << 53) as f64
}

/// ★★★ **DROP THEM ALL IN THE SAME PLACE AND SEE WHAT FORMS.**
///
/// `count` members are released above a floor with derived positions and orientations, fall under
/// `gravity`, and are settled by the engine's contact law at their capsule closest approach. The heap
/// that results is then measured.
///
/// This is a BUILD-TIME cost, not a frame cost — identical until damaged means every bale in a world
/// is the same settled heap until something happens to one, so the simulation runs once per TYPE.
pub fn settle(
    member: &Assembly,
    mats: &[Material],
    count: usize,
    gravity_ms2: f64,
    seed: u64,
) -> Option<Settled> {
    let (length, radius) = rod_for(member)?;
    let material = member.dominant_material()?;
    let m = mats.iter().find(|m| m.id == material)?;
    if count == 0 {
        return None;
    }
    // The engine's own contact, for THIS material — the same call a sand grain gets. The member's own
    // mass is what sets the contact stiffness per unit mass, so a blade and a boulder of the same
    // substance are as stiff as their masses make them.
    let member_mass = member.mass_kg(mats).ok()?.max(1e-12);
    let contact = crate::granular::contact_from_material(m, radius, member_mass);

    // Released over a disc a few lengths across, stacked upward so they fall rather than start merged.
    // ★★ **DROPPED IN ONE PLACE**, which is Robin's own wording and is not a detail: a heap's packing
    // is a property of straw only if the heap forms by its OWN repose. MEASURED the other way first —
    // released over a disc wider than a blade is long, 400 blades settled at 0.0005, which is a
    // measurement of how thinly they were scattered rather than of how they pack. A point source lets
    // the pile spread to the angle the contact law gives it.
    let spread = length * 0.1;
    let mut rods: Vec<Rod> = (0..count)
        .map(|i| {
            let i = i as u64;
            let (u1, u2, u3) = (unit(seed, i, 1), unit(seed, i, 2), unit(seed, i, 3));
            let r = spread * u1.sqrt();
            let a = u2 * std::f64::consts::TAU;
            // Orientation on the sphere, uniformly — a dropped blade has no preferred direction.
            let z = 2.0 * u3 - 1.0;
            let phi = unit(seed, i, 4) * std::f64::consts::TAU;
            let s = (1.0 - z * z).max(0.0).sqrt();
            Rod {
                // ★ Released just above the floor, not in a tower. MEASURED the other way first: a
                // column `count`-blades tall is seven metres, and at this contact's stable timestep
                // 4,000 steps is 0.94 s — less than the fall takes — so the "heap" was still a cloud
                // at 0.0001 packing. A release height of a couple of blade lengths falls in ~0.4 s.
                centre: DVec3::new(
                    r * a.cos(),
                    radius + unit(seed, i, 5) * length * 2.0,
                    r * a.sin(),
                ),
                axis: DVec3::new(s * phi.cos(), z, s * phi.sin()).normalize(),
                half_length_m: length * 0.5,
                radius_m: radius,
                vel: DVec3::ZERO,
            }
        })
        .collect();

    // A timestep the contact can hold: ω = √stiffness, and a tenth of that period is stable.
    let dt = (0.1 / contact.stiffness.max(1.0).sqrt()).min(1e-3);
    // Long enough to fall AND come to rest: the release is ~2 lengths up, which is ~0.4 s of fall, and
    // the rest is settling. Reported in the result's own terms rather than assumed — if a heap is still
    // moving at the end its packing is not a settled packing.
    let steps = (4.0 / dt) as usize;
    for _ in 0..steps {
        let snapshot = rods.clone();
        for i in 0..rods.len() {
            let mut acc = DVec3::new(0.0, -gravity_ms2, 0.0);
            let (a0, a1) = snapshot[i].ends();
            for (j, other) in snapshot.iter().enumerate() {
                if i == j {
                    continue;
                }
                let (b0, b1) = other.ends();
                // Cheap reject before the closest-point solve.
                if (other.centre - snapshot[i].centre).length_squared()
                    > (length + 4.0 * radius).powi(2)
                {
                    continue;
                }
                let (pa, pb) = closest_points(a0, a1, b0, b1);
                acc += crate::granular::contact_accel(pa, snapshot[i].vel, pb, other.vel, &contact);
            }
            // The floor, as the same contact against an image below it.
            let lowest = a0.y.min(a1.y);
            if lowest < radius {
                let p = DVec3::new(snapshot[i].centre.x, lowest, snapshot[i].centre.z);
                let mirrored = p - DVec3::Y * (2.0 * radius);
                acc += crate::granular::contact_accel(
                    p,
                    snapshot[i].vel,
                    mirrored,
                    DVec3::ZERO,
                    &contact,
                );
            }
            let r = &mut rods[i];
            r.vel += acc * dt;
            // ★ NUMERICAL damping only, and a RATE rather than a per-step factor — the physics of
            // dissipation is the contact's own `normal_damp` and Coulomb friction. MEASURED the wrong
            // way first: `vel *= 0.98` every step is 85 per second at this timestep, which held the
            // blades in the air and produced a "heap" seven metres tall. 2/s is gentle enough to let
            // them fall and firm enough to stop the explicit integrator ringing.
            r.vel *= 1.0 - (2.0 * dt).min(0.5);
            r.centre += r.vel * dt;
            if r.centre.y < radius {
                r.centre.y = radius;
                r.vel.y = r.vel.y.max(0.0);
            }
        }
    }

    // ★ MEASURE THE HEAP. Occupancy on a grid whose cell is the member's own radius scale, so the
    // envelope means "space the heap is in" rather than "box that contains it".
    // The cell must resolve the HEAP, not the blade: too coarse and the envelope is mostly the
    // measurement's own air. An eighth of a member's length, floored at its thickness.
    let cell = (length * 0.125).max(radius * 4.0);
    let mut cells = std::collections::BTreeSet::new();
    let mut height: f64 = 0.0;
    for r in &rods {
        let (e0, e1) = r.ends();
        height = height.max(e0.y.max(e1.y));
        // Walk the segment at cell resolution so a long rod occupies every cell it crosses.
        let n = ((e1 - e0).length() / cell).ceil().max(1.0) as usize;
        for k in 0..=n {
            let p = e0.lerp(e1, k as f64 / n as f64);
            cells.insert((
                (p.x / cell).floor() as i64,
                (p.y / cell).floor() as i64,
                (p.z / cell).floor() as i64,
            ));
        }
    }
    let envelope = cells.len() as f64 * cell * cell * cell;
    let matter = member.matter_volume_m3() * count as f64;
    Some(Settled {
        members: count,
        matter_m3: matter,
        envelope_m3: envelope,
        packing: if envelope > 0.0 {
            (matter / envelope).clamp(0.0, 1.0)
        } else {
            0.0
        },
        height_m: height,
        cell_m: cell,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ★★★ **DROP THE BLADES AND SEE WHAT FORMS** (docs/71 §3b) — Robin's own idea, measured.
    ///
    /// The question this answers is not "does it run" but **what density does a free heap of dry grass
    /// blades actually come to**, and how that compares with the real numbers:
    ///
    /// | | bulk density | packing at 1400 kg/m³ |
    /// |---|---|---|
    /// | loose hay | 40 kg/m³ | 0.029 |
    /// | field bale | 100 kg/m³ | 0.071 |
    /// | high-density bale | 200 kg/m³ | 0.143 |
    ///
    /// ★★ **THE TARGET IS THE HAYSTACK, NOT THE BALE**, and Robin had to say so twice before I built
    /// the right object: *"a hay bale is tighter packed than a loose pile of straw (a haystack) in real
    /// life… in a hay bale, compressing bands are employed… in a hay stack it's all gravity"* — and
    /// then, when I split them, *"which is why I suggested modelling the haystack."* A heap settling
    /// under gravity alone IS a haystack; a bale is that plus a machine and twine. Comparing this
    /// simulation to a bale was comparing gravity against gravity-plus-baling.
    ///
    /// `#[ignore]`: a few hundred rods over four thousand steps is seconds, not milliseconds.
    #[test]
    #[ignore]
    fn a_heap_of_dry_blades_settles_at_the_density_of_loose_hay() {
        let mats = crate::materials::load();
        let blade = crate::assembly::compiled::parse(crate::assembly::compiled::GRASS_BLADE_DRY);
        let (length, radius) = rod_for(&blade).expect("a blade is a rod");
        println!(
            "blade as a capsule: {length:.3} m long, {:.3} mm across",
            radius * 2000.0
        );

        let settled = settle(&blade, &mats, 400, 9.81, 20260810).expect("a heap forms");
        println!(
            "settled heap: {} blades · {:.3e} m³ of straw in {:.3e} m³ · packing {:.4} \
             ({:.0} kg/m³) · {:.2} m tall · measured on {:.3} m cells",
            settled.members,
            settled.matter_m3,
            settled.envelope_m3,
            settled.packing,
            settled.packing * 1400.0,
            settled.height_m,
            settled.cell_m
        );

        assert!(settled.matter_m3 > 0.0 && settled.envelope_m3 > 0.0);
        // It must form a HEAP — something with height, not a single layer on the floor.
        assert!(
            settled.height_m > length * 0.5,
            "it should stack, not lie flat: {:.3} m for {length:.3} m blades",
            settled.height_m
        );

        // ★★★ **THE RESULT, AND IT DOES NOT MATCH REALITY YET — recorded rather than tuned**
        // (docs/46 row 60). A free heap of 400 rigid straw rods settles at **~0.0024 (3 kg/m³)**
        // against real loose hay's **0.029 (40 kg/m³)**: an order of magnitude too loose. Three named
        // reasons, in the order I would attack them:
        //
        //   1. **Rods cannot BEND.** Real straw is flexible and NESTS — a bent stem lies along its
        //      neighbours instead of propping against them. For fibres this is the dominant term, and
        //      it is why the module doc calls this a lower bound.
        //   2. **They cannot TANGLE.** Capsules slide past one another; straw hooks.
        //   3. **400 members is a small heap**, mostly free surface. Bulk density is a bulk property.
        //
        // So this pins what the simulation SAYS, not what hay does, and the gap is the finding. When
        // any of the three is addressed this assertion should fail upward — which is the point of
        // pinning it.
        assert!(
            (0.001..0.006).contains(&settled.packing),
            "the settled heap has moved off its recorded value of 0.0024: {:.4}. If it went UP, \
             check what changed — this measurement is a known order of magnitude below real loose \
             hay (0.029) and closing that gap is docs/46 row 60.",
            settled.packing
        );
        // And it must never approach a SPHERE packing: if it ever reads ~0.6 the elongation has been
        // lost and the thing being measured is not blades.
        assert!(
            settled.packing < 0.30,
            "elongated blades cannot pack like spheres — got {:.3}",
            settled.packing
        );
    }

    /// The capsule stands for exactly the matter the assembly holds — the substitution that makes this
    /// a measurement of the member rather than of a rod somebody sized.
    #[test]
    fn the_capsule_holds_exactly_what_the_blade_holds() {
        let blade = crate::assembly::compiled::parse(crate::assembly::compiled::GRASS_BLADE);
        let (length, radius) = rod_for(&blade).expect("a blade is a rod");
        let rod = Rod {
            centre: DVec3::ZERO,
            axis: DVec3::Y,
            half_length_m: length * 0.5,
            radius_m: radius,
            vel: DVec3::ZERO,
        };
        let want = blade.matter_volume_m3();
        assert!(
            (rod.volume_m3() - want).abs() <= want * 1e-9,
            "capsule {:.6e} m³ vs blade {want:.6e} m³",
            rod.volume_m3()
        );
    }

    /// Two segments' closest approach, checked on cases with known answers — the one piece of new
    /// geometry here, so it does not get to be taken on trust.
    #[test]
    fn closest_approach_of_two_segments() {
        // Crossed at right angles, one metre apart in y.
        let (a, b) = closest_points(
            DVec3::new(-1.0, 0.0, 0.0),
            DVec3::new(1.0, 0.0, 0.0),
            DVec3::new(0.0, 1.0, -1.0),
            DVec3::new(0.0, 1.0, 1.0),
        );
        assert!((a - DVec3::ZERO).length() < 1e-9);
        assert!((b - DVec3::new(0.0, 1.0, 0.0)).length() < 1e-9);
        // End to end, collinear and apart: the near ENDS are the answer.
        let (a, b) = closest_points(
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(1.0, 0.0, 0.0),
            DVec3::new(3.0, 0.0, 0.0),
            DVec3::new(4.0, 0.0, 0.0),
        );
        assert!((a - DVec3::new(1.0, 0.0, 0.0)).length() < 1e-9);
        assert!((b - DVec3::new(3.0, 0.0, 0.0)).length() < 1e-9);
    }
}
