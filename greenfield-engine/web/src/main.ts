// Thin browser host for greenfield-engine.
//
// Responsibilities live ONLY here: create the canvas backing store, load the WASM core, and
// pump requestAnimationFrame. All simulation and rendering happen inside the Rust/WASM `Engine`.

import init, { Engine } from "./wasm/engine.js";

function fail(message: string): void {
  const el = document.getElementById("error");
  if (el) {
    el.textContent = message;
    el.hidden = false;
  }
  console.error(message);
}

/** Size the canvas backing store to its CSS box, capped at 2x DPR to bound the pixel count. */
function sizeCanvas(canvas: HTMLCanvasElement): void {
  const dpr = Math.min(window.devicePixelRatio || 1, 2);
  const w = Math.max(1, Math.floor(canvas.clientWidth * dpr));
  const h = Math.max(1, Math.floor(canvas.clientHeight * dpr));
  canvas.width = w;
  canvas.height = h;
}

async function main(): Promise<void> {
  const canvas = document.getElementById("gpu-canvas") as HTMLCanvasElement | null;
  if (!canvas) {
    fail("Canvas element #gpu-canvas not found.");
    return;
  }

  if (!("gpu" in navigator)) {
    fail(
      "WebGPU is not available in this browser. Use a recent Chrome/Edge/Firefox, or Safari 26+.",
    );
    return;
  }

  sizeCanvas(canvas);

  try {
    await init(); // instantiate the WASM module
    const engine = await Engine.create(canvas);

    window.addEventListener("resize", () => {
      sizeCanvas(canvas);
      engine.resize(canvas.width, canvas.height);
    });

    // --- Orbit camera controls (drag to rotate, wheel to zoom) ---
    const cam = { yaw: 0.7, pitch: 0.5, zoom: 1.0 };
    let dragging = false;
    let lastX = 0;
    let lastY = 0;

    canvas.addEventListener("pointerdown", (e) => {
      dragging = true;
      lastX = e.clientX;
      lastY = e.clientY;
      canvas.setPointerCapture(e.pointerId);
    });
    canvas.addEventListener("pointerup", (e) => {
      dragging = false;
      canvas.releasePointerCapture(e.pointerId);
    });
    canvas.addEventListener("pointermove", (e) => {
      if (!dragging) return;
      cam.yaw -= (e.clientX - lastX) * 0.008;
      cam.pitch += (e.clientY - lastY) * 0.008;
      cam.pitch = Math.max(-1.4, Math.min(1.4, cam.pitch));
      lastX = e.clientX;
      lastY = e.clientY;
    });
    canvas.addEventListener(
      "wheel",
      (e) => {
        e.preventDefault();
        cam.zoom *= Math.exp(e.deltaY * 0.001);
        cam.zoom = Math.max(0.3, Math.min(4.0, cam.zoom));
      },
      { passive: false },
    );

    // Slow idle auto-rotation until the user first interacts, so the world is obviously 3D.
    let userInteracted = false;
    const markInteract = () => {
      userInteracted = true;
    };
    canvas.addEventListener("pointerdown", markInteract, { once: true });
    canvas.addEventListener("wheel", markInteract, { once: true });

    // --- Keyboard: re-drop the probe (Space/R), change time-scale ([ ]) ---
    window.addEventListener("keydown", (e) => {
      if (e.code === "Space" || e.code === "KeyR") {
        e.preventDefault();
        engine.reset_drop();
      } else if (e.code === "BracketLeft") {
        engine.set_time_scale(engine.time_scale() / 1.5);
      } else if (e.code === "BracketRight") {
        engine.set_time_scale(engine.time_scale() * 1.5);
      }
    });

    // --- Live physics HUD ---
    const stats = document.getElementById("stats");
    const fmt = (x: number) => x.toExponential(2);
    const updateStats = () => {
      if (!stats) return;
      const g = engine.surface_gravity();
      const resting = engine.is_resting();
      stats.innerHTML =
        `world mass: <b>${fmt(engine.total_mass())}</b> kg &nbsp;·&nbsp; ` +
        `gravity here: <b>${fmt(g)}</b> m/s² &nbsp;(asteroid-scale micro-g — real physics)<br>` +
        `probe (5&nbsp;kg): altitude <b>${engine.sphere_altitude().toFixed(2)}</b> m &nbsp;·&nbsp; ` +
        `speed <b>${fmt(engine.sphere_speed())}</b> m/s &nbsp;·&nbsp; ` +
        (resting ? "<b>at rest ✔</b>" : "falling…") +
        `<br>time-scale: <b>${engine.time_scale().toFixed(0)}×</b> ` +
        `(fast-forward — <kbd>[</kbd>/<kbd>]</kbd>) &nbsp;·&nbsp; <kbd>Space</kbd> re-drop`;
    };

    const frame = () => {
      if (!userInteracted) cam.yaw += 0.0025;
      engine.set_orbit(cam.yaw, cam.pitch, cam.zoom);
      try {
        engine.render();
      } catch (e) {
        fail(`render error: ${String(e)}`);
        return; // stop the loop on a hard error
      }
      updateStats();
      requestAnimationFrame(frame);
    };
    requestAnimationFrame(frame);
  } catch (e) {
    fail(`Failed to start engine: ${String(e)}`);
  }
}

void main();
