//! Incandescence — the colour a hot body *emits* from its temperature (`docs/20`).
//!
//! This is the honest analogue of the space band's "brightness = illumination × reflectance": molten
//! rock isn't lit bright, it *glows* — it emits light because it is hot. So its colour comes from its
//! temperature (a black-body ramp: dull red → orange → yellow → white as it heats), added on top of
//! its reflected colour, so it self-illuminates even on the dark side. A first approximation of the
//! Planckian locus, not a spectral integration (flagged, `docs/20`).

#![allow(dead_code)] // used by the wasm renderer and native tests

/// Emitted (added) linear-RGB for a body at `temp_k`. Zero below a visible-glow threshold (~800 K),
/// then ramping in brightness and shifting red → orange → yellow → white with temperature. The result
/// can exceed 1 (very hot = bright, clips toward white) — that's intended HDR-ish emission.
pub fn incandescence(temp_k: f32) -> [f32; 3] {
    const GLOW_START: f32 = 800.0; // K — first visible dull-red glow
    if temp_k <= GLOW_START {
        return [0.0, 0.0, 0.0];
    }
    let t = temp_k - GLOW_START;
    // Brightness climbs with temperature (∝ how far above the glow threshold), allowed to exceed 1.
    let intensity = (t / 2200.0).clamp(0.0, 4.0);
    // Colour: red always present; green fills in 800→3000 K; blue only once white-hot (>2600 K).
    let r = 1.0;
    let g = (t / 2200.0).clamp(0.0, 1.0);
    let b = ((temp_k - 2600.0) / 2400.0).clamp(0.0, 1.0);
    [r * intensity, g * intensity, b * intensity]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matter::REF_TEMP_K;

    fn sum(c: [f32; 3]) -> f32 {
        c[0] + c[1] + c[2]
    }

    #[test]
    fn cold_matter_does_not_glow_and_hotter_glows_brighter_and_whiter() {
        // Ambient/cold: no emission at all.
        assert_eq!(incandescence(REF_TEMP_K), [0.0, 0.0, 0.0]);
        assert_eq!(incandescence(700.0), [0.0, 0.0, 0.0]);

        // Just-glowing rock (~1200 K) is dim and red-dominant.
        let warm = incandescence(1200.0);
        assert!(warm[0] > 0.0, "glowing");
        assert!(
            warm[0] > warm[1] && warm[1] >= warm[2],
            "red-dominant when cool"
        );

        // Hotter → brighter overall and less red-dominated (green/blue rise toward white).
        let hot = incandescence(4000.0);
        assert!(sum(hot) > sum(warm), "hotter glows brighter");
        assert!(hot[1] > warm[1] && hot[2] >= warm[2], "shifts toward white");
    }
}
