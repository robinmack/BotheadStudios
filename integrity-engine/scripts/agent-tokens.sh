#!/usr/bin/env bash
# Near-real-time token usage for background agents in the current Claude Code session.
# Parses each agent's JSONL transcript and sums per-message usage. Output tokens are the
# cost driver (what an agent actually GENERATES); input/cache shown for context.
#
#   bash scripts/agent-tokens.sh              # one-shot snapshot
#   watch -n 5 bash scripts/agent-tokens.sh   # live meter, refreshes every 5s
#
# Optional arg: a tasks dir (defaults to the newest session's tasks/ under /tmp/claude-*).
set -uo pipefail
TASKS_DIR="${1:-$(ls -dt /tmp/claude-*/*/*/tasks 2>/dev/null | head -1)}"
[ -d "$TASKS_DIR" ] || { echo "no tasks dir found (looked in /tmp/claude-*/*/*/tasks)"; exit 1; }

python3 - "$TASKS_DIR" <<'PY'
import json, os, sys, time, glob
tasks_dir = sys.argv[1]
now = time.time()

def human(n):
    for unit, div in (("M",1_000_000),("k",1_000)):
        if n >= div: return f"{n/div:.1f}{unit}"
    return str(int(n))

def ago(secs):
    s=int(secs)
    if s < 60: return f"{s}s ago"
    if s < 3600: return f"{s//60}m ago"
    return f"{s//3600}h ago"

rows=[]; grand_out=0
for path in glob.glob(os.path.join(tasks_dir, "*.output")):
    o_in=o_out=o_cc=o_cr=0; label=""; had_usage=False
    try:
        with open(path) as f:
            for line in f:
                line=line.strip()
                if not line: continue
                try: obj=json.loads(line)
                except: continue
                if not isinstance(obj,dict): continue
                m=obj.get("message")
                if not isinstance(m,dict): continue
                if not label:  # first human-readable text = the task prompt's first line
                    cont=m.get("content")
                    if isinstance(cont,list):
                        for c in cont:
                            if isinstance(c,dict) and c.get("type")=="text" and str(c.get("text","")).strip():
                                label=c["text"].strip().splitlines()[0][:46]; break
                u=m.get("usage")
                if isinstance(u,dict):
                    had_usage=True
                    o_in += u.get("input_tokens",0) or 0
                    o_out+= u.get("output_tokens",0) or 0
                    o_cc += u.get("cache_creation_input_tokens",0) or 0
                    o_cr += u.get("cache_read_input_tokens",0) or 0
    except FileNotFoundError:
        continue
    if not had_usage: continue
    mtime=os.path.getmtime(path)
    active = (now - mtime) < 180   # touched in last 3min ≈ still working (compiles/tests go quiet)
    aid=os.path.basename(path).replace(".output","")[:8]
    rows.append((active, mtime, aid, label, o_out, o_in, o_cc+o_cr))
    grand_out += o_out

window_min = float(os.environ.get("AGENT_TOKENS_WINDOW_MIN", "20"))
recent = [r for r in rows if (now - r[1]) < window_min*60]
older  = [r for r in rows if (now - r[1]) >= window_min*60]
recent.sort(key=lambda r:(not r[0], -r[1]))   # live first, then most-recently-updated
print(f"agent tokens · {time.strftime('%H:%M:%S')} · current wave (last {int(window_min)}m)")
print("─"*74)
if not recent:
    print("  (no agent active in the last {}m — all quiet)".format(int(window_min)))
for active, mtime, aid, label, out, inp, cache in recent:
    tag = "▶ live" if active else "· idle"
    print(f"{tag} {aid}  out {human(out):>6}  in {human(inp):>6}  cache {human(cache):>6}  {ago(now-mtime):>7}  {label}")
print("─"*74)
live_out=sum(r[4] for r in recent)
print(f"output this wave: {human(live_out)}   ·   session total: {human(grand_out)} ({len(older)} older agents hidden)")
PY
