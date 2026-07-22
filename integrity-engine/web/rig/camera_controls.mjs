// The camera scheme must be IDENTICAL in every scene: right-drag / alt-drag looks (pivoting in place),
// left-or-ctrl goes forward, +shift reverses. Verified by DOING the gesture and measuring the result.
//
// THREE measurement mistakes this rig made before, each of which produced a confident wrong answer:
//
//  1. A fixed screen CROP — landed on empty sky in twomoons, so the samples were black and the comparison
//     measured nothing. Reported the camera broken when it was fine.
//  2. PNG BYTE LENGTH as a change proxy — size tracks image complexity, not change, so a rotated globe can
//     compress to nearly the same size and read as "the gesture was ignored".
//  3. A whole-frame PIXEL diff in the space band — which is a black field with Earth a few dozen pixels
//     across, dead centre. An orbit camera rotating about its focus KEEPS that focus centred, so a working
//     camera scores ~0.1 against ~0.05 of drift. Again: broken, when it was fine.
//
// So each scene is measured where the evidence actually is. Where the page exposes the camera's own state
// (`window.__cam`), read THAT — the control either moved or it did not, regardless of what is on screen.
// Otherwise compare real pixels, decoded from the compositor screenshot (a WebGPU canvas cannot be read
// back with drawImage — that returns solid black). Either way the gesture must beat an IDLE CONTROL
// measured the same way in the same scene, because these scenes animate whether or not you touch them.
import { launch, PORT, OUT } from './_launch.mjs';
import { decodePng, meanLevel, meanDiff } from './_png.mjs';

const SCENES = [
  ['ground.html', 7000],
  ['terra.html', 9000],
  ['orbit.html', 9000],
  ['twomoons.html', 9000],
  ['birth.html', 12000],
];
const HOLD = 900; // ms a gesture (and the idle control) lasts

const b = await launch();
let bad = 0;
for (const [pg, settle] of SCENES) {
  const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
  p.on('pageerror', (e) => console.log(`  PAGEERR ${pg}: ${e.message}`));
  await p.goto(`http://127.0.0.1:${PORT}/${pg}`, { waitUntil: 'load' });
  await p.waitForTimeout(settle);

  const camState = async () => await p.evaluate(() => (window.__cam ? { ...window.__cam } : null));
  const pixels = async () => decodePng(await p.locator('#gpu-canvas').screenshot());
  const hasState = (await camState()) !== null;
  // One scalar, whichever path: total camera motion ×100, or mean per-channel pixel difference.
  const camMove = (a, x) =>
    100 * (Math.abs(x.yaw - a.yaw) + Math.abs(x.pitch - a.pitch) + Math.abs(x.zoom - a.zoom));
  const take = async () => (hasState ? await camState() : await pixels());
  const score = (a, x) => (hasState ? camMove(a, x) : meanDiff(a, x));

  if (!hasState) {
    const frame = await pixels();
    if (meanLevel(frame) < 2) {
      console.log(`${pg.padEnd(14)} SKIPPED — canvas is blank (mean ${meanLevel(frame).toFixed(2)}) and no camera state exposed`);
      bad++; await p.close(); continue;
    }
  }

  // IDLE CONTROL — what this scene does on its own over the same interval (the space band drifts its yaw
  // gently when untouched, so this matters for the state path too).
  const idle0 = await take();
  await p.waitForTimeout(HOLD);
  const drift = score(idle0, await take());
  const threshold = Math.max(3 * drift, hasState ? 1.0 : 1.5);

  const box = await p.locator('#gpu-canvas').boundingBox();
  const cx = box.x + box.width / 2, cy = box.y + box.height / 2;
  const gesture = async (fn) => {
    const before = await take();
    await fn();
    await p.waitForTimeout(500);
    return score(before, await take());
  };

  const dRight = await gesture(async () => {
    await p.mouse.move(cx, cy);
    await p.mouse.down({ button: 'right' });
    for (let i = 1; i <= 10; i++) await p.mouse.move(cx + i * 18, cy);
    await p.mouse.up({ button: 'right' });
  });
  const dAlt = await gesture(async () => {
    await p.keyboard.down('Alt');
    await p.mouse.move(cx, cy); await p.mouse.down();
    for (let i = 1; i <= 10; i++) await p.mouse.move(cx, cy + i * 14);
    await p.mouse.up(); await p.keyboard.up('Alt');
  });
  const dFwd = await gesture(async () => {
    await p.mouse.move(cx, cy);
    await p.mouse.down();
    await p.waitForTimeout(HOLD);
    await p.mouse.up();
  });

  const yn = (d) => (d > threshold ? 'yes' : 'NO ');
  const ok = dRight > threshold && dAlt > threshold && dFwd > threshold;
  if (!ok) bad++;
  console.log(
    `${pg.padEnd(14)} via=${hasState ? 'state ' : 'pixels'} drift=${drift.toFixed(2)} thr=${threshold.toFixed(2)} | ` +
    `right-drag=${yn(dRight)}(${dRight.toFixed(2)}) alt-drag=${yn(dAlt)}(${dAlt.toFixed(2)}) ` +
    `forward=${yn(dFwd)}(${dFwd.toFixed(2)})  ${ok ? 'OK' : 'FAIL'}`,
  );
  await p.screenshot({ path: `${OUT}/camera-${pg.replace('.html', '')}.png` });
  await p.close();
}
console.log(`\nscenes failing: ${bad}/${SCENES.length}`);
await b.close();
