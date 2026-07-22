// **Mouse-look, one implementation, every scene.**
//
// Robin's scheme, which together gives 360° control in every scene:
//   * **right-drag, or alt/option + drag → LOOK**, pivoting from where the camera is as a fixed point.
//     The camera turns its head; it does not orbit a target and does not move.
//   * **left-button (or ctrl) held → move FORWARD** along the look direction.
//   * **shift + left-button (or shift+ctrl) → move BACKWARD.**
//
// Consistency matters more than cleverness: a viewer who learns the control in one scene must not have
// to relearn it in the next.
//
// Each scene previously grew its own pointer handling (ground orbited on plain left-drag, terra called
// `drag_look`, orbit tracked a pinch map), so the same gesture meant three different things. This module
// is the one answer; a scene supplies what to DO with the delta and keeps its own scene-specific gestures
// (pinch-zoom, wheel) around it.
//
// **Why the camera must not translate here:** the camera is matter (docs, canonical) and obeys the same
// contact law as a grain. Turning in place cannot push it into the ground, so mouse-look needs no
// collision pass; anything that MOVES the eye does, and that stays with the scene that owns the rig.

export type LookHandler = (dyawRad: number, dpitchRad: number) => void;

/** What the camera is being asked to do this frame. */
export interface CameraIntent {
  /** −1 backward, 0 still, +1 forward. Polled per frame so movement is smooth and frame-rate driven. */
  forward(): number;
  /** Stop listening. */
  detach(): void;
}

export interface CameraInputOptions {
  /** Radians of rotation per pixel of mouse travel. */
  sensitivity?: number;
  /** Invert the vertical axis. */
  invertY?: boolean;
}

/**
 * Attach the shared camera controls to a canvas.
 *
 * LOOK is delivered as a callback (it is a rotation, applied immediately). MOVEMENT is POLLED via
 * `forward()` rather than pushed, because it must be integrated per frame against real time — and
 * because moving the eye has to go through the scene's camera-shell collision (the camera is matter and
 * obeys the same contact law as a grain), which only the scene can do.
 */
export function attachCameraInput(
  canvas: HTMLCanvasElement,
  onLook: LookHandler,
  opts: CameraInputOptions = {},
): CameraIntent {
  const { sensitivity = 0.005, invertY = false } = opts;
  let looking = false;
  let lastX = 0;
  let lastY = 0;
  let pointerId: number | null = null;

  // A gesture counts as look if the right button is held (buttons bit 2) or ALT is down. `buttons` is
  // used rather than `button` so a modifier pressed mid-drag still reads correctly on move events.
  const isLook = (e: PointerEvent | MouseEvent): boolean => (e.buttons & 2) !== 0 || e.altKey;

  // Movement state, polled each frame. `ctrl` is the keyboard equivalent of holding the left button, so
  // the scheme works on a trackpad without a right button or a comfortable click-drag.
  let leftHeld = false;
  let ctrlHeld = false;
  let shiftHeld = false;

  const down = (e: PointerEvent) => {
    if ((e.buttons & 1) !== 0 && !e.altKey) leftHeld = true;
    shiftHeld = e.shiftKey;
    if (!isLook(e)) return;
    looking = true;
    lastX = e.clientX;
    lastY = e.clientY;
    pointerId = e.pointerId;
    canvas.setPointerCapture(e.pointerId);
    e.preventDefault();
  };
  const move = (e: PointerEvent) => {
    if (!looking) return;
    // Releasing the modifier/button mid-drag ends the look rather than leaving it stuck on.
    if (!isLook(e)) {
      looking = false;
      return;
    }
    shiftHeld = e.shiftKey;
    const dx = e.clientX - lastX;
    const dy = e.clientY - lastY;
    lastX = e.clientX;
    lastY = e.clientY;
    onLook(-dx * sensitivity, (invertY ? dy : -dy) * sensitivity);
    e.preventDefault();
  };
  const up = (e: PointerEvent) => {
    if ((e.buttons & 1) === 0) leftHeld = false;
    if (!looking) return;
    looking = false;
    if (pointerId !== null && canvas.hasPointerCapture(pointerId)) {
      canvas.releasePointerCapture(pointerId);
    }
    pointerId = null;
  };
  // Without this, right-dragging opens the context menu over the canvas mid-look.
  const menu = (e: Event) => e.preventDefault();

  const key = (e: KeyboardEvent) => {
    ctrlHeld = e.ctrlKey || e.metaKey;
    shiftHeld = e.shiftKey;
  };
  // Releasing a modifier while the window is unfocused would otherwise leave the camera walking forever.
  const blur = () => {
    leftHeld = false;
    ctrlHeld = false;
    shiftHeld = false;
    looking = false;
  };

  canvas.addEventListener("pointerdown", down);
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
  canvas.addEventListener("contextmenu", menu);
  window.addEventListener("keydown", key);
  window.addEventListener("keyup", key);
  window.addEventListener("blur", blur);

  return {
    forward: () => {
      const moving = leftHeld || ctrlHeld;
      if (!moving) return 0;
      return shiftHeld ? -1 : 1;
    },
    detach: () => {
      canvas.removeEventListener("pointerdown", down);
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      canvas.removeEventListener("contextmenu", menu);
      window.removeEventListener("keydown", key);
      window.removeEventListener("keyup", key);
      window.removeEventListener("blur", blur);
    },
  };
}

/** The controls hint every scene shows, so the wording is identical everywhere too. */
export const CAMERA_HINT =
  "right-drag or alt+drag to look · left-click or ctrl to go forward · +shift to reverse";
