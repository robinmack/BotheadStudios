// **Share view — one implementation, every scene.**
//
// Capture exactly what is on screen and POST it to `/__shot`, where the dev server writes it to
// `shots/shot-<n>.png` with `shots/shot-<n>.json` beside it — the pose and state that reproduce it
// (see `vite.config.ts`, `ShotRecord`). That is how a picture gets from a scene to whoever is looking
// at the repo, without anyone screenshotting a browser by hand; on the public site, where there is no
// receiver, the same two files are downloaded for the user to send.
//
// This lived inline in `orbit.ts`, so the birth/space/two-moons pages had it and the others did not.
// Copying it into each new scene is how one answer becomes four that drift, so it is a module: a scene
// calls `createShareView(canvas)`, places the returned button wherever suits it, and calls
// `afterPresent()` once per frame.
//
// **Why `afterPresent`, and why it must be called right after `render()`:** a WebGPU canvas is only
// readable while its drawing buffer is current. Capture at any other point in the frame and
// `toDataURL` returns an empty or stale image — which looks like "the screenshot feature is broken"
// rather than "it was called at the wrong time".

export type ShareView = {
  /** The button. Place it in whatever control strip the scene already has. */
  button: HTMLButtonElement;
  /** Request a capture; it is taken on the next `afterPresent()`. */
  request(): void;
  /** Call ONCE per frame, immediately after the scene presents. */
  afterPresent(): void;
};

/// **What a screenshot carries besides the pixels** (Robin, 2026-11-08: *"a number for the shot,
/// the client's view, and the coordinates, heading, attitude, etc so we can confidently reproduce
/// what the user is seeing"*). The scene supplies `state` — for Terra, the engine's own
/// `view_state_json()`: pose, the sky's instant, the clock, what is drawn, what the stream held —
/// and this module adds what only the page knows: the build, the page, the viewport, the device
/// pixel ratio, the browser, and when. `web/rig/reproduce_shot.mjs <shot.json>` stands the engine
/// in front of it again.
export type ShotRecord = {
  n: number;
  taken_at: string;
  build: string;
  page: string;
  viewport: { width: number; height: number; dpr: number };
  user_agent: string;
  state: unknown;
};

/// The number the NEXT shot gets when no receiver answers: kept in the browser, so a user's shots
/// are numbered in the order they took them and the number they quote is the one on their files.
function nextLocalShotNumber(): number {
  try {
    const n = Number(localStorage.getItem("integrity.shot.next") || "1") || 1;
    localStorage.setItem("integrity.shot.next", String(n + 1));
    return n;
  } catch {
    return Math.floor(Date.now() / 1000) % 100000;
  }
}

function download(name: string, href: string): void {
  const a = document.createElement("a");
  a.href = href;
  a.download = name;
  document.body.appendChild(a);
  a.click();
  a.remove();
}

/// Is this page served from somewhere the `/__shot` receiver actually exists?
///
/// The receiver is a **vite dev-server middleware**, so it exists on localhost and over the LAN
/// (`scripts/dev-lan.sh`) and NOT in production — integrity.bothead.net answers `405`. A button that
/// silently fails is worse than no button, so the honest signal is where the page came FROM: a private
/// or loopback host means the dev server is serving it.
///
/// Deliberately host-based rather than probing `/__shot`: a probe costs a request on every page load and
/// races the first paint, and the answer is already knowable from the URL.
export function hasShotReceiver(hostname: string = location.hostname): boolean {
  const h = hostname.toLowerCase();
  if (h === "localhost" || h === "" || h.endsWith(".local")) return true;
  if (h === "::1" || h === "[::1]") return true;
  // 127.0.0.0/8
  if (/^127\./.test(h)) return true;
  // RFC1918: 10/8, 192.168/16, 172.16/12
  if (/^10\./.test(h)) return true;
  if (/^192\.168\./.test(h)) return true;
  if (/^172\.(1[6-9]|2\d|3[01])\./.test(h)) return true;
  return false;
}

export function createShareView(
  canvas: HTMLCanvasElement,
  opts: {
    label?: string;
    onStatus?: (msg: string, bad?: boolean) => void;
    /** The scene's reproducible state at the moment of capture (Terra: `terra.view_state_json()`, parsed). */
    state?: () => unknown;
  } = {},
): ShareView {
  const { label = "📷 Screenshot", onStatus, state } = opts;
  let want = false;

  const button = document.createElement("button");
  button.className = "gf-btn";
  button.id = "share-view";
  button.textContent = label;
  Object.assign(button.style, {
    padding: "9px 13px",
    font: "600 14px/1 system-ui, sans-serif",
    color: "#fff",
    background: "rgba(20,24,40,0.72)",
    border: "1px solid rgba(255,255,255,0.25)",
    borderRadius: "10px",
    backdropFilter: "blur(6px)",
    cursor: "pointer",
  });
  button.addEventListener("click", () => {
    want = true;
    onStatus?.("capturing view…");
  });
  // Where the receiver exists (the dev server, the LAN) the shot is POSTed and the receiver numbers
  // it; everywhere else — the public site — the shot and its record are DOWNLOADED, numbered here,
  // so a user can send the two files. The button used to be hidden on the public site because a
  // button that silently fails is worse than none; now it has something honest to do there.
  const receiver = hasShotReceiver();
  button.title = receiver
    ? "Save this view, numbered, with its pose and state, on the dev server"
    : "Download this view, numbered, with its pose and state (two files)";

  return {
    button,
    request() {
      want = true;
    },
    afterPresent() {
      if (!want) return;
      want = false;
      let url: string;
      try {
        url = canvas.toDataURL("image/png");
      } catch (e) {
        // A WebGPU canvas read outside the presented frame throws or yields nothing; say so plainly
        // rather than silently posting a blank image.
        onStatus?.(`capture failed: ${String(e)}`, true);
        setTimeout(() => onStatus?.(""), 2200);
        return;
      }
      let sceneState: unknown = null;
      try {
        sceneState = state?.() ?? null;
      } catch (e) {
        sceneState = { error: String(e) };
      }
      const record: Omit<ShotRecord, "n"> = {
        taken_at: new Date().toISOString(),
        build: __BUILD_ID__,
        page: location.pathname,
        viewport: { width: canvas.width, height: canvas.height, dpr: devicePixelRatio },
        user_agent: navigator.userAgent,
        state: sceneState,
      };
      if (receiver) {
        void fetch("/__shot", {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ png: url, ...record }),
        })
          .then(async (r) => {
            if (!r.ok) {
              onStatus?.(`shot failed: HTTP ${r.status}`, true);
              return;
            }
            const { n } = (await r.json()) as { n: number };
            console.info(`[shot] #${n} saved`, record);
            onStatus?.(`shot #${n} saved — quote the number`);
          })
          .catch((e) => onStatus?.(`shot failed: ${String(e)}`, true));
      } else {
        const n = nextLocalShotNumber();
        const stamp = record.taken_at.replace(/[:.]/g, "-");
        const full: ShotRecord = { n, ...record };
        download(`integrity-shot-${n}-${stamp}.png`, url);
        download(
          `integrity-shot-${n}-${stamp}.json`,
          "data:application/json;charset=utf-8," + encodeURIComponent(JSON.stringify(full, null, 1)),
        );
        console.info(`[shot] #${n} downloaded`, full);
        onStatus?.(`shot #${n} downloaded (image + record) — send both`);
      }
      setTimeout(() => onStatus?.(""), 4000);
    },
  };
}
