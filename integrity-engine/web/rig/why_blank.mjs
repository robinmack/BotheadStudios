import { launch, PORT, VIEWPORT } from './_launch.mjs';
const b = await launch();
for (const pg of ['birth.html','terra.html']) {
  const p = await b.newPage({ viewport: VIEWPORT });
  const errs = [];
  p.on('pageerror', e => errs.push(e.message));
  p.on('console', m => { const t = m.text(); if (/error|fail|panic|Validation|expected/i.test(t)) errs.push(t.slice(0,220)); });
  await p.goto(`http://127.0.0.1:${PORT}/${pg}`, { waitUntil: 'load' });
  await p.waitForTimeout(8000);
  console.log(`--- ${pg} ---`);
  errs.slice(0,4).forEach(e => console.log('   ', e));
  if (!errs.length) console.log('    (no errors reported)');
  await p.close();
}
await b.close();
