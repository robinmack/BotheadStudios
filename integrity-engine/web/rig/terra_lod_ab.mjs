// Does the generated relief actually change the PICTURE? Same camera, same frame, octaves off then on.
// A claim that detail improved is worth nothing without the pair.
import { launch } from './_launch.mjs';
const out = process.env.OUT || '/tmp'; const PORT = process.env.PORT || '5173';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1000, height: 800 } });
p.on('pageerror', e => console.log('PAGEERR:', e.message));
await p.goto(`http://127.0.0.1:${PORT}/terra.html`, { waitUntil: 'load' });
await p.waitForFunction(() => !!window.__terra, null, { timeout: 60000 });
await p.waitForTimeout(3000);
for (const alt of [8000, 500]) {
  for (const oct of [0, 16]) {
    await p.evaluate(({ oct, alt }) => {
      window.__terra.set_cap_ladder(1, oct);
      window.__terra.set_fly(28, 86, alt, 0.6, -0.30);
    }, { oct, alt });
    await p.waitForTimeout(2200);
    await p.screenshot({ path: `${out}/ab-${alt}m-oct${oct}.png` });
    console.log(`alt ${alt} octaves ${oct}: shot`);
  }
}
await b.close(); console.log('done');
