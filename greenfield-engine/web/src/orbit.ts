// Space-band host (scale-relative "orbit-to-ground", Step A).
//
// Loads the Rust/WASM core and drives the OrbitDemo: the real Earth + Moon, positioned by the
// validated N-body physics (orbit.rs). Camera-only input (drag orbit, pinch/wheel zoom) — this band
// is a spectator view of celestial motion. Mirrors main.ts's log relay + status banner so a
// console-less device (iPad) can still be debugged.

import init, { OrbitDemo } from "./wasm/engine.js";
import "./scene-nav";
import { createSimHud } from "./sim-hud";

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
  report("info", `build ${__BUILD_ID__}`);
  report("info", `UA: ${navigator.userAgent}`);
  report("info", `secureContext=${window.isSecureContext} · gpu in navigator=${"gpu" in navigator}`);
  // The scene identity comes from the page. The DEORBIT scenes (Space, Two Moons) are now DATA worlds
  // (docs/43, worlds-as-data): `<body data-world="…/world.json">` names an N-body "system" world that the
  // engine loads. Birth of the Moon stays on the code path (`data-scene="birth"` → GPU SPH impact). Resolve
  // the identity now so the banner stamps before WASM loads (a stale Safari cache is obvious at a glance).
  const worldUrl = document.body.getAttribute("data-world");
  const birthScene = document.body.getAttribute("data-scene") === "birth";
  let world: { name?: string; bodies?: Array<{ role?: string }> } | null = null;
  let worldJson = "";
  if (worldUrl) {
    try {
      worldJson = await fetch(worldUrl).then((r) => {
        if (!r.ok) throw new Error(`world fetch ${worldUrl} → HTTP ${r.status}`);
        return r.text();
      });
      world = JSON.parse(worldJson);
    } catch (e) {
      report("error", `world load failed, falling back to defaults: ${String(e)}`);
    }
  }
  const numMoons = world
    ? (world.bodies ?? []).filter((b) => b.role === "moon").length || 1
    : Number(document.body.getAttribute("data-moons")) || 1;
  const sceneName =
    world?.name?.toLowerCase() ?? (birthScene ? "birth of the moon" : numMoons === 2 ? "two moons" : "space");
  const hud = createSimHud(sceneName);

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
    // In DEV the wasm has a STABLE url (src/wasm/engine_bg.wasm) that Safari caches indefinitely, so a
    // rebuild's new bytes never load. Append the build stamp to force a fresh fetch. In BUILD the wasm is
    // content-hashed already, so let the glue use its default (hashed) url — a ?query there would 404.
    await init(
      import.meta.env.DEV
        ? new URL(`./wasm/engine_bg.wasm?v=${__BUILD_ID__}`, import.meta.url)
        : undefined,
    );
    setStatus("Requesting GPU device…");
    // Moon count / birth flag were resolved from the page above (orbit.html and twomoons.html share
    // this one script via <body data-moons>/<body data-scene>).
    const demo = await OrbitDemo.create(canvas, numMoons);
    // docs/43: for a DATA world (the deorbit scenes), hand the engine the JSON — it replaces the built-in
    // Sun/Earth/Moon constants with the world's declared initial conditions, spin, tints, time scale, and
    // orbit-camera framing. `create` above was given the world's moon count so the GPU per-moon buffers match.
    if (world && worldJson) {
      demo.load_world(worldJson);
      report("info", `loaded system world: ${world.name ?? "?"}`);
    }
    // Birth of the Moon: THE scene is now the GPU SPH deformable-Earth giant impact (docs/33 stage 5, docs/41)
    // — two differentiated bodies, stepped by sph_step.wgsl in-browser, forming a rotationally-SUSTAINED disk
    // (the docs/41 spin fix) that accretes a Moon. The relax runs non-blocking in chunks (the phase machine),
    // so it auto-starts on load. The old CPU-Aggregate impact (`start_birth`) is retired.
    if (birthScene) demo.start_gpu_impact();
    hideStatus();
    const stats = document.getElementById("stats");
    if (stats) stats.hidden = false;
    report("info", "orbit demo created OK");
    // Rig-watch / debug handle (docs/33 stage 4c.4): lets a headless driver call demo.start_gpu_impact().
    (window as unknown as { __demo?: OrbitDemo }).__demo = demo;

    // --- Control bar: frame of reference + the orbital-decay experiment + time control ---
    // Controls live on the LEFT, stacked vertically — the bottom bar overlapped the simulation
    // readout (Robin).
    const bar = document.createElement("div");
    Object.assign(bar.style, {
      position: "fixed",
      left: "12px",
      top: "50%",
      transform: "translateY(-50%)",
      zIndex: "10",
      display: "flex",
      flexDirection: "column",
      gap: "6px",
      alignItems: "stretch",
      maxHeight: "90vh",
    });
    // Button feedback: a hover lift, a pressed (held) state, and a brief accent flash on click so a tap
    // visibly registers — the controls fire a one-shot action (Geologic, Replay) with no other on-screen
    // acknowledgement, so without this you can't tell a click landed.
    const btnStyle = document.createElement("style");
    btnStyle.textContent = `
      .gf-btn { transition: background 120ms, transform 80ms, box-shadow 120ms; }
      .gf-btn:hover { background: rgba(38,46,74,0.82) !important; }
      .gf-btn:active { transform: scale(0.96); }
      .gf-btn.gf-flash { background: rgba(90,150,255,0.9) !important;
        box-shadow: 0 0 0 2px rgba(120,170,255,0.7), 0 0 14px rgba(90,150,255,0.6); }
    `;
    document.head.appendChild(btnStyle);
    const mkBtn = (label: string, onClick: () => void): HTMLButtonElement => {
      const b = document.createElement("button");
      b.className = "gf-btn";
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
      b.addEventListener("click", () => {
        b.classList.add("gf-flash");
        setTimeout(() => b.classList.remove("gf-flash"), 180);
        onClick();
      });
      bar.appendChild(b);
      return b;
    };

    // The viewport is a physical frame of reference (docs/17): the camera rides a body, so we can watch
    // the encounter from either standpoint. "Camera on Moon" frames the impact site once it shatters.
    let wantShot = false;
    // 👁 (eye) not 📷 (camera): these set the camera's FRAME OF REFERENCE (whose eyes we watch from); the
    // camera icon is reserved for "Share view" (capture the screen). Distinct icons so the two don't blur.
    const camEarth = mkBtn("👁 Earth", () => demo.focus_earth());
    const camMoon = mkBtn("👁 Luna", () => demo.focus_moon());
    // Share view: upload exactly what's on screen (the canvas) so the agent can look at the debris swarm /
    // disk. Grabbed in the render loop right after present; POSTed to /__shot (dev server, or the deployed
    // shot receiver proxied at /__shot).
    mkBtn("📷 Share view", () => {
      wantShot = true;
      setStatus("capturing view…");
    });
    void camEarth;
    void camMoon;

    // docs/42 render-layer slider: cross-fade the PRETTY render (sphere/atmosphere) ⇄ the raw PHYSICS particles.
    // The physics underneath is always the real GPU SPH impact; this only changes how it's drawn. Birth scene only.
    if (birthScene) {
      const slot = document.createElement("div");
      Object.assign(slot.style, {
        display: "flex", flexDirection: "column", gap: "3px",
        padding: "8px 11px", color: "#fff",
        font: "600 12px/1.2 system-ui, sans-serif",
        background: "rgba(20,24,40,0.72)", border: "1px solid rgba(255,255,255,0.25)",
        borderRadius: "10px", backdropFilter: "blur(6px)",
      });
      const lbl = document.createElement("div");
      lbl.textContent = "Pretty ⇄ Physics";
      const slider = document.createElement("input");
      slider.type = "range";
      slider.min = "0"; slider.max = "100"; slider.value = "0"; // pretty by default (docs/42)
      slider.style.width = "128px";
      slider.style.cursor = "pointer";
      slider.addEventListener("input", () => demo.set_render_blend(Number(slider.value) / 100));
      slot.append(lbl, slider);
      bar.appendChild(slot);
    }

    // Zoom slider — a reliable direct-value control. Under heavy GPU load the browser coalesces/drops wheel
    // events, so scroll-zoom becomes unreliable; a range input always registers. Universal to every scene.
    // Log-mapped over the camera's zoom range (left = out/far, right = in/close), and kept two-way in sync
    // with wheel/pinch/follow (updated from cam.zoom each frame, unless the user is dragging it).
    const ZMIN = 0.05, ZMAX = 6;
    const zoomToSlider = (z: number): number => 100 * Math.log(z / ZMAX) / Math.log(ZMIN / ZMAX);
    const sliderToZoom = (v: number): number => ZMAX * Math.pow(ZMIN / ZMAX, v / 100);
    let zoomDragging = false;
    const zoomSlider = document.createElement("input");
    {
      const slot = document.createElement("div");
      Object.assign(slot.style, {
        display: "flex", flexDirection: "column", gap: "3px",
        padding: "8px 11px", color: "#fff",
        font: "600 12px/1.2 system-ui, sans-serif",
        background: "rgba(20,24,40,0.72)", border: "1px solid rgba(255,255,255,0.25)",
        borderRadius: "10px", backdropFilter: "blur(6px)",
      });
      const lbl = document.createElement("div");
      lbl.textContent = "Zoom  out ⇄ in";
      zoomSlider.type = "range";
      zoomSlider.min = "0"; zoomSlider.max = "100"; zoomSlider.value = "50";
      zoomSlider.style.width = "128px";
      zoomSlider.style.cursor = "pointer";
      zoomSlider.style.touchAction = "none"; // let the slider own the drag under load, not the page
      zoomSlider.addEventListener("input", () => {
        cam.zoom = sliderToZoom(Number(zoomSlider.value));
        followMoon = false; // manual zoom takes the camera over (like wheel/pinch)
        userInteracted = true;
      });
      zoomSlider.addEventListener("pointerdown", () => { zoomDragging = true; });
      window.addEventListener("pointerup", () => { zoomDragging = false; });
      slot.append(lbl, zoomSlider);
      bar.appendChild(slot);
    }

    // (The GPU deformable-Earth impact is now the DEFAULT birth scene — auto-started on load — so the old
    // "🌋 GPU Impact" trigger button is retired; "Replay" below re-runs it.)

    // Orbital decay: brake the Moon until its orbit crashes into the planet. (The birth scene has no
    // such controls — the encounter IS the scene; Reset replays it.)
    if (!birthScene) {
      mkBtn("Brake Moon ½×", () => demo.brake_moon());
      mkBtn("Drop Moon", () => {
        demo.drop_moon();
        followMoon = true; // ride the descent down
      });
    }
    if (birthScene) {
      // Geologic time (docs/27): retire the particle cloud, evolve the settled moonlets by the
      // validated secular tidal law — millennia per second. Watch the Moon merge and migrate out.
      mkBtn("⏭ Geologic", () => demo.enter_geologic_time());
    }
    mkBtn(birthScene ? "Replay" : "Reset", () => {
      if (birthScene) demo.start_gpu_impact(); // re-run the GPU impact (now the default scene)
      else demo.reset_moon();
      followMoon = true;
    });

    // Variable time multiplier. Before an impact this scales the orbital fast-forward; AFTER an
    // impact it scales the aftermath rate (the disk evolves over months — you need the throttle).
    let timeScale = demo.time_scale_value();
    const applyTime = (): void => demo.set_time_scale(timeScale);
    mkBtn("⏪ slower", () => {
      if (demo.has_impacted()) {
        demo.nudge_aftermath_rate(false);
      } else {
        timeScale = Math.max(1, timeScale / 2);
        applyTime();
      }
    });
    mkBtn("⏩ faster", () => {
      if (demo.has_impacted()) {
        demo.nudge_aftermath_rate(true);
      } else {
        timeScale = Math.min(2_000_000, timeScale * 2);
        applyTime();
      }
    });

    document.body.appendChild(bar);

    window.addEventListener("resize", () => {
      sizeCanvas(canvas);
      demo.resize(canvas.width, canvas.height);
    });

    // --- Camera-only input (pointer events cover mouse + touch) ---
    // Descent-follow camera (pure camera work — it only READS the physics): start wide enough to watch
    // the Moon orbit/deorbit (zoom 1 = 1.7× lunar distance, the whole-orbit framing), then track the
    // Moon's real separation as it falls, flooring at 25% of lunar distance for the impact close-up.
    // Manual zoom (wheel/pinch) takes over; Drop/Reset re-engage the follow.
    const LUNAR_KM = 384_400;
    const CLOSE_ZOOM = 0.25 / 1.7; // the moon-collision's final framing (25% of lunar distance)
    const followZoom = (): number => {
      if (birthScene) {
        // Pre-impact: hold the close framing. Post-impact: ride OUT with the ejecta — view distance
        // tracks the debris extent until it reaches the wide whole-orbit framing (zoom 1).
        const ext = demo.debris_extent_km();
        if (ext <= 0) return CLOSE_ZOOM;
        return Math.max(CLOSE_ZOOM, Math.min(1, (3.0 * ext) / (1.7 * LUNAR_KM)));
      }
      return Math.max(CLOSE_ZOOM, Math.min(1, demo.moon_distance_km() / LUNAR_KM));
    };
    let followMoon = true;
    // Start on the SUN side of the focus body: with the ambient fudge removed, the night side is
    // honestly black — the old default yaw opened on darkness (watched via the rig). Earth sits at +x
    // of the Sun, so an eye direction near −x̂ (yaw ≈ −π/2) sees the lit hemisphere; offsets give a
    // pleasant ¾ lighting.
    const cam = { yaw: -Math.PI / 2 + 0.35, pitch: 0.35, zoom: followZoom() };
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
          cam.zoom = Math.max(0.05, Math.min(6, cam.zoom));
          followMoon = false; // manual zoom takes the camera over
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
        cam.zoom = Math.max(0.05, Math.min(6, cam.zoom));
        followMoon = false; // manual zoom takes the camera over
        userInteracted = true;
      },
      { passive: false },
    );

    // --- Live HUD (the canonical shared Sim HUD — same banner/frame/sim-line as every scene) ---
    const windowTitle = birthScene ? "The Birth of the Moon" : numMoons === 2 ? "Two Moons" : "Space";
    const body3 = birthScene ? "Theia" : "Moon"; // the third body in view
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
      // The aftermath clock (Robin): SIM time since the impact, at the unit the scale deserves —
      // the honest answer to "what timeframe are we watching this over?" (time-LOD ≠ wall time).
      const fmtSim = (s: number): string => {
        const units: [number, string][] = [
          [31_557_600, "y"],
          [2_629_800, "mo"],
          [86_400, "d"],
          [3_600, "h"],
          [60, "m"],
          [1, "s"],
        ];
        let rest = s;
        const parts: string[] = [];
        for (const [sec, label] of units) {
          if (rest >= sec && parts.length < 2) {
            const n = Math.floor(rest / sec);
            parts.push(`${n}${label}`);
            rest -= n * sec;
          }
        }
        return parts.length ? parts.join(" ") : "0s";
      };
      const tPlus = demo.sim_since_impact_s();
      const countdown = demo.impact_countdown_s();
      // Event lines (IMPACT / countdown / T+ aftermath clock) — the scene's own timeline, re-homed into
      // the shared window's event slot. All the same info as before, just below the universal sim line.
      const events: string[] = [];
      if (demo.has_impacted()) {
        events.push(
          `<b style="color:#ffd08a">IMPACT</b> · ${demo.debris_count()} fragments · ` +
            `${(demo.impact_energy_j() / 1e30).toFixed(2)}e30 J`,
        );
      }
      if (tPlus >= 0) {
        let tLine = `<b style="color:#ffd08a">T+${fmtSim(tPlus)}</b> after impact`;
        // Live disk stats — the measured answer to "did we achieve orbit?" (O(n²) + JSON across the
        // wasm boundary: throttled to ~1 Hz; ?nostats disables it entirely for profiling via the rig).
        try {
          if (statsSkip) throw new Error("skipped");
          diskCache ??= demo.disk_stats_json();
          const d = JSON.parse(diskCache) as {
            bound: number; escaped: number; biggest: number; clumps: number; earth: number;
          } | null;
          if (d && d.bound > 0.005) {
            // Provenance of the bound disk (docs/28 step 1): the real Moon is Earth-like, so a disk that
            // is all Theia is the deficit. Shown as ●Theia / ●Earth to match the render's origin tint.
            const earth = d.earth ?? 0;
            const theia = Math.max(0, d.bound - earth);
            tLine +=
              ` · disk <b>${d.bound.toFixed(2)} M☾</b> in <b>${d.clumps}</b> moonlet${d.clumps === 1 ? "" : "s"}` +
              ` · biggest <b>${d.biggest.toFixed(2)} M☾</b>` +
              ` · origin <b style="color:#ff9a52">${theia.toFixed(2)} Theia</b> / <b style="color:#6aa0ff">${earth.toFixed(2)} Earth</b>` +
              (d.escaped > 0.005 ? ` · escaped ${d.escaped.toFixed(2)} M☾` : "");
          }
        } catch { /* stats unavailable */ }
        events.push(tLine);
      } else if (birthScene && countdown >= 0) {
        events.push(`<b style="color:#ff8a8a; font-size:16px">IMPACT IN T−${countdown.toFixed(1)} s</b>`);
      }
      // Scene-specific physics lines (bodies distance/speed, orbit/impact state, Earth's day).
      const physics: string[] = [
        `Earth–${body3} <b>${demo.moon_distance_km().toFixed(0)}</b> km · v <b>${demo.moon_speed_kms().toFixed(2)}</b> km/s`,
        line2,
      ];
      if (demo.earth_day_hours() > 0) {
        physics.push(`Earth day <b>${demo.earth_day_hours().toFixed(1)} h</b>`);
      }
      // GPU SPH impact (docs/33 stage 5): live disk provenance from the read-back particle field.
      const gpuDisk = demo.gpu_disk_stats_json();
      if (gpuDisk !== "null") {
        const g = JSON.parse(gpuDisk) as { disk: number; earth_pct: number; moon: number };
        physics.push(
          `GPU impact · disk <b>${g.disk.toFixed(2)}</b> M☾ (<b>${g.earth_pct}%</b> Earth) · moon <b>${g.moon.toFixed(2)}</b> M☾`,
        );
      }
      hud.update({
        title: `<b>${windowTitle}</b> · Sun · Earth · ${body3} · frame <b>${demo.focus_label()}</b>`,
        physics,
        timeScale: demo.time_scale_value(),
        fps,
        metersPerPixel: demo.meters_per_pixel(),
        controls: `drag orbit · pinch / wheel zoom · buttons ↖`,
        events,
      });
    };

    const statsSkip = new URLSearchParams(location.search).has("nostats");
    let diskCache: string | null = null;
    setInterval(() => { diskCache = null; }, 1000); // refresh the disk stats at 1 Hz
    let firstFrame = true;
    let lastFrameT = performance.now();
    const frame = () => {
      framesSinceFps++;
      const nowT = performance.now();
      if (nowT - lastFpsTime >= 500) {
        fps = Math.round((framesSinceFps * 1000) / (nowT - lastFpsTime));
        framesSinceFps = 0;
        lastFpsTime = nowT;
      }
      // Physics/render decoupling: advance the PHYSICS by real wall-clock time (frame-rate independent
      // — a 30 fps client simulates the same world as a 120 fps one); the render then samples the state
      // ~100 ms behind, so every collision it draws is already resolved.
      const dtS = (nowT - lastFrameT) / 1000;
      lastFrameT = nowT;
      const __t0 = performance.now();
      demo.advance(dtS);
      const __t1 = performance.now();
      (window as unknown as { __adv: number[] }).__adv ??= [];
      (window as unknown as { __adv: number[] }).__adv.push(__t1 - __t0);
      if (!userInteracted) cam.yaw += 0.0015; // gentle idle drift
      if (followMoon) {
        // Ease toward the follow target so re-engaging (Drop/Reset) glides instead of jump-cutting.
        cam.zoom += (followZoom() - cam.zoom) * 0.08;
      }
      demo.set_orbit(cam.yaw, cam.pitch, cam.zoom);
      // Keep the zoom slider in sync with wheel/pinch/follow-driven zoom (unless the user is dragging it).
      if (!zoomDragging) zoomSlider.value = String(zoomToSlider(cam.zoom));
      try {
        const __r0 = performance.now();
        demo.render();
        const __r1 = performance.now();
        (window as unknown as { __ren: number[] }).__ren ??= [];
        (window as unknown as { __ren: number[] }).__ren.push(__r1 - __r0);
      } catch (err) {
        setStatus(`render error: ${String(err)}`, true);
        return;
      }
      // Share view: capture the freshly-presented frame and upload it (see the button above).
      if (wantShot) {
        wantShot = false;
        try {
          const url = canvas.toDataURL("image/png");
          void fetch("/__shot", {
            method: "POST",
            headers: { "content-type": "text/plain" },
            body: url,
          })
            .then(() => report("info", `view posted (${url.length} chars)`))
            .catch((e) => report("error", `view upload failed: ${String(e)}`));
        } catch (e) {
          report("error", `view capture failed: ${String(e)} (WebGPU canvas may need readback)`);
        }
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
