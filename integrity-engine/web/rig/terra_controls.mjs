// docs/43 Phase 6 — data-driven controls + HUD. Verify world.controls.keys drive the camera and the HUD reads out.
import { launch } from './_launch.mjs';
const out = process.env.OUT || '/tmp'; const PORT = process.env.PORT || '5173';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1000, height: 800 } });
p.on('pageerror', e => console.log('PAGEERR:', e.message));
await p.goto(`http://127.0.0.1:${PORT}/terra.html`, { waitUntil: 'load' });
await p.waitForTimeout(3500);
const read = () => p.evaluate(() => ({ lat:+window.__terra.latitude().toFixed(3), lon:+window.__terra.longitude().toFixed(3), alt:Math.round(window.__terra.altitude_m()), biome: window.__terra.ground_biome() }));
const hud = () => p.evaluate(() => document.getElementById('stats')?.textContent ?? '');

// Position over land at a modest altitude.
await p.evaluate(() => window.__terra.set_fly(28, 84, 4000, 0, 0.05));
await p.waitForTimeout(200);
console.log('start:', JSON.stringify(await read()));

// R = climb (world.controls binds KeyR→up). Hold ~500ms.
const a0 = (await read()).alt;
await p.keyboard.down('r'); await p.waitForTimeout(500); await p.keyboard.up('r');
const a1 = (await read()).alt;
console.log(`KeyR (up): alt ${a0} -> ${a1}  climbed=${a1 > a0}`);

// F = descend (KeyF→down).
await p.keyboard.down('f'); await p.waitForTimeout(500); await p.keyboard.up('f');
const a2 = (await read()).alt;
console.log(`KeyF (down): alt ${a1} -> ${a2}  descended=${a2 < a1}`);

// D = move east (KeyD→right): lon should increase.
const l0 = (await read()).lon;
await p.keyboard.down('d'); await p.waitForTimeout(500); await p.keyboard.up('d');
const l1 = (await read()).lon;
console.log(`KeyD (right): lon ${l0} -> ${l1}  movedEast=${l1 > l0}`);

// Biome readback over ocean vs land.
await p.evaluate(() => window.__terra.set_fly(0, -140, 3000, 0, 0.05)); await p.waitForTimeout(150);
console.log('mid-Pacific biome:', (await read()).biome);
await p.evaluate(() => window.__terra.set_fly(23, 10, 3000, 0, 0.05)); await p.waitForTimeout(150);
console.log('Sahara biome:', (await read()).biome);

await p.evaluate(() => window.__terra.set_fly(28, 84, 1500, 0, 0.08)); await p.waitForTimeout(300);
console.log('HUD:', await hud());
await p.screenshot({ path: `${out}/controls-hud.png` });
await b.close(); console.log('done');
