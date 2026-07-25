/// <reference types="vite/client" />

// Injected by vite.config.ts `define` — a per-build stamp (YYYYMMDD.HHMMSS) shown in the HUD so we can
// confirm the browser is running freshly-shipped code and not a stale Safari cache.
declare const __BUILD_ID__: string;
/** The commit this bundle was built from (short SHA, `+` if the tree was dirty), or "nogit". */
declare const __BUILD_REL__: string;
