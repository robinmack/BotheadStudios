// Find a camera that actually SHOWS the world: sweep yaw/pitch at a given altitude and report how much of
// each frame is not black. A screenshot of the night side verifies nothing, so pick the framing by
// measurement rather than by guessing which way the Sun is right now.
import { launch } from './_launch.mjs';
const out = process.env.OUT || '/tmp'; const PORT = process.env.PORT || '5173';
const ALT = Number(process.env.ALT || 2500000);
const b = await launch();
const p = await b.newPage({ viewport: { width: 500, height: 400 } });
p.on('pageerror', e => console.log('PAGEERR:', e.message));
await p.goto(`http://127.0.0.1:${PORT}/terra.html`, { waitUntil: 'load' });
await p.waitForFunction(() => !!window.__terra, null, { timeout: 60000 });
await p.waitForTimeout(3000);
const results = [];
for (const lon of [-140, -100, -60, -20, 20, 60]) {
  for (const pitch of [-1.4, -1.0, -0.7]) {
    await p.evaluate(({ alt, lon, pitch }) => window.__terra.place_camera(10, lon, alt, 0, pitch), { alt: ALT, lon, pitch });
    await p.waitForTimeout(700);
    const buf = await p.screenshot();
    // Mean luminance of the raw PNG bytes is a crude but sufficient "is anything there" measure.
    const lit = buf.reduce((a, v) => a + v, 0) / buf.length;
    results.push({ lon, pitch, lit: +lit.toFixed(1) });
  }
}
results.sort((a, b2) => b2.lit - a.lit);
console.log('BEST', JSON.stringify(results.slice(0, 6)));
await b.close(); console.log('done');
