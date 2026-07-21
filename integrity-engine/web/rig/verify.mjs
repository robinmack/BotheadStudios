import { chromium } from 'playwright';
const PORT = process.env.PORT || '5173';
const out = process.env.OUT || '/tmp';
const browser = await chromium.launch({ headless: false,
  args: ['--enable-unsafe-webgpu', '--enable-features=Vulkan', '--use-angle=vulkan', '--no-sandbox'] });
const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
await page.goto(`http://127.0.0.1:${PORT}/birth.html`, { waitUntil: 'load' });
await page.waitForTimeout(4000);
await page.screenshot({ path: `${out}/v1-blue.png` });   // pre-impact: the blue marble check
await page.waitForTimeout(9000);
await page.screenshot({ path: `${out}/v2-post.png` });
await page.waitForTimeout(25000);
await page.screenshot({ path: `${out}/v3-late.png` });
const stats = await page.evaluate(() => document.getElementById('stats')?.textContent ?? '');
console.log(stats.replace(/\s+/g, ' ').slice(0, 260));
await browser.close();
