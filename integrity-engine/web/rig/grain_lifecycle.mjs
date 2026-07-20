// Does matter that becomes a grain ever go back to being ground?
//
// Robin's concern: voxel -> grain with no return path means grains accumulate without bound and, once
// reposed, carry no information the field could not hold. This rig measures the actual lifecycle:
// fire a meteor, then watch `particle_count()` while everything settles.
//
// What the numbers mean:
//   rises then falls to ~baseline  -> grain -> voxel de-resolution keeps up; the loop is closed
//   rises and PLATEAUS high        -> grains are stranded; the model pays for them forever
//   pinned at MAX_PARTICLES        -> saturated; new debris cannot be created
//
//   PORT=5173 node web/rig/grain_lifecycle.mjs
//   xvfb-run -a node web/rig/grain_lifecycle.mjs
import { chromium } from 'playwright';

const PORT = process.env.PORT || '5173';
const SAMPLES = parseInt(process.env.SAMPLES || '80', 10);
const b = await chromium.launch({
  headless: false,
  args: ['--enable-unsafe-webgpu', '--enable-features=Vulkan', '--use-angle=vulkan', '--no-sandbox'],
});
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
p.on('pageerror', (e) => console.log('PAGEERR:', e.message));
await p.goto(`http://127.0.0.1:${PORT}/terrain.html`, { waitUntil: 'load' });
await p.waitForTimeout(4000);

// #stats carries "debris <b>N</b>" for the terrain scene; fall back to scraping any integer near it.
const count = async () =>
  p.evaluate(() => {
    const t = document.getElementById('stats')?.textContent ?? '';
    const m = t.match(/debris[^0-9]*([0-9,]+)/i) || t.match(/grains[^0-9]*([0-9,]+)/i);
    return m ? parseInt(m[1].replace(/,/g, ''), 10) : null;
  });

const baseline = await count();
if (baseline === null) {
  const t = await p.evaluate(() => document.getElementById('stats')?.textContent ?? '(no #stats)');
  console.log('could not read a debris count from #stats. Raw stats text:');
  console.log('  ' + t.replace(/\s+/g, ' ').slice(0, 400));
  await b.close();
  process.exit(1);
}
console.log(`  baseline debris: ${baseline}`);

// Fire a meteor (main.ts binds "M"), then watch the population.
await p.keyboard.press('KeyM');
const series = [];
for (let i = 0; i < SAMPLES; i++) {
  await p.waitForTimeout(500);
  series.push(await count());
}

const peak = Math.max(...series);
const tail = series.slice(-10);
const settled = tail[tail.length - 1];
const drift = Math.max(...tail) - Math.min(...tail);

console.log(`  trace: ${series.filter((_, i) => i % 4 === 0).join(' → ')}`);
console.log(`  peak ${peak} · settled ${settled} · last-10 drift ${drift}`);
const recovered = peak > baseline ? (100 * (peak - settled)) / (peak - baseline) : 0;
console.log(`  recovered to field: ${recovered.toFixed(0)}% of the debris the meteor created`);
if (settled > baseline + 0.5 * (peak - baseline)) {
  console.log('  => STRANDED: more than half the debris never returned to the field');
} else if (recovered > 90) {
  console.log('  => loop closed: grains de-resolve back to voxels');
} else {
  console.log('  => partial: some debris returns, some is stranded');
}
await b.close();
