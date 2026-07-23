// Dev-only diagnostics relay. Mirrors the browser's console output and uncaught errors to the dev
// server's stdout (the `/__log` middleware in vite.config.ts), so a headless rig or a console-less
// device (iPad Safari) surfaces failures INSTANTLY in the terminal — a WGSL compile error, a Rust
// panic, a missing asset, a stuck "Requesting GPU device…" — instead of a blank screenshot you have to
// re-run a rig to diagnose.
//
// **Import this FIRST in every page entry, before the wasm glue** (`import "./dev-log";`), so a failure
// DURING wasm load/instantiate is captured too (that is the whole point Robin asked for it — a broken
// build should announce itself, not render blank). It replaces the three hand-copied relays that used to
// live in orbit.ts / terra.ts / gpu-probe.ts (one answer, one place).
//
// No-op in a production build: `import.meta.env.DEV` is false there, and `/__log` does not exist on the
// static host, so this never fires in front of a real user.

function safeStringify(a: unknown): string {
  if (typeof a === "string") return a;
  if (a instanceof Error) return `${a.name}: ${a.message}${a.stack ? `\n${a.stack}` : ""}`;
  try {
    return JSON.stringify(a);
  } catch {
    return String(a);
  }
}

/// Send one line to the dev server's terminal. Exported so a page can log explicit status/diagnostics
/// (setStatus banners, the GPU probe). A no-op outside dev, where `/__log` does not exist.
export function report(level: string, msg: string): void {
  if (!import.meta.env.DEV) return;
  try {
    void fetch("/__log", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ level, msg }),
      keepalive: true, // survive a navigation / a page that dies right after logging
    });
  } catch {
    /* best-effort — never let logging throw */
  }
}

if (import.meta.env.DEV) {
  (["log", "info", "warn", "error", "debug"] as const).forEach((lvl) => {
    const orig = console[lvl].bind(console);
    console[lvl] = (...args: unknown[]) => {
      orig(...args);
      report(lvl, args.map(safeStringify).join(" "));
    };
  });
  window.addEventListener("error", (e) =>
    report("error", `window.onerror: ${e.message} @ ${e.filename}:${e.lineno}:${e.colno}`),
  );
  window.addEventListener("unhandledrejection", (e) =>
    report("error", `unhandledrejection: ${safeStringify((e as PromiseRejectionEvent).reason)}`),
  );
  // A heartbeat so the terminal shows the relay is live and WHICH build/page is loading — the first
  // line you see confirms the page even got this far (before any wasm work).
  report("info", `▶ ${location.pathname} loading · build ${__BUILD_ID__}`);
}
