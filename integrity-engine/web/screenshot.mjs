// Headless-Chromium screenshot harness for verifying the WebGPU render.
//   node screenshot.mjs <url> <out.png> [waitMs] [action]
// action: "dig:x,y", "blast:x,y" (normalized 0..1 canvas coords), "orbit:dx,dy", or omit.
import { chromium } from "playwright";

const url = process.argv[2] || "https://localhost:5173/";
const out = process.argv[3] || "shot.png";
const waitMs = Number(process.argv[4] || 6000);
const action = process.argv[5] || "";

const browser = await chromium.launch({
  headless: true,
  args: [
    "--no-sandbox",
    "--enable-unsafe-webgpu",
    "--enable-features=Vulkan",
    "--ignore-gpu-blocklist",
    "--enable-gpu",
  ],
});
const ctx = await browser.newContext({
  ignoreHTTPSErrors: true,
  viewport: { width: 1280, height: 800 },
  deviceScaleFactor: 1,
});
const page = await ctx.newPage();
page.on("console", (m) => console.log("PAGE", m.type(), m.text()));
page.on("pageerror", (e) => console.log("PAGEERR", e.message));

await page.goto(url, { waitUntil: "load", timeout: 30000 });
await page.waitForTimeout(waitMs);

if (action) {
  const [kind, coords] = action.split(":");
  const [nx, ny] = coords.split(",").map(Number);
  const x = Math.round(nx * 1280);
  const y = Math.round(ny * 800);
  if (kind === "dig") await page.mouse.click(x, y);
  else if (kind === "blast") {
    await page.mouse.move(x, y);
    await page.mouse.down();
    await page.waitForTimeout(700); // long-press
    await page.mouse.up();
  } else if (kind === "orbit") {
    await page.mouse.move(640, 400);
    await page.mouse.down();
    await page.mouse.move(640 + nx, 400 + ny, { steps: 10 });
    await page.mouse.up();
  }
  await page.waitForTimeout(3000);
}

await page.screenshot({ path: out });
console.log("saved", out);
await browser.close();
