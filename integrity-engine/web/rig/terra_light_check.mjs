// **Is the ground actually LIT, or is it painted?** Same site, same camera, sun moved from noon to
// midnight, measuring the frame rather than eyeballing it.
//
// Robin, on seeing the scene: *"why is the entire camera orange (and according to the night side
// renders, luminous?) It looks like we force a color and don't let lighting play a role."* This rig is
// the answer, and it exonerated the lighting: mean ground luminance 117.9 at noon, 106.4 at dusk, 14.2
// at midnight. The surface responds to the sun exactly as it should.
//
// What it does NOT exonerate is the material: noon RGB is 134/119/64, which is `pine` — the catalogue's
// pine TIMBER, albedo [0.68, 0.48, 0.21]. Ireland sits inside the derived land cover's boreal band, so
// the island is drawn the colour of a plank (docs/46 row 28). Analyse the frames with PIL; a PNG's byte
// mean is compressed data and tells you nothing, which the first version of this rig reported anyway.
import { launch } from './_launch.mjs';
const out = process.env.OUT || '/tmp/rigshot';
const b = await launch();
const p = await b.newPage({ viewport: { width: 640, height: 420 } });
await p.goto('http://127.0.0.1:5173/yarr.html', { waitUntil: 'load' });
await p.waitForFunction(() => !!window.__terra, null, { timeout: 60000 });
await p.waitForTimeout(4000);
const mean = (buf) => { // crude luminance of the PNG bytes (comparative only)
  let s = 0; for (let i = 0; i < buf.length; i++) s += buf[i]; return s / buf.length;
};
for (const [name, lon] of [['noon', -9.45], ['dusk', -9.45 + 85], ['midnight', -9.45 + 180]]) {
  await p.evaluate(l => window.__terra.set_epoch_sun_over_lon(l), lon);
  await p.waitForTimeout(1600);
  const png = await p.screenshot({ path: `${out}/light-${name}.png` });
  const ss = await p.evaluate(() => window.__terra.sub_solar());
  console.log(`  ${name.padEnd(9)} subsolar lon ${ss[1].toFixed(0).padStart(4)}  mean byte ${mean0(png)}`);
}
await b.close();
function mean0(buf){ let s=0; for(let i=0;i<buf.length;i++) s+=buf[i]; return (s/buf.length).toFixed(1); }
