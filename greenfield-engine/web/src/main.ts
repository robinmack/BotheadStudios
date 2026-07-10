// Thin browser host for greenfield-engine.
//
// Loads the Rust/WASM core, drives requestAnimationFrame, and handles input. Also mirrors all
// console output + errors to the dev server (POST /__log) so console-less devices (e.g. iPad) can
// be debugged, and shows a big on-screen status/error banner.

import init, { Engine } from "./wasm/engine.js";
import "./scene-nav";

// --- Log relay: mirror console + global errors to the dev server ---
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
    report(
      lvl,
      args.map((a) => (typeof a === "string" ? a : JSON.stringify(a))).join(" "),
    );
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
  const w = Math.max(1, Math.floor(canvas.clientWidth * dpr));
  const h = Math.max(1, Math.floor(canvas.clientHeight * dpr));
  canvas.width = w;
  canvas.height = h;
}

async function main(): Promise<void> {
  report("info", `UA: ${navigator.userAgent}`);
  report("info", `secureContext=${window.isSecureContext} · gpu in navigator=${"gpu" in navigator}`);

  const canvas = document.getElementById("gpu-canvas") as HTMLCanvasElement | null;
  if (!canvas) {
    setStatus("Canvas element not found.", true);
    return;
  }

  if (!("gpu" in navigator)) {
    setStatus(
      "WebGPU is not available in this browser.<br><br>" +
        "On <b>iPad (Safari)</b>: Settings → Apps → Safari → Advanced → Feature Flags → " +
        "turn on <b>WebGPU</b>, then reload. (Needs iPadOS 18+.)<br><br>" +
        "Recent Chrome / Edge / Firefox also work.",
      true,
    );
    return;
  }

  sizeCanvas(canvas);

  try {
    setStatus("Loading engine… (compiling WASM)");
    await init();
    setStatus("Requesting GPU device…");
    const engine = await Engine.create(canvas);
    hideStatus();
    const stats = document.getElementById("stats");
    if (stats) stats.hidden = false;
    report("info", "engine created OK");

    // Meteor button: fire a high-energy impact at screen centre — carves a crater and throws
    // incandescent (glowing) molten ejecta from the hot core (docs/20). Same operator as dig, more
    // energy. (Also bound to the "m" key.)
    const meteorBtn = document.createElement("button");
    meteorBtn.textContent = "☄ Meteor";
    Object.assign(meteorBtn.style, {
      position: "fixed",
      right: "12px",
      bottom: "12px",
      zIndex: "10",
      padding: "10px 14px",
      font: "600 15px/1 system-ui, sans-serif",
      color: "#fff",
      background: "rgba(40,20,16,0.78)",
      border: "1px solid rgba(255,180,120,0.4)",
      borderRadius: "10px",
      backdropFilter: "blur(6px)",
      cursor: "pointer",
      touchAction: "manipulation",
    });
    meteorBtn.addEventListener("click", () => engine.meteor(0, 0));
    document.body.appendChild(meteorBtn);

    // Screenshot: capture the canvas and upload it to the dev server (web/shots/), so on-device visual
    // bugs (e.g. levitating particles) can be shown. The actual grab happens in the frame loop right
    // after render() presents, so the WebGPU drawing buffer is still current.
    let wantShot = false;
    const shotBtn = document.createElement("button");
    shotBtn.textContent = "📷 Shot";
    Object.assign(shotBtn.style, {
      position: "fixed",
      right: "12px",
      bottom: "58px",
      zIndex: "10",
      padding: "10px 14px",
      font: "600 15px/1 system-ui, sans-serif",
      color: "#fff",
      background: "rgba(16,28,40,0.78)",
      border: "1px solid rgba(120,180,255,0.4)",
      borderRadius: "10px",
      backdropFilter: "blur(6px)",
      cursor: "pointer",
      touchAction: "manipulation",
    });
    shotBtn.addEventListener("click", () => {
      wantShot = true;
      setStatus("capturing screenshot…");
      window.setTimeout(hideStatus, 1200);
    });
    document.body.appendChild(shotBtn);
    report(
      "info",
      `canvas ${canvas.width}x${canvas.height} client ${canvas.clientWidth}x${canvas.clientHeight} dpr ${window.devicePixelRatio}`,
    );

    window.addEventListener("resize", () => {
      sizeCanvas(canvas);
      engine.resize(canvas.width, canvas.height);
    });

    // --- Camera + input (pointer events cover mouse and touch) ---
    const cam = { yaw: 0.7, pitch: 0.5, zoom: 1.0 };
    let userInteracted = false;
    let dragging = false;
    let lastX = 0;
    let lastY = 0;
    let moved = 0;
    const active = new Map<number, { x: number; y: number }>();
    let pinchDist = 0;
    const LONG_PRESS_MS = 450;
    let longPressTimer = 0;
    let didLongPress = false;

    const twoFingerDist = (): number => {
      const pts = [...active.values()];
      return pts.length < 2 ? 0 : Math.hypot(pts[0].x - pts[1].x, pts[0].y - pts[1].y);
    };

    canvas.addEventListener("pointerdown", (e) => {
      active.set(e.pointerId, { x: e.clientX, y: e.clientY });
      if (active.size === 1) {
        dragging = true;
        lastX = e.clientX;
        lastY = e.clientY;
        moved = 0;
        // Long-press (hold still) = blast — the touch equivalent of shift-click (breaks rock).
        didLongPress = false;
        const dx = e.clientX;
        const dy = e.clientY;
        longPressTimer = window.setTimeout(() => {
          if (dragging && moved < 8) {
            const rect = canvas.getBoundingClientRect();
            const ndcX = ((dx - rect.left) / rect.width) * 2 - 1;
            const ndcY = 1 - ((dy - rect.top) / rect.height) * 2;
            engine.dig(ndcX, ndcY, true);
            didLongPress = true;
          }
        }, LONG_PRESS_MS);
      } else if (active.size === 2) {
        dragging = false;
        pinchDist = twoFingerDist();
        clearTimeout(longPressTimer);
      }
      canvas.setPointerCapture(e.pointerId);
    });
    canvas.addEventListener("pointerup", (e) => {
      const wasSingle = active.size === 1;
      active.delete(e.pointerId);
      dragging = false;
      canvas.releasePointerCapture(e.pointerId);
      clearTimeout(longPressTimer);
      // Quick stationary tap = dig (blast if shift held); a long-press already blasted; a drag orbits.
      if (wasSingle && !didLongPress && moved < 8) {
        const rect = canvas.getBoundingClientRect();
        const ndcX = ((e.clientX - rect.left) / rect.width) * 2 - 1;
        const ndcY = 1 - ((e.clientY - rect.top) / rect.height) * 2;
        engine.dig(ndcX, ndcY, e.shiftKey);
      }
    });
    canvas.addEventListener("pointermove", (e) => {
      if (active.has(e.pointerId)) active.set(e.pointerId, { x: e.clientX, y: e.clientY });
      if (active.size === 2) {
        const d = twoFingerDist();
        if (pinchDist > 0 && d > 0) {
          cam.zoom *= pinchDist / d;
          cam.zoom = Math.max(0.3, Math.min(4, cam.zoom));
        }
        pinchDist = d;
        userInteracted = true;
        return;
      }
      if (!dragging) return;
      moved += Math.abs(e.clientX - lastX) + Math.abs(e.clientY - lastY);
      if (moved >= 8) clearTimeout(longPressTimer); // it's a drag, not a long-press
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
        cam.zoom = Math.max(0.3, Math.min(4, cam.zoom));
        userInteracted = true;
      },
      { passive: false },
    );

    window.addEventListener("keydown", (e) => {
      if (e.code === "Space" || e.code === "KeyR") {
        e.preventDefault();
        engine.reset_drop();
      } else if (e.code === "BracketLeft") {
        engine.set_time_scale(engine.time_scale() / 1.5);
      } else if (e.code === "BracketRight") {
        engine.set_time_scale(engine.time_scale() * 1.5);
      } else if (e.code === "KeyM") {
        engine.meteor(0, 0); // fire a meteor at screen centre
      }
    });

    // --- Live HUD ---
    const fmt = (x: number) => x.toExponential(2);
    let fps = 0;
    let framesSinceFps = 0;
    let lastFpsTime = performance.now();
    const updateStats = () => {
      if (!stats) return;
      stats.innerHTML =
        `world mass <b>${fmt(engine.total_mass())}</b> kg · g <b>${fmt(engine.surface_gravity())}</b> m/s² (micro-g)<br>` +
        `probe (iron ball): alt <b>${engine.sphere_altitude().toFixed(1)}</b> m · integrity <b>${(engine.probe_integrity() * 100).toFixed(0)}%</b><br>` +
        `debris <b>${engine.particle_count()}</b> · time ×<b>${engine.time_scale().toFixed(0)}</b> · <b>${fps}</b> fps<br>` +
        `tap dig · long-press blast · ☄/m meteor · drag orbit · pinch zoom`;
    };

    let firstFrame = true;
    const frame = () => {
      framesSinceFps++;
      const nowT = performance.now();
      if (nowT - lastFpsTime >= 500) {
        fps = Math.round((framesSinceFps * 1000) / (nowT - lastFpsTime));
        framesSinceFps = 0;
        lastFpsTime = nowT;
      }
      if (!userInteracted) cam.yaw += 0.0025;
      engine.set_orbit(cam.yaw, cam.pitch, cam.zoom);
      try {
        engine.render();
      } catch (err) {
        setStatus(`render error: ${String(err)}`, true);
        return;
      }
      // Grab the screenshot HERE — immediately after render() presented, while the WebGPU drawing
      // buffer is current — then upload it to the dev server.
      if (wantShot) {
        wantShot = false;
        try {
          const url = canvas.toDataURL("image/png");
          void fetch("/__shot", {
            method: "POST",
            headers: { "content-type": "text/plain" },
            body: url,
          })
            .then(() => report("info", `screenshot posted (${url.length} chars)`))
            .catch((e) => report("error", `screenshot upload failed: ${String(e)}`));
        } catch (e) {
          report("error", `screenshot capture failed: ${String(e)} (WebGPU canvas may need readback)`);
        }
      }
      if (firstFrame) {
        report("info", "first frame rendered OK");
        firstFrame = false;
      }
      updateStats();
      requestAnimationFrame(frame);
    };
    requestAnimationFrame(frame);
  } catch (e) {
    setStatus(`Failed to start engine: ${String(e)}`, true);
  }
}

void main();
