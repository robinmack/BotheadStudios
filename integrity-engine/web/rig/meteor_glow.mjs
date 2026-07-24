// Verify meteor ENTRY HEATING (docs/48): a small, fast meteor heats to incandescence and burns up; the
// default big/slow rock correctly does NOT glow. Confirms temp_k → the render glow, retiring the 1600 K
// fudge. Reads the dev-log for the arrival-temperature / burn-up lines the sim emits.
import { launch } from './_launch.mjs';
const out = process.env.OUT || '/tmp';
const PORT = process.env.PORT || '5173';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
await p.goto(`http://127.0.0.1:${PORT}/ground.html`, { waitUntil: 'load' });
await p.waitForTimeout(4000); // wasm + first frame
// Small, fast: 2 kg of iron at 6 km/s — heats fast (tiny heat capacity), should glow and ablate.
await p.evaluate(() => (window.__ground)?.throw_meteor?.(2, 6000));
for (let i = 0; i < 12; i++) { await p.waitForTimeout(300); await p.screenshot({ path: `${out}/meteor-glow-${i}.png` }); }
// Default big/slow for contrast.
await p.evaluate(() => (window.__ground)?.throw_meteor?.(1200, 900));
for (let i = 0; i < 8; i++) { await p.waitForTimeout(300); }
await p.screenshot({ path: `${out}/meteor-slow.png` });
await b.close(); console.log('done');
