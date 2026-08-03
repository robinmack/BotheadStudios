//! **The assembly compiler** (docs/64) — derive once, offline, so the runtime reads instead of computes.
//!
//! `cargo run -p engine --bin compile-assemblies`
//!
//! Robin (2026-08-03): *"since we can pre-calculate the mass we should do so with the assembly to save
//! compute"*, and *"compile an 'earth' from data and material sources for time savings (one high cost
//! compile and done/fast)."* This is the smallest true instance of that: it reads every source assembly
//! in `assets/assemblies/`, computes the bulk quantities from the parts and the material catalogue, and
//! writes the compiled form to `assets/assemblies/compiled/`.
//!
//! ## What makes this a COMPILER and not a second source of truth
//!
//! The direction is one-way, and three properties enforce it (docs/64 §2):
//!
//! * **The source carries no `derived` block.** A hand-written mass would be a number tracing to
//!   nothing (Law V); the compiler is the only thing that may write one.
//! * **The output is deterministic** — same sources, byte-identical output, so it can be regenerated
//!   and compared rather than trusted.
//! * **The cache is checkable against the parts at any time** (`Assembly::verify_cache`), and a
//!   mismatch means the compiled file is STALE and the parts win. `--check` runs exactly that and
//!   returns non-zero, which is what a CI job would call.
//!
//! Compiled output is JSON today. That is the same CONTENT the docs/64 binary format will carry — the
//! binary encoding is an optimisation of this, not a different artifact, and doing it in this order
//! means the data model is settled before anything is quantised or byte-packed.

use engine::assembly::Assembly;

fn main() {
    let check_only = std::env::args().any(|a| a == "--check");
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/assemblies");
    let out = root.join("compiled");
    let mats = engine::materials::load();

    let mut sources: Vec<std::path::PathBuf> = std::fs::read_dir(&root)
        .unwrap_or_else(|e| panic!("{}: {e}", root.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    // Sorted, because a compiler whose output depends on directory order is not deterministic.
    sources.sort();

    if !check_only {
        std::fs::create_dir_all(&out).expect("create compiled/");
    }
    let mut failed = 0usize;
    for path in &sources {
        let text = std::fs::read_to_string(path).expect("read source");
        let mut a = match Assembly::from_json(&text) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("SKIP {}: {e}", path.display());
                failed += 1;
                continue;
            }
        };
        if a.derived.is_some() {
            eprintln!(
                "FAIL {}: a SOURCE assembly must not declare `derived` — that is the compiler's to write",
                path.display()
            );
            failed += 1;
            continue;
        }
        let d = match a.derive(&mats) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("FAIL {}: {e}", path.display());
                failed += 1;
                continue;
            }
        };
        a.derived = Some(d);
        let encoded = serde_json::to_string_pretty(&a).expect("encode") + "\n";
        let target = out.join(path.file_name().expect("a file name"));

        if check_only {
            match std::fs::read_to_string(&target) {
                Ok(existing) if existing == encoded => {}
                Ok(_) => {
                    eprintln!("STALE {}: recompiling would change it", target.display());
                    failed += 1;
                }
                Err(_) => {
                    eprintln!("MISSING {}: never compiled", target.display());
                    failed += 1;
                }
            }
        } else {
            std::fs::write(&target, &encoded).expect("write compiled");
        }
        println!(
            "{:<28} {:>3} parts  {:>9.2} kg  envelope {:>7.1} L  CoM [{:+.3} {:+.3} {:+.3}]",
            a.id,
            a.parts.len(),
            d.mass_kg,
            d.envelope_volume_m3 * 1000.0,
            d.centre_of_mass_m[0],
            d.centre_of_mass_m[1],
            d.centre_of_mass_m[2]
        );
    }
    if failed > 0 {
        eprintln!("\n{failed} assembly/assemblies failed");
        std::process::exit(1);
    }
    println!(
        "\n{} assemblies {}",
        sources.len(),
        if check_only { "up to date" } else { "compiled" }
    );
}
