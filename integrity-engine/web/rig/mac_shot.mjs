// macOS screenshot host. Self-contained on purpose: it does NOT use _launch.mjs, because every flag
// in there exists for the headless-Xorg Linux box (Vulkan/ANGLE selection, the 1 Hz frame-rate-limit
// workaround, occlusion suppression) and none of them apply here. On macOS a HEADED Chromium gets the
// real Apple Metal WebGPU adapter with zero flags; headless gets no adapter at all, and headless plus
// --enable-unsafe-webgpu gets SwiftShader (software). So: headed, default flags, no Xorg, no MESA env.
//
// Run:  npx vite --port 5199 &   then   node rig/mac_shot.mjs
// PORT, OUT and PAGES work like every other rig (defaults below suit a local Mac session);
// PAGES is a comma-separated page list, e.g. PAGES=orbit.html,terra.html,ground.html.
import { chromium } from 'playwright';
import { mkdirSync } from 'node:fs';

const PORT = process.env.PORT || '5199';
const OUT = process.env.OUT || '/tmp/mac-rig';
const PAGES = (process.env.PAGES || 'orbit.html,birth.html,ground.html').split(',');
mkdirSync(OUT, { recursive: true });

const b = await chromium.launch({ headless: false });
let bad = 0;
for (const pg of PAGES) {
  const p = await b.newPage({ viewport: { width: 1024, height: 768 } });
  const errs = [];
  p.on('pageerror', e => errs.push(String(e.message).split('\n')[0].slice(0, 160)));
  p.on('console', m => { const t = m.text(); if (m.type() === 'error' || /parsing WGSL|ShaderModule|is invalid|CreateRenderPipeline/i.test(t)) errs.push(t.slice(0, 160)); });
  await p.goto(`http://127.0.0.1:${PORT}/${pg}`, { waitUntil: 'load' });
  // First load of a page compiles the dev wasm and builds meshes; "Requesting GPU device" can sit on
  // screen past 10 s. Wait for the loading text to clear, then let the scene run a few frames.
  await p.waitForFunction(() => !document.body.innerText.includes('GPU device'), { timeout: 30000 }).catch(() => {});
  await p.waitForTimeout(10000);
  await p.screenshot({ path: `${OUT}/${pg.replace('.html', '')}.png` });
  const uniq = [...new Set(errs)];
  if (uniq.length) { bad++; console.log(`${pg.padEnd(14)} FAIL`); uniq.slice(0, 3).forEach(e => console.log('   ', e)); }
  else console.log(`${pg.padEnd(14)} ok -> ${OUT}/${pg.replace('.html', '')}.png`);
  await p.close();
}
console.log(`\nChromium ${b.version()} · scenes with errors: ${bad}/${PAGES.length}`);
await b.close();
process.exit(bad ? 1 : 0);
