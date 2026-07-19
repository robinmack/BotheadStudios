// Receiver for the space-band "📷 Share view" button on the DEPLOYED (static) site. The client POSTs the
// canvas as a data-URL PNG to /__shot; nginx (integrity vhost) proxies that here, and we decode + save it
// where the agent can look at it. The Vite dev server already handles /__shot itself (web/shots/); this is
// the production equivalent so the button works on integrity.bothead.net too. Run persistently (systemd:
// integrity-shot.service).
//
//   SHOT_DIR=/home/ratwood/integrity-shots PORT=9099 node tools/shot-server.mjs
import http from "node:http";
import { writeFileSync, mkdirSync } from "node:fs";

const DIR = process.env.SHOT_DIR || "/home/ratwood/integrity-shots";
const PORT = Number(process.env.PORT || 9099);
mkdirSync(DIR, { recursive: true });

http
  .createServer((req, res) => {
    if (req.method === "POST" && (req.url === "/__shot" || req.url === "/")) {
      const chunks = [];
      req.on("data", (c) => chunks.push(c));
      req.on("end", () => {
        try {
          const body = Buffer.concat(chunks).toString("utf8");
          const buf = Buffer.from(body.replace(/^data:image\/\w+;base64,/, ""), "base64");
          const file = `${DIR}/shot-${Date.now()}.png`;
          writeFileSync(file, buf);
          console.log(`saved ${file} (${buf.length} bytes)`);
          res.writeHead(204).end();
        } catch (e) {
          console.error("save failed:", e);
          res.writeHead(500).end();
        }
      });
    } else {
      res.writeHead(404).end();
    }
  })
  .listen(PORT, "127.0.0.1", () => console.log(`shot receiver on 127.0.0.1:${PORT} → ${DIR}`));
