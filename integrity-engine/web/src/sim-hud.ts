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
// WHAT is in this bundle, not just when it was built — the question asked after every deploy.
const REL = __BUILD_REL__;

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
  /** Add a SCENE-owned widget. The global widgets are mounted by this module and are not reachable here —
   *  a scene cannot forget them because it never places them. */
  add(region: HudRegion, ...els: HTMLElement[]): void;
  /** Declare which camera systems THIS scene offers. The selector appears automatically when there is
   *  more than one, and enforces the one rule that keeps two camera systems Law-abiding: several
   *  producers, ONE sink — engaging one releases the other, so "where is the camera" always has exactly
   *  one answer. Passing fewer than two producers hides the control rather than showing a dead choice. */
  cameras(producers: CameraProducer[]): void;
  /** Switch producers programmatically. Needed because a producer can END ITSELF — Terra's follow camera
   *  releases when the fragment it is riding lands — and the HUD must not go on showing it as engaged. */
  selectCamera(id: string): void;
}

/** Where a widget sits. SEMANTIC, not positional: placement is decided here once, so moving the furniture
 *  does not mean editing every scene. */
export type HudRegion = "nav" | "camera" | "actions" | "status";

/** One way of driving the camera. A scene registers the ones it has; the HUD renders the choice.
 *  `engage` takes control, `release` gives it up — the HUD guarantees exactly one is engaged. */
export interface CameraProducer {
  id: string;
  label: string;
  title?: string;
  engage(): void;
  release(): void;
}

/** Shared widget styling, so no scene hand-rolls it and two buttons cannot drift apart. */
export function hudButton(text: string, title?: string): HTMLButtonElement {
  const b = document.createElement("button");
  b.className = "gf-btn";
  b.textContent = text;
  if (title) b.title = title;
  Object.assign(b.style, {
    padding: "9px 13px",
    font: "600 14px/1 system-ui, sans-serif",
    color: "#fff",
    background: "rgba(20,24,40,0.72)",
    border: "1px solid rgba(255,255,255,0.25)",
    borderRadius: "10px",
    backdropFilter: "blur(6px)",
    cursor: "pointer",
  });
  return b;
}

const REGION_PLACEMENT: Record<HudRegion, Partial<CSSStyleDeclaration>> = {
  nav: { left: "16px", top: "96px" },
  // Choosing a camera system is a MODE. Kept away from the one-shot action buttons on purpose: a mode
  // control sitting among actions gets pressed by accident.
  camera: { right: "16px", top: "16px", alignItems: "flex-end" },
  actions: { left: "16px", bottom: "16px" },
  status: { right: "16px", bottom: "16px", alignItems: "flex-end" },
};

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
  return `time ×<b>${n}</b> · <b>${frame.fps}</b> fps · build <b>${BUILD}</b> <span style="opacity:.7">(${REL})</span> · scale${bar}<b>${label}</b>`;
}

/** Create the one canonical Sim HUD for a scene. `sceneName` fills the shared upper-left banner:
 *  `Integrity · <scene name> · build <build id>`. */
export function createSimHud(sceneName: string): SimHud {
  // Banner (upper-left) — identical structure every scene. Stamped immediately so a stale cache shows
  // the wrong build at a glance, before the first frame even renders.
  const hudEl = document.getElementById("hud");
  if (hudEl) hudEl.textContent = `Integrity · ${sceneName} · build ${BUILD} (${REL})`;

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

  // ---- The widget layer -------------------------------------------------------------------------
  //
  // The readout above has always been shared. The interactive widgets were NOT: each scene hand-rolled a
  // `position:fixed` slot and remembered to place the Share-view button, so "every scene has a Share view
  // button" (CLAUDE.md rule 0) was enforced by remembering. It is structural now — a scene gets the global
  // widgets by calling `createSimHud`, which it already does, and cannot decline them.
  const layer = document.createElement("div");
  layer.id = "sim-hud-widgets";
  // Transparent to the pointer so HUD chrome never eats a canvas drag; each region turns events back on.
  Object.assign(layer.style, { position: "fixed", inset: "0", pointerEvents: "none", zIndex: "6" });
  const regions = {} as Record<HudRegion, HTMLElement>;
  for (const name of Object.keys(REGION_PLACEMENT) as HudRegion[]) {
    const el = document.createElement("div");
    el.dataset.hudRegion = name;
    Object.assign(el.style, {
      position: "fixed",
      display: "flex",
      flexDirection: "column",
      gap: "8px",
      pointerEvents: "auto",
      ...REGION_PLACEMENT[name],
    });
    regions[name] = el;
    layer.appendChild(el);
  }
  document.body.appendChild(layer);

  // GLOBAL widget: the camera selector. Rendered only once a scene declares two or more producers —
  // a selector offering one choice is noise, and a scene with one camera has nothing to select.
  const cameraBox = document.createElement("div");
  Object.assign(cameraBox.style, { display: "flex", gap: "6px" });
  cameraBox.hidden = true;
  regions.camera.appendChild(cameraBox);

  let engaged: CameraProducer | null = null;
  let registered: CameraProducer[] = [];
  let repaint: () => void = () => {};
  /** The ONE switching path — release the current producer, then engage the new one. Both the buttons
   *  and `selectCamera` go through it, so a scene-driven switch cannot skip the release the user-driven
   *  one performs. */
  function selectById(id: string): void {
    const next = registered.find((p) => p.id === id);
    if (!next || engaged?.id === id) return;
    engaged?.release();
    engaged = next;
    next.engage();
    repaint();
  }
  function renderCameras(producers: CameraProducer[]): void {
    registered = producers;
    cameraBox.replaceChildren();
    cameraBox.hidden = producers.length < 2;
    if (producers.length < 2) return;
    const buttons = new Map<string, HTMLButtonElement>();
    repaint = (): void => {
      for (const [id, b] of buttons) {
        const on = engaged?.id === id;
        b.style.background = on ? "rgba(90,150,255,0.85)" : "rgba(20,24,40,0.72)";
        b.style.borderColor = on ? "rgba(160,200,255,0.9)" : "rgba(255,255,255,0.25)";
        b.setAttribute("aria-pressed", String(on));
      }
    };
    const paint = repaint;
    for (const p of producers) {
      const b = hudButton(p.label, p.title);
      b.dataset.cameraProducer = p.id;
      // ONE SINK: `selectById` releases before engaging, always, so "where is the camera" cannot have
      // two answers even for a frame. That constraint is what makes two camera systems Law-abiding.
      b.addEventListener("click", () => selectById(p.id));
      buttons.set(p.id, b);
      cameraBox.appendChild(b);
    }
    // The first producer a scene lists is its default driver; engage it so the HUD never shows "none".
    engaged = producers[0];
    producers[0].engage();
    paint();
  }

  return {
    add(region: HudRegion, ...els: HTMLElement[]): void {
      for (const e of els) regions[region].appendChild(e);
    },
    cameras(producers: CameraProducer[]): void {
      renderCameras(producers);
    },
    selectCamera(id: string): void {
      selectById(id);
    },
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
