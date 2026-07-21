// Hold a scene open so an external profiler (nvidia-smi dmon) can sample it. Prints HUD fps at the end.
import { launch } from './_launch.mjs';
const PORT = process.env.PORT || '5173';
const SCENE = process.env.SCENE || 'terrain.html';
const SECS = Number(process.env.SECS || 30);
const b = await launch();
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
await p.goto(`http://127.0.0.1:${PORT}/${SCENE}`, { waitUntil: 'load' });
console.log('loaded, holding', SECS, 's');
await p.waitForTimeout(SECS * 1000);
const hud = (await p.locator('#stats').innerText().catch(()=>'')).replace(/\s+/g,' ');
console.log('HUD:', (hud.match(/·\s*[\d.]+\s*fps/)||['?'])[0], '|', (hud.match(/debris\s+[\d,]+/)||[''])[0]);
await b.close();
