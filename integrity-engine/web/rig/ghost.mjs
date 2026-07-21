import { chromium } from 'playwright';
const PORT = process.env.PORT || '5173';
const out = process.env.OUT || '/tmp';
const browser = await chromium.launch({ headless: false,
  args: ['--enable-unsafe-webgpu', '--enable-features=Vulkan', '--use-angle=vulkan', '--no-sandbox'] });
const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
// The sim readout lives in #stats (bottom bar), not #hud (top banner).
const readout = async () => {
  const s = await page.locator('#stats').innerText().catch(() => '');
  return s.replace(/\s+/g, ' ').trim();
};
const grab = async (name) => { await page.screenshot({ path: `${out}/${name}.png` }); console.log('---', name, '\n', await readout()); };
const zoomOut = async (n) => {
  const c = page.locator('#gpu-canvas');
  const box = await c.boundingBox();
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  for (let i = 0; i < n; i++) { await page.mouse.wheel(0, 240); await page.waitForTimeout(120); }
};

await page.goto(`http://127.0.0.1:${PORT}/birth.html`, { waitUntil: 'load' });
await page.waitForTimeout(13500);          // through the ~5s countdown + impact + disk
await grab('h1-post-impact');
await page.getByText('Geologic').click().catch(e => console.log('no geologic btn', e.message));
await page.waitForTimeout(3000);
await grab('h2-geologic-near');
await zoomOut(12);                          // pull back to reveal any moonlet at orbital radius
await page.waitForTimeout(1500);
await grab('h3-geologic-wide');
await page.waitForTimeout(4000);
await grab('h4-geologic-wide-later');
await browser.close();
