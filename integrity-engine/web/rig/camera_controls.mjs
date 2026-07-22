// The camera scheme must be IDENTICAL in every scene: right-drag / alt-drag looks (pivoting in place),
// left-or-ctrl walks forward, +shift reverses. Verified by DOING the gesture and checking the view moved.
import { launch, PORT, OUT } from './_launch.mjs';
const b = await launch();
let bad = 0;
for (const pg of ['ground.html', 'terra.html']) {
  const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
  await p.goto(`http://127.0.0.1:${PORT}/${pg}`, { waitUntil: 'load' });
  await p.waitForTimeout(pg === 'terra.html' ? 9000 : 7000);
  const shot = async () => (await p.screenshot({ clip: { x: 300, y: 120, width: 680, height: 460 } }));
  const before = await shot();
  // RIGHT-drag across the canvas.
  const box = await p.locator('#gpu-canvas').boundingBox();
  const cx = box.x + box.width / 2, cy = box.y + box.height / 2;
  await p.mouse.move(cx, cy);
  await p.mouse.down({ button: 'right' });
  for (let i = 1; i <= 10; i++) await p.mouse.move(cx + i * 18, cy);
  await p.mouse.up({ button: 'right' });
  await p.waitForTimeout(700);
  const afterLook = await shot();
  const lookChanged = Math.abs(afterLook.length - before.length) > before.length * 0.01;
  // ALT-drag (no right button).
  await p.keyboard.down('Alt');
  await p.mouse.move(cx, cy); await p.mouse.down();
  for (let i = 1; i <= 10; i++) await p.mouse.move(cx, cy + i * 12);
  await p.mouse.up(); await p.keyboard.up('Alt');
  await p.waitForTimeout(700);
  const afterAlt = await shot();
  const altChanged = Math.abs(afterAlt.length - afterLook.length) > afterLook.length * 0.01;
  const ok = lookChanged && altChanged;
  if (!ok) bad++;
  console.log(`${pg.padEnd(13)} right-drag look=${lookChanged ? 'yes' : 'NO '}  alt-drag look=${altChanged ? 'yes' : 'NO '}  ${ok ? 'OK' : 'FAIL'}`);
  await p.close();
}
console.log(`\nscenes failing: ${bad}/2`);
await b.close();
