// The Share view button must EXIST where the /__shot receiver does (localhost, LAN) and be HIDDEN where
// it does not (the public site, which answers 405). A button that silently fails is worse than none.
import { launch, PORT } from './_launch.mjs';
const b = await launch();
const targets = [
  ['LOCAL  ', `http://127.0.0.1:${PORT}/ground.html`, true],
  ['PUBLIC ', 'https://integrity.bothead.net/ground.html', false],
];
let bad = 0;
for (const [label, url, wantVisible] of targets) {
  const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
  await p.goto(url, { waitUntil: 'load' });
  await p.waitForTimeout(7000);
  const exists = await p.locator('#share-view').count() > 0;
  const visible = exists ? await p.locator('#share-view').isVisible() : false;
  const ok = visible === wantVisible;
  if (!ok) bad++;
  console.log(`${label} ${url.replace(/^https?:\/\//,'').slice(0,34).padEnd(36)} button visible=${String(visible).padEnd(5)} want=${String(wantVisible).padEnd(5)} ${ok ? 'OK' : 'FAIL'}`);
  await p.close();
}
console.log(`\nfailures: ${bad}/2`);
await b.close();
