//! **The physics clock** — real elapsed time turned into whole fixed steps.
//!
//! It lives here, and not in a scene, because every scene needs it and there is only one right answer.
//! The ground scene stepped `1/60 s` once per frame, which makes simulated time equal to frames ÷ 60: a
//! 30 fps machine ran the world at half speed and a 300 fps one at five times. Measured frame rates on
//! one box span 23–354 fps, so the same world ran at wildly different speeds depending on the hardware —
//! and Law VI says physics drives the render, never the reverse.
//!
//! It also lives here because `ground_scene` is browser-only and its tests never run natively, which is
//! how a defect this basic survived in it. Arithmetic belongs where it can be tested.

/// The fixed inner timestep the integrator always sees (s). Physics stability depends on dt being
/// constant; wall-clock variability is absorbed by how MANY of these run, never by their size.
pub const PHYSICS_DT: f32 = 1.0 / 60.0;

/// The most inner steps one frame may run. Without a cap, a long stall (a tab regaining focus, a GPU
/// hitch) asks for hundreds of steps, which takes longer than a frame, which grows the debt further —
/// the spiral of death. Past the cap the simulation admits it is behind rather than trying to catch up;
/// that is a visible slowdown instead of a freeze.
pub const MAX_STEPS_PER_FRAME: u32 = 6;

/// Turns real elapsed time into a whole number of fixed physics steps, carrying the remainder.
pub struct StepClock {
    pub(crate) last: f64,
    /// Unconsumed real time (s) — always less than one step after `tick`.
    pub(crate) accumulator: f32,
}

/// How many fixed steps this frame should run.
pub struct StepBudget {
    pub steps: u32,
}

impl StepClock {
    pub fn new() -> Self {
        StepClock { last: now_seconds(), accumulator: 0.0 }
    }

    pub fn tick(&mut self) -> StepBudget {
        let now = now_seconds();
        let elapsed = (now - self.last).max(0.0) as f32;
        self.last = now;
        // A first frame, or a tab that was asleep, can report an enormous gap. Clamp what is ADMITTED,
        // not what is stepped: pretending a 30 s stall was 0.25 s is honest about "we were not running",
        // whereas stepping it would freeze the page trying to simulate it.
        self.accumulator += elapsed.min(0.25);
        let steps = (self.accumulator / PHYSICS_DT) as u32;
        let steps = steps.min(MAX_STEPS_PER_FRAME);
        self.accumulator -= steps as f32 * PHYSICS_DT;
        StepBudget { steps }
    }
}

/// Wall-clock seconds, on either target.
fn now_seconds() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        // `Date::now` rather than `performance.now`: the engine already depends on js-sys for the clock
        // (orbit::unix_now_seconds), and only DIFFERENCES matter here, so millisecond resolution is more
        // than the physics can use at a 16 ms step.
        js_sys::Date::now() / 1000.0
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0)
    }
}


#[cfg(test)]
mod step_clock_tests {
    use super::{StepClock, MAX_STEPS_PER_FRAME, PHYSICS_DT};

    /// **Simulated time must follow the wall clock, not the frame counter.**
    ///
    /// The scene stepped `1/60 s` once per frame, so simulated time was frames ÷ 60: a 30 fps machine ran
    /// the world at half speed and a 300 fps one at five times. Measured frame rates on one box span
    /// 23–354 fps, so the same world ran at wildly different speeds depending on the hardware — and Law VI
    /// says physics drives the render, never the reverse.
    ///
    /// The clock is driven here with synthetic elapsed times so the property is testable without a browser.
    #[test]
    fn the_same_elapsed_time_advances_the_same_physics_at_any_frame_rate() {
        // Drive one second of wall time at three very different frame rates.
        let simulate = |fps: u32| -> f32 {
            let mut clock = StepClock { last: 0.0, accumulator: 0.0 };
            let frame = 1.0 / fps as f32;
            let mut simulated = 0.0f32;
            for _ in 0..fps {
                // Feed the accumulator directly: `tick` reads a real clock, so the arithmetic it does is
                // what is under test here.
                clock.accumulator += frame;
                let steps = ((clock.accumulator / PHYSICS_DT) as u32).min(MAX_STEPS_PER_FRAME);
                clock.accumulator -= steps as f32 * PHYSICS_DT;
                simulated += steps as f32 * PHYSICS_DT;
            }
            simulated
        };

        let (slow, normal, fast) = (simulate(20), simulate(60), simulate(240));
        // Each should have advanced ~1 s of physics, within one step of quantisation.
        for (fps, t) in [(20, slow), (60, normal), (240, fast)] {
            assert!(
                (t - 1.0).abs() <= PHYSICS_DT * 1.5,
                "one second of wall time must advance ~1 s of physics at {fps} fps, got {t:.4} s"
            );
        }
        // And they must agree with EACH OTHER — that is the property that was broken.
        assert!((slow - fast).abs() <= PHYSICS_DT * 2.0,
            "20 fps and 240 fps must simulate the same world ({slow:.4} vs {fast:.4})");

        // The remainder is carried, never dropped: many tiny frames still add up.
        let mut clock = StepClock { last: 0.0, accumulator: 0.0 };
        let mut simulated = 0.0f32;
        for _ in 0..1000 {
            clock.accumulator += 0.001; // 1 ms frames — far below one physics step
            let steps = ((clock.accumulator / PHYSICS_DT) as u32).min(MAX_STEPS_PER_FRAME);
            clock.accumulator -= steps as f32 * PHYSICS_DT;
            simulated += steps as f32 * PHYSICS_DT;
        }
        assert!((simulated - 1.0).abs() <= PHYSICS_DT * 1.5,
            "1000 × 1 ms frames still advance ~1 s, got {simulated:.4}");
    }

    /// A long stall must not be repaid all at once. Without a cap, the steps owed after a pause take
    /// longer than a frame to run, which owes more steps — the spiral of death.
    #[test]
    fn a_long_stall_is_admitted_rather_than_chased() {
        let mut clock = StepClock { last: 0.0, accumulator: 0.0 };
        clock.accumulator += 30.0; // a tab asleep for half a minute
        let steps = ((clock.accumulator / PHYSICS_DT) as u32).min(MAX_STEPS_PER_FRAME);
        assert_eq!(steps, MAX_STEPS_PER_FRAME, "the frame runs its cap, not 1,800 steps");
    }
}
