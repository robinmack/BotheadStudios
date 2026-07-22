#!/usr/bin/env bash
#
# deploy.sh — build the Integrity engine web app and publish it to https://integrity.bothead.net.
#
# HOSTING (see docs/29-deployment.md). The live site is a STATIC build, served like the macklepenny
# sites:
#   • `npm run build` emits web/dist — a release-wasm + Vite bundle with content-hashed /assets.
#   • This script syncs web/dist → /var/www/integrity (ratwood-owned; no sudo needed).
#   • nginx (/etc/nginx/conf.d/integrity.bothead.net.conf) serves that dir on :8080, routing by Host.
#   • The Cloudflare tunnel (/etc/cloudflared/config.yml) maps integrity.bothead.net → localhost:8080;
#     TLS terminates at Cloudflare's edge, so nginx listens on plain :8080.
# No server restart is needed — it is static files. nginx sends `no-cache` on HTML and `immutable` on
# /assets, so a freshly-shipped index.html immediately points browsers at the new hashed assets
# (the server-side half of cache-busting).
#
# Usage:  ./scripts/deploy.sh          (from anywhere; resolves its own paths)
#         DEST=/tmp/integrity-preview ./scripts/deploy.sh   (dry-run somewhere else)
#
# In Claude Code you can run this yourself with:  ! ./scripts/deploy.sh
set -euo pipefail

ENGINE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"   # …/integrity-engine
WEB_DIR="$ENGINE_DIR/web"
DIST="$WEB_DIR/dist"
DEST="${DEST:-/var/www/integrity}"

echo "▶ building release bundle (wasm:release + vite build) in $WEB_DIR …"
# **Say what is about to go live, and refuse to guess.** A deploy once shipped from a `main` that did not
# contain the work being deployed: the PR merge had failed ("base branch was modified"), a `;` let the
# deploy run regardless, and it printed a cheerful "deployed" over the previous release. The build step
# cannot tell — it builds whatever is checked out — so the check belongs here.
branch="$(git rev-parse --abbrev-ref HEAD)"
commit="$(git log --oneline -1)"
if [[ -n "$(git status --porcelain)" ]]; then
  echo "▶ deploying UNCOMMITTED work from $branch" >&2
else
  echo "▶ deploying $branch — $commit" >&2
fi
if [[ "$branch" == "main" ]]; then
  git fetch --quiet origin main 2>/dev/null || true
  if [[ -n "$(git log --oneline HEAD..origin/main 2>/dev/null)" ]]; then
    echo "deploy: REFUSING — origin/main is ahead of this checkout. Pull first, or you will publish" >&2
    echo "        an older build over a newer one." >&2
    exit 1
  fi
fi

( cd "$WEB_DIR" && npm run build )

[ -f "$DIST/index.html" ] || { echo "✗ build produced no $DIST/index.html — aborting" >&2; exit 1; }

echo "▶ publishing $DIST → $DEST"
mkdir -p "$DEST"
# --delete so stale content-hashed assets from prior builds don't accumulate; the build reproduces the
# full page set (index/terrain/orbit/twomoons/birth + assets), so nothing live is lost.
rsync -a --delete "$DIST/" "$DEST/"

echo "✓ deployed — live at https://integrity.bothead.net  (nginx :8080 via the Cloudflare tunnel)"
