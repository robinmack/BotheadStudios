// macOS descent-camera verification: one continuous camera path from the whole globe down to
// standing height, screenshotted at stepped altitudes so a reader can check each regime by eye
// (no jitter tearing, no z-fighting bands, terrain still textured at 2 m). Same self-contained
// headed-Chromium pattern as mac_shot.mjs (headless macOS Chromium gets no WebGPU adapter).
//
// Run:  npx vite --port 5599 &   then   PORT=5599 node rig/mac_descent.mjs
import { chromium } from 'playwright';
import { mkdirSync } from 'node:fs';

const PORT = process.env.PORT || '5199';
const OUT = process.env.OUT || '/tmp/mac-descent';
mkdirSync(OUT, { recursive: true });

// pitch: orbital views blend to straight-down on their own; near the ground look at the horizon
// with a little downward tilt so both the foreground terrain and the horizon are in frame.
const STEPS = [
  // [name, lat, lon, alt_m, pitch] — over the Himalaya so the final metres land on real mountains,
  // plus a cross-fade-band step (globe + cap co-drawn: the z-fighting regime) and a look-down over
  // Sahara sand (a biome whose relief texture reads at 2 m, unlike flat-lit snow).
  ['1-globe',       28.0,  86.9, 12_000_000, -1.2],
  ['2-continental', 28.0,  86.9,  2_000_000, -1.2],
  ['3-blendband',   28.0,  86.9,     25_000, -0.35],
  ['4-mountain',    28.0,  86.9,     12_000, -0.25],
  ['5-100m',        28.0,  86.9,        100, -0.15],
  ['6-2m',          28.0,  86.9,          2, -0.08],
  ['7-2m-ground',   23.0,  10.0,          2, -0.7],
];

const b = await chromium.launch({ headless: false });
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
const errs = [];
p.on('pageerror', e => errs.push(String(e.message).split('\n')[0].slice(0, 160)));
p.on('console', m => { const t = m.text(); if (m.type() === 'error' || /parsing WGSL|ShaderModule|is invalid|CreateRenderPipeline/i.test(t)) errs.push(t.slice(0, 160)); });
await p.goto(`http://127.0.0.1:${PORT}/terra.html`, { waitUntil: 'load' });
await p.waitForFunction(() => !!window.__terra, { timeout: 60000 });
await p.waitForTimeout(4000); // let the globe mesh build + first frames settle

for (const [name, lat, lon, alt, pitch] of STEPS) {
  await p.evaluate(([la, lo, a, pt]) => window.__terra.set_fly(la, lo, a, 0.0, pt), [lat, lon, alt, pitch]);
  await p.waitForTimeout(1200);
  await p.screenshot({ path: `${OUT}/${name}.png` });
  const hud = (await p.locator('#stats').innerText().catch(() => '')).replace(/\s+/g, ' ').trim();
  console.log(`${name.padEnd(14)} -> ${OUT}/${name}.png | ${hud.slice(0, 90)}`);
}
const uniq = [...new Set(errs)];
uniq.slice(0, 5).forEach(e => console.log('ERR', e));
console.log(`errors: ${uniq.length}`);
await b.close();
process.exit(uniq.length ? 1 : 0);
