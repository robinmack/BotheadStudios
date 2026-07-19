// docs/43 — worlds-as-data host. The scene is defined by a DATA world file (named in <body data-world>);
// this thin host fetches it, hands it to the engine's `Terra` scene, and drives the render loop. Phase 1 uses
// an orbit camera (drag / wheel-zoom); the continuous fly camera (WASD + zoom + look) lands in Phase 4.

import init, { Terra } from "./wasm/engine.js";
import "./scene-nav";

// --- Log relay: mirror console + errors to the dev server (parity with the other scenes) ---
function report(level: string, msg: string): void {
  try {
    void fetch("/__log", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ level, msg }),
      keepalive: true,
    });
  } catch {
    /* best-effort */
  }
}
(["log", "warn", "error"] as const).forEach((lvl) => {
  const orig = console[lvl].bind(console);
  console[lvl] = (...args: unknown[]) => {
    orig(...args);
    report(lvl, args.map((a) => (typeof a === "string" ? a : JSON.stringify(a))).join(" "));
  };
});
window.addEventListener("error", (e) =>
  report("error", `window.onerror: ${e.message} @ ${e.filename}:${e.lineno}:${e.colno}`),
);
window.addEventListener("unhandledrejection", (e) =>
  report("error", `unhandledrejection: ${String((e as PromiseRejectionEvent).reason)}`),
);

const statusEl = document.getElementById("status");
function setStatus(html: string, isError = false): void {
  if (statusEl) {
    statusEl.innerHTML = html;
    statusEl.className = isError ? "err" : "";
    statusEl.hidden = false;
  }
  report(isError ? "error" : "status", (statusEl?.textContent ?? html).slice(0, 400));
}
function hideStatus(): void {
  if (statusEl) statusEl.hidden = true;
}

function sizeCanvas(canvas: HTMLCanvasElement): void {
  const dpr = Math.min(window.devicePixelRatio || 1, 2);
  canvas.width = Math.max(1, Math.floor(canvas.clientWidth * dpr));
  canvas.height = Math.max(1, Math.floor(canvas.clientHeight * dpr));
}

async function main(): Promise<void> {
  report("info", `build ${__BUILD_ID__}`);
  const worldUrl = document.body.getAttribute("data-world") ?? "/worlds/earth/world.json";

  const canvas = document.getElementById("gpu-canvas") as HTMLCanvasElement | null;
  if (!canvas) {
    setStatus("Canvas element not found.", true);
    return;
  }
  if (!("gpu" in navigator)) {
    setStatus("WebGPU is not available in this browser.", true);
    return;
  }
  sizeCanvas(canvas);

  try {
    setStatus("Loading engine… (compiling WASM)");
    await init(
      import.meta.env.DEV ? new URL(`./wasm/engine_bg.wasm?v=${__BUILD_ID__}`, import.meta.url) : undefined,
    );

    setStatus("Fetching world…");
    const worldJson = await fetch(worldUrl).then((r) => {
      if (!r.ok) throw new Error(`world fetch ${worldUrl} → HTTP ${r.status}`);
      return r.text();
    });

    setStatus("Requesting GPU device…");
    const terra = await Terra.create(canvas);
    terra.load_world(worldJson);
    hideStatus();
    report("info", `Terra world loaded: ${terra.world_name()}`);
    (window as unknown as { __terra?: Terra }).__terra = terra;

    const stats = document.getElementById("stats");
    if (stats) stats.hidden = false;

    // --- Orbit camera (Phase 1; fly camera comes in Phase 4) ---
    const cam = { yaw: 0.6, pitch: 0.35, zoom: 1.0 };
    let userInteracted = false;
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
      userInteracted = true;
    });
    canvas.addEventListener(
      "wheel",
      (e) => {
        e.preventDefault();
        cam.zoom *= Math.exp(e.deltaY * 0.001);
        cam.zoom = Math.max(0.15, Math.min(8, cam.zoom));
        userInteracted = true;
      },
      { passive: false },
    );

    window.addEventListener("resize", () => {
      sizeCanvas(canvas);
      terra.resize(canvas.width, canvas.height);
    });

    let firstFrame = true;
    const frame = () => {
      if (!userInteracted) cam.yaw += 0.0012; // gentle idle spin
      terra.set_orbit(cam.yaw, cam.pitch, cam.zoom);
      try {
        terra.render();
      } catch (err) {
        setStatus(`render error: ${String(err)}`, true);
        return;
      }
      if (stats) stats.innerHTML = `<b>${terra.world_name()}</b> · drag orbit · wheel zoom`;
      if (firstFrame) {
        report("info", "first terra frame rendered OK");
        firstFrame = false;
      }
      requestAnimationFrame(frame);
    };
    requestAnimationFrame(frame);
  } catch (e) {
    setStatus(`Failed to start world: ${String(e)}`, true);
  }
}

void main();
