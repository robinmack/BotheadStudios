import { chromium } from 'playwright';
const url = process.argv[2];
const browser = await chromium.launch({ headless: false,
  args: ['--enable-unsafe-webgpu', '--enable-features=Vulkan', '--use-angle=vulkan', '--no-sandbox'] });
const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
await page.goto(url, { waitUntil: 'load' });
await page.waitForTimeout(14000); // through impact + settle
const stats = await page.evaluate(() => document.getElementById('stats')?.textContent ?? '');
console.log(url.split('/').pop(), '→', (stats.match(/(\d+)\s*fps/) ?? ['', '?'])[1], 'fps');
await browser.close();
