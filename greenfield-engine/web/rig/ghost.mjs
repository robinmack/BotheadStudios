import { chromium } from 'playwright';
const out = '/tmp/claude-1000/-home-ratwood/b8643c15-d933-437e-8ec8-236cf9ecf634/scratchpad';
const browser = await chromium.launch({ headless: false,
  args: ['--enable-unsafe-webgpu', '--enable-features=Vulkan', '--use-angle=vulkan', '--no-sandbox'] });
const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
const hud = async () => (await page.locator('#hud').innerText()).replace(/\s+/g, ' ').trim();
const grab = async (name) => { await page.screenshot({ path: `${out}/${name}.png` }); console.log('grabbed', name, '::', await hud()); };

await page.goto('http://127.0.0.1:5280/birth.html', { waitUntil: 'load' });
await page.waitForTimeout(4500);
await grab('g1-pre-impact');
await page.waitForTimeout(9000);           // let Theia strike + disk begin
await grab('g2-post-impact');
// Enter geologic time — this is where the Theia ghost used to appear.
await page.getByText('Geologic').click().catch(e => console.log('no geologic btn', e.message));
await page.waitForTimeout(6000);
await grab('g3-geologic');
// Replay/Reset — must restore the pristine (non-spinning proto-Earth) state, no residual spin.
await page.getByText('Replay').click().catch(e => console.log('no replay btn', e.message));
await page.waitForTimeout(1500);
await grab('g4-after-replay');
await browser.close();
