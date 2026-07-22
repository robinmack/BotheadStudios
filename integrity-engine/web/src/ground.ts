// The GROUND scene host (docs/55). Thin by design: fetch a world definition, hand it to the engine,
// drive requestAnimationFrame, and wire two controls. Every number about the world lives in the JSON.
import init, { Ground } from "./wasm/engine.js";
import "./scene-nav";
import { createSimHud } from "./sim-hud";
import { createShareView } from "./share-view";
import { attachCameraInput, CAMERA_HINT } from "./camera-input";

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
  const worldUrl = document.body.getAttribute("data-world") ?? "/worlds/ground/world.json";
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

  window.addEventListener("resize", () => {
    fit();
    g.resize(canvas.width, canvas.height);
  });

  // THE shared camera controls (camera-input.ts) — the same gesture grammar as every other scene.
  let yaw = 0.6, pitch = -0.25, zoom = 1.0;
  const cam = attachCameraInput(canvas, (dyaw, dpitch) => {
    yaw += dyaw;
    pitch = Math.max(-1.4, Math.min(0.4, pitch + dpitch));
    g.set_orbit(yaw, pitch, zoom);
  });
  canvas.addEventListener("wheel", (e) => {
    e.preventDefault();
    zoom = Math.min(6, Math.max(0.15, zoom * (1 + Math.sign(e.deltaY) * 0.12)));
    g.set_orbit(yaw, pitch, zoom);
  }, { passive: false });
  g.set_orbit(yaw, pitch, zoom);

  // --- Drop meteor. Energy is a real number in joules, not a "power" dial.
  const drop = document.getElementById("drop-meteor");
  const fire = () => {
    g.throw_meteor(1200, 900);
    setStatus("meteor away — 1,200 kg of iron at 900 m/s", false);
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
    // Forward/back walks the camera in (the scene's rig holds the declared eye height, and the camera
    // shell stops it entering the ground — the camera is matter).
    const walk = cam.forward();
    if (walk !== 0) {
      zoom = Math.min(6, Math.max(0.15, zoom * (1 - walk * 0.02)));
      g.set_orbit(yaw, pitch, zoom);
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
        ],
        timeScale: 1,
        fps,
        metersPerPixel: 0,
        controls: `${CAMERA_HINT} · wheel zoom · <b>M</b> or the button drops a meteor`,
      });
    }
    requestAnimationFrame(frame);
  };
  requestAnimationFrame(frame);
}

main().catch((e) => setStatus(`Failed to start ground scene: ${String(e)}`, true));
