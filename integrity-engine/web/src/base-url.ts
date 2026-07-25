// Join a root-absolute path ("/worlds/...", "/sky/stars.bin", "/bodies/...") to the site's base.
// Vite rewrites asset URLs it can see at build time, but runtime fetches and the URLs the engine
// hands back (body_surface_urls) are strings it cannot touch. When the site is mounted under a
// subpath (BASE_PATH at build time, e.g. GitHub Pages at /<repo>/), those root-absolute paths would
// escape the site; this maps them under the base instead. Relative paths pass through unchanged.
export function withBase(path: string): string {
  return path.startsWith("/") ? import.meta.env.BASE_URL + path.slice(1) : path;
}
