import { launch, PORT, VIEWPORT } from './_launch.mjs';
const b = await launch();
const p = await b.newPage({ viewport: VIEWPORT });
await p.goto(`http://127.0.0.1:${PORT}/birth.html`, { waitUntil: 'load' });
await p.waitForTimeout(9000);
for (const id of ['status', 'stats']) {
  const t = await p.locator('#' + id).innerText().catch(() => '(absent)');
  console.log(`#${id}:`, t.replace(/\s+/g, ' ').slice(0, 220));
}
await b.close();
