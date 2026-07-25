//! **The standalone engine, driven by a file** (`docs/53`).
//!
//! `cargo run -p engine --bin run-definition -- definitions/ejecta-ground.json [steps]`
//!
//! No browser, no canvas, no scene struct — the engine loads a DEFINITION and runs physics. This is the
//! shape Robin named ("standalone, with external definitions"), and it is what stops the failure docs/46
//! ledger row 15 recorded: deleting the terrain scene left `MatterSim`, `ResolutionField` and the voxel
//! `World` with zero production consumers, because capability was reachable only THROUGH a scene. Here
//! the consumer is a file, so no scene's deletion can orphan anything.

fn main() {
    let mut args = std::env::args().skip(1);
    let path = match args.next() {
        Some(p) => p,
        None => {
            eprintln!("usage: run-definition <world.json> [steps]");
            std::process::exit(2);
        }
    };
    let steps: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(120);

    let json = match std::fs::read_to_string(&path) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("cannot read {path}: {e}");
            std::process::exit(1);
        }
    };
    let mut sim = match engine::simulation::Simulation::from_json(&json, engine::materials::load()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{path}: {e}");
            std::process::exit(1);
        }
    };

    // Count solid voxels so the report can distinguish the two ways a particle disappears:
    // DE-RESOLUTION puts it back in the world as a voxel (matter conserved), while the off-world CULL in
    // `matter::step` deletes it (matter lost). Both read as "0 particles" and only one is honest.
    let solids = |w: &engine::world::World| -> usize {
        (0..w.w as i32)
            .flat_map(|x| (0..w.d as i32).map(move |z| (x, z)))
            .map(|(x, z)| (0..w.h as i32).filter(|&y| w.is_solid(x, y, z)).count())
            .sum()
    };
    let voxels_before = solids(&sim.world);

    println!("definition : {path}");
    println!("after load : {} particles, {} analytic effect(s), {voxels_before} solid voxels",
             sim.particle_count(), sim.analytic_count());
    // The declared solid bodies (docs/23 step 1): cohesive matter, reported so a definition author can
    // see where each body starts relative to the ground it will fall onto.
    let report_bodies = |sim: &engine::simulation::Simulation, when: &str| {
        for (i, b) in sim.cohesive_bodies().iter().enumerate() {
            let c = b.agg.com();
            let g = sim.world.ground_height(c.x as f32, c.z as f32);
            println!(
                "body {i} {when}: {} particles, {} bonds, com y {:.2} m, ground beneath {:.2} m",
                b.agg.particles.len(),
                b.agg.active_bonds(),
                c.y,
                g
            );
        }
    };
    report_bodies(&sim, "at load");
    for i in 0..steps {
        let resolved = sim.step(1.0 / 60.0);
        if resolved > 0 {
            println!(
                "step {i:>4}  : {resolved} effect(s) entered view and materialised -> {} particles",
                sim.particle_count()
            );
        }
    }
    let voxels_after = solids(&sim.world);
    // Full matter accounting. "0 particles" and "+N voxels" each tell half the story; what matters is
    // whether every grain ended up SOMEWHERE. Grains that leave the patch are culled by `matter::step`
    // (docs/46 ledger row 9, "matter leaks at the seam") and that loss is otherwise invisible.
    let returned = voxels_after as i64 - voxels_before as i64;
    let in_flight = sim.particle_count() as i64;
    let created = sim.created_total() as i64;
    let lost = created - returned - in_flight;
    println!(
        "after {steps} : {} particles, {} still analytic, {} resolved in total",
        sim.particle_count(),
        sim.analytic_count(),
        sim.resolved_total()
    );
    println!(
        "matter     : {created} grains created | {returned} returned to voxels | {in_flight} still in \
         flight | {lost} LOST off-patch ({:.1}%)",
        if created > 0 { 100.0 * lost as f64 / created as f64 } else { 0.0 }
    );
    println!("voxels     : {voxels_before} -> {voxels_after}");
    // The batch rung's energy books (docs/61, docs/46 row 17): the grain-to-voxel crossing has no
    // thermal sink on the voxel side yet, so the kinetic energy and carried heat of binned matter
    // are MEASURED at the crossing. Reported whenever the rung ran, so the loss is a number a
    // definition author sees, never a silent zero.
    if sim.recohered_voxels() > 0 {
        println!(
            "recohered  : {} voxels via the batch rung | {:.3e} J kinetic + {:.3e} J heat measured \
             at the crossing (no voxel thermal sink yet, docs/46 row 17)",
            sim.recohered_voxels(),
            sim.recohered_kinetic_j(),
            sim.recohered_heat_j()
        );
    }
    report_bodies(&sim, "at end ");
}
