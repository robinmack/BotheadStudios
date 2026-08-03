// docs/43 — worlds-as-data host. The scene is defined by a DATA world file (named in <body data-world>);
// this thin host fetches it, hands it to the engine's `Terra` scene, and drives the render loop. Phase 1 uses
// an orbit camera (drag / wheel-zoom); the continuous fly camera (WASD + zoom + look) lands in Phase 4.

import { report } from "./dev-log"; // FIRST — relay console/errors to the dev terminal before wasm loads
import init, { Terra, body_surface_urls } from "./wasm/engine.js";
import "./scene-nav";
import { createShareView } from "./share-view";
import { createSimHud } from "./sim-hud";
import { attachCameraInput, CAMERA_HINT } from "./camera-input";
import { withBase } from "./base-url";

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
  const worldUrl = withBase(document.body.getAttribute("data-world") ?? "/worlds/earth/world.json");

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
      const bmp = await fetch(url.startsWith("/") ? withBase(url) : base + url)
        .then((r) => r.blob())
        .then((b) => createImageBitmap(b));
      const cv = new OffscreenCanvas(bmp.width, bmp.height);
      const ctx = cv.getContext("2d", { willReadFrequently: true }) as OffscreenCanvasRenderingContext2D;
      ctx.drawImage(bmp, 0, 0);
      const img = ctx.getImageData(0, 0, bmp.width, bmp.height);
      return { data: new Uint8Array(img.data.buffer.slice(0)), w: bmp.width, h: bmp.height };
    }
    setStatus("Loading surface rasters…");
    // Earth's continents come from Earth's DEFINITION, not from this world file — the same rasters the
    // space and impact scenes load, so every scene shows one Earth.
    const urls: string[] = JSON.parse(body_surface_urls(world.body ?? "earth"));
    const [lm, ev, lc] = await Promise.all(urls.map((u) => decode(u)));
    report("info", `rasters: land ${lm.w}x${lm.h}, elev ${ev.w}x${ev.h}, cover ${lc.w}x${lc.h}`);

    setStatus("Requesting GPU device…");
    const terra = await Terra.create(canvas);

    // THE SKY. Real stars at real positions (HYG), shared by every scene — the sky is not a property of a
    // planet, nor of a scene: it is the universe everything here is inside. A scene contributes only where
    // the observer stands. Failure is non-fatal: you lose the stars, never the scene.
    try {
      const bytes = new Uint8Array(await fetch(withBase("/sky/stars.bin")).then((r) => r.arrayBuffer()));
      terra.load_star_catalog(bytes);
      report("info", `sky: ${bytes.length / 16} catalogued stars`);
    } catch (e) {
      report("warn", `star catalogue unavailable (${e}); the sky will be empty`);
    }
    terra.load_world(worldJson, lm.data, lm.w, lm.h, ev.data, ev.w, ev.h, lc.data, lc.w, lc.h);
    hideStatus();
    report("info", `Terra world loaded: ${terra.world_name()}`);
    (window as unknown as { __terra?: Terra }).__terra = terra;

    // **Elevation streamed by necessity** (docs/46 row 27). The engine says which tiles this view needs
    // and never has more than a 3x3 patch outstanding; this only performs the I/O, which is the same split
    // `load_world` already uses for the world's own rasters — the browser decodes PNGs, the engine decides
    // what is worth decoding.
    //
    // Source: AWS Terrain Tiles (`terrarium`), global, unauthenticated, `Access-Control-Allow-Origin: *`.
    // A 404 is not an error: it is the measured ladder running out of rungs, and generated relief takes
    // over from there. Failures are remembered so a missing tile is not re-requested every frame.
    const TILE_URL = "https://s3.amazonaws.com/elevation-tiles-prod/terrarium";
    const tileInFlight = new Set<string>();
    const tileFailed = new Set<string>();
    let tilePump = 0;
    function pumpTiles() {
      // Two a frame is plenty to keep a patch filled while a camera moves, and it keeps a descent from
      // opening dozens of sockets at once.
      if (performance.now() - tilePump < 60) return;
      tilePump = performance.now();
      let wanted: number[][] = [];
      try {
        wanted = JSON.parse(terra.tiles_wanted());
      } catch {
        return;
      }
      let started = 0;
      for (const [z, x, y] of wanted) {
        const key = `${z}/${x}/${y}`;
        if (tileInFlight.has(key) || tileFailed.has(key)) continue;
        if (started++ >= 2) break;
        tileInFlight.add(key);
        (async () => {
          try {
            const r = await fetch(`${TILE_URL}/${key}.png`);
            if (!r.ok) throw new Error(`HTTP ${r.status}`);
            const bmp = await createImageBitmap(await r.blob());
            const cv = new OffscreenCanvas(bmp.width, bmp.height);
            const cx = cv.getContext("2d", { willReadFrequently: true }) as OffscreenCanvasRenderingContext2D;
            cx.drawImage(bmp, 0, 0);
            const img = cx.getImageData(0, 0, bmp.width, bmp.height);
            terra.add_tile(z, x, y, new Uint8Array(img.data.buffer.slice(0)), bmp.width);
          } catch {
            tileFailed.add(key); // no data here at this zoom; the generator covers it
          } finally {
            tileInFlight.delete(key);
          }
        })();
      }
    }
    (window as unknown as { __tiles?: () => number }).__tiles = () => terra.tile_count();

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
      `${moveHint ? `${moveHint} fly · ` : ""}${altHint ? `${altHint} alt · ` : ""}wheel zoom · ` +
      `${CAMERA_HINT} · shift-drag, middle-drag or shift+scroll to pan`;

    // THE shared camera controls (camera-input.ts): right-drag / alt-drag looks, left-or-ctrl walks
    // forward, +shift reverses. Terra's own `drag_look` and `move_tangent` do the work; the gesture
    // grammar is identical to every other scene.
    // THE pan path (one per scene): the viewpoint slides across the surface through the same mover
    // as the strafe keys (`pan_tangent` → `move_tangent`). Deltas arrive in CSS pixels; the engine
    // scales in its own device-pixel grid, so convert by the canvas's dpr. Every pan gesture
    // (shift-drag, middle-drag, shift+scroll, horizontal scroll) lands here.
    const pan = (dxPx: number, dyPx: number) => {
      const dpr = Math.min(window.devicePixelRatio || 1, 2);
      terra.pan_tangent(dxPx * dpr, dyPx * dpr);
    };
    const cam = attachCameraInput(
      canvas,
      (dyaw, dpitch) => {
        // `drag_look` takes pixel deltas; the module reports radians, so convert back through its own
        // sensitivity rather than inventing a second constant.
        terra.drag_look(-dyaw / 0.005, -dpitch / 0.005);
      },
      { onPan: pan },
    );
    canvas.addEventListener(
      "wheel",
      (e) => {
        e.preventDefault();
        // Shift+scroll and the horizontal wheel axis are trackpad pan: the SAME pan path as the
        // drag, with the sign of a grab (the world follows the fingers). Bare vertical scroll
        // stays altitude: scroll down → climb (zoom out); scroll up → descend (zoom in).
        if (e.shiftKey) {
          pan(-e.deltaX, -e.deltaY);
          return;
        }
        if (e.deltaX !== 0) pan(-e.deltaX, 0);
        if (e.deltaY !== 0) terra.zoom_alt(e.deltaY * 0.01);
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
    // The one canonical HUD, created BEFORE the widgets so they mount into its layer rather than into a
    // hand-rolled fixed div. (It used to be created 90 lines further down, after the widgets.)
    const hud = createSimHud("earth");
    // Share view — the same module every scene uses.
    const share = createShareView(canvas, {
      onStatus: (m, bad) => setStatus(m, bad),
    });
    hud.add("actions", share.button);
    // **The meteor-swarm button.** The whole of the scene's contribution to the event: it declares the
    // initial conditions (a mass, on a trajectory) and the engine does the rest — entry, ablation, the
    // trail, the arrival. The same button belongs on any scene that hands the engine its bodies.
    const swarm = document.createElement("button");
    swarm.textContent = "Meteor swarm";
    swarm.title = "Release a disintegrated asteroid on an entry trajectory toward the point below the camera";
    Object.assign(swarm.style, {
      marginLeft: "8px", padding: "8px 14px", borderRadius: "999px", cursor: "pointer",
      border: "1px solid rgba(255,255,255,0.25)", background: "rgba(20,22,30,0.72)", color: "#eee",
      font: "600 13px system-ui, sans-serif",
    });
    swarm.addEventListener("click", () => {
      terra.launch_swarm();
      hud.notify("swarm released — entering the atmosphere");
    });
    hud.add("actions", swarm);

    // **The cannon.** The scene's entire contribution is placement: it says a 24-pounder stands at the
    // point below the camera and points along this bearing. The engine burns the charge, checks the
    // barrel holds, works out the muzzle velocity and flies the shot through the air — the same flight
    // path the meteor swarm uses. Nothing here touches physics, and `laws::scene_purity_tests` fails the
    // build if it ever does.
    const fire = document.createElement("button");
    fire.textContent = "Fire cannon";
    fire.title = "Fire a 24-pounder from the ground below the camera, seaward at 20 degrees elevation";
    Object.assign(fire.style, {
      marginLeft: "8px", padding: "8px 14px", borderRadius: "999px", cursor: "pointer",
      border: "1px solid rgba(255,255,255,0.25)", background: "rgba(20,22,30,0.72)", color: "#eee",
      font: "600 13px system-ui, sans-serif",
    });
    fire.addEventListener("click", () => {
      // The camera's own heading is where the gun points — you fire where you are looking.
      const bearing = (terra.camera_bearing?.() ?? 0);
      const v = terra.fire_cannon(bearing, 20);
      hud.notify(v > 0
        ? `cannon fired — ${v.toFixed(0)} m/s at bearing ${bearing.toFixed(0)}\u00b0`
        : "the gun did not fire — see the console");
    });
    hud.add("actions", fire);

    // **Follow a fragment down.** The engine says where its matter IS (`heaviest_fragment` / `fragment`);
    // this decides where to put a camera because of it and hands the engine a POSE. The engine has no
    // notion of "following" and does not need one — which is the whole point of feeding it coordinates and
    // a field of view rather than a mode. Nothing here touches physics; the other 1,199 fragments keep
    // falling whether or not anyone is watching this one (Law IV).
    let followId: number | null = null;
    // **Two camera producers, and the HUD guarantees only one drives.** "Fly" is the manual rig; "Follow
    // fragment" hands the engine a POSE each frame. The engine has no notion of following and needs none
    // — that is the point of feeding it coordinates and a field of view rather than a mode. Nothing here
    // touches physics: the other fragments keep falling whether or not anyone watches this one (Law IV).
    const stopFollowing = (why: string) => {
      followId = null;
      terra.clear_camera_pose();
      hud.notify(why);
    };
    hud.cameras([
      {
        id: "fly",
        label: "🛩 Fly",
        title: "Drive the camera yourself — the continuous orbit⇄ground fly rig",
        engage: () => {
          if (followId !== null) stopFollowing("camera released");
          else terra.clear_camera_pose();
        },
        release: () => {},
      },
      {
        id: "follow",
        label: "🎯 Follow fragment",
        title: "Ride the largest surviving fragment down — the one the air takes least of",
        engage: () => {
          const f = terra.heaviest_fragment();
          if (f.length === 0) {
            // Nothing to ride. Say so and hand the wheel straight back, rather than sitting in a mode
            // that silently does nothing.
            hud.notify("nothing in flight to follow");
            hud.selectCamera("fly");
            return;
          }
          followId = f[0];
        },
        release: () => {
          followId = null;
          terra.clear_camera_pose();
        },
      },
    ]);
    (window as unknown as { followFragment?: () => void }).followFragment = () =>
      hud.selectCamera(followId === null ? "follow" : "fly");

    // The chase pose. Offsets are multiples of the fragment's OWN radius, so the framing is the same
    // whether it is riding a pebble or a boulder — a declared framing choice (where to stand), not a
    // physical quantity, and scale-free so it needs no per-scene tuning.
    //
    // But it cannot be arbitrarily close: one f32 depth range cannot hold both a metre-wide fragment and
    // the planet behind it, so the engine floors its near plane at a ten-thousandth of the altitude and
    // anything nearer is clipped. So the chase also stands back a thousandth of the altitude — 219 m at
    // 219 km, closing to a few tens of metres near the ground — and the fragment stays visible the whole
    // way down because the engine draws matter smaller than a pixel AS a pixel. Depth partitioning is what
    // would let the camera sit right on its shoulder at any altitude.
    // **Trail RIGHT BEHIND it.** Was 40 back / 12 up / 16 to the SIDE — the side offset put the camera
    // beside the fragment looking across at it, and 40 radii back made it a speck in its own shot.
    // Directly behind now, close, with just enough lift to keep it off the horizon line rather than
    // silhouetted on it.
    //
    // The ALTITUDE FLOOR still governs the real standoff and is untouched: `scale` below takes the larger
    // of the fragment's own radius and `alt·ALT_STANDOFF/CHASE_BACK`, so the eye distance
    // `CHASE_BACK·scale` can never fall below `alt·ALT_STANDOFF` whatever CHASE_BACK is. That is the f32
    // depth-range constraint (docs/59: one depth range cannot hold an 80 m fragment AND a 6,371 km
    // planet). Shrinking CHASE_BACK therefore closes the gap only where the fragment's OWN size
    // dominates — the low, close, fast part of the descent this camera exists for.
    // CHASE_SIDE stays 0 — MEASURED, not assumed. The worry was that the ablation trail, which streams
    // backwards along −v straight into the lens, would obscure the shot. Photographed at peak density
    // (2.7 km, fragment 2853 K, 7,814 trail parcels at 2616 K, 11,288 items drawn): the fragment reads
    // bright and clear at frame centre, and the parcels are sparse scattered points rather than a
    // curtain. A side offset was trialled at 4 radii and is not needed to see the subject.
    //
    // What sitting on the axis DOES mean is that the camera flies THROUGH the wake rather than alongside
    // it — parcels all around instead of a streak beside you. That is a framing preference, not a
    // visibility problem, and it is left at 0 because nothing measured argues for moving it.
    const CHASE_BACK = 14, CHASE_UP = 3, CHASE_SIDE = 0;
    const ALT_STANDOFF = 1 / 1000;
    const driveFollowCamera = (): boolean => {
      if (followId === null) return false;
      const f = terra.fragment(followId);
      if (f.length === 0) {
        // The producer ENDS ITSELF here: the fragment it was riding has landed. Route the hand-back
        // through the HUD rather than calling stopFollowing directly, or the selector goes on showing
        // "Follow fragment" as the active driver while the fly rig is actually in control.
        hud.notify("the fragment is down — camera released");
        hud.selectCamera("fly");
        return false;
      }
      const p = [f[1], f[2], f[3]], v = [f[4], f[5], f[6]], r = Math.max(f[7], 0.05);
      const norm = (a: number[]) => { const L = Math.hypot(...a) || 1; return a.map((x) => x / L); };
      const cross = (a: number[], b: number[]) => [
        a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0],
      ];
      const upHat = norm(p);
      const vHat = norm(v);
      const sideHat = norm(cross(vHat, upHat));
      // Stand back by whichever is larger: the fragment's own size, or the altitude's depth-range floor.
      const scale = Math.max(r, (terra.altitude_m() * ALT_STANDOFF) / CHASE_BACK);
      const eye = [0, 1, 2].map(
        (i) => p[i] - vHat[i] * (CHASE_BACK * scale) + upHat[i] * (CHASE_UP * scale) + sideHat[i] * (CHASE_SIDE * scale),
      );
      const fwd = norm([0, 1, 2].map((i) => p[i] - eye[i]));
      terra.set_camera_pose(eye[0], eye[1], eye[2], fwd[0], fwd[1], fwd[2], upHat[0], upHat[1], upHat[2], 0.9);
      return true;
    };
    (window as unknown as { launchSwarm?: () => void }).launchSwarm = () => terra.launch_swarm();

    const frame = () => {
      // Held keys → move/altitude intents (the engine scales the step by altitude). Fully data-driven.
      const fwd = (active("forward") ? 1 : 0) - (active("back") ? 1 : 0);
      const right = (active("right") ? 1 : 0) - (active("left") ? 1 : 0);
      const climb = (active("up") ? 1 : 0) - (active("down") ? 1 : 0);
      // Keyboard intents plus the shared pointer scheme; both feed the same mover.
      // While following, the pose owns the camera; manual controls resume the moment it is released.
      const following = driveFollowCamera();
      const walk = fwd + cam.forward();
      if (!following) {
        if (walk !== 0 || right !== 0) terra.move_tangent(walk, right);
        if (climb !== 0) terra.zoom_alt(climb * 0.35); // ~4%/frame altitude change while held
      }
      pumpTiles();
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
            ...(terra.flight_count() > 0 || terra.trail_mass_kg() > 0
              ? [
                  `in flight <b>${terra.flight_count()}</b> · drawn <b>${terra.drawn_count()}</b> · ` +
                    `ablated <b>${terra.trail_mass_kg().toFixed(1)} kg</b>`,
                ]
              : []),
            ...(followId !== null ? [`following fragment <b>#${followId}</b>`] : []),
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
