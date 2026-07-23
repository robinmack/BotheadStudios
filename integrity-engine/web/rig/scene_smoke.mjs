// Load EVERY scene page and fail on any shader-compile error or pageerror. WGSL is only validated at
// pipeline creation in the browser, so native tests cannot catch a struct/uniform mismatch — space.wgsl
// referenced `u.atm` after the Rust struct grew the field but the shader did not, and every orbital
// scene rendered nothing while the whole native suite stayed green. This is the guard for that class.
import { launch, PORT } from './_launch.mjs';
const PAGES = ['orbit.html', 'twomoons.html', 'birth.html', 'terra.html', 'ground.html'];
const b = await launch();
let bad = 0;
for (const pg of PAGES) {
  const p = await b.newPage({ viewport: { width: 800, height: 600 } });
  const errs = [];
  p.on('pageerror', e => errs.push(String(e.message).split('\n')[0].slice(0, 160)));
  p.on('console', m => { const t = m.text(); if (/parsing WGSL|ShaderModule|is invalid|CreateRenderPipeline/i.test(t)) errs.push('WGSL: ' + t.slice(0, 160)); });
  await p.goto(`http://127.0.0.1:${PORT}/${pg}`, { waitUntil: 'load' });
  await p.waitForTimeout(9000);
  const uniq = [...new Set(errs)];
  if (uniq.length) { bad++; console.log(`${pg.padEnd(14)} FAIL`); uniq.slice(0, 3).forEach(e => console.log('   ', e)); }
  else console.log(`${pg.padEnd(14)} ok`);
  await p.close();
}
console.log(`\nscenes with errors: ${bad}/${PAGES.length}`);
await b.close();
process.exit(bad ? 1 : 0);
