// Run the cross-device GPU probe (gpu-probe.html) in desktop Chromium and print its results.
//
// WHY: the probe's whole point is to run on hardware we cannot attach a profiler to (iPad / M4). This
// rig runs it on hardware we CAN measure, so the probe itself is validated against the native
// tools/gpu-verify baseline in JOURNAL BEFORE any conclusion is drawn from an unfamiliar device. A
// probe that disagrees with the native numbers on known hardware is broken, not informative.
//
// Chromium on Linux drives WebGPU over Vulkan, so this compares like-for-like with the native harness.
//
//   PORT=5173 node web/rig/gpu_probe.mjs          (dev server must already be running)
//   xvfb-run -a node web/rig/gpu_probe.mjs        (headed Chromium is required — see rig/README.md)
//
// Headless cannot composite WebGPU swapchains; this page never draws, but the flags/headed mode are
// kept identical to the other rigs so behaviour matches what the scenes see.
import { launch } from './_launch.mjs';

const PORT = process.env.PORT || '5173';
const URL = `http://127.0.0.1:${PORT}/gpu-probe.html`;

const b = await launch();
const p = await b.newPage({ viewport: { width: 1000, height: 900 } });
p.on('pageerror', (e) => console.log('PAGEERR:', e.message));
p.on('console', (m) => { if (m.type() === 'error') console.log('CONSOLE-ERR:', m.text()); });

await p.goto(URL, { waitUntil: 'load' });

// The sweep runs several sized batches back to back; poll for the completion handle the page sets.
const DEADLINE_MS = 300_000;
const t0 = Date.now();
let out = null;
while (Date.now() - t0 < DEADLINE_MS) {
  await p.waitForTimeout(2000);
  out = await p.evaluate(() => window.__probe ?? null);
  if (out?.done) break;
  const status = await p.evaluate(() => document.getElementById('status')?.textContent ?? '');
  if (/failed|not available|secure context/i.test(status)) {
    console.log('PROBE ERROR:', status.trim());
    await b.close();
    process.exit(1);
  }
}

if (!out?.done) {
  console.log(`probe did not finish within ${DEADLINE_MS / 1000}s`);
  await b.close();
  process.exit(1);
}

// The BROWSER's GPUAdapterInfo is the identity that matters — wgpu's AdapterInfo is empty under
// BROWSER_WEBGPU (it delegates to the browser and cannot see the driver). vendor/architecture is what
// distinguishes e.g. nvidia/turing from nvidia/blackwell, or apple/* on an iPad.
const bi = out.browser ?? {};
console.log(
  `adapter: ${bi.vendor ?? '?'} / ${bi.architecture ?? '?'}` +
  `${bi.device ? ` (${bi.device})` : ''}` +
  `${bi.fallback ? '  [FALLBACK — SOFTWARE ADAPTER]' : ''}` +
  `  · wgpu backend ${out.adapter?.backend}`,
);
console.log('       N   ms/frame   µs/particle        total E      v max');
for (const r of out.rows) {
  const per = (r.ms_per_frame * 1000) / r.n;
  console.log(
    `${String(r.n).padStart(8)}  ${r.ms_per_frame.toFixed(3).padStart(9)}  ${per.toFixed(4).padStart(12)}` +
    `  ${r.tot.toExponential(3).padStart(13)}  ${r.vmax.toFixed(3).padStart(9)}`,
  );
}

await b.close();
