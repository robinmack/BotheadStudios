// The GROUND scene host (docs/55). Thin by design: fetch a world definition, hand it to the engine,
// drive requestAnimationFrame, and wire two controls. Every number about the world lives in the JSON.
import "./dev-log"; // FIRST — relay console/errors to the dev terminal before wasm loads
import init, { Ground } from "./wasm/engine.js";
import "./scene-nav";
import { createSimHud } from "./sim-hud";
import { createShareView } from "./share-view";
import { attachCameraInput, CAMERA_HINT } from "./camera-input";
import { withBase } from "./base-url";

const canvas = document.getElementById("gpu-canvas") as HTMLCanvasElement;
const stats = document.getElementById("stats");
const status = document.getElementById("status");
const setStatus = (m: string, bad = false) => {
  if (!status) return;
  status.textContent = m;
  status.style.color = bad ? "#ff8080" : "#cfd6e4";
  status.style.display = m ? "block" : "none";
};


async function main() {
  if (!navigator.gpu) {
    setStatus("This browser has no WebGPU. Try Chrome/Edge 113+ or Safari 18+.", true);
    return;
  }
  const worldUrl = withBase(document.body.getAttribute("data-world") ?? "/worlds/ground/world.json");
  const worldJson = await fetch(worldUrl).then((r) => {
    if (!r.ok) throw new Error(`world fetch ${worldUrl} → HTTP ${r.status}`);
    return r.text();
  });

  await init();
  const fit = () => {
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    canvas.width = Math.floor(canvas.clientWidth * dpr);
    canvas.height = Math.floor(canvas.clientHeight * dpr);
  };
  fit();
  const g = await Ground.create(canvas, worldJson);
  setStatus("");
  // The HUD panel ships hidden in the HTML; every scene that fills it must show it (orbit and terra
  // do the same). Without this the ball's parcel/bond report was invisible on this page.
  if (stats) stats.hidden = false;

  // **The aim crosshair** — the ground point the camera is looking at, which is where a dropped meteor
  // lands (Robin: "crosshair hud projected on ground; that will be the user-chosen impact point"). A DOM
  // overlay tracking the engine's projected aim point, so it follows the terrain and perspective and
  // hides when the camera looks at the sky (no ground to hit).
  const crosshair = document.createElement("div");
  Object.assign(crosshair.style, {
    position: "absolute", left: "0", top: "0", width: "26px", height: "26px",
    marginLeft: "-13px", marginTop: "-13px", pointerEvents: "none", display: "none",
    zIndex: "5",
  });
  const crosshairSvg = (color: string) =>
    '<svg width="26" height="26" viewBox="0 0 26 26">' +
    `<circle cx="13" cy="13" r="9" fill="none" stroke="${color}" stroke-width="1.5"/>` +
    `<line x1="13" y1="0" x2="13" y2="6" stroke="${color}" stroke-width="1.5"/>` +
    `<line x1="13" y1="20" x2="13" y2="26" stroke="${color}" stroke-width="1.5"/>` +
    `<line x1="0" y1="13" x2="6" y2="13" stroke="${color}" stroke-width="1.5"/>` +
    `<line x1="20" y1="13" x2="26" y2="13" stroke="${color}" stroke-width="1.5"/></svg>`;
  crosshair.innerHTML = crosshairSvg("rgba(255,90,60,0.9)");
  (canvas.parentElement ?? document.body).appendChild(crosshair);

  window.addEventListener("resize", () => {
    fit();
    g.resize(canvas.width, canvas.height);
  });

  // THE shared camera controls (camera-input.ts) — the same gesture grammar as every other scene.
  // Metres per frame while a move key/button is held. Scaled to the scene's human dimensions (a couple
  // of metres of eye height), so it crosses the patch in a few seconds rather than crawling or teleporting.
  const WALK_STEP = 0.8;
  let yaw = 0.6, pitch = -0.25, zoom = 1.0;
  // THE pan path (one per scene): the eye translates laterally in the view plane, under the same
  // movement law as walking (the matter shell still keeps it out of the ground). Deltas arrive in
  // CSS pixels; the engine scales in its own device-pixel grid, so convert by the canvas's dpr.
  // Every pan gesture (shift-drag, middle-drag, shift+scroll, horizontal scroll) lands here.
  const pan = (dxPx: number, dyPx: number) => {
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    g.pan_view(dxPx * dpr, dyPx * dpr);
  };
  const cam = attachCameraInput(
    canvas,
    (dyaw, dpitch) => {
      yaw += dyaw;
      pitch = Math.max(-1.4, Math.min(0.4, pitch + dpitch));
      g.set_orbit(yaw, pitch, zoom);
    },
    { onPan: pan },
  );
  // Wheel: bare vertical scroll dollies the camera along its look direction, the same free
  // movement as dragging forward, just faster; no zoom clamp to get stuck against. Shift+scroll
  // and the horizontal wheel axis are trackpad pan: they feed the SAME pan path as the drag,
  // with the sign of a grab (the world follows the fingers).
  canvas.addEventListener("wheel", (e) => {
    e.preventDefault();
    if (e.shiftKey) {
      pan(-e.deltaX, -e.deltaY);
      return;
    }
    if (e.deltaX !== 0) pan(-e.deltaX, 0);
    if (e.deltaY !== 0) g.walk(-Math.sign(e.deltaY) * WALK_STEP * 6);
  }, { passive: false });
  g.set_orbit(yaw, pitch, zoom);

  // --- Drop meteor. The caller creates a rock and lets go; the outcome is the rock's own 1/2 mv2.
  // 17 km/s is a typical asteroid arrival speed, so what the button drops is a real cosmic impactor,
  // enough to shatter the iron ball where a slower rock would only dent it (docs/23).
  const drop = document.getElementById("drop-meteor");
  const fire = () => {
    g.throw_meteor(1200, 17000);
    setStatus("meteor away: 1,200 kg of iron at 17 km/s", false);
    setTimeout(() => setStatus(""), 2500);
  };
  drop?.addEventListener("click", fire);
  window.addEventListener("keydown", (e) => { if (e.key === "m") fire(); });

  // Share view: one shared implementation (see share-view.ts), placed in this scene's control strip.
  const share = createShareView(canvas, { onStatus: (m, bad) => setStatus(m, bad) });
  document.getElementById("ground-controls")?.appendChild(share.button);

  const hud = createSimHud("ground");
  let fps = 0, frames = 0, last = performance.now();
  const frame = () => {
    frames++;
    const now = performance.now();
    if (now - last >= 500) { fps = Math.round((frames * 1000) / (now - last)); frames = 0; last = now; }
    // Forward/back WALKS the camera along its look direction — the same free-fly grammar as every other
    // scene (left/ctrl forward, +shift back). The matter shell in the engine stops the eye entering the
    // ground; nothing else constrains where it may go.
    const walk = cam.forward();
    if (walk !== 0) g.walk(walk * WALK_STEP);

    // Track the aim point (engine-projected, normalised) with the crosshair — CSS pixels, so it lines up
    // with the mouse regardless of the canvas's device-pixel scale.
    const aim = g.aim_screen();
    if (aim.length === 3) {
      crosshair.style.display = "block";
      crosshair.style.left = `${aim[0] * canvas.clientWidth}px`;
      crosshair.style.top = `${aim[1] * canvas.clientHeight}px`;
      // The engine says WHAT the aim ray meets first: gold on a solid body's matter (the ball),
      // red on the bulk terrain. Exposed as a data attribute so a rig can assert the aim too.
      const onBody = aim[2] > 0.5;
      const was = crosshair.dataset.aim;
      crosshair.dataset.aim = onBody ? "body" : "ground";
      if (was !== crosshair.dataset.aim) {
        crosshair.innerHTML = crosshairSvg(onBody ? "rgba(255,214,70,0.95)" : "rgba(255,90,60,0.9)");
      }
    } else {
      crosshair.style.display = "none";
      delete crosshair.dataset.aim;
    }
    try {
      g.render();
    } catch (err) {
      setStatus(`render error: ${String(err)}`, true);
      return;
    }
    // Immediately after present, while the WebGPU drawing buffer is still readable.
    share.afterPresent();
    if (stats) {
      hud.update({
        title: `<b>${g.world_name()}</b>`,
        physics: [
          `standing on <b>${g.surface_material()}</b> · eye <b>${g.eye_altitude_m().toFixed(0)}</b> m above ground`,
          `grains <b>${g.particle_count()}</b> · meteors in flight <b>${g.meteors_in_flight()}</b> · total ever <b>${g.created_total()}</b>`,
          // The declared solid body (the iron ball), reported from the same state the physics runs on.
          // The VERDICT leads: parcels are conserved matter (the count never drops), so a first-time
          // viewer read "33 parcels" as "still intact" after a direct hit. The one word the sim
          // already knows answers the question the line is really asked - did it survive? - and the
          // parcel/bond counts stay as the supporting numbers.
          ...(() => {
            const bp = g.body_probe();
            if (bp.length !== 4) return [];
            const verdict = g.body_verdict();
            const color =
              verdict === "shattered" ? "#ff8a8a" : verdict === "dented" ? "#ffd08a" : "#9fe0a2";
            return [
              `ball <b style="color:${color}">${verdict.toUpperCase()}</b> · ` +
                `<b>${bp[0]}</b> parcels · <b>${bp[1]}</b> bonds · ` +
                `com <b>${bp[2].toFixed(1)}</b> m over ground <b>${bp[3].toFixed(1)}</b> m`,
            ];
          })(),
        ],
        timeScale: 1,
        fps,
        metersPerPixel: 0,
        controls: `${CAMERA_HINT} · shift-drag, middle-drag or shift+scroll to pan · wheel zoom · <b>M</b> or the button drops a meteor`,
      });
    }
    requestAnimationFrame(frame);
  };
  requestAnimationFrame(frame);
}

main().catch((e) => setStatus(`Failed to start ground scene: ${String(e)}`, true));
