# The rigs — one-off instruments, not a test suite

A rig is a **playwright script that drives a scene and takes screenshots**, so a claim about what the
engine looks like or does can be checked with eyes instead of asserted (the "watch locally" rule: Robin
is not the test runner).

## Read this before trusting any rig in here

**Most of these were written as ONE-OFFS** — built to choose a path or to fuel a piece of documentation,
then finished. They are *not* proper tests, they are not maintained, and **it must not be assumed that
any of them is still relevant or valid.** A rig exiting 0 and writing a PNG means the script ran; it does
NOT mean the scene is correct, and a green rig is not evidence for a claim.

The engine's actual guards are the native suite (`bash scripts/test.sh`) and the out-of-process
verifiers (`tools/gpu-verify`, `tools/sph-verify`). A rig is for *looking*. If you need to verify
something visually, read the rig you are about to run and confirm it measures what you think — or write
a small new one, which is usually a dozen lines.

## Running one

```bash
bash scripts/start-render-xorg.sh        # once per session: GPU-backed Xorg :2 on the 5060 Ti
cd web && npx vite --port 5173 &         # serve the build
bash scripts/rigshot.sh birth_shot.mjs   # run a rig; screenshots land in $OUT
```

`rigshot.sh` is the only supported entry point: it forces WebGPU onto the same GPU as the X compositor
(`MESA_VK_DEVICE_SELECT`) and sets the shared defaults.

- `PORT` — dev-server port, default **5173**. Every rig reads it.
- `OUT` — screenshot directory, default **/tmp/rigshot** (created for you).
- `URL` — a few rigs take a full URL instead; these default to LOCAL.

**Do NOT use `xvfb-run`.** It is a software compositor and cannot read back the GPU swapchain, so
screenshots come back as the DOM HUD over a BLANK canvas. That trap cost prior sessions and this file
used to recommend it (CLAUDE.md rule 4).

**Restart vite after `npm run wasm`.** The wasm URL is cache-busted with a build stamp vite computes at
STARTUP, so a server left running from before the rebuild serves the OLD wasm — the rig then verifies
stale code while looking perfectly green. Check the `build` stamp in the HUD matches your build.

## History

Ports and output paths used to be hardcoded per rig — 13 different dead ports, and 30 rigs writing into
a long-gone session's scratchpad directory. That is now one default in `rigshot.sh`. It removed friction
in reusing a rig; it did not make any of them a test.
