/*
 * The playground runner.
 *
 * It parses the same .jsonl script the binary parses, validates it against the same op set,
 * and executes it against a same-origin iframe while camera.js drives the view. What it does
 * NOT do is produce a file: there is no ffmpeg in a browser tab and pretending otherwise would
 * be the wrong kind of demo. This shows you the camera. The binary and the hosted service make
 * the MP4.
 *
 * The iframe has to be same-origin, because the runner reads bounding boxes out of it. That is
 * why the target is a short list of bundled apps rather than a URL box: a URL box would look
 * like it worked and then fail on every real site with a cross-origin error.
 */

import { Camera } from "./camera.js";

const APPS = {
  "app": "Parcel, a tracking form",
};

const els = {};
let running = null;

/* ------------------------------------------------------------------ parsing */

const OPS = {
  navigate: { need: ["url"], allow: ["url"] },
  wait: { need: [], allow: ["ms", "selector", "timeout_ms"] },
  start_recording: { need: [], allow: ["path"] },
  stop_recording: { need: [], allow: [] },
  click: { need: [], allow: ["selector", "x", "y"] },
  type: { need: ["text"], allow: ["selector", "text", "typewriter_ms"] },
  scroll: { need: [], allow: ["y", "smooth"] },
  mark: { need: ["label"], allow: ["label"] },
};

/** Validate the whole script before running any of it, the way the binary does. */
export function parseScript(text) {
  const ops = [];
  const errors = [];
  text.split("\n").forEach((line, i) => {
    const n = i + 1;
    const s = line.trim();
    if (!s) return;
    let o;
    try {
      o = JSON.parse(s);
    } catch (e) {
      errors.push(`line ${n}: not JSON (${e.message})`);
      return;
    }
    if (typeof o !== "object" || o === null || Array.isArray(o)) {
      errors.push(`line ${n}: every line must be a JSON object`);
      return;
    }
    const spec = OPS[o.op];
    if (!spec) {
      errors.push(`line ${n}: unknown op ${JSON.stringify(o.op ?? null)}`);
      return;
    }
    for (const k of spec.need) {
      if (o[k] === undefined) errors.push(`line ${n}: ${o.op} needs "${k}"`);
    }
    for (const k of Object.keys(o)) {
      if (k !== "op" && !spec.allow.includes(k)) {
        errors.push(`line ${n}: ${o.op} has no field "${k}"`);
      }
    }
    if (o.op === "wait" && o.ms === undefined && o.selector === undefined) {
      errors.push(`line ${n}: wait needs either "ms" or "selector"`);
    }
    if (o.op === "click" && o.selector === undefined && (o.x === undefined || o.y === undefined)) {
      errors.push(`line ${n}: click needs "selector", or both "x" and "y"`);
    }
    if (o.op === "type" && o.selector === undefined) {
      errors.push(`line ${n}: type needs "selector"`);
    }
    ops.push({ n, o });
  });

  /*
   * The two script-level mistakes that produce a video rather than an error, which is what
   * makes them worth catching here rather than at the end of a take.
   */
  const rec = ops.findIndex((p) => p.o.op === "start_recording");
  const stop = ops.findIndex((p) => p.o.op === "stop_recording");
  if (rec === -1) errors.push("nothing is recorded: the script has no start_recording");
  if (stop === -1) errors.push("the take never ends: the script has no stop_recording");
  if (rec > -1 && stop > -1) {
    const tail = ops.slice(0, stop).reverse().find((p) => p.o.op !== "mark");
    if (tail && tail.o.op !== "wait") {
      errors.push(
        "the last thing before stop_recording is a " +
          tail.o.op +
          ", so its zoom will be cut off. End with a wait of 1600ms or more."
      );
    }
  }
  return { ops, errors };
}

/* ---------------------------------------------------------------- execution */

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function doc() {
  return els.frame.contentDocument;
}

function boxOf(el) {
  const r = el.getBoundingClientRect();
  return { x: r.left, y: r.top, w: r.width, h: r.height };
}

/** Roughly where the caret is, for the follow. The binary measures this properly over CDP. */
function caretBox(input) {
  const r = input.getBoundingClientRect();
  const cs = getComputedStyle(input);
  const c = document.createElement("canvas").getContext("2d");
  c.font = `${cs.fontStyle} ${cs.fontWeight} ${cs.fontSize} ${cs.fontFamily}`;
  const w = c.measureText(input.value).width;
  const left = r.left + parseFloat(cs.paddingLeft || 0) + parseFloat(cs.borderLeftWidth || 0);
  return {
    x: Math.min(left + w, r.right - 2),
    y: r.top + 4,
    w: 2,
    h: Math.max(r.height - 8, 8),
  };
}

function resolve(selector, n) {
  const el = doc().querySelector(selector);
  if (!el) throw new Error(`line ${n}: selector never became visible: ${selector}`);
  const b = boxOf(el);
  if (b.w === 0 || b.h === 0) throw new Error(`line ${n}: ${selector} has no size on screen`);
  /*
   * The occlusion check. An overlay over the target is the single most common reason a
   * generated take films the wrong thing, and clicking through it would be worse than
   * refusing: the video would look plausible and be wrong.
   */
  const top = doc().elementFromPoint(b.x + b.w / 2, b.y + b.h / 2);
  if (top && top !== el && !el.contains(top) && !top.contains(el)) {
    const id = top.id ? `#${top.id}` : top.className ? `.${String(top.className).split(" ")[0]}` : "";
    throw new Error(
      `line ${n}: selector ${selector} is covered by <${top.tagName.toLowerCase()}${id}> at its centre point`
    );
  }
  return { el, box: b };
}

function log(kind, text, bad) {
  const li = document.createElement("li");
  li.className = bad ? "bad" : "";
  li.innerHTML = `<b>${kind}</b> ${text.replace(/[<&]/g, (c) => (c === "<" ? "&lt;" : "&amp;"))}`;
  els.log.appendChild(li);
  els.log.scrollTop = els.log.scrollHeight;
}

async function runOp({ n, o }, cam, state) {
  switch (o.op) {
    case "navigate": {
      const name = String(o.url).replace(/^.*\//, "").replace(/\.html$/, "");
      if (!APPS[name]) {
        throw new Error(
          `line ${n}: the playground can only drive the bundled apps (${Object.keys(APPS).join(
            ", "
          )}). The binary navigates anywhere.`
        );
      }
      await new Promise((res) => {
        els.frame.addEventListener("load", res, { once: true });
        els.frame.src = name;
      });
      log("navigate", name);
      return;
    }
    case "wait": {
      if (o.selector !== undefined) {
        const deadline = performance.now() + (o.timeout_ms ?? 20000);
        for (;;) {
          const el = doc().querySelector(o.selector);
          if (el && el.getBoundingClientRect().width > 0) break;
          if (performance.now() > deadline) {
            throw new Error(`line ${n}: selector never became visible: ${o.selector}`);
          }
          await sleep(40);
        }
        log("wait", o.selector);
      } else {
        await sleep(o.ms);
        log("wait", `${o.ms}ms`);
      }
      return;
    }
    case "start_recording":
      state.recording = true;
      els.rec.hidden = false;
      log("start_recording", "the take begins here");
      return;
    case "stop_recording":
      state.recording = false;
      els.rec.hidden = true;
      log("stop_recording", `${state.interactions} interaction(s) filmed`);
      return;
    case "mark":
      log("mark", o.label);
      return;
    case "scroll": {
      doc().scrollingElement.scrollTo({ top: o.y, behavior: o.smooth ? "smooth" : "auto" });
      await sleep(o.smooth ? 500 : 60);
      log("scroll", `y=${o.y}`);
      return;
    }
    case "click": {
      let box;
      if (o.selector !== undefined) {
        const r = resolve(o.selector, n);
        box = r.box;
        cam.aim(box, "click", performance.now() / 1000);
        state.interactions++;
        await sleep(260); // the camera is allowed to arrive before the click lands
        r.el.click();
        log("click", o.selector);
      } else {
        box = { x: o.x, y: o.y, w: 2, h: 2 };
        cam.aim(box, "click", performance.now() / 1000);
        state.interactions++;
        await sleep(260);
        log("click", `${o.x},${o.y}`);
      }
      return;
    }
    case "type": {
      const { el } = resolve(o.selector, n);
      el.focus();
      el.value = "";
      const per = o.typewriter_ms ?? 18;
      state.interactions++;
      for (const ch of String(o.text)) {
        el.value += ch;
        el.dispatchEvent(new els.frame.contentWindow.Event("input", { bubbles: true }));
        // Re-aim at the caret as it moves. In the binary this is a CDP measurement every
        // 0.12s; here it is every character, which the deadzone flattens out either way.
        cam.aim(caretBox(el), "type", performance.now() / 1000);
        await sleep(per);
      }
      log("type", `${o.selector} <- ${JSON.stringify(o.text)}`);
      return;
    }
  }
}

/* -------------------------------------------------------------------- driver */

function startCamera(cam) {
  let last = performance.now();
  let stop = false;
  const tick = (now) => {
    if (stop) return;
    // Clamp the step: a backgrounded tab hands back a gap of seconds, and integrating that in
    // one go throws the spring across the frame.
    const dt = Math.min((now - last) / 1000, 1 / 20);
    last = now;
    cam.step(dt, now / 1000);
    els.stage.style.transform = cam.transform();
    requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
  return () => {
    stop = true;
  };
}

async function run() {
  if (running) return;
  els.log.innerHTML = "";
  const { ops, errors } = parseScript(els.script.value);
  if (errors.length) {
    errors.forEach((e) => log("error", e, true));
    return;
  }
  els.run.disabled = true;
  els.run.textContent = "Running";

  const frame = { w: els.frame.clientWidth, h: els.frame.clientHeight };
  const cam = new Camera(frame);
  const stopCam = startCamera(cam);
  const state = { recording: false, interactions: 0 };
  running = true;

  try {
    for (const op of ops) await runOp(op, cam, state);
    if (state.interactions === 0) {
      log("error", "no click or type resolved a box, so the take has no camera work at all", true);
    }
  } catch (e) {
    log("error", e.message, true);
  } finally {
    // Let the last hold lapse on screen rather than snapping back the moment the script ends.
    setTimeout(() => {
      stopCam();
      els.stage.style.transform = "";
      els.rec.hidden = true;
      els.run.disabled = false;
      els.run.textContent = "Run";
      running = null;
    }, 2600);
  }
}

function boot() {
  for (const id of ["script", "run", "frame", "stage", "log", "rec", "reset"]) {
    els[id] = document.getElementById(id);
  }
  els.run.addEventListener("click", run);
  els.reset.addEventListener("click", () => {
    els.script.value = els.script.dataset.default;
    els.log.innerHTML = "";
  });
  els.script.dataset.default = els.script.value;
}

document.addEventListener("DOMContentLoaded", boot);
