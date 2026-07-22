// **Share view — one implementation, every scene.**
//
// Capture exactly what is on screen and POST it to `/__shot`, where the dev server writes it to
// `shots/shot-<ts>.png` (see `vite.config.ts`). That is how a picture gets from a scene to whoever is
// looking at the repo, without anyone screenshotting a browser by hand.
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

export function createShareView(
  canvas: HTMLCanvasElement,
  opts: { label?: string; onStatus?: (msg: string, bad?: boolean) => void } = {},
): ShareView {
  const { label = "📷 Share view", onStatus } = opts;
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

  return {
    button,
    request() {
      want = true;
    },
    afterPresent() {
      if (!want) return;
      want = false;
      try {
        const url = canvas.toDataURL("image/png");
        void fetch("/__shot", {
          method: "POST",
          headers: { "content-type": "text/plain" },
          body: url,
        })
          .then((r) => onStatus?.(r.ok ? "view shared" : `share failed: HTTP ${r.status}`, !r.ok))
          .catch((e) => onStatus?.(`share failed: ${String(e)}`, true));
      } catch (e) {
        // A WebGPU canvas read outside the presented frame throws or yields nothing; say so plainly
        // rather than silently posting a blank image.
        onStatus?.(`capture failed: ${String(e)}`, true);
      }
      setTimeout(() => onStatus?.(""), 2200);
    },
  };
}
