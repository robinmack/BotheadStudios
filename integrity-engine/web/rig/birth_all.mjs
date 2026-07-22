import { launch, PORT } from './_launch.mjs';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
p.on('console', m => console.log(`[${m.type()}]`, m.text().slice(0, 300)));
p.on('pageerror', e => console.log('[PAGEERROR]', e.message.slice(0, 300)));
await p.goto(`http://127.0.0.1:${PORT}/birth.html`, { waitUntil: 'load' });
await p.waitForTimeout(9000);
await b.close();
