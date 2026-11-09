#!/usr/bin/env python3
"""The `/__shot` receiver for a deployed site — what `vite.config.ts`'s middleware is on the dev server.

The 📷 Screenshot button POSTs JSON `{png: <data URL>, taken_at, build, page, viewport, user_agent,
state}` (see `web/src/share-view.ts`, `ShotRecord`). This numbers it, writes `shot-<n>.png` and
`shot-<n>.json` under DEST, and answers `{"n": <n>}` so the page can tell the user the number to quote.
`web/rig/reproduce_shot.mjs shot-<n>.json` stands the engine in front of it again.

NOT WIRED TO THE LIVE SITE BY THIS FILE. It listens on loopback only; nginx proxies `/__shot` to it:

    location = /__shot {
        client_max_body_size 32m;
        limit_except POST { deny all; }
        proxy_pass http://127.0.0.1:8781;
    }

and `share-view.ts`'s `hasShotReceiver()` must then answer true for the public host. Robin's call (docs/72).

    SHOT_DEST=/var/lib/integrity/shots SHOT_PORT=8781 scripts/shot-receiver.py
"""
import base64
import json
import os
import re
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

DEST = os.environ.get("SHOT_DEST", "/var/lib/integrity/shots")
PORT = int(os.environ.get("SHOT_PORT", "8781"))
MAX_BYTES = 32 * 1024 * 1024
_lock = threading.Lock()


def next_number() -> int:
    counter = os.path.join(DEST, ".next")
    with _lock:
        try:
            n = int(open(counter).read().strip() or "1")
        except (OSError, ValueError):
            n = 1
        with open(counter, "w") as f:
            f.write(str(n + 1))
    return n


class Receiver(BaseHTTPRequestHandler):
    server_version = "integrity-shot-receiver/1"

    def log_message(self, fmt, *args):  # one line per shot, not one per request header
        pass

    def do_POST(self):
        if self.path != "/__shot":
            self.send_error(404)
            return
        length = int(self.headers.get("content-length") or 0)
        if length <= 0 or length > MAX_BYTES:
            self.send_error(413 if length > MAX_BYTES else 400)
            return
        body = self.rfile.read(length)
        try:
            if "application/json" in (self.headers.get("content-type") or ""):
                record = json.loads(body)
                data_url = str(record.pop("png", ""))
            else:
                record, data_url = {}, body.decode("utf8", "replace")
            png = base64.b64decode(re.sub(r"^data:image/\w+;base64,", "", data_url), validate=False)
            if png[:8] != b"\x89PNG\r\n\x1a\n":
                self.send_error(400, "not a PNG")
                return
            os.makedirs(DEST, exist_ok=True)
            n = next_number()
            with open(os.path.join(DEST, f"shot-{n}.png"), "wb") as f:
                f.write(png)
            if record:
                record = {"n": n, **{k: v for k, v in record.items() if k != "n"}}
                with open(os.path.join(DEST, f"shot-{n}.json"), "w") as f:
                    json.dump(record, f, indent=1)
            print(f"shot #{n}: {len(png)} bytes, build {record.get('build', '?')}, page {record.get('page', '?')}", flush=True)
            out = json.dumps({"n": n}).encode()
            self.send_response(200)
            self.send_header("content-type", "application/json")
            self.send_header("content-length", str(len(out)))
            self.end_headers()
            self.wfile.write(out)
        except Exception as e:  # a malformed shot is the client's problem, said plainly
            self.send_error(400, str(e)[:200])


if __name__ == "__main__":
    os.makedirs(DEST, exist_ok=True)
    print(f"shot receiver on 127.0.0.1:{PORT} → {DEST}", flush=True)
    ThreadingHTTPServer(("127.0.0.1", PORT), Receiver).serve_forever()
