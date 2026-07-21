import { launch, OUT } from './_launch.mjs';
const b = await launch();
for (const [name, page] of [['terrain','terrain.html'],['birth','birth.html'],['terra','terra.html']]) {
  const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
  const errs = []; p.on('pageerror', e => errs.push(e.message));
  await p.goto(`https://integrity.bothead.net/${page}`, { waitUntil: 'load' });
  await p.waitForTimeout(name === 'terra' ? 9000 : 7000);
  const hud = (await p.locator('#stats').innerText().catch(()=>'')).replace(/\s+/g,' ').trim();
  const crop = await p.screenshot({ path: `${OUT}/live-${name}.png`, clip: { x: 300, y: 120, width: 680, height: 460 } });
  console.log(`${name.padEnd(8)} ${crop.length > 20000 ? 'RENDERS' : 'BLANK!'} (${crop.length} B) errs=${errs.length} | ${hud.slice(0, 90)}`);
  await p.close();
}
await b.close();
