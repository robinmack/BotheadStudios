import { defineConfig } from "vite";

// wasm-pack (--target web) emits ESM glue that fetches `*_bg.wasm` via `import.meta.url`.
// Vite serves that fine in dev; for build we make sure .wasm is treated as an asset and the
// glue isn't pre-bundled (which would break the relative wasm URL).
export default defineConfig({
  assetsInclude: ["**/*.wasm"],
  server: {
    fs: {
      // Allow importing the generated wasm package that lives under src/wasm.
      allow: [".."],
    },
  },
  optimizeDeps: {
    exclude: ["engine"],
  },
});
