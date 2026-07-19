# Deployment — integrity.bothead.net

The engine's public demo is live at **https://integrity.bothead.net**. It is a **static build** — no
app server, no database — served exactly like the macklepenny sites.

## The pipeline (one command)

```bash
./scripts/deploy.sh
```

That script does the whole thing:

1. **Build** — `cd web && npm run build` (i.e. `wasm-pack build … --release` then `vite build`) emits
   `web/dist`: the release WASM plus a Vite bundle whose JS/CSS/WASM are **content-hashed** (filenames
   change every build).
2. **Publish** — `rsync -a --delete web/dist/ /var/www/integrity/`. The dir is owned by `ratwood`, so no
   `sudo`. `--delete` clears stale hashed assets; the build always reproduces the full page set
   (`index` / `terrain` / `orbit` / `twomoons` / `birth` + `assets/`), so nothing live is lost.

No restart is needed — it is static files.

## The serving stack (how a request reaches the files)

```
browser ──TLS──► Cloudflare edge ──tunnel──► cloudflared ──► nginx :8080 ──► /var/www/integrity
        integrity.bothead.net                (this box)      (by Host)        (static build)
```

- **Cloudflare tunnel** — `/etc/cloudflared/config.yml` ingress maps
  `integrity.bothead.net → http://localhost:8080` (TLS terminates at Cloudflare's edge, so nginx listens
  on plain `:8080`). The tunnel is `cloudflared … tunnel run` (shared with the macklepenny / bothead /
  avsecure hostnames).
- **nginx** — `/etc/nginx/conf.d/integrity.bothead.net.conf` listens on `:8080`, routes by
  `server_name integrity.bothead.net`, and serves `root /var/www/integrity`. Cache policy is the
  server-side half of cache-busting: **`immutable`** on `/assets/` (hashed, so safe to cache forever) and
  **`no-cache`** on HTML (so browsers always fetch the fresh `index.html`, which then points at the new
  hashed assets). SPA-style `try_files $uri $uri/ /index.html`.

## One-time wiring (already done; recorded here so it can be reproduced)

- **nginx**: install the vhost at `/etc/nginx/conf.d/integrity.bothead.net.conf`, `nginx -t`, reload.
- **cloudflared**: add the `integrity.bothead.net → localhost:8080` ingress rule **before** the
  `http_status:404` catch-all in `/etc/cloudflared/config.yml`, then restart `cloudflared`.
- **DNS**: `cloudflared tunnel route dns <tunnel-uuid> integrity.bothead.net` (or a CNAME → the tunnel).

## Verifying a deploy

```bash
# local, bypassing Cloudflare — should show the just-built hashed asset name:
curl -s -H 'Host: integrity.bothead.net' http://127.0.0.1:8080/ | grep -oE 'main-[A-Za-z0-9_-]+\.js'
# public, end-to-end through the tunnel:
curl -s https://integrity.bothead.net/ | grep -o '<title>[^<]*</title>'
```

## Notes

- The deploy script was previously local-only (an untracked `web/deploy/deploy.sh` referenced by the
  nginx conf) and was lost; it now lives in-repo at `scripts/deploy.sh` so it is versioned.
- WebGPU requires a secure context; Cloudflare provides the TLS, so the public site works on any
  WebGPU-capable browser. For on-device LAN testing without deploying, use `scripts/dev-lan.sh` (HTTPS
  dev server on `:5173`).
