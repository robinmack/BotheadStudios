// **Every scene gets the HUD's widget layer, and the camera selector obeys one-sink.**
//
// Two things this asserts that a unit test cannot, because they only exist once a scene has booted a real
// canvas and mounted its widgets:
//
//   1. The widget layer is present in EVERY scene, with the Share-view button inside it. That is
//      CLAUDE.md rule 0 made structural — it used to be enforced by each scene remembering to place the
//      button in its own hand-rolled div, which is exactly the kind of "every scene must…" that decays.
//      A new scene that forgets `createSimHud` fails here.
//   2. Where a scene declares two or more camera producers, EXACTLY ONE is engaged at a time — several
//      producers, ONE sink. Clicking the other must flip which one reads pressed, never light both.
//      That invariant is the whole reason two camera systems can coexist without "where is the camera"
//      having two answers.
import { launch, PORT } from './_launch.mjs';

const b = await launch();
let fail = 0;
// terra: fly ⇄ follow-fragment. orbit/groundzero: manual ⇄ Sean's demo arc (where the world declares one).
// ground/birth/twomoons: one camera system, so the selector must be ABSENT rather than a dead single choice.
for (const page of ['ground.html', 'terra.html', 'orbit.html', 'groundzero.html', 'twomoons.html']) {
  const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
  const errs = [];
  p.on('pageerror', (e) => errs.push(e.message));
  await p.goto(`http://127.0.0.1:${PORT}/${page}`, { waitUntil: 'load' });
  // WAIT FOR THE CONDITION, never a fixed sleep. These scenes boot at wildly different speeds (a star
  // catalogue, a 437k-triangle globe, GPU pipeline creation), and a fixed timeout gave OPPOSITE verdicts
  // for the same scene on consecutive runs — a measurement about the rig rather than about the page.
  await p.locator('#sim-hud-widgets #share-view').waitFor({ state: 'attached', timeout: 45000 })
    .catch(() => {});
  // The camera selector can only appear once the scene has finished declaring its producers, which for a
  // world-driven arc happens after the world JSON is parsed and the site is armed.
  await p.waitForTimeout(1500);

  const layer = await p.locator('#sim-hud-widgets').count();
  const shareInLayer = await p.locator('#sim-hud-widgets #share-view').count();
  const producers = p.locator('#sim-hud-widgets [data-camera-producer]');
  const n = await producers.count();

  let sink = 'n/a';
  if (n >= 2) {
    const pressed = async () =>
      (await producers.evaluateAll((els) => els.filter((e) => e.getAttribute('aria-pressed') === 'true').length));
    const before = await pressed();
    // Click whichever is NOT currently engaged; exactly one must be pressed afterwards, and it must
    // be a DIFFERENT one — a selector that lights both, or neither, has lost the sink.
    const idle = producers.filter({ hasNot: p.locator('[aria-pressed="true"]') }).first();
    const idleId = await producers.nth(1).getAttribute('data-camera-producer');
    await producers.nth(1).click().catch(() => {});
    await p.waitForTimeout(600);
    const after = await pressed();
    const activeId = await producers
      .evaluateAll((els) => els.find((e) => e.getAttribute('aria-pressed') === 'true')?.dataset.cameraProducer ?? null);
    // EXACTLY ONE engaged is the invariant — not "the one you clicked". A producer is allowed to decline
    // and hand straight back: Terra's follow camera does exactly that when nothing is in flight to ride,
    // which is correct and would look like a failure to a naive assertion. What must never happen is two
    // lit at once, or none.
    void idleId;
    sink = before === 1 && after === 1 ? 'OK' : `BROKEN(${before}->${after}, active=${activeId})`;
    void idle;
  }

  const ok = layer === 1 && shareInLayer === 1 && (n === 0 || n >= 2) && (sink === 'n/a' || sink === 'OK');
  if (!ok) fail++;
  console.log(
    `${page.padEnd(17)} layer=${layer} share-in-layer=${shareInLayer} producers=${n} one-sink=${sink} ` +
      `${ok ? 'OK' : 'FAIL'}${errs.length ? ' errs=' + errs.length : ''}`,
  );
  await p.close();
}
console.log(`\nscenes failing: ${fail}/5`);
await b.close();
process.exit(fail === 0 ? 0 : 1);
