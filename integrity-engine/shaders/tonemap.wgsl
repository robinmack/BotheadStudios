// **The display law: compress brightness, never invent colour.**
//
// Every shader here tone-maps with Reinhard, and every one of them did it PER CHANNEL —
// `radiance / (1 + radiance)`. That is wrong in a way that only shows up on genuinely bright things, and
// then it is badly wrong: each channel independently walks toward 1, so a hot surface loses its hue.
//
// Measured, for proto-Earth's 1,900 K magma ocean. Planck through the CIE observer gives linear sRGB
// (1.000, 0.243, 0.000) — a deep orange. At the radiance it actually emits (~547x a sunlit white surface):
//
//   per channel   (1.000, 1.000, 0.000)  <- green saturates too, and it reads YELLOW
//   by luminance  (1.000, 0.628, 0.000)  <- still orange, which is what the object IS
//
// The object's chromaticity is a physical fact; the per-channel form manufactures a colour it does not
// have. So compress the LUMINANCE and carry the chromaticity through unchanged. A bright red-hot surface
// stays red-hot instead of turning white, which is also why the night side of a magma ocean reads orange:
// thermal emission does not care where the Sun is, and there is no dark side to be white.
//
// (A camera exposed for daylight really does photograph lava as a white blob — but that is the camera
// clipping, not the lava. The engine should not bake one particular bad exposure into its display law.)

fn tonemap(radiance : vec3<f32>) -> vec3<f32> {
    // Rec. 709 luminance — the same primaries the sRGB conversion assumes.
    let l = dot(radiance, vec3<f32>(0.2126, 0.7152, 0.0722));
    if (l <= 0.0) {
        return vec3<f32>(0.0);
    }
    let compressed = l / (1.0 + l); // Reinhard, on luminance alone
    // Scale the colour to the compressed luminance. Clamped because a saturated hue can still exceed the
    // display gamut in one channel; that clip is the DISPLAY's limit, and it no longer changes the hue of
    // anything that fits.
    return min(radiance * (compressed / l), vec3<f32>(1.0));
}
