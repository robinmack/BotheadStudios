// Integrity landing page. Renders the real engineering JOURNAL (imported as raw markdown at build time)
// and stamps the build id, so the page itself proves it's the freshly-shipped copy (no stale Safari cache).
// No WASM/GPU here — this is just the front door.

// Vite inlines the repo-root JOURNAL.md as a string (fs.allow includes "..").
import journalRaw from "../../JOURNAL.md?raw";

function esc(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

// Inline formatting: code spans, then bold, then italics. (Escape HTML first so the journal's own
// `<angle>` snippets render literally.)
function inline(s: string): string {
  return esc(s)
    .replace(/`([^`]+)`/g, "<code>$1</code>")
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/(^|[^*])\*([^*]+)\*/g, "$1<em>$2</em>");
}

// A deliberately small markdown renderer covering exactly what the journal uses: h1–h3, ---, bold/code/
// italic, unordered + ordered lists, GFM tables, and paragraphs. Not a general parser — our own content.
function renderMarkdown(md: string): string {
  const lines = md.split("\n");
  const out: string[] = [];
  let i = 0;
  let listType: "ul" | "ol" | null = null;
  const closeList = (): void => {
    if (listType) {
      out.push(`</${listType}>`);
      listType = null;
    }
  };
  const isBlock = (l: string): boolean => /^(#{1,3}\s|[-*]\s|\d+\.\s|\||---\s*$|\s*$)/.test(l);

  while (i < lines.length) {
    const line = lines[i];

    // Table block: consecutive lines starting with "|".
    if (/^\s*\|/.test(line)) {
      closeList();
      const rows: string[] = [];
      while (i < lines.length && /^\s*\|/.test(lines[i])) rows.push(lines[i++]);
      const cells = (r: string): string[] =>
        r.trim().replace(/^\||\|$/g, "").split("|").map((c) => c.trim());
      out.push("<table>");
      rows.forEach((r, idx) => {
        if (idx === 1 && /^[\s|:-]+$/.test(r)) return; // header separator row
        const tag = idx === 0 ? "th" : "td";
        out.push("<tr>" + cells(r).map((c) => `<${tag}>${inline(c)}</${tag}>`).join("") + "</tr>");
      });
      out.push("</table>");
      continue;
    }

    let m: RegExpExecArray | null;
    if ((m = /^###\s+(.*)/.exec(line))) {
      closeList();
      out.push(`<h3>${inline(m[1])}</h3>`);
      i++;
      continue;
    }
    if ((m = /^##\s+(.*)/.exec(line))) {
      closeList();
      out.push(`<h2>${inline(m[1])}</h2>`);
      i++;
      continue;
    }
    if ((m = /^#\s+(.*)/.exec(line))) {
      closeList();
      out.push(`<h1>${inline(m[1])}</h1>`);
      i++;
      continue;
    }
    if (/^---\s*$/.test(line)) {
      closeList();
      out.push("<hr>");
      i++;
      continue;
    }
    const um = /^[-*]\s+(.*)/.exec(line);
    const om = /^\d+\.\s+(.*)/.exec(line);
    if (um || om) {
      const want = um ? "ul" : "ol";
      if (listType !== want) {
        closeList();
        out.push(`<${want}>`);
        listType = want;
      }
      out.push(`<li>${inline((um ?? om)![1])}</li>`);
      i++;
      continue;
    }
    if (/^\s*$/.test(line)) {
      closeList();
      i++;
      continue;
    }
    // Paragraph: gather until a blank line or the next block element.
    closeList();
    const para: string[] = [line];
    i++;
    while (i < lines.length && !isBlock(lines[i])) para.push(lines[i++]);
    out.push(`<p>${inline(para.join(" "))}</p>`);
  }
  closeList();
  return out.join("\n");
}

const body = document.getElementById("journal-body");
if (body) body.innerHTML = renderMarkdown(journalRaw);

const stamp = document.getElementById("build-stamp");
if (stamp) stamp.textContent = `build ${__BUILD_ID__}`;

// ── Hero: a live velocity-Verlet N-body gravity field — F = G·mᵢmⱼ/r², integrated honestly.
// Website-only (no engine/WASM, per this file's charter): a 2-D sketch of the same law the engine runs at
// planetary scale. The telemetry strip reads the ACTUAL sim state — nothing scripted. No-ops if #sim is
// absent, mirroring the guards above.
const sim = document.getElementById("sim") as HTMLCanvasElement | null;
const simCtx = sim ? sim.getContext("2d") : null;
if (sim && simCtx) {
  const ctx = simCtx;
  const tmN = document.getElementById("tm-n");
  const tmSteps = document.getElementById("tm-steps");
  const tmKe = document.getElementById("tm-ke");
  const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  const G = 4.2e6; // scale constant (px³·mass⁻¹·s⁻²): sets the timescale, NOT the law
  const SOFT = 26; // Plummer softening (px) so close passes don't singular-kick
  const MAX_BODIES = 40;
  interface Body {
    x: number;
    y: number;
    vx: number;
    vy: number;
    m: number;
    primary: boolean;
  }
  let bodies: Body[] = [];
  let w = 0;
  let h = 0;
  let steps = 0;
  let acc: { ax: Float64Array; ay: Float64Array } = { ax: new Float64Array(0), ay: new Float64Array(0) };

  const resize = (): void => {
    const dpr = Math.min(2, window.devicePixelRatio || 1);
    w = sim.clientWidth;
    h = sim.clientHeight;
    sim.width = Math.round(w * dpr);
    sim.height = Math.round(h * dpr);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.fillStyle = "#080a0e";
    ctx.fillRect(0, 0, w, h); // opaque base so the motion-blur trails build cleanly
  };

  // Softened accelerations: aᵢ = Σ_j G·mⱼ·(rⱼ−rᵢ)/(|Δ|²+ε²)^{3/2} — exactly F = G·mᵢmⱼ/r² ÷ mᵢ.
  const accelOf = (bs: Body[]): { ax: Float64Array; ay: Float64Array } => {
    const n = bs.length;
    const ax = new Float64Array(n);
    const ay = new Float64Array(n);
    for (let i = 0; i < n; i++) {
      for (let j = 0; j < n; j++) {
        if (i === j) continue;
        const dx = bs[j].x - bs[i].x;
        const dy = bs[j].y - bs[i].y;
        const r2 = dx * dx + dy * dy + SOFT * SOFT;
        const inv = (G * bs[j].m) / (r2 * Math.sqrt(r2));
        ax[i] += dx * inv;
        ay[i] += dy * inv;
      }
    }
    return { ax, ay };
  };

  const seed = (): void => {
    bodies = [{ x: w / 2, y: h / 2, vx: 0, vy: 0, m: 1, primary: true }];
    const n = 13;
    for (let i = 0; i < n; i++) {
      const r = 72 + (i / n) * Math.min(w, h) * 0.42;
      const a = i * 2.399963; // golden-angle spread
      const v = Math.sqrt(G / r) * (0.9 + 0.18 * ((i * 0.618) % 1)); // ≈ circular orbit of the primary
      bodies.push({
        x: w / 2 + Math.cos(a) * r,
        y: h / 2 + Math.sin(a) * r,
        vx: -Math.sin(a) * v,
        vy: Math.cos(a) * v,
        m: 0.006,
        primary: false,
      });
    }
    acc = accelOf(bodies);
    steps = 0;
  };

  // One velocity-Verlet step: x += v·dt + ½a·dt²; recompute a; v += ½(a+a')·dt.
  const step = (dt: number): void => {
    const n = bodies.length;
    if (acc.ax.length !== n) acc = accelOf(bodies);
    for (let i = 0; i < n; i++) {
      bodies[i].x += bodies[i].vx * dt + 0.5 * acc.ax[i] * dt * dt;
      bodies[i].y += bodies[i].vy * dt + 0.5 * acc.ay[i] * dt * dt;
    }
    const a2 = accelOf(bodies);
    for (let i = 0; i < n; i++) {
      bodies[i].vx += 0.5 * (acc.ax[i] + a2.ax[i]) * dt;
      bodies[i].vy += 0.5 * (acc.ay[i] + a2.ay[i]) * dt;
    }
    acc = a2;
    steps++;
    // A satellite that has genuinely escaped far off-canvas is recycled to a fresh orbit (an honest
    // departure + arrival; the KE readout reflects it). The primary is never recycled.
    const lim = Math.max(w, h) * 1.35;
    let recycled = false;
    for (const b of bodies) {
      if (b.primary) continue;
      if (Math.abs(b.x - w / 2) > lim || Math.abs(b.y - h / 2) > lim) {
        const r = 80 + ((steps * 53) % 100) * 0.01 * Math.min(w, h) * 0.3;
        const a = (steps * 2.399963) % (Math.PI * 2);
        const v = Math.sqrt(G / r);
        b.x = w / 2 + Math.cos(a) * r;
        b.y = h / 2 + Math.sin(a) * r;
        b.vx = -Math.sin(a) * v;
        b.vy = Math.cos(a) * v;
        recycled = true;
      }
    }
    if (recycled) acc = accelOf(bodies);
  };

  const render = (): void => {
    ctx.fillStyle = "rgba(8,10,14,0.20)"; // fade the previous frame → glowing motion trails
    ctx.fillRect(0, 0, w, h);
    for (const b of bodies) {
      ctx.beginPath();
      ctx.arc(b.x, b.y, b.primary ? 3.4 : 1.7, 0, Math.PI * 2);
      ctx.fillStyle = b.primary ? "#ffd9a0" : "#ffb454";
      ctx.globalAlpha = b.primary ? 0.95 : 0.82;
      ctx.fill();
    }
    ctx.globalAlpha = 1;
  };

  const telemetry = (): void => {
    if (tmN) tmN.textContent = String(bodies.length);
    if (tmSteps) tmSteps.textContent = steps.toLocaleString();
    if (tmKe) {
      let ke = 0;
      for (const b of bodies) ke += 0.5 * b.m * (b.vx * b.vx + b.vy * b.vy);
      tmKe.textContent = ke.toFixed(0);
    }
  };

  // Drag to toss in a mass: it spawns where you pressed, with velocity along the drag — then obeys the field.
  let dragFrom: { x: number; y: number } | null = null;
  const local = (e: PointerEvent): { x: number; y: number } => {
    const rect = sim.getBoundingClientRect();
    return { x: e.clientX - rect.left, y: e.clientY - rect.top };
  };
  sim.addEventListener("pointerdown", (e) => {
    dragFrom = local(e);
  });
  window.addEventListener("pointerup", (e) => {
    if (!dragFrom) return;
    const to = local(e);
    bodies.push({
      x: dragFrom.x,
      y: dragFrom.y,
      vx: (to.x - dragFrom.x) * 2.2,
      vy: (to.y - dragFrom.y) * 2.2,
      m: 0.02,
      primary: false,
    });
    while (bodies.length > MAX_BODIES) {
      const idx = bodies.findIndex((b) => !b.primary);
      if (idx < 0) break;
      bodies.splice(idx, 1);
    }
    acc = accelOf(bodies);
    dragFrom = null;
  });

  resize();
  seed();
  window.addEventListener("resize", resize);
  if (reduceMotion) {
    // Honor prefers-reduced-motion: paint one static frame, wire the readouts, don't animate.
    ctx.fillStyle = "#080a0e";
    ctx.fillRect(0, 0, w, h);
    render();
    telemetry();
  } else {
    let last = 0;
    const frame = (t: number): void => {
      const dt = last ? Math.min((t - last) / 1000, 1 / 30) : 1 / 60;
      last = t;
      for (let s = 0; s < 3; s++) step(dt / 3); // substep for stability at high frame deltas
      render();
      telemetry();
      requestAnimationFrame(frame);
    };
    requestAnimationFrame(frame);
  }
}

// TODO(hero field): paint the hero's live velocity-Verlet N-body field onto <canvas id="sim"> and wire
// the telemetry readouts (#tm-int / #tm-n / #tm-steps / #tm-ke). Both are commented out in index.html
// until this exists — the copy claims "F = G·m/r² integrated live", so it must be a real sim, not a loop.
// Full spec + guardrails: web/HERO-FIELD-HANDOFF.md. Guard on the element (it may be absent) like above.
