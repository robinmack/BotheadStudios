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

/** Pointer travel while panning, in CSS pixels (screen x right, screen y down). */
export type PanHandler = (dxPx: number, dyPx: number) => void;

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
  /**
   * PAN: **shift + left-drag, or middle-drag** translates the view sideways, one gesture in every
   * scene, mapped to each camera's own semantics: the orbit band slides its look target off the
   * focused body; the free-fly scenes translate laterally in the view plane through the same mover
   * as their strafe keys. A scene that supplies no handler keeps the plain grammar, where
   * shift + left-button means move backward. When a handler IS supplied, shift + left-button
   * always means pan, no matter whether shift or the button landed first, and reverse movement
   * lives on shift + ctrl; while a pan drag is active the movement keys are not engaged, so the
   * two meanings never fire together.
   */
  onPan?: PanHandler;
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
  const { sensitivity = 0.005, invertY = false, onPan } = opts;
  let looking = false;
  let panning = false;
  let lastX = 0;
  let lastY = 0;
  let pointerId: number | null = null;

  // A gesture counts as look if the right button is held (buttons bit 2) or ALT is down. `buttons` is
  // used rather than `button` so a modifier pressed mid-drag still reads correctly on move events.
  const isLook = (e: PointerEvent | MouseEvent): boolean => (e.buttons & 2) !== 0 || e.altKey;
  // A gesture counts as pan (when the scene supplied a handler) if the middle button is held
  // (buttons bit 4) or shift accompanies the left button. Alt keeps its look meaning.
  const isPan = (e: PointerEvent | MouseEvent): boolean =>
    !!onPan && ((e.buttons & 4) !== 0 || ((e.buttons & 1) !== 0 && e.shiftKey && !e.altKey));

  // Movement state, polled each frame. `ctrl` is the keyboard equivalent of holding the left button, so
  // the scheme works on a trackpad without a right button or a comfortable click-drag.
  let leftHeld = false;
  let ctrlHeld = false;
  let shiftHeld = false;

  // Anchor a drag on this pointer: remember where it is and capture it, so the gesture keeps
  // tracking outside the canvas and a later event can measure a delta from here.
  const anchor = (e: PointerEvent) => {
    lastX = e.clientX;
    lastY = e.clientY;
    pointerId = e.pointerId;
    canvas.setPointerCapture(e.pointerId);
  };

  const down = (e: PointerEvent) => {
    if (isPan(e)) {
      // A pan drag owns the pointer: the left button (if any) drives the pan, not the walk.
      panning = true;
      anchor(e);
      e.preventDefault();
      return;
    }
    if ((e.buttons & 1) !== 0 && !e.altKey) {
      leftHeld = true;
      // Anchor even though this begins as a walk: a real hand often lands the button a beat
      // before shift registers, and the drag must be able to BECOME a pan mid-hold.
      anchor(e);
    }
    shiftHeld = e.shiftKey;
    if (!isLook(e)) return;
    looking = true;
    anchor(e);
    e.preventDefault();
  };
  const move = (e: PointerEvent) => {
    // Shift may arrive AFTER the button (the common human ordering race). The moment a
    // left-drag that started on the canvas shows the pan chord, it upgrades to a pan,
    // measuring from here so there is no jump. Without this, the gesture fell into the
    // shift+left = reverse-walk meaning and pan looked dead to a real user.
    if (!panning && !looking && pointerId === e.pointerId && isPan(e)) {
      panning = true;
      leftHeld = false;
      lastX = e.clientX;
      lastY = e.clientY;
    }
    if (!looking && !panning) return;
    // Releasing the modifier/button mid-drag ends the gesture rather than leaving it stuck on;
    // a still-held left button falls back to its plain meaning (walk forward).
    if (panning && !isPan(e)) {
      panning = false;
      leftHeld = (e.buttons & 1) !== 0 && !e.altKey;
      return;
    }
    if (looking && !isLook(e)) {
      looking = false;
      return;
    }
    shiftHeld = e.shiftKey;
    const dx = e.clientX - lastX;
    const dy = e.clientY - lastY;
    lastX = e.clientX;
    lastY = e.clientY;
    if (panning) onPan?.(dx, dy);
    else onLook(-dx * sensitivity, (invertY ? dy : -dy) * sensitivity);
    e.preventDefault();
  };
  const up = (e: PointerEvent) => {
    if ((e.buttons & 1) === 0) leftHeld = false;
    // Gestures only ever ride the anchored pointer; another pointer lifting (a second touch)
    // must not end them.
    if (pointerId === null || e.pointerId !== pointerId) return;
    if (canvas.hasPointerCapture(pointerId)) canvas.releasePointerCapture(pointerId);
    pointerId = null;
    looking = false;
    panning = false;
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
    panning = false;
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
      if (panning) return 0;
      // DECISION: in a scene with a pan handler, shift + left BUTTON is the pan chord and never
      // the reverse walk. The same two inputs cannot mean both, and which one fired used to
      // depend on which finger won a millisecond race at pointerdown. Reverse stays fully
      // available on the keyboard side of the grammar (shift + ctrl).
      if (onPan && leftHeld && shiftHeld && !ctrlHeld) return 0;
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
