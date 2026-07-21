// Screenshot the space-band BIRTH-OF-THE-MOON scene so the agent can see the debris swarm / proto-lunar
// disk with its own eyes (the "watch locally" rule).
//
//   bash scripts/rigshot.sh birth_shot.mjs           # the LOCAL dev build (the default)
//   URL=https://integrity.bothead.net/birth.html bash scripts/rigshot.sh birth_shot.mjs   # production
//
// Defaults to LOCAL deliberately: it used to default to the public site, so a bare run screenshotted
// PRODUCTION and looked exactly like a verified local change. Run it through `rigshot.sh`, not
// `xvfb-run` — xvfb is a software compositor that cannot read back the GPU swapchain, so it returns the
// DOM HUD over a BLANK canvas (CLAUDE.md rule 4; the trap that cost prior sessions).
//
// Captures a time series (pre-impact → strike → aftermath) so we can watch the disk form, not just a
// single frame. Prints the on-screen #stats (the Sim HUD) at each mark.
import { launch } from './_launch.mjs';

const PORT = process.env.PORT || '5173';
const URL = process.env.URL || `http://127.0.0.1:${PORT}/birth.html`;
const OUT = process.env.OUT || '/tmp';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
p.on('console', (m) => console.log('[page]', m.text()));
p.on('pageerror', (e) => console.log('[pageerror]', e.message));
const stat = async () => (await p.locator('#stats').innerText().catch(() => '')).replace(/\s+/g, ' ').trim();

await p.goto(URL, { waitUntil: 'load' });
// Marks (seconds of wall-clock after load): catch just-before the strike, the strike, and the aftermath as
// the debris swarm evolves under the scene's time-LOD.
const marks = [3, 7, 12, 20, 32, 48];
let last = 0;
for (const s of marks) {
  await p.waitForTimeout((s - last) * 1000);
  last = s;
  await p.screenshot({ path: `${OUT}/birth-${s}s.png` });
  console.log(`t+${s}s: ${await stat()}`);
}
await b.close();
