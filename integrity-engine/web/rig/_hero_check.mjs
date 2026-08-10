// The hero must put its content ON the first screen at any monitor size — the failure was a 2560x1440
// display showing 60% empty canvas above the wordmark.
import { launch } from './_launch.mjs';
const b = await launch();
const URL = process.env.URL || 'http://127.0.0.1:5173/';
for (const [w, h] of [[2560,1440],[1920,1080],[1440,900],[1280,800]]) {
  const p = await b.newPage({ viewport: { width: w, height: h } });
  await p.goto(URL, { waitUntil: 'networkidle', timeout: 60000 });
  await p.waitForTimeout(2500);
  await p.screenshot({ path: `/tmp/hero-${w}.png` });
  const m = await p.evaluate(() => {
    const h1 = document.querySelector('h1').getBoundingClientRect();
    const cta = document.querySelector('.cta-row')?.getBoundingClientRect();
    return { h1_top: Math.round(h1.y), h1_bot: Math.round(h1.bottom),
             cta_bot: cta ? Math.round(cta.bottom) : null, vh: innerHeight };
  });
  const frac = (m.h1_top / m.vh * 100).toFixed(0);
  const ok = m.h1_top < m.vh * 0.45 && (m.cta_bot ?? 0) < m.vh;
  console.log(`${w}x${h}: wordmark starts ${frac}% down, CTA ends at ${m.cta_bot}/${m.vh}  ${ok ? 'OK' : '← TOO LOW'}`);
  await p.close();
}
await b.close();
