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

    // ★ **A GUN EMPLACEMENT AS SCENE DATA.** `data-cannon="lat,lon,bearing,elevation"` on the page is
    // the whole of what makes `yarr.html` a different scene from `terra.html`: the SAME Earth, the same
    // world file, the same struct — one assembly placed on it, and somewhere to stand and watch.
    //
    // The camera is DERIVED from the gun rather than stated beside it, so there is one fact here and
    // not two that can disagree: stand behind the breech along the reverse bearing, a little above,
    // looking down the barrel's line.
    // ★★ **MORE THAN ONE SHORE, AND THE SUN DECIDES WHICH ONE YOU ARRIVE AT.**
    //
    // Robin, finding the gun standing in the dark: *"It's dark in Galway… let's switch back to Chile.
    // Probably need a scene button where you can flip to wherever it's daylight of the two."*
    //
    // `data-cannon` is a `;`-separated list of `Name@lat,lon,bearing,elevation`; a bare
    // `lat,lon,bearing,elevation` is still one unnamed site, so the old form keeps working. Which site
    // is LIT is not computed here — `terra.sun_elevation_deg` answers it, because solar geometry is
    // the engine's and a page doing its own spherical trig is a second answer to a settled question.
    // The button moves where you stand. It does not move the sun.
    const emplacement = document.body.dataset.cannon;
    if (emplacement) {
      type Site = { name: string; lat: number; lon: number; bearing: number; elev: number };
      const sites: Site[] = emplacement.split(";").filter(Boolean).map((chunk, i) => {
        const at = chunk.indexOf("@");
        const name = at >= 0 ? chunk.slice(0, at) : `site ${i + 1}`;
        const [lat, lon, bearing, elev] = chunk.slice(at + 1).split(",").map(Number);
        return { name, lat, lon, bearing, elev };
      });
      // Open on the sunniest one. Ask the engine where the sun is; do not work it out here.
      let current = 0;
      for (let i = 1; i < sites.length; i++) {
        if (terra.sun_elevation_deg(sites[i].lat, sites[i].lon) >
            terra.sun_elevation_deg(sites[current].lat, sites[current].lon)) current = i;
      }
      const site = () => sites[current];
      let { lat: gLat, lon: gLon, bearing: gBearing } = site();
      const M_PER_DEG = 111320;
      // Metres along a compass bearing from the gun, as a (lat, lon) offset.
      const along = (d: number, bearing: number): [number, number] => {
        const r = (bearing * Math.PI) / 180;
        return [
          gLat + (d * Math.cos(r)) / M_PER_DEG,
          gLon + (d * Math.sin(r)) / (M_PER_DEG * Math.cos((gLat * Math.PI) / 180)),
        ];
      };
      const standBehind = (back: number, up: number, pitch: number) => {
        const [la, lo] = along(back, gBearing + 180);
        terra.place_camera(la, lo, up, (gBearing * Math.PI) / 180, pitch);
      };

      terra.set_alt_bounds(0.5, 4.0e7);

      // Stand the gun at the current site and take up the watching position behind it. Called on load
      // and again whenever the site changes, so there is ONE description of "here is the gun, here is
      // where you watch from" rather than two that can drift apart.
      const emplaceHere = () => {
        ({ lat: gLat, lon: gLon, bearing: gBearing } = site());
        terra.place_camera(gLat, gLon, 3, (gBearing * Math.PI) / 180, -0.1);
        terra.emplace_cannon(gBearing);
        const back = 8.5, up = 2.8;
        standBehind(back, up, -Math.atan((up - 0.7) / back));
      };
      // Behind the breech and above it, looking down the barrel out to sea — the view Robin asked for,
      // where the gun can actually be SEEN.
      // ★ The pitch is DERIVED from the geometry, not guessed: standing `back` metres behind and
      // `up` metres above a gun whose barrel sits ~0.7 m off the ground, the line to it is
      // atan((up - 0.7) / back) below horizontal. A first version used a flat -0.16 and the gun sat on
      // the bottom edge behind the HUD — the same class of error as every other typed number today.
      emplaceHere();

      const sunAt = (s: Site) => terra.sun_elevation_deg(s.lat, s.lon);
      const describe = (s: Site) => {
        const e = sunAt(s);
        return `${s.name} (sun ${e >= 0 ? "+" : ""}${e.toFixed(0)}°)`;
      };
      if (sites.length > 1) {
        // **Weigh anchor.** Sail to the next shore — the gun, the camera and the horizon all follow
        // from the site, so this changes one number and everything else is derived from it again.
        const sail = document.createElement("button");
        Object.assign(sail.style, {
          marginLeft: "8px", padding: "8px 14px", borderRadius: "999px", cursor: "pointer",
          border: "1px solid rgba(255,255,255,0.25)", background: "rgba(20,22,30,0.72)", color: "#eee",
          font: "600 13px system-ui, sans-serif",
        });
        const label = () => {
          const next = sites[(current + 1) % sites.length];
          sail.textContent = `Sail to ${next.name}`;
          sail.title =
            `Move the gun to ${describe(next)}. Currently at ${describe(site())}. ` +
            `The sun is where the sun is — this moves the ship, not the sky.`;
        };
        label();
        sail.addEventListener("click", () => {
          current = (current + 1) % sites.length;
          emplaceHere();
          label();
          hud.notify(`made landfall at ${describe(site())}`);
        });
        hud.add("actions", sail);
      }
      hud.notify(`a 24-pounder, loaded and run out at ${describe(site())}. Fire when ready.`);

      // **Fire, then follow the shot out and watch it land.** The scene names a seat on the shot; the
      // engine keeps the observer in it. This comment used to say the engine "has no notion of
      // following and does not need one" — it needs one, because it is the thing that knows where its
      // own matter is, and the alternative was a scene reading coordinates back to do the maths.
      // ★★ **FIRE, THEN HAND THE CAMERA TO THE FOLLOWER THAT ALREADY EXISTS.**
      //
      // Robin, watching: *"cannon seems to shoot to the side?! ... it all looks very weird."* The engine
      // was innocent — `the_shot_leaves_along_the_barrel_it_was_drawn_with` pins the shot to within
      // 0.01 degrees of the barrel as drawn, at four sites. The weirdness was a camera I hand-rolled
      // here: it swung from behind the gun, THROUGH it, to a point downrange while racing 9 -> 909 m
      // out, so half a second after firing the gun was off-screen and the view was 66 m in the air.
      //
      // Selecting "follow" is the whole of what firing does to the camera, so the shot is watched by
      // the same code that watches a meteor — and as of docs/65 that code is `Terra::camera_follow`,
      // in the engine, rather than a chase camera this scene wrote for itself.
      fire.addEventListener("click", () => {
        emplaceHere();
        // The shot exists the moment the gun fires, so there is something to ride at once — and if
        // there is not, the engine says so rather than seating the camera on nothing.
        hud.selectCamera("follow");
      });
    }

    // **Ride the largest surviving fragment down** — and the scene's whole contribution is the SEAT.
    //
    // Robin's second camera verb (2026-08-03): *"camera-follow `<assembly>`, `<relative position>`,
    // `<heading>`."* So this names what to ride, where to sit in its frame, and where to look. It does
    // not read where the matter is, does not build a basis, does not compute a standoff, and does not
    // hand the engine a pose every frame.
    //
    // ★ What stood here was 43 lines of vector maths run per frame — normalising a velocity, crossing
    // it with the local up, scaling an offset by the fragment's own radius — plus its own mode state to
    // decide when to stop. All of it is the engine's job (docs/65), and all of it is now `camera_follow`.
    // It also carried the bug that follows from a radius-scaled standoff: riding a 7 cm cannonball put
    // the eye about a metre away and filled the frame with sky. A seat stated in METRES cannot do that.
    const SEAT = { back: 60, up: 12, side: 0 }; // metres behind, above and beside what we ride
    hud.cameras([
      {
        id: "fly",
        label: "🛩 Fly",
        title: "Drive the camera yourself — the continuous orbit⇄ground fly rig",
        engage: () => terra.camera_follow("", 0, 0, 0, 0, 0),
        release: () => {},
      },
      {
        id: "follow",
        label: "🎯 Follow fragment",
        title: "Ride the largest surviving fragment down — the one the air takes least of",
        engage: () => {
          // The engine says whether there is anything to ride; it refuses rather than pointing the
          // camera at nothing, so there is no "is it there yet" check to get wrong here.
          if (!terra.camera_follow("heaviest", SEAT.back, SEAT.up, SEAT.side, 0, 0)) {
            hud.notify("nothing in flight to follow");
            hud.selectCamera("fly");
          }
        },
        release: () => terra.camera_follow("", 0, 0, 0, 0, 0),
      },
    ]);
    (window as unknown as { followFragment?: () => void }).followFragment = () =>
      hud.selectCamera(terra.camera_is_following() ? "fly" : "follow");

    (window as unknown as { launchSwarm?: () => void }).launchSwarm = () => terra.launch_swarm();

    const frame = () => {
      // Held keys → move/altitude intents (the engine scales the step by altitude). Fully data-driven.
      const fwd = (active("forward") ? 1 : 0) - (active("back") ? 1 : 0);
      const right = (active("right") ? 1 : 0) - (active("left") ? 1 : 0);
      const climb = (active("up") ? 1 : 0) - (active("down") ? 1 : 0);
      // Keyboard intents plus the shared pointer scheme; both feed the same mover.
      // While riding an actor the engine owns the camera; manual controls resume the moment it lets go
      // — and it lets go BY ITSELF when the subject lands, which the scene used to have to notice.
      const following = terra.camera_is_following();
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
            ...(terra.camera_is_following() ? ["following a fragment"] : []),
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
