// docs/43 — worlds-as-data host. The scene is defined by a DATA world file (named in <body data-world>);
// this thin host fetches it, hands it to the engine's `Terra` scene, and drives the render loop. Phase 1 uses
// an orbit camera (drag / wheel-zoom); the continuous fly camera (WASD + zoom + look) lands in Phase 4.

import init, { Terra } from "./wasm/engine.js";
import "./scene-nav";
import { createShareView } from "./share-view";
import { createSimHud } from "./sim-hud";
import { attachCameraInput, CAMERA_HINT } from "./camera-input";

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
    const world = JSON.parse(worldJson);
    const base = worldUrl.slice(0, worldUrl.lastIndexOf("/") + 1);

    // Decode a surface raster PNG → raw RGBA bytes (ImageBitmap → OffscreenCanvas → getImageData) for the engine.
    type Raster = { data: Uint8Array; w: number; h: number };
    async function decode(url?: string): Promise<Raster> {
      if (!url) return { data: new Uint8Array(0), w: 0, h: 0 };
      const bmp = await fetch(base + url)
        .then((r) => r.blob())
        .then((b) => createImageBitmap(b));
      const cv = new OffscreenCanvas(bmp.width, bmp.height);
      const ctx = cv.getContext("2d", { willReadFrequently: true }) as OffscreenCanvasRenderingContext2D;
      ctx.drawImage(bmp, 0, 0);
      const img = ctx.getImageData(0, 0, bmp.width, bmp.height);
      return { data: new Uint8Array(img.data.buffer.slice(0)), w: bmp.width, h: bmp.height };
    }
    setStatus("Loading surface rasters…");
    const s = world.surface ?? {};
    const [lm, ev, lc] = await Promise.all([
      decode(s.landmask_url),
      decode(s.elevation_url),
      decode(s.landcover_url),
    ]);
    report("info", `rasters: land ${lm.w}x${lm.h}, elev ${ev.w}x${ev.h}, cover ${lc.w}x${lc.h}`);

    setStatus("Requesting GPU device…");
    const terra = await Terra.create(canvas);
    terra.load_world(worldJson, lm.data, lm.w, lm.h, ev.data, ev.w, ev.h, lc.data, lc.w, lc.h);
    hideStatus();
    report("info", `Terra world loaded: ${terra.world_name()}`);
    (window as unknown as { __terra?: Terra }).__terra = terra;

    const stats = document.getElementById("stats");
    if (stats) stats.hidden = false;

    // --- Continuous fly camera + data-driven controls (Phase 6). The engine's fly camera blends orbit⇄ground by
    // altitude; the KEY BINDINGS come from the world file (`controls.keys`: code → action), not hardcoded here —
    // this is the worlds-as-data controls contract (docs/43). Actions: forward/back/left/right (move), up/down
    // (climb/descend). Wheel = zoom(=altitude); drag = orbit high / free-look low.
    type Action = "forward" | "back" | "left" | "right" | "up" | "down";
    const codeAction = new Map<string, Action>();
    for (const k of (world.controls?.keys ?? []) as Array<{ code?: string; action?: string }>) {
      if (k?.code && k?.action) codeAction.set(k.code, k.action as Action);
    }
    const held = new Set<string>();
    const active = (a: Action): boolean => {
      for (const [code, act] of codeAction) if (act === a && held.has(code)) return true;
      return false;
    };
    window.addEventListener("keydown", (e) => {
      if (codeAction.has(e.code)) {
        held.add(e.code);
        e.preventDefault();
      }
    });
    window.addEventListener("keyup", (e) => held.delete(e.code));
    window.addEventListener("blur", () => held.clear());
    // A controls hint derived from the actual bindings (so it stays true to the world file).
    const keyFor = (a: Action): string => {
      for (const [code, act] of codeAction) if (act === a) return code.replace(/^Key/, "");
      return "";
    };
    const moveHint = ["forward", "left", "back", "right"].map((a) => keyFor(a as Action)).join("");
    const altHint = [keyFor("up"), keyFor("down")].filter(Boolean).join("/");
    const controlsHint =
      `${moveHint ? `${moveHint} fly · ` : ""}${altHint ? `${altHint} alt · ` : ""}wheel zoom · ${CAMERA_HINT}`;

    // THE shared camera controls (camera-input.ts): right-drag / alt-drag looks, left-or-ctrl walks
    // forward, +shift reverses. Terra's own `drag_look` and `move_tangent` do the work; the gesture
    // grammar is identical to every other scene.
    const cam = attachCameraInput(canvas, (dyaw, dpitch) => {
      // `drag_look` takes pixel deltas; the module reports radians, so convert back through its own
      // sensitivity rather than inventing a second constant.
      terra.drag_look(-dyaw / 0.005, -dpitch / 0.005);
    });
    canvas.addEventListener(
      "wheel",
      (e) => {
        e.preventDefault();
        terra.zoom_alt(e.deltaY * 0.01); // scroll down → climb (zoom out); scroll up → descend (zoom in)
      },
      { passive: false },
    );

    window.addEventListener("resize", () => {
      sizeCanvas(canvas);
      terra.resize(canvas.width, canvas.height);
    });

    const fmtAlt = (m: number) => (m >= 1000 ? `${(m / 1000).toFixed(m >= 100000 ? 0 : 1)} km` : `${m.toFixed(0)} m`);
    let firstFrame = true;
    let fps = 0;
    let lastT = performance.now();
    // Share view — the same module every scene uses.
    const share = createShareView(canvas, {
      onStatus: (m, bad) => setStatus(m, bad),
    });
    const shareSlot = document.createElement("div");
    Object.assign(shareSlot.style, { position: "fixed", left: "16px", bottom: "16px", zIndex: "5" });
    shareSlot.appendChild(share.button);
    document.body.appendChild(shareSlot);

    const hud = createSimHud("earth");
    const frame = () => {
      // Held keys → move/altitude intents (the engine scales the step by altitude). Fully data-driven.
      const fwd = (active("forward") ? 1 : 0) - (active("back") ? 1 : 0);
      const right = (active("right") ? 1 : 0) - (active("left") ? 1 : 0);
      const climb = (active("up") ? 1 : 0) - (active("down") ? 1 : 0);
      // Keyboard intents plus the shared pointer scheme; both feed the same mover.
      const walk = fwd + cam.forward();
      if (walk !== 0 || right !== 0) terra.move_tangent(walk, right);
      if (climb !== 0) terra.zoom_alt(climb * 0.35); // ~4%/frame altitude change while held
      try {
        terra.render();
      } catch (err) {
        setStatus(`render error: ${String(err)}`, true);
        return;
      }
      share.afterPresent(); // while the WebGPU drawing buffer is still current
      const now = performance.now();
      const dt = now - lastT;
      lastT = now;
      if (dt > 0) fps = fps === 0 ? 1000 / dt : fps * 0.9 + (1000 / dt) * 0.1;
      if (stats) {
        // The SHARED HUD, like every other scene. Terra was the only one writing `stats.innerHTML`
        // itself, which is why it showed no BUILD STAMP — and without that you cannot tell whether what
        // you are looking at is the build you just deployed.
        hud.update({
          title: `<b>${terra.world_name()}</b>`,
          physics: [
            `alt <b>${fmtAlt(terra.altitude_m())}</b> · lat <b>${terra.latitude().toFixed(2)}°</b> ` +
              `lon <b>${terra.longitude().toFixed(2)}°</b>`,
            `standing on <b>${terra.ground_biome()}</b>`,
          ],
          timeScale: 1,
          fps: Math.round(fps),
          metersPerPixel: 0,
          controls: controlsHint,
        });
      }
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
