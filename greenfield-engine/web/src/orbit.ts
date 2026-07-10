// Space-band host (scale-relative "orbit-to-ground", Step A).
//
// Loads the Rust/WASM core and drives the OrbitDemo: the real Earth + Moon, positioned by the
// validated N-body physics (orbit.rs). Camera-only input (drag orbit, pinch/wheel zoom) — this band
// is a spectator view of celestial motion. Mirrors main.ts's log relay + status banner so a
// console-less device (iPad) can still be debugged.

import init, { OrbitDemo } from "./wasm/engine.js";
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
  canvas.width = Math.max(1, Math.floor(canvas.clientWidth * dpr));
  canvas.height = Math.max(1, Math.floor(canvas.clientHeight * dpr));
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
    // Moon count comes from the page (<body data-moons="2">) so orbit.html and twomoons.html share
    // this one script. Default 1.
    const numMoons = Number(document.body.getAttribute("data-moons")) || 1;
    const demo = await OrbitDemo.create(canvas, numMoons);
    hideStatus();
    const stats = document.getElementById("stats");
    if (stats) stats.hidden = false;
    report("info", "orbit demo created OK");

    // --- Control bar: frame of reference + the orbital-decay experiment + time control ---
    const bar = document.createElement("div");
    Object.assign(bar.style, {
      position: "fixed",
      left: "50%",
      bottom: "12px",
      transform: "translateX(-50%)",
      zIndex: "10",
      display: "flex",
      gap: "6px",
      flexWrap: "wrap",
      justifyContent: "center",
      maxWidth: "96vw",
    });
    const mkBtn = (label: string, onClick: () => void): HTMLButtonElement => {
      const b = document.createElement("button");
      b.textContent = label;
      Object.assign(b.style, {
        padding: "9px 13px",
        font: "600 14px/1 system-ui, sans-serif",
        color: "#fff",
        background: "rgba(20,24,40,0.72)",
        border: "1px solid rgba(255,255,255,0.25)",
        borderRadius: "10px",
        backdropFilter: "blur(6px)",
        cursor: "pointer",
        touchAction: "manipulation",
      });
      b.addEventListener("click", onClick);
      bar.appendChild(b);
      return b;
    };

    // The viewport is a physical frame of reference (docs/17): re-centre on Earth or Moon.
    const focusBtn = mkBtn("", () => {
      demo.cycle_focus();
      focusBtn.textContent = `Focus: ${demo.focus_label()}`;
    });
    focusBtn.textContent = `Focus: ${demo.focus_label()}`;

    // Orbital decay: brake the Moon until its orbit crashes into the planet.
    mkBtn("Brake Moon ½×", () => demo.brake_moon());
    mkBtn("Drop Moon", () => demo.drop_moon());
    mkBtn("Reset", () => demo.reset_moon());

    // Variable time multiplier.
    let timeScale = demo.time_scale_value();
    const applyTime = (): void => demo.set_time_scale(timeScale);
    mkBtn("⏪ slower", () => {
      timeScale = Math.max(1, timeScale / 2);
      applyTime();
    });
    mkBtn("⏩ faster", () => {
      timeScale = Math.min(2_000_000, timeScale * 2);
      applyTime();
    });

    document.body.appendChild(bar);

    window.addEventListener("resize", () => {
      sizeCanvas(canvas);
      demo.resize(canvas.width, canvas.height);
    });

    // --- Camera-only input (pointer events cover mouse + touch) ---
    const cam = { yaw: 0.6, pitch: 0.5, zoom: 1.0 };
    let userInteracted = false;
    let dragging = false;
    let lastX = 0;
    let lastY = 0;
    const active = new Map<number, { x: number; y: number }>();
    let pinchDist = 0;

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
      } else if (active.size === 2) {
        dragging = false;
        pinchDist = twoFingerDist();
      }
      canvas.setPointerCapture(e.pointerId);
    });
    canvas.addEventListener("pointerup", (e) => {
      active.delete(e.pointerId);
      dragging = false;
      canvas.releasePointerCapture(e.pointerId);
    });
    canvas.addEventListener("pointermove", (e) => {
      if (active.has(e.pointerId)) active.set(e.pointerId, { x: e.clientX, y: e.clientY });
      if (active.size === 2) {
        const d = twoFingerDist();
        if (pinchDist > 0 && d > 0) {
          cam.zoom *= pinchDist / d;
          cam.zoom = Math.max(0.25, Math.min(6, cam.zoom));
        }
        pinchDist = d;
        userInteracted = true;
        return;
      }
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
        cam.zoom = Math.max(0.25, Math.min(6, cam.zoom));
        userInteracted = true;
      },
      { passive: false },
    );

    // --- Live HUD ---
    let fps = 0;
    let framesSinceFps = 0;
    let lastFpsTime = performance.now();
    const updateStats = () => {
      if (!stats) return;
      const peri = demo.moon_perigee_km();
      let line2: string;
      if (demo.has_impacted()) {
        const e = demo.impact_energy_j();
        const shatter = Math.round(e / demo.moon_binding_energy_j());
        const earthPct = (100 * e) / demo.earth_binding_energy_j();
        line2 =
          `<b style="color:#ff8a8a">💥 IMPACT — ${e.toExponential(2)} J</b> · ` +
          `~${shatter.toLocaleString()}× the Moon's binding energy (Moon shatters) · ` +
          `${earthPct.toFixed(1)}% of Earth's (survives → planet-scale crater) ` +
          `<span style="opacity:.7">· fragmentation/crater not yet materialised</span>`;
      } else if (peri < 0) {
        line2 = `perigee <b>unbound</b> (would escape)`;
      } else {
        const crash = peri < 8108; // Earth radius + Moon radius, km → surfaces meet
        line2 =
          `perigee <b style="color:${crash ? "#ff8a8a" : "#dfe6ff"}">` +
          `${Math.round(peri).toLocaleString()}</b> km ` +
          `<span style="opacity:.7">(Earth R ≈ 6,371 — brake below this to crash)</span>`;
      }
      stats.innerHTML =
        `<b>Sun · Earth · Moon</b> · frame <b>${demo.focus_label()}</b> · ` +
        `Earth–Moon <b>${demo.moon_distance_km().toFixed(0)}</b> km<br>` +
        `${line2}<br>` +
        `time <b>${Math.round(demo.time_scale_value()).toLocaleString()}×</b> · ` +
        `<b>${fps}</b> fps · drag / pinch`;
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
      if (!userInteracted) cam.yaw += 0.0015; // gentle idle drift
      demo.set_orbit(cam.yaw, cam.pitch, cam.zoom);
      try {
        demo.render();
      } catch (err) {
        setStatus(`render error: ${String(err)}`, true);
        return;
      }
      if (firstFrame) {
        report("info", "first orbit frame rendered OK");
        firstFrame = false;
      }
      updateStats();
      requestAnimationFrame(frame);
    };
    requestAnimationFrame(frame);
  } catch (e) {
    setStatus(`Failed to start orbit demo: ${String(e)}`, true);
  }
}

void main();
