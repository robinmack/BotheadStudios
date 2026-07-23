// Cross-device GPU probe host.
//
// Drives `GpuProbe` (crates/engine/src/lib.rs), which runs the REAL particle_step.wgsl through the
// REAL GpuParticles — so results here are statements about shipping code, not a reimplementation.
//
// Answers three things on whatever device opens the page:
//   1. WHICH adapter ran (on iPadOS this is what proves the backend is Metal).
//   2. How per-frame cost scales with N — the saturation curve. A single point cannot distinguish
//      "GPU is slow" from "workload too small to use it" (JOURNAL 2026-07-19).
//   3. Whether total energy stays bounded. GpuParticles::dispatch splits its four stages into four
//      passes because fusing them can RACE on non-Vulkan backends (lib.rs, Metal / M4); a race shows
//      up as rising energy.
//
// Mirrors orbit.ts's log relay + status banner so a console-less device (iPad) is still debuggable:
// everything printed here also reaches the dev server via POST /__log.

import { report } from "./dev-log"; // FIRST — relay console/errors to the dev terminal before wasm loads
import init, { GpuProbe } from "./wasm/engine.js";

const statusEl = document.getElementById("status");
function setStatus(html: string, isError = false): void {
  if (statusEl) {
    statusEl.innerHTML = html;
    statusEl.className = isError ? "err" : "";
    statusEl.hidden = false;
  }
  report(isError ? "error" : "status", (statusEl?.textContent ?? html).slice(0, 400));
}

// Sweep points. Spans "far too small to use the GPU" → MAX_PARTICLES (lib.rs:155), so the knee is
// visible rather than inferred. `frames` is chosen per point so every run lasts long enough that the
// poll granularity below is a small fraction of it — a single frame is not measurable this way.
const SWEEP: Array<{ n: number; frames: number }> = [
  { n: 1, frames: 400 },
  { n: 1_000, frames: 300 },
  { n: 10_000, frames: 150 },
  { n: 60_000, frames: 60 },
];

interface ProbeResult {
  n: number;
  frames: number;
  substeps: number;
  grains: number;
  ke: number;
  pe: number;
  tot: number;
  vmax: number;
}

/// Resolve once the queued GPU batch has completed and the grains are read back. The engine cannot
/// block on a browser buffer map (`Maintain::Wait` is a no-op), so it exposes the same two-phase
/// pattern GpuParticles uses internally: submit, then poll. rAF is the cheapest poll that yields to
/// the event loop the map callback needs.
function awaitRun(probe: GpuProbe): Promise<void> {
  return new Promise((resolve, reject) => {
    const deadline = performance.now() + 120_000;
    const tick = (): void => {
      if (probe.poll()) {
        resolve();
        return;
      }
      if (performance.now() > deadline) {
        reject(new Error("GPU run did not complete within 120s"));
        return;
      }
      requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
  });
}

/// Adapter identity as the BROWSER reports it.
///
/// This — not wgpu's `AdapterInfo` — is the authoritative source in a browser. Under
/// `Backends::BROWSER_WEBGPU` wgpu delegates to the browser and cannot see the underlying driver, so
/// `adapter.get_info()` comes back with an empty name/driver and `backend: BrowserWebGpu`; it can
/// never tell you whether you are on Metal. `navigator.gpu`'s own `GPUAdapterInfo` does expose
/// `vendor`/`architecture` (e.g. "nvidia"/"turing", "apple"/"apple-m"), which is what actually answers
/// the question. Both are shown: the browser's for identity, wgpu's for the limits it negotiated.
async function browserAdapterInfo(): Promise<Record<string, unknown>> {
  try {
    const a = await navigator.gpu.requestAdapter({ powerPreference: "high-performance" });
    if (!a) return { error: "requestAdapter returned null" };
    const i = (a as GPUAdapter & { info?: GPUAdapterInfo }).info;
    return {
      vendor: i?.vendor ?? "(unreported)",
      architecture: i?.architecture ?? "(unreported)",
      device: i?.device ?? "",
      description: i?.description ?? "",
      fallback: (a as GPUAdapter & { isFallbackAdapter?: boolean }).isFallbackAdapter ?? false,
      maxBufferSize: a.limits?.maxBufferSize ?? 0,
    };
  } catch (e) {
    return { error: String(e) };
  }
}

const esc = (s: string): string => s.replace(/[<&]/g, (c) => (c === "<" ? "&lt;" : "&amp;"));

function renderAdapter(wgpuJson: string, browser: Record<string, unknown>): void {
  const a = JSON.parse(wgpuJson) as Record<string, unknown>;
  const dl = document.getElementById("adapter");
  if (!dl) return;
  const arch = String(browser.architecture ?? "");
  const rows: Array<[string, string]> = [
    ["vendor", String(browser.vendor ?? "?")],
    ["architecture", arch],
    ["device", String(browser.device || "(masked)")],
    ["description", String(browser.description || "(masked)")],
    ["fallback adapter", browser.fallback ? "YES — software" : "no"],
    ["wgpu backend", String(a.backend)],
    ["max buffer", `${(Number(a.max_buffer_size) / 1024 ** 2).toFixed(0)} MiB`],
  ];
  dl.innerHTML = rows.map(([k, v]) => `<dt>${k}</dt><dd>${esc(v)}</dd>`).join("");
  (document.getElementById("adapter-box") as HTMLElement).hidden = false;
}

function renderResults(rows: Array<{ r: ProbeResult; msPerFrame: number }>): void {
  const table = document.getElementById("results");
  if (!table) return;
  const head =
    "<tr><th>N</th><th>ms/frame</th><th>µs/particle</th><th>total E</th><th>v&nbsp;max</th></tr>";
  const body = rows
    .map(({ r, msPerFrame }) => {
      const perParticle = (msPerFrame * 1000) / r.n;
      return (
        `<tr><td>${r.n.toLocaleString()}</td>` +
        `<td><b>${msPerFrame.toFixed(3)}</b></td>` +
        `<td>${perParticle.toFixed(3)}</td>` +
        `<td>${r.tot.toExponential(3)}</td>` +
        `<td>${r.vmax.toFixed(3)}</td></tr>`
      );
    })
    .join("");
  table.innerHTML = head + body;
  (document.getElementById("results-box") as HTMLElement).hidden = false;

  // Per-particle cost falling with N means the GPU is being used; flat means fixed overhead dominates
  // everywhere and no shader tuning will help (see the gpu-perf method, §7).
  const note = document.getElementById("results-note");
  if (note && rows.length >= 2) {
    const first = (rows[0].msPerFrame * 1000) / rows[0].r.n;
    const last = (rows[rows.length - 1].msPerFrame * 1000) / rows[rows.length - 1].r.n;
    const ratio = first / last;
    note.innerHTML =
      ratio > 2
        ? `Per-particle cost falls ${ratio.toFixed(0)}× across the sweep — the GPU is doing real work at scale.`
        : `<span class="bad">Per-particle cost is roughly flat (${ratio.toFixed(1)}×)</span> — fixed overhead dominates at every N here.`;
  }
}

async function main(): Promise<void> {
  report("info", `build ${__BUILD_ID__}`);
  report("info", `UA: ${navigator.userAgent}`);
  report("info", `secureContext=${window.isSecureContext} · gpu in navigator=${"gpu" in navigator}`);

  if (!("gpu" in navigator)) {
    setStatus(
      "WebGPU is not available in this browser.<br><br>" +
        "On <b>iPad (Safari)</b>: Settings → Apps → Safari → Advanced → Feature Flags → " +
        "turn on <b>WebGPU</b>, then reload. (Needs iPadOS 18+.)<br><br>" +
        "Recent Chrome / Edge / Firefox also work.",
      true,
    );
    return;
  }
  // navigator.gpu only exists in a secure context — over LAN that means HTTPS. `npm run dev:lan`
  // handles this (LAN=1 adds basic-ssl); a plain `vite --host` would fail the check above instead.
  if (!window.isSecureContext) {
    setStatus(
      "Not a secure context, so WebGPU is unavailable.<br><br>" +
        "Serve over HTTPS — use <code>npm run dev:lan</code> (or <code>./scripts/dev-lan.sh</code>), " +
        "then accept the self-signed certificate.",
      true,
    );
    return;
  }

  try {
    setStatus("Loading engine… (compiling WASM)");
    // DEV wasm has a stable URL that Safari caches indefinitely; stamp it so a rebuild actually loads.
    await init(
      import.meta.env.DEV
        ? new URL(`./wasm/engine_bg.wasm?v=${__BUILD_ID__}`, import.meta.url)
        : undefined,
    );

    setStatus("Requesting GPU device…");
    const probe = await GpuProbe.create();

    const adapterJson = probe.gpu_adapter_json();
    const browser = await browserAdapterInfo();
    report("info", `adapter(browser): ${JSON.stringify(browser)}`);
    report("info", `adapter(wgpu): ${adapterJson}`);
    renderAdapter(adapterJson, browser);

    const rows: Array<{ r: ProbeResult; msPerFrame: number }> = [];
    for (const { n, frames } of SWEEP) {
      setStatus(`Running N=${n.toLocaleString()} · ${frames} frames…`);
      // Warm-up: first run of a given size pays shader/pipeline setup and clock ramp. Discarded.
      probe.start_run(n, 3);
      await awaitRun(probe);

      const t0 = performance.now();
      probe.start_run(n, frames);
      await awaitRun(probe);
      const elapsed = performance.now() - t0;

      const raw = probe.result_json();
      if (raw === "null") throw new Error(`no result for N=${n}`);
      const r = JSON.parse(raw) as ProbeResult;
      const msPerFrame = elapsed / r.frames;
      rows.push({ r, msPerFrame });
      report(
        "info",
        `RESULT n=${r.n} frames=${r.frames} ms/frame=${msPerFrame.toFixed(4)} ` +
          `us/particle=${((msPerFrame * 1000) / r.n).toFixed(4)} tot=${r.tot.toExponential(4)} vmax=${r.vmax.toFixed(4)}`,
      );
      renderResults(rows);
    }

    // Machine-readable handle for web/rig/gpu_probe.mjs, mirroring orbit.ts's `window.__demo`.
    (window as unknown as { __probe: unknown }).__probe = {
      adapter: JSON.parse(adapterJson),
      browser,
      rows: rows.map(({ r, msPerFrame }) => ({ ...r, ms_per_frame: msPerFrame })),
      done: true,
    };
    setStatus(
      `Done · ${rows.length} points · ${browser.vendor ?? "?"} / ${browser.architecture ?? "?"}`,
    );
    console.log("probe complete");
  } catch (e) {
    setStatus(`Probe failed: ${String(e)}`, true);
    throw e;
  }
}

void main();
