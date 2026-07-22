import { fileURLToPath } from "node:url";
import { resolve } from "node:path";
import { mkdirSync, writeFileSync } from "node:fs";
import { defineConfig, type Plugin } from "vite";
import basicSsl from "@vitejs/plugin-basic-ssl";

const root = fileURLToPath(new URL(".", import.meta.url));

// Save a screenshot the client POSTs (a PNG data URL) to web/shots/, so on-device visual bugs (e.g.
// levitating particles) can be captured and inspected. The button in main.ts POSTs to /__shot.
function shotSink(): Plugin {
  return {
    name: "screenshot-sink",
    apply: "serve",
    configureServer(server) {
      server.middlewares.use("/__shot", (req, res) => {
        if (req.method !== "POST") {
          res.statusCode = 405;
          res.end();
          return;
        }
        // Collect raw Buffer chunks — concatenating as strings truncates/corrupts a large PNG data URL.
        const chunks: Buffer[] = [];
        req.on("data", (c: Buffer) => chunks.push(c));
        req.on("end", () => {
          try {
            const body = Buffer.concat(chunks).toString("utf8");
            const b64 = body.replace(/^data:image\/\w+;base64,/, "");
            const buf = Buffer.from(b64, "base64");
            // OUTSIDE web/ (the Vite-watched root) so saving a screenshot can never trigger a reload.
            const dir = resolve(root, "..", "shots");
            mkdirSync(dir, { recursive: true });
            const file = resolve(dir, `shot-${Date.now()}.png`);
            writeFileSync(file, buf);
            server.config.logger.info(`[client] 📷 screenshot saved: ${file} (${buf.length} bytes)`);
            res.statusCode = 204;
            res.end();
          } catch (e) {
            server.config.logger.error(`[client] screenshot save failed: ${String(e)}`);
            res.statusCode = 500;
            res.end();
          }
        });
      });
    },
  };
}

// Relay the client's console output + errors to the dev-server stdout, so console-less devices
// (iPad Safari, etc.) can be debugged. The page POSTs JSON {level, msg} to /__log.
function logRelay(): Plugin {
  return {
    name: "client-log-relay",
    apply: "serve",
    configureServer(server) {
      server.middlewares.use("/__log", (req, res) => {
        if (req.method !== "POST") {
          res.statusCode = 405;
          res.end();
          return;
        }
        let body = "";
        req.on("data", (c) => (body += c));
        req.on("end", () => {
          let line = body;
          try {
            const j = JSON.parse(body) as { level?: string; msg?: string };
            line = `[${j.level ?? "log"}] ${j.msg ?? ""}`;
          } catch {
            /* fall back to raw body */
          }
          server.config.logger.info(`[client] ${line}`);
          res.statusCode = 204;
          res.end();
        });
      });
    },
  };
}

// wasm-pack (--target web) emits ESM glue that fetches `*_bg.wasm` via `import.meta.url`.
// Vite serves that fine in dev; for build we make sure .wasm is treated as an asset and the
// glue isn't pre-bundled (which would break the relative wasm URL).
//
// LAN=1 serves over self-signed HTTPS bound to all interfaces, so another machine on the local
// network can view the app directly. HTTPS is REQUIRED because WebGPU (`navigator.gpu`) is only
// exposed in a secure context — and a plain-http LAN IP is NOT secure (only https or localhost is).
// Default (no LAN): plain http on 127.0.0.1 only — reach it via an SSH tunnel.
const lan = process.env.LAN === "1";

// A human-readable build stamp, computed once when this config is loaded (i.e. per `npm run build` and
// per dev-server start). Injected as the global `__BUILD_ID__` and shown in the HUD, so we can tell at a
// glance whether the browser is running the code we just shipped — the antidote to Safari serving a stale
// cache. Format: YYYYMMDD.HHMMSS (sortable). (Node context here, so `new Date()` is fine.)
const pad = (n: number): string => String(n).padStart(2, "0");
const now = new Date();
const BUILD_ID =
  `${now.getFullYear()}${pad(now.getMonth() + 1)}${pad(now.getDate())}` +
  `.${pad(now.getHours())}${pad(now.getMinutes())}${pad(now.getSeconds())}`;

// Belt-and-suspenders cache defeat for Safari: the built JS/CSS/wasm are content-hashed (safe to cache
// forever), but a stale HTML or a stable-URL dev wasm will pin old code. Tell the dev AND preview servers
// to never store anything, so every refresh re-fetches. (A static host still needs no-cache on *.html;
// the hashed assets take care of themselves.)
const NO_STORE = { "Cache-Control": "no-store, max-age=0" };

export default defineConfig({
  assetsInclude: ["**/*.wasm"],
  define: { __BUILD_ID__: JSON.stringify(BUILD_ID) },
  plugins: [logRelay(), shotSink(), ...(lan ? [basicSsl()] : [])],
  preview: { headers: NO_STORE },
  server: {
    host: lan ? true : "127.0.0.1",
    port: 5173,
    strictPort: true,
    headers: NO_STORE,
    // Don't hot-reload when a screenshot is written into web/shots — that was reloading (and
    // "crashing") the app every time the 📷 button fired.
    watch: { ignored: ["**/shots/**"] },
    fs: {
      // Allow importing the generated wasm package that lives under src/wasm.
      allow: [".."],
    },
  },
  optimizeDeps: {
    exclude: ["engine"],
  },
  build: {
    rollupOptions: {
      // Multi-page: the landing page (index), the terrain slice, the space band (orbit), the two-moon test.
      // Every page MUST be listed here — vite's dev server resolves any .html off root, so an
      // unregistered page works in `npm run dev` and silently disappears from `npm run build`.
      input: {
        main: resolve(root, "index.html"),
        terra: resolve(root, "terra.html"), // docs/43 — the data-defined Earth world
        orbit: resolve(root, "orbit.html"),
        birth: resolve(root, "birth.html"),
        ground: resolve(root, "ground.html"),
        twomoons: resolve(root, "twomoons.html"),
        gpuProbe: resolve(root, "gpu-probe.html"), // cross-device GPU probe (no scene, compute only)
      },
    },
  },
});
