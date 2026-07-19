// The canonical Sim HUD — ONE shared overlay used by EVERY scene (terrain, space, birth, twomoons).
//
// Robin: the HUD never became universal — each scene rolled its own, with different fields, order and
// styling, and the terrain one didn't even show the build number. This module fixes that: the banner,
// the window frame/styling, and the UNIVERSAL SIM LINE (time × / fps / build / scale) are byte-for-byte
// identical on every screen. Only the scene-specific physics content differs — which is honest, because
// different scenes genuinely report different things (a probe's altitude vs a proto-lunar disk's mass).
//
// It owns two existing DOM elements (same in every scene's HTML): #hud (the upper-left banner) and
// #stats (the lower-left window). A scene builds its per-frame content and hands it to `update()`; the
// module renders the shared frame around it and computes the live scale bar from the camera.

const BUILD = __BUILD_ID__;

/** One frame's worth of HUD content. The scene supplies its own physics/event lines; the module owns
 *  the banner, the window frame, the universal sim line (time/fps/build/scale) and the controls slot. */
export interface SimHudFrame {
  /** Line 1 of the window: scene title + the bodies in view (HTML). Scene supplies content, shared slot. */
  title: string;
  /** Scene-specific physics lines (HTML), rendered in order under the title. */
  physics: string[];
  /** Timescale multiplier for the universal sim line (`time ×N`). */
  timeScale: number;
  /** Measured frames per second for the universal sim line. */
  fps: number;
  /** World metres per DEVICE pixel at the focal plane (the wasm `meters_per_pixel()` getter). Drives
   *  the scale bar. The module converts to CSS pixels itself. */
  metersPerPixel: number;
  /** The controls line (HTML) — how to drive this scene. */
  controls: string;
  /** Optional event lines (HTML) — IMPACT / countdown / T+ / disk stats. Rendered last. */
  events?: string[];
}

export interface SimHud {
  update(frame: SimHudFrame): void;
  /** Toggle a centered crosshair overlay (hook for a future Meteor-Deployment-Prep mode). Off by default. */
  setCrosshair(on: boolean): void;
}

const AU_M = 1.496e11; // metres in one astronomical unit (Earth–Sun distance)

// Device-pixel ratio used to size the canvas backing store (mirrors sizeCanvas in main.ts/orbit.ts).
// meters_per_pixel is metres per DEVICE pixel; the on-screen bar is measured in CSS pixels, so
// metres-per-CSS-pixel = metersPerPixel · dpr.
const dpr = (): number => Math.min(window.devicePixelRatio || 1, 2);

/** Round a positive number DOWN to the nearest 1/2/5 × 10ⁿ — a map-style "nice" scale value. */
function niceRound(x: number): number {
  if (!(x > 0) || !isFinite(x)) return 0;
  const pow = Math.pow(10, Math.floor(Math.log10(x)));
  const f = x / pow;
  const nf = f >= 5 ? 5 : f >= 2 ? 2 : 1;
  return nf * pow;
}

/** Compute a live scale bar from the camera's metres-per-pixel: a bar of known screen length labelled
 *  with the round world distance it represents, unit auto-selected by magnitude (m → km → AU). Honest —
 *  it reflects the ACTUAL rendered scale, so it changes as you zoom. */
function scaleBar(metersPerPixel: number): { barPx: number; label: string } {
  const mppCss = metersPerPixel * dpr(); // metres per on-screen (CSS) pixel
  if (!(mppCss > 0) || !isFinite(mppCss)) return { barPx: 0, label: "—" };
  const targetPx = 84; // aim for a bar ~this wide, then snap to a round world distance
  const rawWorld = mppCss * targetPx; // metres the target bar would span
  // Pick the unit by magnitude: metres at the surface, km between, AU at solar-system scale.
  let unitM: number;
  let unit: string;
  if (rawWorld >= 0.1 * AU_M) {
    unitM = AU_M;
    unit = "AU";
  } else if (rawWorld >= 1000) {
    unitM = 1000;
    unit = "km";
  } else {
    unitM = 1;
    unit = "m";
  }
  const nice = niceRound(rawWorld / unitM); // round distance in the chosen unit
  const worldM = nice * unitM;
  const barPx = worldM / mppCss; // exact pixel length for that round distance
  const num = nice >= 100 ? nice.toLocaleString(undefined, { maximumFractionDigits: 0 }) : String(nice);
  return { barPx, label: `${num} ${unit}` };
}

/** Build the universal sim line — BYTE-IDENTICAL layout on every scene:
 *  `time ×<N> · <F> fps · build <build id> · scale <SCALE BAR>`. This is the canonical part Robin wants
 *  uniform: timescale, fps, version, and the live scale. */
function simLine(frame: SimHudFrame): string {
  const n = Math.round(frame.timeScale).toLocaleString();
  const { barPx, label } = scaleBar(frame.metersPerPixel);
  // A classic map scale bar: a bracket (bottom edge + two end ticks) of exact pixel length, then the
  // round world distance it spans. currentColor keeps it consistent with the window text.
  const bar =
    `<span style="display:inline-block;width:${barPx.toFixed(0)}px;height:5px;` +
    `border:2px solid currentColor;border-top:none;vertical-align:middle;margin:0 5px;"></span>`;
  return `time ×<b>${n}</b> · <b>${frame.fps}</b> fps · build <b>${BUILD}</b> · scale${bar}<b>${label}</b>`;
}

/** Create the one canonical Sim HUD for a scene. `sceneName` fills the shared upper-left banner:
 *  `Integrity · <scene name> · build <build id>`. */
export function createSimHud(sceneName: string): SimHud {
  // Banner (upper-left) — identical structure every scene. Stamped immediately so a stale cache shows
  // the wrong build at a glance, before the first frame even renders.
  const hudEl = document.getElementById("hud");
  if (hudEl) hudEl.textContent = `Integrity · ${sceneName} · build ${BUILD}`;

  const statsEl = document.getElementById("stats");

  // Crosshair overlay — off by default; a scene flips it on (e.g. the future Meteor-Deployment-Prep
  // mode) via setCrosshair(true). Pure overlay, pointer-transparent, centered on the viewport.
  let crosshairEl: HTMLDivElement | null = null;
  const ensureCrosshair = (): HTMLDivElement => {
    if (crosshairEl) return crosshairEl;
    const el = document.createElement("div");
    el.id = "sim-crosshair";
    el.hidden = true;
    Object.assign(el.style, {
      position: "fixed",
      left: "50%",
      top: "50%",
      transform: "translate(-50%, -50%)",
      zIndex: "15",
      pointerEvents: "none",
      width: "42px",
      height: "42px",
    });
    // Two thin lines crossing at centre, with a small gap in the middle so the aim point stays visible.
    el.innerHTML =
      `<div style="position:absolute;left:50%;top:0;width:2px;height:16px;` +
      `background:rgba(230,240,255,0.85);transform:translateX(-50%);"></div>` +
      `<div style="position:absolute;left:50%;bottom:0;width:2px;height:16px;` +
      `background:rgba(230,240,255,0.85);transform:translateX(-50%);"></div>` +
      `<div style="position:absolute;top:50%;left:0;height:2px;width:16px;` +
      `background:rgba(230,240,255,0.85);transform:translateY(-50%);"></div>` +
      `<div style="position:absolute;top:50%;right:0;height:2px;width:16px;` +
      `background:rgba(230,240,255,0.85);transform:translateY(-50%);"></div>`;
    document.body.appendChild(el);
    crosshairEl = el;
    return el;
  };

  return {
    update(frame: SimHudFrame): void {
      if (!statsEl) return;
      // The shared window: title, then scene physics, then the UNIVERSAL sim line, then controls, then
      // any event lines — the same frame on every screen; only the physics/event text differs by scene.
      const lines: string[] = [frame.title, ...frame.physics, simLine(frame), frame.controls];
      if (frame.events && frame.events.length) lines.push(...frame.events);
      statsEl.innerHTML = lines.join("<br>");
    },
    setCrosshair(on: boolean): void {
      ensureCrosshair().hidden = !on;
    },
  };
}
