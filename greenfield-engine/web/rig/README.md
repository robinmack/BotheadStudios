# The watch rig — see it before shipping it

Headless-box visual verification for the WebGPU scenes (the "watch locally" rule: the agent watches
scenes with its own eyes before claiming a visual works — Robin is not the test runner).

Requires: `xvfb` (headed Chromium under a virtual display — headless mode cannot composite WebGPU
swapchains for screenshots), playwright (devDependency), a running vite dev server.

```bash
npx vite --port 5280 &                          # serve
xvfb-run -a node rig/watch.mjs                  # timed screenshots of orbit + birth → scratch dir
xvfb-run -a node rig/prof.mjs                   # advance()/render() ms per frame (found powf: 161→22 ms)
xvfb-run -a node rig/fps.mjs <url>              # HUD fps for a scene URL
```
