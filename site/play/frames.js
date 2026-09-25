/*
 * Device frames for the playground: the same platforms the binary's --frame and the
 * {"op":"frame"} line draw, done in CSS round the live iframe.
 *
 * The binary renders its chrome once per take and composites it over the video. Here the
 * chrome is ordinary markup beside the viewport, so the camera, which transforms #stage
 * inside the viewport, moves the app and never the frame, exactly as in the MP4.
 *
 * A phone is laid out at its real size in points (393 or 412 wide) and the whole device is
 * scaled to fit with CSS zoom, so the app inside sees a phone's width and lays itself out
 * as it would on one, and every box the runner measures is in the app's own pixels.
 */

export const PLATFORMS = [
  { id: "none", label: "No frame" },
  { id: "macos", label: "macOS" },
  { id: "windows", label: "Windows" },
  { id: "linux", label: "Linux" },
  { id: "ios", label: "iPhone" },
  { id: "ios:recording", label: "iPhone screen recording" },
  { id: "android", label: "Android" },
  { id: "android:recording", label: "Android screen recording" },
  { id: "ios-simulator", label: "iOS Simulator" },
  { id: "android-emulator", label: "Android Emulator" },
];

/** The picker's value for a frame op, and back. */
export function platformOf(op) {
  if (!op) return "none";
  const p = op.platform ?? op.frame ?? "none";
  return op.style === "recording" ? `${p}:recording` : p;
}

export function opFor(id) {
  if (id === "none") return null;
  const [platform, style] = id.split(":");
  return style ? { op: "frame", platform, style } : { op: "frame", platform };
}

const PHONES = {
  ios: { w: 393, h: 852, status: 54, home: 34 },
  android: { w: 412, h: 915, status: 36, home: 24 },
};

const svg = (w, h, body, extra = "") =>
  `<svg width="${w}" height="${h}" viewBox="0 0 ${w} ${h}" ${extra} aria-hidden="true">${body}</svg>`;

const ICON = {
  signal: svg(18, 12, '<rect x="0" y="8" width="3" height="4" rx=".8"/><rect x="5" y="5.5" width="3" height="6.5" rx=".8"/><rect x="10" y="3" width="3" height="9" rx=".8"/><rect x="15" y="0" width="3" height="12" rx=".8"/>', 'fill="currentColor"'),
  wifi: svg(16, 12, '<path d="M8 2.4c2.3 0 4.4.9 6 2.3l1.2-1.3A10.4 10.4 0 0 0 8 .6C5.2.6 2.7 1.7.8 3.4L2 4.7a8.6 8.6 0 0 1 6-2.3zM8 5.9c1.4 0 2.6.5 3.6 1.3l1.2-1.3A7.2 7.2 0 0 0 8 4.1c-1.8 0-3.5.7-4.8 1.8l1.2 1.3c1-.8 2.2-1.3 3.6-1.3zM8 9.2c.5 0 1 .2 1.3.5L8 11.2 6.7 9.7c.3-.3.8-.5 1.3-.5z"/>', 'fill="currentColor"'),
  battery: svg(27, 13, '<rect x=".5" y=".5" width="23" height="12" rx="3.8" fill="none" stroke="currentColor" stroke-opacity=".4"/><rect x="2" y="2" width="20" height="9" rx="2.4" fill="currentColor"/><path d="M25 4.5v4c.8-.3 1.3-1.1 1.3-2s-.5-1.7-1.3-2z" fill="currentColor" fill-opacity=".45"/>'),
  cell: svg(14, 14, '<path d="M14 0v14H0z"/>', 'fill="currentColor"'),
  rec: svg(16, 16, '<circle cx="8" cy="8" r="7" fill="none" stroke="currentColor" stroke-width="1.6"/><circle cx="8" cy="8" r="3.6" fill="currentColor"/>'),
};

const lights = '<span class="dv-lights"><i></i><i></i><i></i></span>';

function desktopBar(os, browser) {
  const controls =
    os === "windows"
      ? '<span class="dv-winctl"><i>&#x2013;</i><i>&#x25a1;</i><i>&#x2715;</i></span>'
      : os === "linux"
      ? '<span class="dv-gnomectl"><i>&#x2013;</i><i>&#x25a1;</i><i>&#x2715;</i></span>'
      : "";
  if (!browser) {
    return `<div class="dv-titlebar">${os === "macos" ? lights : ""}<b>Parcel</b>${controls}</div>`;
  }
  return `<div class="dv-tabs">${os === "macos" ? lights : ""}<span class="dv-tab"><i class="dv-fav"></i>Parcel<em>&#x2715;</em></span><span class="dv-plus">+</span>${controls}</div>
<div class="dv-toolbar"><span class="dv-nav">&#x2190; &#x2192; &#x21bb;</span><span class="dv-omni">kaviri.dev<em>/play/app</em></span></div>`;
}

function phoneStatus(kind, recording) {
  if (kind === "ios") {
    return `<div class="dv-status dv-status-ios"><span class="dv-clock${recording ? " dv-recpill" : ""}">9:41</span>${
      recording ? "" : '<span class="dv-island"></span>'
    }<span class="dv-icons">${ICON.signal}${ICON.wifi}${ICON.battery}</span></div>`;
  }
  return `<div class="dv-status dv-status-android"><span class="dv-clock">9:41${
    recording ? `<span class="dv-rec">${ICON.rec}</span>` : ""
  }</span>${recording ? "" : '<span class="dv-hole"></span>'}<span class="dv-icons">${ICON.wifi}${ICON.cell}</span></div>`;
}

const emuToolbar = `<div class="dv-emu">${["&#x2715; &#x2013;", "&#x23fb;", "&#x1f50a;", "&#x1f509;", "&#x27f2;", "&#x27f3;", "&#x25ce;", "&#x25c1;", "&#x25cb;", "&#x25a1;", "&#x22ef;"]
  .map((g) => `<i>${g}</i>`)
  .join("")}</div>`;

/**
 * Put `id`'s frame round the viewport. `els` holds device (the wrapper), top and bottom (the
 * chrome slots either side of the viewport) and viewport.
 */
export function applyFrame(els, id) {
  const [platform, style] = id.split(":");
  const recording = style === "recording";
  const kind = platform.startsWith("ios") ? "ios" : platform.startsWith("android") ? "android" : null;
  const d = els.device;
  d.dataset.platform = platform;
  d.dataset.style = recording ? "recording" : "";
  d.dataset.kind = kind || (platform === "none" ? "none" : "desktop");
  els.top.innerHTML = "";
  els.bottom.innerHTML = "";
  els.side.innerHTML = "";
  els.over.innerHTML = "";

  if (kind) {
    const p = PHONES[kind];
    const vh = recording ? p.h - p.status - p.home : p.h - p.status - p.home;
    els.viewport.style.width = `${p.w}px`;
    els.viewport.style.height = `${vh}px`;
    els.viewport.style.aspectRatio = "auto";
    els.top.innerHTML = phoneStatus(kind, recording);
    els.bottom.innerHTML = '<div class="dv-homebar"><i></i></div>';
    if (platform === "android-emulator") els.side.innerHTML = emuToolbar;
    if (platform === "ios-simulator") {
      els.over.innerHTML = `<div class="dv-simbar">${lights}<span><b>iPhone 16 Pro</b><small>iOS 18.2</small></span><span class="dv-simicons">&#x2302; &#x25ce; &#x27f2;</span></div>`;
    }
  } else {
    els.viewport.style.width = "";
    els.viewport.style.height = "";
    els.viewport.style.aspectRatio = "";
    if (platform !== "none") els.top.innerHTML = desktopBar(platform, true);
  }
  fit(els);
}

/** Scale a phone to the space it has. A desktop frame is fluid already. */
export function fit(els) {
  const d = els.device;
  if (d.dataset.kind === "none" || d.dataset.kind === "desktop") {
    d.style.zoom = "";
    return;
  }
  d.style.zoom = "1";
  const room = els.device.parentElement.clientWidth;
  const natural = d.offsetWidth + (d.dataset.platform === "android-emulator" ? 0 : 0);
  const naturalH = d.offsetHeight;
  const z = Math.min(1, (room - 8) / natural, 640 / naturalH);
  d.style.zoom = String(Math.max(0.3, z));
}
