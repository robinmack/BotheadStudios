// Where does a frame actually go? Attaches the CDP sampling profiler to a scene and reports self-time
// by function, plus the idle fraction. Idle-dominated => the main thread is WAITING (GPU / present /
// readback), not computing. Written for the ~1 fps investigation; screenshots cannot answer this.
//
//   SCENE=terrain.html bash scripts/rigshot.sh prof_cpu.mjs
import { launch } from './_launch.mjs';
const PORT = process.env.PORT || '5173';
const SCENE = process.env.SCENE || 'terrain.html';
const SECS = Number(process.env.SECS || 12);
const b = await launch();
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
p.on('pageerror', e => console.log('[pageerror]', e.message));
await p.goto(`http://127.0.0.1:${PORT}/${SCENE}`, { waitUntil: 'load' });
await p.waitForTimeout(6000);   // let it settle past load/compile

const cdp = await p.context().newCDPSession(p);
await cdp.send('Profiler.enable');
await cdp.send('Profiler.setSamplingInterval', { interval: 200 }); // µs
await cdp.send('Profiler.start');
await p.waitForTimeout(SECS * 1000);
const { profile } = await cdp.send('Profiler.stop');

// Self time per node from the sample stream.
const byId = new Map(profile.nodes.map(n => [n.id, n]));
const self = new Map();
const dt = profile.timeDeltas, ids = profile.samples;
let total = 0;
for (let i = 0; i < ids.length; i++) {
  const d = Math.max(0, dt[i] ?? 0); total += d;
  const n = byId.get(ids[i]); if (!n) continue;
  const f = n.callFrame;
  const key = `${f.functionName || '(anonymous)'}  ${(f.url || '').split('/').pop()}:${f.lineNumber}`;
  self.set(key, (self.get(key) || 0) + d);
}
const rows = [...self.entries()].sort((a, b) => b[1] - a[1]);
console.log(`\nprofiled ${(total/1000).toFixed(0)} ms of wall clock, ${ids.length} samples`);
const idle = rows.filter(([k]) => /\(idle\)|\(program\)|\(garbage collector\)/.test(k))
                 .reduce((s, [, v]) => s + v, 0);
console.log(`IDLE/program: ${(100*idle/total).toFixed(1)}%  -> main thread ${idle/total > 0.5 ? 'is WAITING (GPU/present/readback)' : 'is BUSY (CPU-bound)'}`);
console.log('\ntop self-time:');
for (const [k, v] of rows.slice(0, 14)) console.log(`  ${(100*v/total).toFixed(1).padStart(5)}%  ${(v/1000).toFixed(0).padStart(6)} ms  ${k}`);
await b.close();
