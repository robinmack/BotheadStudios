// Scene picker — switch between the engine's simulations. Injected on every scene page (side-effect
// import), so the list of scenes lives in ONE place: add an entry here and the picker updates
// everywhere. Pure DOM, no dependency on the WASM/GPU having loaded, so it works even if a scene
// fails to start.

type Scene = { path: string; label: string };

const SCENES: Scene[] = [
  { path: "/", label: "Home" },
  { path: "/orbit.html", label: "Space" },
  { path: "/birth.html", label: "Birth of the Moon" },
  { path: "/ground.html", label: "Ground" },
  { path: "/terra.html", label: "Earth" },
  { path: "/twomoons.html", label: "Two Moons" },
];

function install(): void {
  // Normalise "/index.html" → "/" so the landing page highlights correctly.
  const here = window.location.pathname.replace(/\/index\.html$/, "/");

  const nav = document.createElement("nav");
  nav.setAttribute("aria-label", "scene picker");
  Object.assign(nav.style, {
    position: "fixed",
    top: "30px", // clear the top-left "Integrity · … · build …" version line (was 10px, overlapped it)
    left: "50%",
    transform: "translateX(-50%)",
    zIndex: "20",
    display: "flex",
    gap: "4px",
    padding: "4px",
    background: "rgba(20,24,40,0.6)",
    border: "1px solid rgba(255,255,255,0.18)",
    borderRadius: "999px",
    backdropFilter: "blur(6px)",
    font: "600 14px/1 system-ui, sans-serif",
  });

  for (const s of SCENES) {
    const current = here === s.path;
    const a = document.createElement("a");
    a.href = s.path;
    a.textContent = s.label;
    a.setAttribute("aria-current", current ? "page" : "false");
    Object.assign(a.style, {
      textDecoration: "none",
      padding: "7px 14px",
      borderRadius: "999px",
      color: current ? "#0b0e18" : "#dfe6ff",
      background: current ? "#dfe6ff" : "transparent",
      touchAction: "manipulation",
    });
    nav.appendChild(a);
  }

  document.body.appendChild(nav);
}

if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", install);
} else {
  install();
}
