#!/usr/bin/env bash
#
# dev-lan.sh — launch (or reuse) the Integrity engine LAN dev server over HTTPS for on-device
# (iPad / VR headset / phone) testing, and print the URLs.
#
# Designed to be cheap to re-run:
#   • If an Integrity dev server is already up on the port, it is REUSED (no rebuild, no restart).
#   • The wasm is rebuilt ONLY when the Rust core changed since the last build (make-style: rebuild
#     iff a source file is newer than the built wasm) — so a plain restart costs ~a second, and we
#     never serve stale code.
#
# Usage:  ./scripts/dev-lan.sh          (from anywhere; resolves its own paths)
#         PORT=5173 ./scripts/dev-lan.sh
#
# In Claude Code you can run this yourself with:  ! ./scripts/dev-lan.sh
set -euo pipefail

PORT="${PORT:-5173}"
ENGINE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"   # …/integrity-engine
WEB_DIR="$ENGINE_DIR/web"
WASM_OUT="$WEB_DIR/src/wasm"
WASM_FILE="$WASM_OUT/engine_bg.wasm"
LOG="${TMPDIR:-/tmp}/greenfield-dev-lan.log"

# First non-loopback IPv4 (so we don't hardcode a LAN address that may change).
lan_ip() { hostname -I 2>/dev/null | tr ' ' '\n' | grep -E '^[0-9]+\.' | grep -v '^127\.' | head -1; }
IP="$(lan_ip)"; IP="${IP:-127.0.0.1}"

print_urls() {
  echo "    terrain slice : https://$IP:$PORT/"
  echo "    space band    : https://$IP:$PORT/orbit.html"
  echo "    GPU probe     : https://$IP:$PORT/gpu-probe.html"
  echo "  (same Wi-Fi; accept the self-signed cert on first load)"
}

serving_body() { curl -sk --max-time 4 "https://127.0.0.1:$PORT/" 2>/dev/null || true; }

# Sentinel identifying OUR dev server in the served `/` body. MUST be a string that actually appears
# in web/index.html — this used to grep for "greenfield", which appears nowhere under web/ (it
# survives only as the wgpu device label in lib.rs and never reaches the HTML). The effect was that
# the reuse check below never matched and the readiness loop always timed out, so the script exited 1
# after a perfectly healthy start. "Integrity" is in the <title> and the page body.
SENTINEL="Integrity"

# ── 1. Reuse an already-running dev server ───────────────────────────────────────────────────────
BODY="$(serving_body)"
if grep -qi "$SENTINEL" <<<"$BODY"; then
  echo "✓ Integrity dev server already running on :$PORT — reusing it."
  echo "  (hard-refresh the page on the device to pick up the latest wasm build)"
  print_urls
  exit 0
elif [[ -n "$BODY" ]]; then
  echo "✗ port $PORT is serving a DIFFERENT app. Stop it or set PORT=… and re-run." >&2
  exit 1
fi

# ── 2. Rebuild wasm only if the Rust core changed ────────────────────────────────────────────────
needs_build() {
  [[ -f "$WASM_FILE" ]] || return 0   # never built
  # Rebuild if any Rust source / manifest / bundled data / SHADER is newer than the built wasm.
  # `shaders/` and `*.wgsl` are in this list because every .wgsl is include_str!'d into the wasm
  # (e.g. lib.rs:1980 for particle_step.wgsl) — editing a shader DOES change the binary. They were
  # previously missing, so a shader edit reported "✓ wasm up to date" and served the OLD shader:
  # silently stale results, which is the worst possible failure for on-device verification.
  local newer
  newer="$(find "$ENGINE_DIR/crates" "$ENGINE_DIR/data" "$ENGINE_DIR/shaders" -type f \
             \( -name '*.rs' -o -name '*.toml' -o -name '*.json' -o -name '*.wgsl' \) \
             -newer "$WASM_FILE" 2>/dev/null | head -1)"
  [[ -n "$newer" ]]
}

if needs_build; then
  echo "· building wasm (Rust core changed)…"
  ( cd "$ENGINE_DIR" && wasm-pack build crates/engine --target web --out-dir "$WASM_OUT" --dev )
else
  echo "✓ wasm up to date (Rust core unchanged) — skipping rebuild."
fi

# ── 3. Start vite (LAN + HTTPS) in the background ────────────────────────────────────────────────
echo "· starting vite on :$PORT (LAN, HTTPS)…"
( cd "$WEB_DIR" && LAN=1 nohup ./node_modules/.bin/vite --port "$PORT" >"$LOG" 2>&1 & )

for _ in $(seq 1 40); do
  if grep -qi "$SENTINEL" <<<"$(serving_body)"; then
    echo "✓ ready."
    print_urls
    echo "  (server log: $LOG)"
    exit 0
  fi
  sleep 1
done

echo "✗ server did not come up within ~40s; last log lines:" >&2
tail -n 20 "$LOG" >&2 || true
exit 1
