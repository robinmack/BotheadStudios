// Every scene must show the build stamp — without it you cannot tell whether you are looking at the
// build you just deployed, which makes every other visual judgement unreliable.
import { launch, PORT } from './_launch.mjs';
const b = await launch();
let bad = 0;
for (const pg of ['ground.html','terra.html','birth.html','orbit.html','twomoons.html']) {
  const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
  await p.goto(`http://127.0.0.1:${PORT}/${pg}`, { waitUntil: 'load' });
  await p.waitForTimeout(pg === 'terra.html' ? 9000 : 7000);
  const hud = (await p.locator('#stats').innerText().catch(()=>'')).replace(/\s+/g,' ');
  const m = hud.match(/build\s+(\S+)/);
  if (!m) bad++;
  console.log(`${pg.padEnd(15)} ${m ? 'build ' + m[1] : 'NO BUILD STAMP'}`);
  await p.close();
}
console.log(`\nscenes without a build stamp: ${bad}/5`);
await b.close();
