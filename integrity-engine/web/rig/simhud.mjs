// Rig: verify the canonical Sim HUD is identical (banner + window frame + universal sim line) across
// terrain / space / birth, and that the scale bar reads metres on the surface vs km/AU in space.
import { launch } from './_launch.mjs';
const out = process.env.OUT || '/tmp';
const PORT = process.env.PORT || '5173';
const browser = await launch();
const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
page.on('console', (m) => { if (m.type() === 'error') console.log('PAGE-ERR', m.text()); });

const grab = async (url, name, waitMs) => {
  await page.goto(`http://127.0.0.1:${PORT}/${url}`, { waitUntil: 'load' });
  await page.waitForTimeout(waitMs);
  await page.screenshot({ path: `${out}/${name}.png` });
  const hud = await page.$eval('#hud', (e) => e.textContent).catch(() => '(no #hud)');
  const stats = await page.$eval('#stats', (e) => e.innerText).catch(() => '(no #stats)');
  console.log(`\n=== ${name} (${url}) ===`);
  console.log('BANNER:', hud);
  console.log('WINDOW:\n' + stats);
};

await grab('orbit.html', 'hud-orbit', 6000);
await grab('birth.html', 'hud-birth', 4000);
await browser.close();
