import { chromium } from 'playwright';
import { writeFileSync } from 'node:fs';
const out = '/tmp/claude-1000/-home-ratwood/b8643c15-d933-437e-8ec8-236cf9ecf634/scratchpad';
const browser = await chromium.launch({ headless: false,
  args: ['--enable-unsafe-webgpu', '--enable-features=Vulkan', '--use-angle=vulkan', '--no-sandbox'] });
const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
const grab = async (name) => {
  await page.screenshot({ path: `${out}/${name}.png` });
  console.log('grabbed', name);
};
await page.goto('http://127.0.0.1:5280/orbit.html', { waitUntil: 'load' });
await page.waitForTimeout(6000);
await grab('c-orbit');
await page.goto('http://127.0.0.1:5280/birth.html', { waitUntil: 'load' });
await page.waitForTimeout(4000);
await grab('c-birth-pre');
await page.waitForTimeout(8000);
await grab('c-birth-post');
await browser.close();
