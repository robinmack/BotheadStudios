import { defineConfig, type Plugin } from "vite";
import basicSsl from "@vitejs/plugin-basic-ssl";

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

export default defineConfig({
  assetsInclude: ["**/*.wasm"],
  plugins: [logRelay(), ...(lan ? [basicSsl()] : [])],
  server: {
    host: lan ? true : "127.0.0.1",
    port: 5173,
    strictPort: true,
    fs: {
      // Allow importing the generated wasm package that lives under src/wasm.
      allow: [".."],
    },
  },
  optimizeDeps: {
    exclude: ["engine"],
  },
});
