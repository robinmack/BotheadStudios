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
    println!(
        "after {steps} : {} particles, {} still analytic, {} resolved in total",
        sim.particle_count(),
        sim.analytic_count(),
        sim.resolved_total()
    );
    println!(
        "matter     : {voxels_before} -> {voxels_after} solid voxels ({:+}) — grains that de-resolved \
         returned to the world; any shortfall left the patch and was culled",
        voxels_after as i64 - voxels_before as i64
    );
}
