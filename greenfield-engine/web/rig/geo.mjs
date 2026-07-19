import { chromium } from 'playwright';
const out = '/tmp/claude-1000/-home-ratwood/b8643c15-d933-437e-8ec8-236cf9ecf634/scratchpad';
const browser = await chromium.launch({ headless: false,
  args: ['--enable-unsafe-webgpu', '--enable-features=Vulkan', '--use-angle=vulkan', '--no-sandbox'] });
const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
await page.goto('http://127.0.0.1:5280/birth.html', { waitUntil: 'load' });
await page.waitForTimeout(14000); // impact + settle
await page.click('text=⏭ Geologic');
await page.waitForTimeout(2000);
for (let i = 0; i < 4; i++) await page.click('text=⏩ faster');
await page.waitForTimeout(12000);
await page.screenshot({ path: `${out}/geo1.png` });
await page.waitForTimeout(15000);
await page.screenshot({ path: `${out}/geo2.png` });
const stats = await page.evaluate(() => document.getElementById('stats')?.textContent ?? '');
console.log(stats.replace(/\s+/g, ' ').slice(0, 300));
await browser.close();
