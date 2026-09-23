/*
 * The camera model, ported from src/zoom.rs.
 *
 * This is a preview, not the renderer. The MP4 is produced by the binary, which runs the same
 * model at a fixed 120Hz and then hands a decimated path to ffmpeg. What runs here is the same
 * spring with the same constants, stepped at whatever rate the browser paints at, so it shows
 * you what kaviri will do without pretending to be the thing that does it.
 *
 * Every constant below is checked against src/zoom.rs by a test in that file. If you change one
 * here and not there, `cargo test the_preview_constants_match` fails, which is the only thing
 * keeping these two copies honest.
 */

export const EASE = 0.7;
export const LEAD_IN = 0.45;
export const HOLD_AFTER = 2.1;
export const FIT_MARGIN = 1.15;
export const LEFT_BIAS_TYPE = 0.18;
export const LEFT_BIAS_CLICK = 0.12;
export const KEEP_IN_FRAME = 0.06;
export const SPRING_TAU = 0.38;
export const DEADZONE = 0.07;
export const MAX_SPEED = 1.1;

/**
 * Where the camera should sit for one interaction, and how tight.
 *
 * box is in app pixels, {x, y, w, h}. frame is {w, h}. kind is "click" or "type".
 */
export function aimAt(box, frame, kind) {
  // Bigger targets get a gentler zoom, then whichever axis runs out first bounds it, so a
  // full width nav bar gets z = 1 and no zoom rather than the tightest one.
  const ladder = box.h > frame.h * 0.45 ? 1.5 : box.h > frame.h * 0.25 ? 1.7 : 1.85;
  const fitW = frame.w / (box.w * FIT_MARGIN);
  const fitH = frame.h / (box.h * FIT_MARGIN);
  const z = Math.max(1, Math.min(ladder, fitW, fitH));

  const cropW = frame.w / z;
  const want = kind === "type" ? LEFT_BIAS_TYPE : LEFT_BIAS_CLICK;
  // The lean has to leave room for the target itself AND for the camera trailing by a
  // deadzone, because the trailing leans the same way the bias does.
  const room = 0.5 - box.w / 2 / cropW - KEEP_IN_FRAME - DEADZONE;
  const bias = Math.min(want, Math.max(room, 0));

  return {
    cx: box.x + box.w / 2 - cropW * bias,
    cy: box.y + box.h / 2,
    z,
  };
}

/** The part of an error outside the deadzone. Inside it, the camera does not move. */
function beyond(err, dead) {
  if (err > dead) return err - dead;
  if (err < -dead) return err + dead;
  return 0;
}

/**
 * A critically damped spring chasing a target.
 *
 * Velocity is a state variable, which is the whole reason this looks like a camera and a line
 * drawn between interactions does not. Position is continuous because everything is, and
 * velocity is continuous because it is integrated rather than assigned.
 */
export class Camera {
  constructor(frame) {
    this.frame = frame;
    this.cx = frame.w / 2;
    this.cy = frame.h / 2;
    this.z = 1;
    this.vx = 0;
    this.vy = 0;
    this.vz = 0;
    this.target = { cx: frame.w / 2, cy: frame.h / 2, z: 1 };
    this.heldUntil = 0;
  }

  /** Point the camera at an interaction. It holds there until HOLD_AFTER has passed. */
  aim(box, kind, now) {
    this.target = aimAt(box, this.frame, kind);
    this.heldUntil = now + HOLD_AFTER;
  }

  /** Let the hold lapse: back to the wide shot. */
  release() {
    this.target = { cx: this.frame.w / 2, cy: this.frame.h / 2, z: 1 };
  }

  step(dt, now) {
    if (now > this.heldUntil) this.release();

    const omega = 1 / SPRING_TAU;
    const cropW = this.frame.w / Math.max(this.z, 1);
    const cropH = this.frame.h / Math.max(this.z, 1);

    const tx = this.cx + beyond(this.target.cx - this.cx, cropW * DEADZONE);
    const ty = this.cy + beyond(this.target.cy - this.cy, cropH * DEADZONE);

    this.vx += (omega * omega * (tx - this.cx) - 2 * omega * this.vx) * dt;
    this.vy += (omega * omega * (ty - this.cy) - 2 * omega * this.vy) * dt;

    const vMax = MAX_SPEED * cropW;
    const speed = Math.hypot(this.vx, this.vy);
    if (speed > vMax) {
      const k = vMax / speed;
      this.vx *= k;
      this.vy *= k;
    }

    this.cx += this.vx * dt;
    this.cy += this.vy * dt;

    /*
     * The zoom is a spring too, slower than the pan. In the renderer z is a smoothstep over
     * EASE rather than a spring, because there it has a known start and end time. Here the
     * hold can be cut short by the next interaction, so there is no end time to ease towards
     * and a spring is the honest equivalent.
     */
    const oz = 1 / (EASE * 0.62);
    this.vz += (oz * oz * (this.target.z - this.z) - 2 * oz * this.vz) * dt;
    this.z += this.vz * dt;
    if (this.z < 1) {
      this.z = 1;
      this.vz = Math.max(this.vz, 0);
    }

    // Keep the crop inside the frame, the same clamp the ffmpeg expression applies.
    const halfW = this.frame.w / this.z / 2;
    const halfH = this.frame.h / this.z / 2;
    this.cx = Math.min(Math.max(this.cx, halfW), this.frame.w - halfW);
    this.cy = Math.min(Math.max(this.cy, halfH), this.frame.h - halfH);
  }

  /** The CSS transform that puts (cx, cy) in the middle of a frame-sized viewport. */
  transform() {
    const x = this.frame.w / 2 - this.cx * this.z;
    const y = this.frame.h / 2 - this.cy * this.z;
    return `translate(${x.toFixed(2)}px, ${y.toFixed(2)}px) scale(${this.z.toFixed(4)})`;
  }
}
