// Thin browser host for greenfield-engine.
//
// Responsibilities live ONLY here: create the canvas backing store, load the WASM core, and
// pump requestAnimationFrame. All simulation and rendering happen inside the Rust/WASM `Engine`.

import init, { Engine } from "./wasm/engine.js";

function fail(message: string): void {
  const el = document.getElementById("error");
  if (el) {
    el.textContent = message;
    el.hidden = false;
  }
  console.error(message);
}

/** Size the canvas backing store to its CSS box, capped at 2x DPR to bound the pixel count. */
function sizeCanvas(canvas: HTMLCanvasElement): void {
  const dpr = Math.min(window.devicePixelRatio || 1, 2);
  const w = Math.max(1, Math.floor(canvas.clientWidth * dpr));
  const h = Math.max(1, Math.floor(canvas.clientHeight * dpr));
  canvas.width = w;
  canvas.height = h;
}

async function main(): Promise<void> {
  const canvas = document.getElementById("gpu-canvas") as HTMLCanvasElement | null;
  if (!canvas) {
    fail("Canvas element #gpu-canvas not found.");
    return;
  }

  if (!("gpu" in navigator)) {
    fail(
      "WebGPU is not available in this browser. Use a recent Chrome/Edge/Firefox, or Safari 26+.",
    );
    return;
  }

  sizeCanvas(canvas);

  try {
    await init(); // instantiate the WASM module
    const engine = await Engine.create(canvas);

    window.addEventListener("resize", () => {
      sizeCanvas(canvas);
      engine.resize(canvas.width, canvas.height);
    });

    const frame = () => {
      try {
        engine.render();
      } catch (e) {
        fail(`render error: ${String(e)}`);
        return; // stop the loop on a hard error
      }
      requestAnimationFrame(frame);
    };
    requestAnimationFrame(frame);
  } catch (e) {
    fail(`Failed to start engine: ${String(e)}`);
  }
}

void main();
