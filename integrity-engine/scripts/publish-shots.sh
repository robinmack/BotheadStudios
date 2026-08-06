#!/usr/bin/env bash
# **Publish rig shots to the live site WITHOUT rebuilding it.**
#
# Robin (2026-08-04): *"Can we find a way to set up a photo gallery of your PNGs accessible from
# integrity.bothead.net with newest at the top, thumbnails that expand when clicked… I think that would
# be helpful for me to see what you see without requiring a full deployment."*
#
# So this copies PNGs and writes a manifest. It touches nothing else — no wasm build, no vite build, no
# HTML. `gallery.html` itself deploys once with the site and then just reads whatever manifest is there.
#
#   scripts/publish-shots.sh            # publish to the live site (needs the deploy dir writable)
#   DEST=web/public/shots scripts/publish-shots.sh   # publish locally, for `npx vite`
#
# Newest first, grouped by run so a descent ladder or an A/B pair reads as one thing.
set -euo pipefail
cd "$(dirname "$0")/.."

DEST="${DEST:-/var/www/integrity/shots}"
SOURCES=("${RIGSHOT_DIR:-/tmp/rigshot}" "web/shots")

mkdir -p "$DEST"
# Start clean so a deleted shot actually disappears rather than lingering forever.
find "$DEST" -maxdepth 1 -name '*.png' -delete 2>/dev/null || true

n=0
for src in "${SOURCES[@]}"; do
  [ -d "$src" ] || continue
  while IFS= read -r -d '' f; do
    cp -p "$f" "$DEST/$(basename "$f")"
    n=$((n + 1))
  done < <(find "$src" -maxdepth 1 -name '*.png' -print0)
done

# The manifest: name, url, mtime, and a GROUP taken from the filename's stem before the first digit or
# dash-number, so `flora-3m` / `flora-120m` land together and `season-maine-oct` with its siblings.
python3 - "$DEST" <<'PY'
import json, os, re, sys
dest = sys.argv[1]
rows = []
for name in os.listdir(dest):
    if not name.endswith(".png"):
        continue
    p = os.path.join(dest, name)
    stem = name[:-4]
    # Group = the leading words before the first numeric-ish segment.
    parts = stem.split("-")
    keep = []
    for seg in parts:
        if re.match(r"^\d", seg) or re.match(r"^[0-9]+p[0-9]+", seg):
            break
        keep.append(seg)
    group = "-".join(keep[:2]) if keep else "shots"
    rows.append({
        "name": stem,
        "url": f"/shots/{name}",
        "mtime_ms": int(os.path.getmtime(p) * 1000),
        "group": group,
    })
# Newest first, and keep a run together once its newest member has placed it.
rows.sort(key=lambda r: -r["mtime_ms"])
order, seen = [], {}
for r in rows:
    seen.setdefault(r["group"], len(seen))
rows.sort(key=lambda r: (seen[r["group"]], -r["mtime_ms"]))
with open(os.path.join(dest, "manifest.json"), "w") as f:
    json.dump(rows, f, indent=1)
print(f"  manifest: {len(rows)} shots in {len(seen)} groups")
PY

echo "✓ published $n PNGs → $DEST"
echo "  gallery: https://integrity.bothead.net/gallery.html  (or /gallery.html on the dev server)"
