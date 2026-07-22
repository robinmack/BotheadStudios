// The ground scene (docs/55): does the definition render, and does a meteor resolve grains ONLY where
// it hits — then settle back into the ground?
import { launch, PORT, OUT } from './_launch.mjs';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
const errs = [];
p.on('pageerror', e => errs.push(e.message));
p.on('console', m => { const t = m.text(); if (/error|panic|failed/i.test(t)) errs.push(t); });
await p.goto(`http://127.0.0.1:${PORT}/ground.html`, { waitUntil: 'load' });
await p.waitForTimeout(7000);
const hud = async () => (await p.locator('#stats').innerText().catch(()=>'')).replace(/\s+/g,' ').trim();
const grains = async () => { const m = (await hud()).match(/grains\s+([\d,]+)/); return m ? +m[1].replace(/,/g,'') : -1; };
const shot = async (n) => (await p.screenshot({ path: `${OUT}/ground-${n}.png`, clip: { x: 300, y: 120, width: 680, height: 460 } })).length;

console.log('before   :', await hud());
console.log('render   :', await shot('0-before'), 'B (blank control is ~1900)');
await p.locator('#drop-meteor').click();
console.log('thrown   :', await hud());
// Catch the grains EARLY — they settle fast, so a late shot shows an empty crater and proves nothing.
for (const ms of [120, 400, 1200]) {
  await p.waitForTimeout(ms === 120 ? 120 : 0);
  console.log(`t+${ms}ms  : ${await grains()} grains, ${await shot(`1-t${ms}`)} B`);
  if (ms !== 1200) await p.waitForTimeout(ms === 120 ? 280 : 800);
}
for (const s of [3, 8, 16]) {
  await p.waitForTimeout(s * 1000 - 900);
  console.log(`t+${s}s     : ${await grains()} grains, ${await shot(`2-t${s}`)} B`);
}
console.log('after    :', await hud());
console.log('errors   :', errs.length ? errs.slice(0,3) : 'none');
await b.close();
