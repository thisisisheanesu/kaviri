// kaviri motion runtime.
//
// The page is a pure function of time. `KV.seek(t)` puts every layer where it
// belongs at t seconds and returns; kaviri photographs the result. Nothing here
// reads a clock or calls Math.random, and no CSS transition or animation runs
// on its own, so frame 900 looks the same whether it is rendered first, last,
// or by the third of four browsers working in parallel.
//
// The spec arrives as window.__KV_SPEC, compiled by src/motion.rs: every time
// is already in seconds, every asset inlined, and every name checked against
// the tables below.
(function () {
'use strict';

const S = window.__KV_SPEC;
const W = S.video.width, H = S.video.height;
const FPS = S.video.fps || 30;
const GRID = S.grid || { bpm: 120, bpb: 4, offset: 0 };
const BEAT = 60 / GRID.bpm;
const warnings = [];

// ---------------------------------------------------------------- utilities

// Fields of a keyframe that are not properties.
const NON_PROPS = { t: 1, at: 1, dur: 1, ease: 1, line: 1, op: 1, scene: 1, keys: 1 };
const clamp = (v, a, b) => (v < a ? a : v > b ? b : v);
const lerp = (a, b, t) => a + (b - a) * t;
const TAU = Math.PI * 2;

function hashStr(s) {
  let h = 2166136261 >>> 0;
  for (let i = 0; i < s.length; i++) { h ^= s.charCodeAt(i); h = Math.imul(h, 16777619); }
  return h >>> 0;
}
function rng(seed) {
  let a = seed >>> 0;
  return function () {
    a = (a + 0x6D2B79F5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
const SEED = (S.video.seed || 1) >>> 0;
// Smooth 1D value noise in -1..1, the same for the same (x, seed).
function noise(x, seed) {
  const i = Math.floor(x), f = x - i;
  const h = (n) => { const r = rng((n * 374761393 + seed * 668265263) >>> 0)(); return r * 2 - 1; };
  const u = f * f * (3 - 2 * f);
  return lerp(h(i), h(i + 1), u);
}

function parseColor(c) {
  if (Array.isArray(c)) return c;
  if (typeof c !== 'string') return null;
  const s = c.trim();
  if (s[0] === '#') {
    let h = s.slice(1);
    if (h.length === 3 || h.length === 4) h = h.split('').map((x) => x + x).join('');
    const n = parseInt(h, 16);
    if (h.length === 6) return [(n >> 16) & 255, (n >> 8) & 255, n & 255, 1];
    if (h.length === 8) return [(n >>> 24) & 255, (n >> 16) & 255, (n >> 8) & 255, (n & 255) / 255];
    return null;
  }
  const m = s.match(/^rgba?\(([^)]+)\)$/);
  if (m) {
    const p = m[1].split(/[ ,/]+/).filter(Boolean).map(parseFloat);
    return [p[0], p[1], p[2], p.length > 3 ? p[3] : 1];
  }
  return null;
}
const colorStr = (c) => `rgba(${Math.round(c[0])},${Math.round(c[1])},${Math.round(c[2])},${+c[3].toFixed(3)})`;
function mixColor(a, b, t) {
  const A = parseColor(a), B = parseColor(b);
  if (!A || !B) return t < 0.5 ? a : b;
  return colorStr([lerp(A[0], B[0], t), lerp(A[1], B[1], t), lerp(A[2], B[2], t), lerp(A[3], B[3], t)]);
}
function alpha(c, a) {
  const C = parseColor(c);
  return C ? colorStr([C[0], C[1], C[2], C[3] * a]) : c;
}
const isColor = (v) => typeof v === 'string' && (v[0] === '#' || v.startsWith('rgb'));

function esc(s) {
  return String(s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
}
// A length: a number of pixels, "50%" of the reference, or "40%+12".
function len(v, ref, dflt) {
  if (v === undefined || v === null) return dflt;
  if (typeof v === 'number') return v;
  const s = String(v).trim();
  const m = s.match(/^(-?[\d.]+)%\s*([+-]\s*[\d.]+)?$/);
  if (m) return (parseFloat(m[1]) / 100) * ref + (m[2] ? parseFloat(m[2].replace(/\s/g, '')) : 0);
  if (s.endsWith('px')) return parseFloat(s);
  if (s === 'c' || s === 'center') return ref / 2;
  const n = parseFloat(s);
  return isNaN(n) ? dflt : n;
}

// ---------------------------------------------------------------- easing

function bezier(x1, y1, x2, y2) {
  const cx = 3 * x1, bx = 3 * (x2 - x1) - cx, ax = 1 - cx - bx;
  const cy = 3 * y1, by = 3 * (y2 - y1) - cy, ay = 1 - cy - by;
  const sx = (t) => ((ax * t + bx) * t + cx) * t;
  const sy = (t) => ((ay * t + by) * t + cy) * t;
  const dx = (t) => (3 * ax * t + 2 * bx) * t + cx;
  return (x) => {
    let t = x;
    for (let i = 0; i < 8; i++) {
      const e = sx(t) - x, d = dx(t);
      if (Math.abs(e) < 1e-5 || Math.abs(d) < 1e-6) break;
      t -= e / d;
    }
    return sy(clamp(t, 0, 1));
  };
}
const c1 = 1.70158, c2 = c1 * 1.525, c3 = c1 + 1;
function bounceOut(x) {
  const n1 = 7.5625, d1 = 2.75;
  if (x < 1 / d1) return n1 * x * x;
  if (x < 2 / d1) return n1 * (x -= 1.5 / d1) * x + 0.75;
  if (x < 2.5 / d1) return n1 * (x -= 2.25 / d1) * x + 0.9375;
  return n1 * (x -= 2.625 / d1) * x + 0.984375;
}
const EASE = {
  linear: (x) => x,
  step: (x) => (x > 0 ? 1 : 0),
  inQuad: (x) => x * x,
  outQuad: (x) => 1 - (1 - x) * (1 - x),
  inOutQuad: (x) => (x < 0.5 ? 2 * x * x : 1 - Math.pow(-2 * x + 2, 2) / 2),
  inCubic: (x) => x * x * x,
  outCubic: (x) => 1 - Math.pow(1 - x, 3),
  inOutCubic: (x) => (x < 0.5 ? 4 * x * x * x : 1 - Math.pow(-2 * x + 2, 3) / 2),
  inQuart: (x) => x * x * x * x,
  outQuart: (x) => 1 - Math.pow(1 - x, 4),
  inOutQuart: (x) => (x < 0.5 ? 8 * x * x * x * x : 1 - Math.pow(-2 * x + 2, 4) / 2),
  inQuint: (x) => x * x * x * x * x,
  outQuint: (x) => 1 - Math.pow(1 - x, 5),
  inOutQuint: (x) => (x < 0.5 ? 16 * x * x * x * x * x : 1 - Math.pow(-2 * x + 2, 5) / 2),
  inExpo: (x) => (x === 0 ? 0 : Math.pow(2, 10 * x - 10)),
  outExpo: (x) => (x === 1 ? 1 : 1 - Math.pow(2, -10 * x)),
  inOutExpo: (x) => (x === 0 ? 0 : x === 1 ? 1 : x < 0.5 ? Math.pow(2, 20 * x - 10) / 2 : (2 - Math.pow(2, -20 * x + 10)) / 2),
  inCirc: (x) => 1 - Math.sqrt(1 - x * x),
  outCirc: (x) => Math.sqrt(1 - Math.pow(x - 1, 2)),
  inOutCirc: (x) => (x < 0.5 ? (1 - Math.sqrt(1 - Math.pow(2 * x, 2))) / 2 : (Math.sqrt(1 - Math.pow(-2 * x + 2, 2)) + 1) / 2),
  inBack: (x) => c3 * x * x * x - c1 * x * x,
  outBack: (x) => 1 + c3 * Math.pow(x - 1, 3) + c1 * Math.pow(x - 1, 2),
  inOutBack: (x) => (x < 0.5 ? (Math.pow(2 * x, 2) * ((c2 + 1) * 2 * x - c2)) / 2 : (Math.pow(2 * x - 2, 2) * ((c2 + 1) * (x * 2 - 2) + c2) + 2) / 2),
  outElastic: (x) => (x === 0 ? 0 : x === 1 ? 1 : Math.pow(2, -10 * x) * Math.sin((x * 10 - 0.75) * (TAU / 3)) + 1),
  outBounce: bounceOut,
  // A critically-underdamped spring that settles by x = 1.
  spring: (x) => (x >= 1 ? 1 : 1 - Math.exp(-6.5 * x) * Math.cos(x * 13) * (1 - x * 0.2)),
  // Most of the distance in the first fifth, then a long settle: the motion-designer's snap.
  snap: bezier(0.12, 0.9, 0.1, 1),
};
function easeFn(e, dflt) {
  if (Array.isArray(e)) return bezier(e[0], e[1], e[2], e[3]);
  return EASE[e] || EASE[dflt] || EASE.outCubic;
}

// ---------------------------------------------------------------- the beat

// Where t sits in its beat, 0 on the beat, and how hard the music is playing.
function beatPhase(t, every) {
  const p = (t - GRID.offset) / (every || BEAT);
  return p - Math.floor(p);
}
function beatIndex(t, every) { return Math.floor((t - GRID.offset) / (every || BEAT)); }
function energyAt(t) {
  const e = S.energy;
  if (!e || !e.length) return 1;
  for (const s of e) if (t >= s.start && t < s.end) return s.energy;
  return t < e[0].start ? 0 : e[e.length - 1].energy;
}
// A kick-shaped envelope: 1 on the beat, falling away before the next.
function beatEnv(t, every, sharp) {
  if (t < GRID.offset) return 0;
  return Math.exp(-beatPhase(t, every) * (sharp || 7));
}

// ---------------------------------------------------------------- theme

const T0 = S.theme || {};
const theme = {
  bg: T0.bg || S.video.bg || '#070b18',
  bg2: T0.bg2 || 'rgba(255,255,255,.04)',
  surface: T0.surface || '#10131c',
  surface2: T0.surface2 || '#151925',
  border: T0.border || 'rgba(255,255,255,.08)',
  borderStrong: T0.border_strong || 'rgba(255,255,255,.16)',
  hover: T0.hover || 'rgba(255,255,255,.06)',
  text: T0.text || '#f2f4f8',
  text2: T0.text2 || '#c9cfdb',
  muted: T0.muted || '#8a93a6',
  faint: T0.faint || '#5d6577',
  accent: T0.accent || '#5b8cff',
  accent2: T0.accent2 || T0.accent || '#9b7bff',
  ok: T0.ok || '#34c77b',
  track: T0.track || 'rgba(255,255,255,.14)',
  radius: T0.radius !== undefined ? T0.radius : 14,
  font: T0.font || 'Inter, "Inter Variable", "SF Pro Display", -apple-system, "Segoe UI", Helvetica, Arial, sans-serif',
  mono: T0.mono || '"JetBrains Mono", "SF Mono", Menlo, Consolas, "DejaVu Sans Mono", monospace',
  shadow: T0.shadow || '0 30px 80px -20px rgba(0,0,0,.65), 0 0 0 1px rgba(255,255,255,.02)',
};

// ---------------------------------------------------------------- the page

const root = document.getElementById('kv-root');
root.style.width = W + 'px';
root.style.height = H + 'px';
const rs = root.style;
rs.setProperty('--kv-bg', theme.bg);
rs.setProperty('--kv-bg2', theme.bg2);
rs.setProperty('--kv-surface', theme.surface);
rs.setProperty('--kv-surface2', theme.surface2);
rs.setProperty('--kv-border', theme.border);
rs.setProperty('--kv-border-strong', theme.borderStrong);
rs.setProperty('--kv-hover', theme.hover);
rs.setProperty('--kv-text', theme.text);
rs.setProperty('--kv-text2', theme.text2);
rs.setProperty('--kv-muted', theme.muted);
rs.setProperty('--kv-faint', theme.faint);
rs.setProperty('--kv-accent', theme.accent);
rs.setProperty('--kv-accent-wash', alpha(theme.accent, 0.16));
rs.setProperty('--kv-ok', theme.ok);
rs.setProperty('--kv-track', theme.track);
rs.setProperty('--kv-radius', theme.radius + 'px');
rs.setProperty('--kv-font', theme.font);
rs.setProperty('--kv-mono', theme.mono);
rs.setProperty('--kv-shadow', theme.shadow);
// Syntax colours follow the theme: pastels on a dark surface, deeper inks on a light one.
const surf = parseColor(theme.surface) || [16, 19, 28, 1];
const lightTheme = 0.2126 * surf[0] + 0.7152 * surf[1] + 0.0722 * surf[2] > 140;
const TOKENS = lightTheme
  ? { k: '#8a2be2', s: '#1f7a3a', n: '#b3470d', c: '#8a8a85', f: '#1d5fd1', t: '#9a6a00', p: '#4a5568' }
  : { k: '#c792ea', s: '#c3e88d', n: '#f78c6c', c: '#697098', f: '#82aaff', t: '#ffcb6b', p: '#89ddff' };
for (const k in TOKENS) rs.setProperty('--tk-' + k, T0['token_' + k] || TOKENS[k]);

for (const f of S.fonts || []) {
  const st = document.createElement('style');
  st.textContent = `@font-face{font-family:${JSON.stringify(f.family)};src:url(${f.src});font-weight:${f.weight};font-style:${f.style};font-display:block}`;
  document.head.appendChild(st);
}

function mk(tag, cls, parent) {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (parent) parent.appendChild(e);
  return e;
}

const bgEl = mk('div', '', root); bgEl.id = 'kv-bg';
const world = mk('div', '', root); world.id = 'kv-world';
world.style.transformOrigin = '50% 50%';
const over = mk('div', '', root); over.id = 'kv-over';
const flashEl = mk('div', '', over); flashEl.id = 'kv-flash'; flashEl.style.opacity = 0;
const vignetteEl = mk('div', '', over); vignetteEl.id = 'kv-vignette';
const grainEl = mk('div', '', over); grainEl.id = 'kv-grain';
const PERSPECTIVE = S.video.perspective || 1600;

// A place for SVG filters that need to be addressed by id.
const svgDefs = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
svgDefs.setAttribute('width', '0'); svgDefs.setAttribute('height', '0');
svgDefs.style.position = 'absolute';
root.appendChild(svgDefs);
function hblurFilter(id) {
  const f = document.createElementNS('http://www.w3.org/2000/svg', 'filter');
  f.setAttribute('id', id);
  f.setAttribute('x', '-20%'); f.setAttribute('width', '140%');
  f.setAttribute('y', '-5%'); f.setAttribute('height', '110%');
  const g = document.createElementNS('http://www.w3.org/2000/svg', 'feGaussianBlur');
  g.setAttribute('stdDeviation', '0 0');
  f.appendChild(g);
  svgDefs.appendChild(f);
  return g;
}

// Style writes are cached, so a layer that did not move costs nothing.
function setStyle(el, k, v) {
  const c = el.__kvs || (el.__kvs = {});
  if (c[k] === v) return;
  c[k] = v;
  el.style[k] = v;
}

// ---------------------------------------------------------------- backgrounds

const BG = S.background || null;
const bgState = { draw: null };
const BACKGROUNDS = {
  nebula: (b) => {
    const cols = b.colors || ['#0a1a4a', '#1d3fa8', '#2a1766'];
    bgEl.style.background = b.base || theme.bg;
    const blobs = cols.map((c, i) => {
      const d = mk('div', '', bgEl);
      d.style.cssText = `position:absolute;width:${W * 1.1}px;height:${W * 1.1}px;left:0;top:0;border-radius:50%;background:radial-gradient(circle, ${alpha(c, 0.9)} 0%, ${alpha(c, 0.35)} 35%, transparent 68%);opacity:${b.intensity || 0.85}`;
      return { d, i };
    });
    const stars = starfield(b.stars !== undefined ? b.stars : 140, b.star_color || '#ffffff', b.drift || 1);
    bgState.draw = (t) => {
      const sp = 0.06 * (b.speed || 1);
      for (const { d, i } of blobs) {
        const x = W * (0.5 + 0.34 * Math.sin(t * sp * (1 + i * 0.37) + i * 2.1)) - W * 0.55;
        const y = H * (0.5 + 0.3 * Math.cos(t * sp * (0.8 + i * 0.29) + i * 1.3)) - W * 0.55;
        const s = 0.8 + 0.2 * Math.sin(t * sp * 1.7 + i);
        setStyle(d, 'transform', `translate(${x.toFixed(1)}px,${y.toFixed(1)}px) scale(${s.toFixed(3)})`);
      }
      stars(t);
    };
  },
  gradient: (b) => {
    const cols = b.colors || [theme.bg, alpha(theme.accent, 0.5)];
    bgState.draw = (t) => {
      const a = (b.angle || 135) + t * (b.spin || 4);
      setStyle(bgEl, 'background', `linear-gradient(${a.toFixed(1)}deg, ${cols.join(', ')})`);
    };
  },
  grid: (b) => {
    bgEl.style.background = b.base || theme.bg;
    bgEl.style.perspective = '600px';
    const floor = mk('div', '', bgEl);
    const c = b.color || alpha(theme.accent, 0.35);
    const cell = b.cell || 80;
    floor.style.cssText = `position:absolute;left:-50%;width:200%;top:48%;height:120%;transform-origin:50% 0;transform:rotateX(72deg);background-image:linear-gradient(${c} 1.5px, transparent 1.5px),linear-gradient(90deg, ${c} 1.5px, transparent 1.5px);background-size:${cell}px ${cell}px;-webkit-mask-image:linear-gradient(to bottom, transparent, #000 40%);`;
    const glow = mk('div', '', bgEl);
    glow.style.cssText = `position:absolute;left:0;right:0;top:30%;height:36%;background:radial-gradient(ellipse at 50% 60%, ${alpha(b.glow || theme.accent, 0.45)}, transparent 65%)`;
    const stars = starfield(b.stars || 0, '#fff', 0.5);
    bgState.draw = (t) => {
      setStyle(floor, 'backgroundPosition', `0 ${((t * (b.speed || 60)) % cell).toFixed(1)}px`);
      stars(t);
    };
  },
  solid: (b) => { bgEl.style.background = b.color || b.base || theme.bg; },
  aurora: (b) => {
    bgEl.style.background = b.base || theme.bg;
    const cols = b.colors || [theme.accent, theme.accent2, '#23d5ab'];
    const bands = cols.map((c) => {
      const d = mk('div', '', bgEl);
      d.style.cssText = `position:absolute;left:-25%;width:150%;height:${H * 0.5}px;top:0;background:linear-gradient(90deg, transparent, ${alpha(c, 0.55)}, transparent);filter:blur(${H * 0.08}px);border-radius:50%`;
      return d;
    });
    const stars = starfield(b.stars || 60, '#fff', 0.6);
    bgState.draw = (t) => {
      bands.forEach((d, i) => {
        const y = H * (0.15 + 0.22 * i) + Math.sin(t * 0.25 + i * 1.7) * H * 0.08;
        const r = Math.sin(t * 0.13 + i) * 8;
        const s = 1 + 0.15 * Math.sin(t * 0.2 + i * 2);
        setStyle(d, 'transform', `translateY(${y.toFixed(1)}px) rotate(${r.toFixed(2)}deg) scaleY(${s.toFixed(3)})`);
      });
      stars(t);
    };
  },
  mesh: (b) => {
    const cols = b.colors || [theme.accent, theme.accent2, '#ff6b9a', '#1fd1c1'];
    bgState.draw = (t) => {
      const sp = 0.15 * (b.speed || 1);
      const parts = cols.map((c, i) => {
        const x = 50 + 40 * Math.sin(t * sp * (1 + i * 0.3) + i * 1.9);
        const y = 50 + 40 * Math.cos(t * sp * (0.7 + i * 0.25) + i);
        return `radial-gradient(circle at ${x.toFixed(1)}% ${y.toFixed(1)}%, ${alpha(c, 0.55)}, transparent 55%)`;
      });
      setStyle(bgEl, 'background', parts.join(',') + ',' + (b.base || theme.bg));
    };
  },
};

// A field of stars on one canvas: each star has a depth, drifts at a speed
// proportional to it, and twinkles on its own phase.
function starfield(n, color, drift) {
  if (!n) return () => {};
  const cv = mk('canvas', 'kv-canvas', bgEl);
  cv.width = W; cv.height = H;
  const cx = cv.getContext('2d');
  const r = rng(SEED * 31 + 7);
  const stars = [];
  for (let i = 0; i < n; i++) {
    const d = r();
    stars.push({ x: r() * W, y: r() * H, d, s: 0.6 + d * 2.2, ph: r() * TAU, tw: 0.5 + r() * 2, blur: d > 0.85 });
  }
  const rgb = parseColor(color) || [255, 255, 255, 1];
  return (t) => {
    cx.clearRect(0, 0, W, H);
    for (const s of stars) {
      const x = (((s.x + t * drift * (4 + s.d * 16)) % W) + W) % W;
      const y = (((s.y - t * drift * s.d * 3) % H) + H) % H;
      const a = (0.25 + 0.75 * s.d) * (0.55 + 0.45 * Math.sin(t * s.tw + s.ph));
      if (s.blur) {
        const g = cx.createRadialGradient(x, y, 0, x, y, s.s * 4);
        g.addColorStop(0, `rgba(${rgb[0]},${rgb[1]},${rgb[2]},${(a * 0.8).toFixed(3)})`);
        g.addColorStop(1, `rgba(${rgb[0]},${rgb[1]},${rgb[2]},0)`);
        cx.fillStyle = g;
        cx.fillRect(x - s.s * 4, y - s.s * 4, s.s * 8, s.s * 8);
      } else {
        cx.fillStyle = `rgba(${rgb[0]},${rgb[1]},${rgb[2]},${a.toFixed(3)})`;
        cx.beginPath(); cx.arc(x, y, s.s * 0.6, 0, TAU); cx.fill();
      }
    }
  };
}

if (BG) {
  const k = BG.kind || 'nebula';
  BACKGROUNDS[k](BG);
} else {
  bgEl.style.background = theme.bg;
}

// Grain: a handful of noise tiles cycled by frame, so it crawls like film.
const GRAIN = S.video.grain !== undefined ? S.video.grain : 0.06;
const grainTiles = [];
if (GRAIN > 0) {
  const r = rng(SEED + 99);
  for (let k = 0; k < 6; k++) {
    const c = document.createElement('canvas');
    c.width = c.height = 192;
    const x = c.getContext('2d');
    const img = x.createImageData(192, 192);
    for (let i = 0; i < img.data.length; i += 4) {
      const v = (r() * 255) | 0;
      img.data[i] = img.data[i + 1] = img.data[i + 2] = v;
      img.data[i + 3] = 255;
    }
    x.putImageData(img, 0, 0);
    grainTiles.push(`url(${c.toDataURL()})`);
  }
  grainEl.style.opacity = GRAIN;
}
const VIG = S.video.vignette !== undefined ? S.video.vignette : 0.55;
if (VIG > 0) vignetteEl.style.background = `radial-gradient(ellipse 75% 70% at 50% 50%, transparent 45%, rgba(0,0,0,${VIG}) 100%)`;
if (S.video.letterbox) {
  const lb = mk('div', '', over); lb.id = 'kv-letterbox';
  const h = len(S.video.letterbox, H, 0);
  lb.style.cssText = `border-top:${h}px solid #000;border-bottom:${h}px solid #000`;
}

// ---------------------------------------------------------------- scenes

const scenes = [];
const sceneById = {};
for (const sc of S.scenes || []) {
  const el = mk('div', 'kv-scene', world);
  const cam = mk('div', 'kv-cam', el);
  cam.style.perspective = PERSPECTIVE + 'px';
  const s = {
    spec: sc, id: sc.id, start: sc.start, end: sc.end, el, cam,
    tin: sc.tin, tout: null, keys: compileKeys(sc.camera && sc.camera.keys ? sc.camera.keys : [], 'scene ' + sc.id),
    hblur: null, hblurId: 'kvhb-' + scenes.length, visible: true,
  };
  if (sc.bg) el.style.background = sc.bg;
  scenes.push(s);
  sceneById[sc.id] = s;
}
// A scene's exit is the next scene's entrance, when the two meet.
for (let i = 0; i < scenes.length; i++) {
  const a = scenes[i];
  for (const b of scenes) {
    if (b !== a && b.tin && Math.abs(b.start - a.end) < 1e-6) a.tout = b.tin;
  }
}
for (const c of S.cameras || []) {
  if (c.scene && sceneById[c.scene]) {
    const s = sceneById[c.scene];
    s.keys = mergeKeys(s.keys, compileKeys(c.keys || [c], 'camera'));
  }
}
const globalCam = compileKeys([].concat(...(S.cameras || []).filter((c) => !c.scene).map((c) => c.keys || [c])), 'camera');
const globalLayer = mk('div', 'kv-cam', world);
globalLayer.style.perspective = PERSPECTIVE + 'px';

// ---------------------------------------------------------------- keyframes

// Keys compile to one track per property: a list of tweens, each ending at a
// value. `t` is when a value is reached; `at` + `dur` is when a tween runs.
function compileKeys(keys, where) {
  const tracks = {};
  const list = (keys || []).slice().map((k, i) => ({ k, i }));
  list.sort((a, b) => keyEnd(a.k) - keyEnd(b.k) || a.i - b.i);
  for (const { k } of list) {
    for (const p in k) {
      if (NON_PROPS[p]) continue;
      const tr = tracks[p] || (tracks[p] = []);
      const prevEnd = tr.length ? tr[tr.length - 1].end : null;
      let start, end;
      if (k.at !== undefined) { start = k.at; end = k.at + (k.dur !== undefined ? k.dur : 0.6); }
      else { end = k.t; start = k.dur !== undefined ? k.t - k.dur : (prevEnd !== null ? prevEnd : 0); }
      tr.push({ start, end, to: k[p], ease: easeFn(k.ease, 'inOutCubic') });
    }
  }
  return tracks;
}
function keyEnd(k) { return k.at !== undefined ? k.at + (k.dur !== undefined ? k.dur : 0.6) : k.t; }
function mergeKeys(a, b) {
  const out = Object.assign({}, a);
  for (const p in b) out[p] = (out[p] || []).concat(b[p]).sort((x, y) => x.end - y.end);
  return out;
}
function trackValue(tr, lt, base) {
  let v = base;
  for (const seg of tr) {
    if (lt >= seg.end) { v = seg.to; continue; }
    if (lt > seg.start) {
      const u = seg.ease((lt - seg.start) / (seg.end - seg.start));
      if (isColor(seg.to) || isColor(v)) return mixColor(v, seg.to, u);
      if (typeof seg.to === 'number' && typeof v === 'number') return lerp(v, seg.to, u);
      return u < 1 ? v : seg.to;
    }
    break;
  }
  return v;
}

// ---------------------------------------------------------------- effects
//
// An effect maps progress to offsets. For an entrance p runs 0 -> 1 and 1 is
// rest; for an exit q runs 0 -> 1 and 0 is rest. `c.u` is the unit of travel
// (the font size for a character, the layer's size otherwise) and `c.r` three
// fixed random numbers in -1..1 for this character or layer.
//
// Offsets: x y z (px), s sx sy (scale factors), r rx ry (degrees),
// o (opacity factor), b (blur px), clip (a CSS clip-path), draw (0..1),
// scr (1 while a scramble is still resolving), vis (0 hides outright).

const IN_FX = {
  fade: { ease: 'outCubic', f: (p) => ({ o: p }) },
  rise: { ease: 'outExpo', f: (p, c) => ({ y: (1 - p) * c.u * 0.8, o: clamp(p * 1.6, 0, 1), b: (1 - p) * c.u * 0.08 }) },
  drop: { ease: 'outExpo', f: (p, c) => ({ y: -(1 - p) * c.u * 0.8, o: clamp(p * 1.6, 0, 1), b: (1 - p) * c.u * 0.08 }) },
  left: { ease: 'outExpo', f: (p, c) => ({ x: -(1 - p) * c.u * 1.2, o: clamp(p * 1.5, 0, 1), b: (1 - p) * c.u * 0.06 }) },
  right: { ease: 'outExpo', f: (p, c) => ({ x: (1 - p) * c.u * 1.2, o: clamp(p * 1.5, 0, 1), b: (1 - p) * c.u * 0.06 }) },
  pop: { ease: 'outBack', f: (p) => ({ s: Math.max(0, p), o: clamp(p * 3, 0, 1) }) },
  zoom: { ease: 'outExpo', f: (p, c) => ({ s: 1 + (1 - p) * 2.2, o: clamp(p * 1.4, 0, 1), b: (1 - p) * c.u * 0.25 }) },
  blur: { ease: 'outCubic', f: (p, c) => ({ o: p, b: (1 - p) * c.u * 0.35, s: 1 + (1 - p) * 0.12 }) },
  wave: { ease: 'outBack', f: (p, c) => ({ y: (1 - p) * c.u * 0.9, r: (1 - p) * -24, o: clamp(p * 2.2, 0, 1), b: Math.max(0, 1 - p) * c.u * 0.05, s: 0.6 + 0.4 * p }) },
  flip: { ease: 'outExpo', f: (p) => ({ rx: (1 - p) * -95, o: clamp(p * 2, 0, 1) }) },
  spin: { ease: 'outBack', f: (p) => ({ r: (1 - p) * -200, s: Math.max(0, p), o: clamp(p * 2, 0, 1) }) },
  swing: { ease: 'outBack', f: (p, c) => ({ r: (1 - p) * -18, y: (1 - p) * c.u * 0.35, o: clamp(p * 2, 0, 1) }) },
  converge: { ease: 'outExpo', f: (p, c) => ({ x: c.r[0] * (1 - p) * c.u * 4, y: c.r[1] * (1 - p) * c.u * 3, r: c.r[2] * (1 - p) * 180, b: (1 - p) * c.u * 0.12, o: clamp(p * 1.6, 0, 1) }) },
  typewriter: { ease: 'linear', f: (p) => ({ vis: p > 0 ? 1 : 0 }) },
  scramble: { ease: 'linear', f: (p) => ({ o: p > 0 ? 1 : 0, scr: p < 1 ? 1 : 0 }) },
  mask: { ease: 'outExpo', f: (p) => ({ y: (1 - p) * 1.15, yem: true }) },
  wipe: { ease: 'inOutQuart', f: (p) => ({ clip: `inset(-20% ${((1 - p) * 100).toFixed(2)}% -20% -2%)` }) },
  'wipe-down': { ease: 'inOutQuart', f: (p) => ({ clip: `inset(-5% -5% ${((1 - p) * 100).toFixed(2)}% -5%)` }) },
  'wipe-up': { ease: 'inOutQuart', f: (p) => ({ clip: `inset(${((1 - p) * 100).toFixed(2)}% -5% -5% -5%)` }) },
  iris: { ease: 'inOutCubic', f: (p) => ({ clip: `circle(${(p * 75).toFixed(2)}% at 50% 50%)` }) },
  draw: { ease: 'inOutCubic', f: (p) => ({ draw: p }) },
  stretch: { ease: 'outBack', f: (p) => ({ sy: Math.max(0, p), sx: 1 + (1 - p) * 0.6, o: clamp(p * 3, 0, 1) }) },
  fly: { ease: 'outExpo', f: (p, c) => ({ z: -(1 - p) * 1800, ry: (1 - p) * 35 * (c.r[0] < 0 ? -1 : 1), o: clamp(p * 1.5, 0, 1), b: (1 - p) * 10 }) },
  glitch: { ease: 'linear', f: (p, c, t) => {
    if (p >= 1) return {};
    const k = Math.floor(t * 40);
    const on = rng(k * 131 + ((c.r[0] * 1000) | 0))() < 0.35 + p * 0.65;
    return { o: on ? 1 : 0, x: (rng(k * 7 + 3)() - 0.5) * c.u * 0.3 * (1 - p), rgb: (1 - p) * 6 };
  } },
  elastic: { ease: 'outElastic', f: (p) => ({ s: Math.max(0, p), o: clamp(p * 4, 0, 1) }) },
  bounce: { ease: 'outBounce', f: (p, c) => ({ y: -(1 - p) * c.u * 1.5, o: clamp(p * 4, 0, 1) }) },
  none: { ease: 'linear', f: () => ({}) },
};

const OUT_FX = {
  fade: { ease: 'inCubic', f: (q) => ({ o: 1 - q }) },
  fall: { ease: 'inBack', f: (q, c) => ({ y: q * c.u * 1.2, r: c.r[0] * q * 35, o: 1 - q }) },
  rise: { ease: 'inExpo', f: (q, c) => ({ y: -q * c.u * 0.9, o: 1 - q, b: q * c.u * 0.08 }) },
  left: { ease: 'inExpo', f: (q, c) => ({ x: -q * c.u * 1.5, o: 1 - q, b: q * c.u * 0.08 }) },
  right: { ease: 'inExpo', f: (q, c) => ({ x: q * c.u * 1.5, o: 1 - q, b: q * c.u * 0.08 }) },
  pop: { ease: 'inBack', f: (q) => ({ s: Math.max(0, 1 - q), o: clamp((1 - q) * 3, 0, 1) }) },
  zoom: { ease: 'inExpo', f: (q, c) => ({ s: 1 + q * 2.5, o: 1 - q, b: q * c.u * 0.25 }) },
  blur: { ease: 'inCubic', f: (q, c) => ({ o: 1 - q, b: q * c.u * 0.35, s: 1 + q * 0.1 }) },
  wave: { ease: 'inBack', f: (q, c) => ({ y: -q * c.u * 0.8, r: q * 20, o: 1 - q, s: 1 - q * 0.4 }) },
  flip: { ease: 'inExpo', f: (q) => ({ rx: q * 95, o: 1 - q }) },
  scatter: { ease: 'inExpo', f: (q, c) => ({ x: c.r[0] * q * c.u * 5, y: c.r[1] * q * c.u * 4 - q * c.u, r: c.r[2] * q * 260, b: q * c.u * 0.15, o: 1 - q, s: 1 + q * 0.6 }) },
  shrink: { ease: 'inBack', f: (q) => ({ s: Math.max(0, 1 - q), o: 1 - q * q }) },
  wipe: { ease: 'inOutQuart', f: (q) => ({ clip: `inset(-20% -2% -20% ${(q * 100).toFixed(2)}%)` }) },
  iris: { ease: 'inOutCubic', f: (q) => ({ clip: `circle(${((1 - q) * 75).toFixed(2)}% at 50% 50%)` }) },
  fly: { ease: 'inExpo', f: (q, c) => ({ z: q * 1400, o: 1 - q, b: q * 12 }) },
  glitch: { ease: 'linear', f: (q, c, t) => {
    if (q <= 0) return {};
    const k = Math.floor(t * 40);
    const on = rng(k * 131 + ((c.r[0] * 1000) | 0))() > q;
    return { o: on ? 1 : 0, x: (rng(k * 7 + 5)() - 0.5) * c.u * 0.3 * q, rgb: q * 6 };
  } },
  spin: { ease: 'inBack', f: (q) => ({ r: q * 200, s: Math.max(0, 1 - q), o: 1 - q }) },
  none: { ease: 'linear', f: () => ({}) },
};

// Loops run while a layer is up. `lt` is scene time, `t` absolute time (for the beat).
const LOOP_FX = {
  float: (lt, t, c, a) => ({ y: (a.amp !== undefined ? a.amp : 10) * Math.sin(TAU * lt / (a.period || 3) + (a.phase || 0) + c.r[0] * 3) }),
  sway: (lt, t, c, a) => ({ r: (a.amp !== undefined ? a.amp : 3) * Math.sin(TAU * lt / (a.period || 4) + (a.phase || 0) + c.r[1] * 3) }),
  spin: (lt, t, c, a) => ({ r: 360 * lt / (a.period || 8) * (a.dir === -1 || a.dir === 'ccw' ? -1 : 1) }),
  pulse: (lt, t, c, a) => ({ s: 1 + (a.amp !== undefined ? a.amp : 0.06) * beatEnv(t, a.every, a.sharp) * (a.energy === false ? 1 : energyAt(t)) }),
  breathe: (lt, t, c, a) => ({ s: 1 + (a.amp !== undefined ? a.amp : 0.03) * Math.sin(TAU * lt / (a.period || 4) + (a.phase || 0)) }),
  wiggle: (lt, t, c, a) => {
    const f = a.freq || 1.5, amp = a.amp !== undefined ? a.amp : 6, seed = (c.r[0] * 1e4) | 0;
    return { x: noise(lt * f, seed) * amp, y: noise(lt * f, seed + 17) * amp, r: noise(lt * f, seed + 31) * amp * 0.25 };
  },
  flicker: (lt, t, c, a) => ({ o: 1 - (a.amp !== undefined ? a.amp : 0.3) * (noise(lt * (a.freq || 12), (c.r[0] * 1e4) | 0) * 0.5 + 0.5) }),
  glow: (lt, t, c, a) => ({ glow: (a.amp !== undefined ? a.amp : 24) * beatEnv(t, a.every, a.sharp || 5) }),
  shine: (lt, t, c, a) => {
    const per = a.period || 3;
    const ph = ((lt - (a.phase || 0)) / per) % 1;
    return { shine: ph < 0 ? ph + 1 : ph };
  },
  drift: (lt, t, c, a) => ({ x: (a.vx || 0) * lt, y: (a.vy || 0) * lt, r: (a.vr || 0) * lt, s: 1 + (a.vs || 0) * lt }),
};

const TRANSITIONS = {
  cut: (u, dir) => (dir === 'in' ? { o: u >= 0.5 ? 1 : 0 } : { o: u < 0.5 ? 1 : 0 }),
  dissolve: (u, dir) => ({ o: dir === 'in' ? EASE.inOutQuad(u) : 1 - EASE.inOutQuad(u) }),
  zoom: (u, dir) => dir === 'out'
    ? { s: 1 + EASE.inExpo(u) * 2.4, b: EASE.inQuad(u) * 26, o: 1 - EASE.inQuart(u) }
    : { s: 0.55 + 0.45 * EASE.outExpo(u), b: (1 - EASE.outCubic(u)) * 18, o: EASE.outQuad(clamp(u * 1.4, 0, 1)) },
  whip: (u, dir) => dir === 'out'
    ? { x: -EASE.inExpo(u) * W * 1.1, hb: EASE.inQuad(u) * 60, o: 1 - EASE.inQuint(u) }
    : { x: (1 - EASE.outExpo(u)) * W * 1.1, hb: (1 - EASE.outQuad(u)) * 60, o: clamp(u * 3, 0, 1) },
  slide: (u, dir) => dir === 'out'
    ? { y: -EASE.inOutExpo(u) * H }
    : { y: (1 - EASE.inOutExpo(u)) * H },
  push: (u, dir) => dir === 'out'
    ? { x: -EASE.inOutQuart(u) * W * 0.35, s: 1 - 0.1 * EASE.inOutQuart(u), o: 1 - EASE.inQuad(u), b: u * 8 }
    : { x: (1 - EASE.inOutQuart(u)) * W, r: 0 },
  flash: (u, dir) => (dir === 'out' ? { o: u < 0.5 ? 1 : 0, s: 1 + u * 0.06 } : { o: u >= 0.5 ? 1 : 0, s: 1.08 - 0.08 * EASE.outExpo(clamp(u * 2 - 1, 0, 1)) }),
  blur: (u, dir) => dir === 'out'
    ? { b: EASE.inQuad(u) * 30, o: 1 - EASE.inCubic(u) }
    : { b: (1 - EASE.outCubic(u)) * 30, o: EASE.outCubic(u) },
  iris: (u, dir) => (dir === 'in' ? { clip: `circle(${(EASE.inOutCubic(u) * 80).toFixed(2)}% at 50% 50%)` } : { s: 1 + u * 0.05, b: u * 4 }),
  glitch: (u, dir) => {
    const k = Math.floor(u * 14);
    const r = rng(k * 97 + (dir === 'in' ? 5 : 11))();
    const show = dir === 'in' ? (u > 0.5 ? r > 0.15 : r > 0.8) : (u < 0.5 ? r > 0.15 : r > 0.8);
    return { o: show ? 1 : 0, x: (rng(k * 31 + 2)() - 0.5) * 60, rgb: 10 * Math.sin(u * Math.PI) };
  },
  spin: (u, dir) => dir === 'out'
    ? { r: EASE.inExpo(u) * 90, s: 1 + EASE.inExpo(u) * 1.5, o: 1 - EASE.inQuart(u), b: u * 20 }
    : { r: -(1 - EASE.outExpo(u)) * 90, s: 0.4 + 0.6 * EASE.outExpo(u), o: EASE.outQuad(u), b: (1 - u) * 16 },
};

function addMods(m, d) {
  if (!d) return;
  if (d.x) m.x += d.x;
  if (d.y) m.y += d.y;
  if (d.z) m.z += d.z;
  if (d.s !== undefined) m.s *= d.s;
  if (d.sx !== undefined) m.sx *= d.sx;
  if (d.sy !== undefined) m.sy *= d.sy;
  if (d.r) m.r += d.r;
  if (d.rx) m.rx += d.rx;
  if (d.ry) m.ry += d.ry;
  if (d.o !== undefined) m.o *= d.o;
  if (d.b) m.b += d.b;
  if (d.hb) m.hb += d.hb;
  if (d.glow) m.glow += d.glow;
  if (d.rgb) m.rgb += d.rgb;
  if (d.clip) m.clip = d.clip;
  if (d.draw !== undefined) m.draw = Math.min(m.draw, d.draw);
  if (d.scr) m.scr = 1;
  if (d.vis === 0) m.vis = 0;
  if (d.shine !== undefined) m.shine = d.shine;
  if (d.yem) m.yem = true;
}
function mods() { return { x: 0, y: 0, z: 0, s: 1, sx: 1, sy: 1, r: 0, rx: 0, ry: 0, o: 1, b: 0, hb: 0, glow: 0, rgb: 0, clip: null, draw: 1, scr: 0, vis: 1, shine: null, yem: false }; }

// A generic tween from explicit offsets, for "in": {"from": {...}} and "out": {"to": {...}}.
function fromTo(obj, p) {
  const m = {};
  const map = { x: 'x', y: 'y', z: 'z', rotate: 'r', rx: 'rx', ry: 'ry', blur: 'b' };
  for (const k in obj) {
    const v = obj[k];
    if (map[k]) m[map[k]] = v * (1 - p);
    else if (k === 'scale') m.s = lerp(v, 1, p);
    else if (k === 'opacity') m.o = lerp(v, 1, p);
    else if (k === 'sx') m.sx = lerp(v, 1, p);
    else if (k === 'sy') m.sy = lerp(v, 1, p);
  }
  return m;
}

function normFx(v, dflt) {
  if (!v) return null;
  if (typeof v === 'string') return { fx: v, at: 0, dur: dflt };
  const o = Object.assign({}, v);
  if (!o.fx && !o.from && !o.to) o.fx = 'fade';
  if (o.at === undefined) o.at = 0;
  if (o.dur === undefined) o.dur = dflt;
  return o;
}

// Progress of one staggered unit. Returns null before it starts.
function unitProgress(fx, lt, i, n, dflt) {
  const st = fx.stagger !== undefined ? fx.stagger : (fx.spread !== undefined && n > 1 ? fx.spread / (n - 1) : dflt);
  let order = i;
  const o = fx.order || 'forward';
  if (o === 'reverse') order = n - 1 - i;
  else if (o === 'center') order = Math.abs(i - (n - 1) / 2);
  else if (o === 'edges') order = (n - 1) / 2 - Math.abs(i - (n - 1) / 2);
  else if (o === 'random') order = rng(i * 7919 + 13)() * (n - 1);
  const start = fx.at + (fx.delay || 0) + order * st;
  return clamp((lt - start) / Math.max(fx.dur, 1e-6), 0, 1);
}

// ---------------------------------------------------------------- layers

const nodes = [];
const byId = {};

function parentBox(n) {
  if (n.parent) return { w: n.parent.slotW || 0, h: n.parent.slotH || 0, group: n.parent.isGroup };
  return { w: W, h: H, group: false };
}

class Node {
  constructor(spec) {
    this.spec = spec;
    this.id = spec.id;
    this.kind = spec.op;
    this.scene = spec.scene ? sceneById[spec.scene] : null;
    this.parent = spec.parent ? byId[spec.parent] : null;
    this.isGroup = this.kind === 'group';
    this.kids = [];
    this.r = (() => { const g = rng(hashStr(this.id) ^ SEED); return [g() * 2 - 1, g() * 2 - 1, g() * 2 - 1]; })();
    this.el = mk('div', 'kv-node' + (this.isGroup ? ' kv-group' : ''));
    this.el.dataset.id = this.id;
    this.inner = this.el;
    this.slot = this.el;
    this.inFx = normFx(spec.in, 0.7);
    this.outFx = normFx(spec.out, 0.5);
    this.loops = [].concat(spec.loop || []).map((l) => (typeof l === 'string' ? { fx: l } : l));
    this.keys = compileKeys(spec.keys || [], this.id);
    this.acts = [];
    this.subs = [];
    this.chars = null;
    this.trail = spec.trail ? Object.assign({ len: 0.45, width: 6 }, spec.trail === true ? {} : spec.trail) : null;
    this.w = spec.w; this.h = spec.h;
    if (spec.layer !== undefined) this.el.style.zIndex = spec.layer;
    if (spec.style) this.el.style.cssText += ';' + spec.style;
    if (spec.class) this.el.className += ' ' + spec.class;
    // A fixed layer sits in its scene but outside the scene's camera, so a zoom never crops it.
    const host = this.parent ? this.parent.slot : this.scene ? (spec.fixed ? this.scene.el : this.scene.cam) : globalLayer;
    host.appendChild(this.el);
    if (this.parent) this.parent.kids.push(this);
    BUILD[this.kind](this, spec);
    if (this.w !== undefined) this.el.style.width = len(this.w, this.parent ? this.parent.slotW || W : W, 0) + 'px';
    if (this.h !== undefined) this.el.style.height = len(this.h, this.parent ? this.parent.slotH || H : H, 0) + 'px';
    const a = spec.anchor || [0.5, 0.5];
    this.ax = a[0]; this.ay = a[1];
    if (spec.shine || this.loops.some((l) => l.fx === 'shine')) {
      this.shineEl = mk('div', 'kv-shine', this.inner);
      if (spec.radius !== undefined) this.shineEl.style.borderRadius = spec.radius + 'px';
    }
  }
  // Time in this layer's scene.
  local(t) { return this.scene ? t - this.scene.start : t; }
  unit() {
    if (this.kind === 'text') return this.fontSize;
    const w = this.boxW || 100, h = this.boxH || 100;
    return clamp(Math.min(w, h), 40, 600);
  }
}

// Evaluate a layer's transform at scene time lt. Pure: no DOM is touched, so a
// trail can ask where a layer was a moment ago.
function evalNode(n, lt, t) {
  const sp = n.spec;
  const pb = parentBox(n);
  const dflX = pb.group ? 0 : pb.w / 2, dflY = pb.group ? 0 : pb.h / 2;
  const p = {
    x: len(sp.x, pb.w, dflX), y: len(sp.y, pb.h, dflY), z: sp.z || 0,
    scale: sp.scale !== undefined ? sp.scale : 1, sx: sp.sx !== undefined ? sp.sx : 1, sy: sp.sy !== undefined ? sp.sy : 1,
    rotate: sp.rotate || 0, rx: sp.rx || 0, ry: sp.ry || 0,
    opacity: sp.opacity !== undefined ? sp.opacity : 1, blur: sp.blur || 0, glow: sp.glow || 0, bright: sp.bright !== undefined ? sp.bright : 1,
    draw: sp.draw !== undefined ? sp.draw : 1,
  };
  if (n.layoutPos) { if (sp.x === undefined) p.x = n.layoutPos[0]; if (sp.y === undefined) p.y = n.layoutPos[1]; }
  for (const k in n.keys) {
    const base = p[k] !== undefined ? p[k] : (sp[k] !== undefined ? sp[k] : (k === 'color' ? n.baseColor : k === 'fill' ? n.baseFill : 0));
    let v = trackValue(n.keys[k], lt, base);
    if ((k === 'x' || k === 'y') && typeof v === 'string') v = len(v, k === 'x' ? pb.w : pb.h, 0);
    p[k] = v;
  }
  const m = mods();
  const c = { u: n.unit(), r: n.r };
  const fxWhole = n.kind !== 'text' || n.split === 'none';
  if (n.inFx && fxWhole) {
    const q = clamp((lt - n.inFx.at - (n.inFx.delay || 0)) / Math.max(n.inFx.dur, 1e-6), 0, 1);
    if (n.inFx.from) addMods(m, fromTo(n.inFx.from, easeFn(n.inFx.ease, 'outExpo')(q)));
    else { const F = IN_FX[n.inFx.fx]; addMods(m, F.f(easeFn(n.inFx.ease, F.ease)(q), c, lt)); }
  }
  if (n.outFx && fxWhole) {
    const q = clamp((lt - n.outFx.at - (n.outFx.delay || 0)) / Math.max(n.outFx.dur, 1e-6), 0, 1);
    if (q > 0) {
      if (n.outFx.to) addMods(m, fromTo(n.outFx.to, 1 - easeFn(n.outFx.ease, 'inExpo')(q)));
      else { const F = OUT_FX[n.outFx.fx]; addMods(m, F.f(easeFn(n.outFx.ease, F.ease)(q), c, lt)); }
    }
  }
  for (const a of n.anims) addMods(m, a(lt, t, c));
  for (const L of n.loops) {
    if (L.at !== undefined && lt < L.at) continue;
    if (L.at !== undefined && L.dur !== undefined && lt > L.at + L.dur) continue;
    addMods(m, LOOP_FX[L.fx](lt - (L.at || 0), t, c, L));
  }
  for (const a of n.acts) if (a.mods) addMods(m, a.mods(lt));
  if (n.parent && n.parent.orbit) addMods(m, orbitOffset(n.parent, n, lt, t));
  return { p, m };
}

// ---------------------------------------------------------------- orbit layout

function orbitParams(g, lt) {
  const o = g.orbit;
  const k = g.keys;
  const val = (name, d) => (k[name] ? trackValue(k[name], lt, d) : d);
  return {
    rx: val('orbit_rx', o.rx !== undefined ? o.rx : 360),
    ry: val('orbit_ry', o.ry !== undefined ? o.ry : 120),
    ang: val('orbit_angle', 0),
    depth: val('orbit_depth', o.depth !== undefined ? o.depth : 0.35),
    tilt: o.tilt || 0,
    period: o.period || 8,
  };
}
function orbitOffset(g, n, lt, t) {
  const o = orbitParams(g, lt);
  const i = g.kids.indexOf(n), cnt = g.kids.length;
  const theta = TAU * (i / cnt + lt / o.period) + (o.ang * Math.PI) / 180 + (g.orbit.phase || 0);
  const cx = Math.cos(theta), sy = Math.sin(theta);
  const tl = (o.tilt * Math.PI) / 180;
  const x = o.rx * cx, y = o.ry * sy;
  const depth = sy; // front is +1
  return {
    x: x * Math.cos(tl) - y * Math.sin(tl),
    y: x * Math.sin(tl) + y * Math.cos(tl),
    s: 1 + depth * o.depth,
    o: 1 - (1 - depth) * 0.5 * o.depth,
    zi: Math.round(depth * 100) + 200,
  };
}

// ---------------------------------------------------------------- layer kinds

function applyTextStyle(el, sp) {
  const fs = sp.size || 72;
  el.style.fontSize = fs + 'px';
  if (sp.weight) el.style.fontWeight = sp.weight;
  el.style.color = sp.color || theme.text;
  el.style.setProperty('--acc', sp.accent || theme.accent);
  if (sp.font) el.style.fontFamily = sp.font === 'mono' ? theme.mono : sp.font;
  if (sp.tracking !== undefined) el.style.setProperty('--tr', sp.tracking + 'em');
  else if (fs < 32) el.style.setProperty('--tr', '-0.005em');
  if (sp.leading) el.style.setProperty('--lh', sp.leading);
  el.style.textAlign = sp.align || 'center';
  if (sp.italic) el.style.fontStyle = 'italic';
  if (sp.upper) { el.style.textTransform = 'uppercase'; }
  if (sp.shadow) el.style.textShadow = sp.shadow === true ? '0 6px 30px rgba(0,0,0,.45)' : sp.shadow;
  if (sp.gradient) {
    el.style.backgroundImage = `linear-gradient(${sp.gradient_angle || 90}deg, ${sp.gradient.join(', ')})`;
    el.style.webkitBackgroundClip = 'text';
    el.style.backgroundClip = 'text';
    el.style.color = 'transparent';
  }
}

// Text markup: [accent], {muted}, ~strike~ and \n. Returns lines of styled chars.
function parseMarkup(s) {
  const lines = [[]];
  let acc = false, mut = false, st = false, strikeRun = -1, runs = 0;
  for (let i = 0; i < s.length; i++) {
    const ch = s[i];
    if (ch === '\\' && i + 1 < s.length) { lines[lines.length - 1].push({ ch: s[++i], acc, mut, st: st ? strikeRun : -1 }); continue; }
    if (ch === '[') { acc = true; continue; }
    if (ch === ']') { acc = false; continue; }
    if (ch === '{') { mut = true; continue; }
    if (ch === '}') { mut = false; continue; }
    if (ch === '~') { st = !st; if (st) strikeRun = runs++; continue; }
    if (ch === '\n') { lines.push([]); continue; }
    lines[lines.length - 1].push({ ch, acc, mut, st: st ? strikeRun : -1 });
  }
  return { lines, strikes: runs };
}

const SCRAMBLE = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz0123456789#%&*+=?<>/';
const SPLIT_NONE = { fade: 1, zoom: 1, wipe: 1, 'wipe-up': 1, iris: 1, none: 1, fly: 1, blur: 1, left: 1, right: 1, draw: 1 };

const BUILD = {
  text(n, sp) {
    const el = mk('div', 'kv-text', n.el);
    n.inner = el;
    applyTextStyle(el, sp);
    n.fontSize = sp.size || 72;
    n.baseColor = sp.color || theme.text;
    const fxName = n.inFx ? n.inFx.fx : null;
    const outName = n.outFx ? n.outFx.fx : null;
    n.split = sp.split || (n.inFx && n.inFx.split) || (n.outFx && n.outFx.split) || ((fxName && !SPLIT_NONE[fxName]) || (outName && !SPLIT_NONE[outName]) ? 'char' : 'none');
    if (n.inFx && (n.inFx.from)) n.split = sp.split || 'none';
    const { lines, strikes } = parseMarkup(String(sp.text));
    n.chars = [];
    n.words = [];
    n.strikes = [];
    const mask = fxName === 'mask';
    lines.forEach((line) => {
      const ld = mk('div', 'kv-line', el);
      let word = null;
      for (const c of line) {
        if (c.ch === ' ') { ld.appendChild(document.createTextNode(' ')); word = null; continue; }
        if (!word) {
          word = mk('span', 'kv-w' + (mask ? ' kv-mask' : ''), ld);
          n.words.push({ el: word, chars: [] });
        }
        const cs = mk('span', 'kv-c' + (c.acc ? ' kv-accent' : '') + (c.mut ? ' kv-muted' : ''), word);
        cs.textContent = c.ch;
        const rec = { el: cs, ch: c.ch, st: c.st, word: n.words.length - 1, r: (() => { const g = rng(hashStr(n.id) + n.chars.length * 7919); return [g() * 2 - 1, g() * 2 - 1, g() * 2 - 1]; })() };
        n.chars.push(rec);
        n.words[n.words.length - 1].chars.push(rec);
      }
    });
    for (let k = 0; k < strikes; k++) {
      const line = mk('div', 'kv-strike-line', el);
      if (sp.strike_color) line.style.background = sp.strike_color;
      n.strikes.push({ el: line, run: k });
    }
    if (fxName === 'typewriter' || sp.caret) {
      n.caret = mk('div', 'kv-tcaret', el);
      if (sp.caret_color) n.caret.style.background = sp.caret_color;
    }
  },
  image(n, sp) {
    if (sp.svg) {
      const d = mk('div', 'kv-svg', n.el); d.innerHTML = sp.svg; n.inner = d; d.style.width = d.style.height = '100%';
    } else {
      const img = mk('img', 'kv-img', n.el);
      img.src = sp.src;
      img.style.objectFit = sp.fit || 'contain';
      n.inner = img;
    }
    if (sp.radius !== undefined) { n.el.style.borderRadius = sp.radius + 'px'; n.el.style.overflow = 'hidden'; }
    if (sp.shadow) n.el.style.boxShadow = sp.shadow === true ? theme.shadow : sp.shadow;
    if (sp.border) n.el.style.border = sp.border === true ? `1px solid ${theme.border}` : sp.border;
    if (n.w === undefined && n.h === undefined) n.w = 400;
    n.drawables = sp.svg ? prepDraw(n.inner) : null;
  },
  svg(n, sp) {
    const d = mk('div', 'kv-svg', n.el);
    d.innerHTML = sp.svg;
    d.style.width = d.style.height = '100%';
    if (sp.color) d.style.color = sp.color;
    n.inner = d;
    if (n.w === undefined) n.w = 200;
    if (n.h === undefined) n.h = n.w;
    n.drawables = prepDraw(d);
  },
  icon(n, sp) {
    const size = sp.size || 96;
    n.w = size; n.h = size;
    const d = mk('div', 'kv-svg', n.el);
    d.innerHTML = sp.svg;
    d.style.width = d.style.height = '100%';
    d.style.filter = 'drop-shadow(0 10px 24px rgba(0,0,0,.45))';
    n.inner = d;
    if (sp.label) { const l = mk('div', 'kv-label', n.el); l.textContent = sp.label; l.style.fontSize = Math.max(11, size * 0.15) + 'px'; }
  },
  shape(n, sp) {
    const kind = sp.kind || 'rect';
    SHAPES[kind](n, sp);
  },
  html(n, sp) {
    if (sp.css) { const st = document.createElement('style'); st.textContent = sp.css; document.head.appendChild(st); }
    const d = mk('div', 'kv-inner', n.el);
    d.innerHTML = sp.html;
    n.inner = d;
    n.slot = d;
    n.drawables = prepDraw(d);
  },
  ui(n, sp) {
    UI[sp.kind](n, sp);
  },
  group(n, sp) {
    n.slotW = 0; n.slotH = 0;
    if (sp.layout && sp.layout.kind === 'orbit') n.orbit = sp.layout;
  },
  particles(n, sp) {
    PARTICLES[sp.kind](n, sp);
  },
};

// Give every stroke in an SVG a length of one, so "draw" can run it from 0 to 1.
function prepDraw(root) {
  const els = root.querySelectorAll('path, line, polyline, polygon, circle, ellipse, rect');
  const out = [];
  els.forEach((e) => {
    const cs = getComputedStyle(e);
    if (e.getAttribute('stroke') || (cs.stroke && cs.stroke !== 'none')) {
      e.setAttribute('pathLength', '1');
      e.style.strokeDasharray = '1 1';
      out.push(e);
    }
  });
  return out.length ? out : null;
}

const SHAPES = {
  rect(n, sp) {
    n.w = n.w !== undefined ? n.w : 200; n.h = n.h !== undefined ? n.h : 120;
    const s = n.el.style;
    s.borderRadius = (sp.radius !== undefined ? sp.radius : 16) + 'px';
    if (sp.fill) s.background = Array.isArray(sp.fill) ? `linear-gradient(${sp.angle || 135}deg, ${sp.fill.join(', ')})` : sp.fill;
    if (sp.stroke) s.border = `${sp.stroke_width || 2}px solid ${sp.stroke}`;
    if (sp.shadow) s.boxShadow = sp.shadow === true ? theme.shadow : sp.shadow;
    n.baseFill = typeof sp.fill === 'string' ? sp.fill : null;
    n.fillProp = 'background';
  },
  circle(n, sp) {
    const d = sp.d || n.w || 120;
    n.w = n.h = d;
    const s = n.el.style;
    s.borderRadius = '50%';
    if (sp.fill) s.background = Array.isArray(sp.fill) ? `radial-gradient(circle at 35% 30%, ${sp.fill.join(', ')})` : sp.fill;
    else s.background = theme.accent;
    if (sp.stroke) s.border = `${sp.stroke_width || 2}px solid ${sp.stroke}`;
    n.baseFill = typeof sp.fill === 'string' ? sp.fill : theme.accent;
    n.fillProp = 'background';
  },
  ring(n, sp) {
    const d = sp.d || n.w || 200;
    n.w = n.h = d;
    const sw = sp.stroke_width || 3;
    const svg = `<svg viewBox="0 0 ${d} ${d}"><circle cx="${d / 2}" cy="${d / 2}" r="${d / 2 - sw / 2}" fill="none" stroke="${sp.stroke || sp.color || theme.accent}" stroke-width="${sw}" stroke-linecap="round" ${sp.dash ? `stroke-dasharray="${sp.dash}"` : ''}/></svg>`;
    const el = mk('div', 'kv-svg', n.el); el.innerHTML = svg; el.style.width = el.style.height = '100%';
    n.inner = el;
    n.drawables = sp.dash ? null : prepDraw(el);
  },
  line(n, sp) {
    const to = sp.to || [n.w || 300, 0];
    const sw = sp.stroke_width || 3;
    const minx = Math.min(0, to[0]), miny = Math.min(0, to[1]);
    const w = Math.abs(to[0]) + sw * 2, h = Math.abs(to[1]) + sw * 2;
    n.w = w; n.h = h;
    const x1 = -minx + sw, y1 = -miny + sw;
    const svg = `<svg viewBox="0 0 ${w} ${h}"><line x1="${x1}" y1="${y1}" x2="${x1 + to[0]}" y2="${y1 + to[1]}" stroke="${sp.stroke || sp.color || theme.accent}" stroke-width="${sw}" stroke-linecap="round"/></svg>`;
    const el = mk('div', 'kv-svg', n.el); el.innerHTML = svg; el.style.width = el.style.height = '100%';
    n.inner = el;
    n.drawables = prepDraw(el);
    if (!sp.anchor) sp.anchor = [x1 / w, y1 / h];
  },
  glow(n, sp) {
    const d = sp.d || n.w || 500;
    n.w = n.h = d;
    const c = sp.color || theme.accent;
    n.el.style.background = `radial-gradient(circle, ${alpha(c, sp.intensity || 0.7)} 0%, ${alpha(c, (sp.intensity || 0.7) * 0.35)} 30%, transparent 68%)`;
    n.el.style.borderRadius = '50%';
    if (sp.blend !== false) n.el.style.mixBlendMode = sp.blend || 'screen';
  },
  path(n, sp) {
    const vb = sp.viewBox || `0 0 ${n.w || 200} ${n.h || 200}`;
    const parts = vb.split(/[ ,]+/).map(Number);
    if (n.w === undefined) n.w = parts[2];
    if (n.h === undefined) n.h = parts[3];
    const svg = `<svg viewBox="${vb}"><path d="${esc(sp.d || '')}" fill="${sp.fill || 'none'}" stroke="${sp.stroke || (sp.fill ? 'none' : theme.accent)}" stroke-width="${sp.stroke_width || 3}" stroke-linecap="round" stroke-linejoin="round"/></svg>`;
    const el = mk('div', 'kv-svg', n.el); el.innerHTML = svg; el.style.width = el.style.height = '100%';
    n.inner = el;
    n.drawables = prepDraw(el);
  },
  star(n, sp) {
    const d = sp.d || n.w || 80;
    n.w = n.h = d;
    const pts = [];
    const k = sp.points || 5, inner = sp.inner || 0.45;
    for (let i = 0; i < k * 2; i++) {
      const r = i % 2 ? inner * 50 : 50;
      const a = (i * Math.PI) / k - Math.PI / 2;
      pts.push(`${(50 + r * Math.cos(a)).toFixed(2)},${(50 + r * Math.sin(a)).toFixed(2)}`);
    }
    const el = mk('div', 'kv-svg', n.el);
    el.innerHTML = `<svg viewBox="0 0 100 100"><polygon points="${pts.join(' ')}" fill="${sp.fill || sp.color || '#ffc53d'}" ${sp.stroke ? `stroke="${sp.stroke}" stroke-width="${sp.stroke_width || 3}"` : ''}/></svg>`;
    el.style.width = el.style.height = '100%';
    n.inner = el;
  },
};

// ---------------------------------------------------------------- component kit

const TICK = '<svg viewBox="0 0 16 16"><path d="M3.5 8.5l3 3 6-7"/></svg>';
function icoHtml(ic, color, cls) {
  if (!ic) return '';
  const bg = color ? `background:${Array.isArray(color) ? `linear-gradient(135deg, ${color.join(', ')})` : color}` : '';
  if (typeof ic === 'string' && ic.startsWith('<svg')) return `<div class="${cls}" style="${bg}">${ic}</div>`;
  if (typeof ic === 'string' && ic.startsWith('data:')) return `<div class="${cls}" style="${bg}"><img src="${ic}"></div>`;
  return `<div class="${cls}" style="${bg}">${esc(ic)}</div>`;
}
function panel(n, cls, sp) {
  const d = mk('div', 'kv-ui kv-panel ' + cls, n.el);
  n.inner = d;
  if (sp.radius !== undefined) d.style.borderRadius = sp.radius + 'px';
  if (sp.bg) d.style.background = sp.bg;
  if (sp.border === false) d.style.border = '0';
  if (sp.shadow === false) d.style.boxShadow = 'none';
  if (sp.glass) { d.style.background = alpha(theme.surface, 0.6); d.style.backdropFilter = 'blur(20px)'; }
  return d;
}
function sizeFrom(n, dw, dh) { if (n.w === undefined && dw !== undefined) n.w = dw; if (n.h === undefined && dh !== undefined) n.h = dh; }
function md(s) {
  // The small slice of markdown a chat message needs: **bold**, `code`, and line breaks.
  return esc(s).replace(/\*\*([^*]+)\*\*/g, '<b>$1</b>').replace(/`([^`]+)`/g, '<code style="font-family:var(--kv-mono);font-size:.92em;padding:1px 5px;border-radius:5px;background:var(--kv-hover)">$1</code>');
}

const UI = {
  window(n, sp) {
    sizeFrom(n, 1200, 720);
    const d = panel(n, 'kv-window', sp);
    d.style.width = d.style.height = '100%';
    const tb = sp.chrome === false ? '' : `<div class="kv-titlebar"><i></i><i></i><i></i>${sp.url ? `<div class="kv-url">${esc(sp.url)}</div>` : `<span>${esc(sp.title || '')}</span>`}</div>`;
    let side = '';
    if (sp.sidebar) {
      side = `<div class="kv-sidebar" ${sp.sidebar_width ? `style="--sbw:${sp.sidebar_width}px"` : ''}>` + sp.sidebar.map((it, i) => {
        if (typeof it === 'string' && it.startsWith('# ')) return `<div class="kv-sidehead">${esc(it.slice(2))}</div>`;
        const o = typeof it === 'string' ? { label: it } : it;
        return `<div class="kv-si${o.active ? ' kv-on' : ''}" data-i="${i}">${o.icon !== undefined ? icoHtml(o.icon, o.color, 'kv-ico') : ''}<span>${esc(o.label || '')}</span></div>`;
      }).join('') + '</div>';
    }
    d.innerHTML = `${tb}<div class="kv-wbody">${side}<div class="kv-wslot"></div></div>`;
    n.slot = d.querySelector('.kv-wslot');
  },
  // A handset: bezel, island and a screen that holds children, like a window.
  phone(n, sp) {
    sizeFrom(n, 390, 800);
    const d = mk('div', 'kv-ui kv-phone', n.el);
    n.inner = d;
    d.style.width = d.style.height = '100%';
    if (sp.frame) d.style.background = sp.frame;
    d.innerHTML = `<div class="kv-phone-scr" style="${sp.screen ? `background:${sp.screen}` : ''}">${sp.status === false ? '' : `<div class="kv-phone-sb"><span>${esc(sp.clock || '9:41')}</span><span>●●● ▮</span></div>`}<div class="kv-phone-island"></div>${sp.title ? `<div class="kv-phone-t"><b>${esc(sp.title)}</b>${sp.action ? `<span>${esc(sp.action)}</span>` : ''}</div>` : ''}<div class="kv-wslot"></div></div>`;
    n.slot = d.querySelector('.kv-wslot');
  },
  card(n, sp) {
    sizeFrom(n, 360, undefined);
    const d = panel(n, 'kv-card', sp);
    d.style.width = '100%';
    if (n.h !== undefined) d.style.height = '100%';
    const icon = sp.icon_src || sp.icon_svg || sp.icon;
    const head = (icon || sp.title) ? `<div class="kv-card-h">${icoHtml(icon, sp.icon_bg || (sp.icon_svg ? null : theme.accent), 'kv-ic')}<div class="kv-grow"><div class="kv-card-t">${esc(sp.title || '')}</div>${sp.subtitle ? `<div class="kv-card-s">${esc(sp.subtitle)}</div>` : ''}</div>${sp.badge ? `<span class="kv-badge">${esc(sp.badge)}</span>` : ''}${sp.check ? `<div class="kv-check">${TICK}</div>` : ''}</div>` : '';
    const body = sp.body ? `<div class="kv-card-b kv-stream">${md(sp.body)}</div>` : '';
    const lines = sp.lines ? `<div class="kv-skel">${Array.from({ length: sp.lines }, (_, i) => `<div style="width:${[92, 78, 85, 60, 70][i % 5]}%"></div>`).join('')}</div>` : '';
    const foot = sp.button ? `<div class="kv-card-f"><div class="kv-grow"></div><div class="kv-btn ${sp.button_style === 'ghost' ? '' : 'kv-primary'}" style="padding:7px 14px;font-size:13px">${esc(sp.button)}</div></div>` : '';
    d.innerHTML = head + body + lines + foot;
    if (sp.check) n.checkEl = d.querySelector('.kv-check');
  },
  input(n, sp) {
    sizeFrom(n, 760, undefined);
    const d = panel(n, 'kv-input', sp);
    d.style.width = '100%';
    const chips = (sp.chips || []).map((c) => { const o = typeof c === 'string' ? { label: c } : c; return `<div class="kv-chip">${o.icon ? icoHtml(o.icon, o.color || theme.accent, 'kv-cd') : ''}${esc(o.label)}</div>`; }).join('');
    const tools = Array.from({ length: sp.tools !== undefined ? sp.tools : 5 }, () => '<div class="kv-dot"></div>').join('');
    d.innerHTML = `${chips ? `<div class="kv-input-top">${chips}</div>` : ''}<div class="kv-input-text"><span class="kv-type">${esc(sp.text || '')}</span><span class="kv-ph">${esc(sp.text ? '' : sp.placeholder || 'Ask anything…')}</span><span class="kv-caret"></span></div><div class="kv-input-bar">${tools}${sp.send === false ? '' : `<div class="kv-send">${esc(sp.send_label || 'Send')} ↑</div>`}</div>`;
    n.caret = d.querySelector('.kv-caret');
    n.caret.style.opacity = sp.caret ? 1 : 0;
  },
  button(n, sp) {
    const d = mk('div', 'kv-ui kv-btn ' + (sp.variant === 'ghost' ? 'kv-ghost' : sp.variant === 'secondary' ? '' : 'kv-primary'), n.el);
    n.inner = d;
    d.innerHTML = `${sp.icon ? `<span>${esc(sp.icon)}</span>` : ''}<span class="kv-lbl">${esc(sp.label || 'Button')}</span>`;
    if (sp.size) d.style.fontSize = sp.size + 'px';
    if (sp.bg) d.style.background = sp.bg;
  },
  toggle(n, sp) {
    const d = mk('div', 'kv-ui kv-row', n.el);
    n.inner = d;
    d.innerHTML = `${sp.label ? `<span>${esc(sp.label)}</span>` : ''}<div class="kv-toggle"><div class="kv-knob"></div></div>`;
    n.toggles = [{ track: d.querySelector('.kv-toggle'), knob: d.querySelector('.kv-knob'), on: sp.on ? 1 : 0 }];
  },
  check(n, sp) {
    const d = mk('div', 'kv-ui kv-row', n.el);
    n.inner = d;
    d.innerHTML = `<div class="kv-check">${TICK}</div>${sp.label ? `<span>${esc(sp.label)}</span>` : ''}`;
    n.checks = [{ box: d.querySelector('.kv-check'), on: sp.checked ? 1 : 0 }];
  },
  chip(n, sp) {
    const d = mk('div', 'kv-ui kv-chip', n.el);
    n.inner = d;
    d.innerHTML = `${sp.icon ? icoHtml(sp.icon, sp.color || theme.accent, 'kv-cd') : ''}<span>${esc(sp.label || '')}</span>`;
    if (sp.size) d.style.fontSize = sp.size + 'px';
  },
  list(n, sp) {
    sizeFrom(n, 380, undefined);
    const d = panel(n, 'kv-list', sp);
    d.style.width = '100%';
    const head = sp.search ? `<div class="kv-list-h">⌕ <span class="kv-type">${esc(sp.search_text || '')}</span><span class="kv-ph">${esc(sp.search)}</span><span class="kv-caret" style="opacity:0"></span></div>` : sp.title ? `<div class="kv-list-h" style="color:var(--kv-muted);font-weight:600">${esc(sp.title)}</div>` : '';
    const items = (sp.items || []).map((it, i) => {
      const o = typeof it === 'string' ? { label: it } : it;
      const right = o.toggle !== undefined ? `<div class="kv-toggle" style="margin-left:${o.meta ? 10 : 'auto'}px"><div class="kv-knob"></div></div>` : o.check !== undefined ? `<div class="kv-check" style="margin-left:auto">${TICK}</div>` : '';
      return `<div class="kv-li" data-i="${i}">${o.icon !== undefined ? icoHtml(o.icon, o.color || null, 'kv-lico') : ''}<span>${esc(o.label || '')}</span>${o.badge ? `<span class="kv-badge">${esc(o.badge)}</span>` : ''}${o.meta ? `<span class="kv-meta">${esc(o.meta)}</span>` : ''}${right}</div>`;
    }).join('');
    d.innerHTML = head + items + '<div class="kv-hl"></div>';
    n.rows = [...d.querySelectorAll('.kv-li')];
    n.hl = d.querySelector('.kv-hl');
    n.caret = d.querySelector('.kv-caret');
    n.selected = sp.selected !== undefined ? sp.selected : -1;
    n.toggles = [];
    n.checks = [];
    (sp.items || []).forEach((it, i) => {
      const row = n.rows[i];
      if (it && it.toggle !== undefined) n.toggles[i] = { track: row.querySelector('.kv-toggle'), knob: row.querySelector('.kv-knob'), on: it.toggle ? 1 : 0 };
      if (it && it.check !== undefined) n.checks[i] = { box: row.querySelector('.kv-check'), on: it.check ? 1 : 0 };
    });
  },
  code(n, sp) {
    sizeFrom(n, 640, undefined);
    const d = panel(n, 'kv-code', sp);
    d.style.width = '100%';
    d.innerHTML = `${sp.title !== false ? `<div class="kv-code-h"><span>${esc(sp.title || sp.lang || 'code')}</span><span>⧉ Copy</span></div>` : ''}<pre><code></code></pre>`;
    if (sp.size) d.querySelector('pre').style.fontSize = sp.size + 'px';
    n.codeEl = d.querySelector('code');
    n.tokens = highlight(String(sp.code || ''), sp.lang || 'js');
    n.codeLen = n.tokens.reduce((a, t) => a + t.s.length, 0);
    renderCode(n, sp.typed === false ? 0 : n.codeLen);
  },
  message(n, sp) {
    sizeFrom(n, 620, undefined);
    const d = panel(n, 'kv-msg', sp);
    d.style.width = '100%';
    const av = sp.avatar_src || sp.avatar_svg || sp.avatar || (sp.name ? sp.name[0] : '•');
    d.innerHTML = `${icoHtml(av, sp.avatar_bg || theme.accent, 'kv-av')}<div style="min-width:0;flex:1"><div class="kv-msg-n">${esc(sp.name || '')}${sp.tag ? `<span class="kv-badge">${esc(sp.tag)}</span>` : ''}</div><div class="kv-msg-t kv-stream">${md(sp.text || '')}</div></div>`;
  },
  field(n, sp) {
    sizeFrom(n, 420, undefined);
    const d = mk('div', 'kv-ui kv-field', n.el);
    n.inner = d;
    d.innerHTML = `${sp.label ? `<label>${esc(sp.label)}</label>` : ''}<div class="kv-field-box"><span class="kv-type">${esc(sp.value || '')}</span><span class="kv-ph" style="color:var(--kv-faint)">${esc(sp.value ? '' : sp.placeholder || '')}</span><span class="kv-caret" style="opacity:0"></span><div class="kv-field-ok" style="opacity:0">${TICK}</div></div>`;
    n.caret = d.querySelector('.kv-caret');
    n.okEl = d.querySelector('.kv-field-ok');
    n.mask = !!sp.mask;
  },
  cursor(n, sp) {
    n.w = n.h = sp.size || 34;
    const hand = sp.style === 'hand';
    const d = mk('div', 'kv-cursor', n.el);
    d.innerHTML = hand
      ? '<svg viewBox="0 0 32 32"><path d="M11 4.5c1.4 0 2.5 1.1 2.5 2.5v7.2l1-.2c1.1-.2 2.1.4 2.5 1.4l.3.8.9-.2c1.2-.2 2.3.5 2.6 1.7l.1.3.6-.1c1.3-.2 2.5.8 2.6 2.1l.4 4.6c.2 2.6-1.3 5-3.8 5.9l-2.2.8c-2.3.8-4.9.1-6.4-1.8l-5.4-6.9c-.8-1-.6-2.4.4-3.2 1-.7 2.3-.6 3.1.3l.8.9V7c0-1.4 1.1-2.5 2.5-2.5z" fill="#fff" stroke="#000" stroke-width="1.4"/></svg>'
      : '<svg viewBox="0 0 32 32"><path d="M6 3.5v21.2l5.1-4.9 3.4 7.9 3.6-1.5-3.3-7.7h7.1z" fill="#000" stroke="#fff" stroke-width="1.8" stroke-linejoin="round"/></svg>';
    n.inner = d;
    if (!sp.anchor) sp.anchor = hand ? [0.35, 0.12] : [0.19, 0.11];
  },
  stat(n, sp) {
    const d = mk('div', 'kv-ui kv-stat', n.el);
    n.inner = d;
    d.innerHTML = `<div class="kv-stat-v">${esc(sp.prefix || '')}<span class="kv-num"></span>${esc(sp.suffix || '')}</div>${sp.label ? `<div class="kv-stat-l">${esc(sp.label)}</div>` : ''}`;
    if (sp.size) d.querySelector('.kv-stat-v').style.fontSize = sp.size + 'px';
    if (sp.size && sp.label) d.querySelector('.kv-stat-l').style.fontSize = Math.max(13, sp.size * 0.2) + 'px';
    if (sp.color) d.querySelector('.kv-stat-v').style.color = sp.color;
    n.numEl = d.querySelector('.kv-num');
    n.count = sp.value !== undefined ? sp.value : 0;
  },
  rating(n, sp) {
    const d = mk('div', 'kv-ui kv-rating', n.el);
    n.inner = d;
    const laurel = (flip) => `<svg class="kv-laurel" viewBox="0 0 26 58" style="${flip ? 'transform:scaleX(-1)' : ''}"><path d="M20 4C9 12 5 24 7 36s8 18 14 20" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>${[10, 18, 26, 34, 42].map((y, i) => `<ellipse cx="${9 + i * 0.6}" cy="${y}" rx="5" ry="2.4" transform="rotate(${-40 + i * 12} ${9 + i * 0.6} ${y})" fill="currentColor"/>`).join('')}</svg>`;
    const stars = '★★★★★';
    d.innerHTML = `${sp.laurel === false ? '' : laurel(false)}<div class="kv-rating-b">${sp.title ? `<div class="kv-rating-t">${esc(sp.title)}</div>` : ''}<div class="kv-rating-v">${esc(sp.value_label || (sp.value !== undefined ? sp.value : '5.0'))}</div><div class="kv-stars"><span class="kv-s0">${stars}</span><span class="kv-s1">${stars}</span></div>${sp.source ? `<div class="kv-stat-l" style="margin-top:3px;font-size:12px">${esc(sp.source)}</div>` : ''}</div>${sp.laurel === false ? '' : laurel(true)}`;
    n.starFill = d.querySelector('.kv-s1');
    n.stars = (sp.stars !== undefined ? sp.stars : 5) / 5;
    n.starFill.style.width = (n.stars * 100) + '%';
  },
  progress(n, sp) {
    sizeFrom(n, 360, undefined);
    const d = mk('div', 'kv-ui kv-prog', n.el);
    n.inner = d;
    d.innerHTML = '<div></div>';
    n.progEl = d.firstChild;
    n.prog = sp.value !== undefined ? sp.value : 0;
    if (sp.color) n.progEl.style.background = sp.color;
  },
  skeleton(n, sp) {
    sizeFrom(n, 360, undefined);
    const d = mk('div', 'kv-ui kv-skel', n.el);
    n.inner = d;
    const k = sp.lines || 3;
    d.innerHTML = Array.from({ length: k }, (_, i) => `<div style="width:${[100, 86, 92, 64, 78][i % 5]}%"></div>`).join('');
  },
  kbd(n, sp) {
    const d = mk('div', 'kv-ui kv-kbd', n.el);
    n.inner = d;
    d.textContent = sp.label || sp.key || '⌘';
    if (sp.size) { d.style.fontSize = sp.size * 0.42 + 'px'; d.style.height = d.style.minWidth = sp.size + 'px'; }
  },
  tile(n, sp) {
    const size = sp.size || 88;
    n.w = n.w || size; n.h = n.h || size;
    const d = mk('div', 'kv-tile' + (sp.shape === 'circle' ? ' kv-circle' : ''), n.el);
    n.inner = d;
    const bg = sp.bg ? (Array.isArray(sp.bg) ? `linear-gradient(145deg, ${sp.bg.join(', ')})` : sp.bg) : (sp.icon_svg || sp.src ? 'transparent' : `linear-gradient(145deg, ${theme.accent}, ${theme.accent2})`);
    d.style.background = bg;
    if (sp.radius !== undefined) d.style.setProperty('--tr', sp.radius + (typeof sp.radius === 'number' ? 'px' : ''));
    if (sp.icon_svg) { d.innerHTML = sp.icon_svg; d.style.boxShadow = '0 12px 30px -10px rgba(0,0,0,.6)'; }
    else if (sp.src) { d.innerHTML = `<img src="${sp.src}" style="object-fit:${sp.fit || 'cover'}">`; }
    else if (sp.svg) { d.innerHTML = sp.svg; }
    else { d.textContent = sp.glyph || sp.icon || '★'; d.style.fontSize = (sp.glyph_size || size * 0.46) + 'px'; if (sp.color) d.style.color = sp.color; }
    if (sp.ring) d.style.boxShadow += `, 0 0 0 ${sp.ring_width || 2}px ${sp.ring}`;
    if (sp.label) { const l = mk('div', 'kv-label', n.el); l.textContent = sp.label; l.style.fontSize = Math.max(11, size * 0.14) + 'px'; }
  },
};

// A highlighter good enough for a code block that is on screen for two seconds.
const KW = new Set('const let var function return if else for while import from export default class new async await fn pub use struct impl enum match mut self type interface extends def print in of true false null None True False let yield try catch throw'.split(' '));
function highlight(code, lang) {
  const out = [];
  const re = /(\/\/[^\n]*|#[^\n]*|\/\*[\s\S]*?\*\/)|("(?:[^"\\\n]|\\.)*"|'(?:[^'\\\n]|\\.)*'|`(?:[^`\\]|\\.)*`)|(\b\d+(?:\.\d+)?\b)|([A-Za-z_$][\w$]*)(\s*\()?|([{}()[\];,.<>=+\-*/!&|:?]+)|(\s+)|(.)/g;
  let m;
  while ((m = re.exec(code))) {
    if (m[1]) { if (m[1][0] === '#' && lang !== 'py' && lang !== 'python' && lang !== 'sh' && lang !== 'bash') { out.push({ s: '#', c: 'p' }); re.lastIndex = m.index + 1; } else out.push({ s: m[1], c: 'c' }); }
    else if (m[2]) out.push({ s: m[2], c: 's' });
    else if (m[3]) out.push({ s: m[3], c: 'n' });
    else if (m[4]) {
      const w = m[4];
      out.push({ s: w, c: KW.has(w) ? 'k' : m[5] ? 'f' : /^[A-Z]/.test(w) ? 't' : '' });
      if (m[5]) out.push({ s: m[5], c: 'p' });
    }
    else if (m[6]) out.push({ s: m[6], c: 'p' });
    else out.push({ s: m[7] || m[8], c: '' });
  }
  return out;
}
function renderCode(n, k) {
  if (n.codeShown === k) return;
  n.codeShown = k;
  let left = k, html = '';
  for (const t of n.tokens) {
    if (left <= 0) break;
    const s = t.s.length <= left ? t.s : t.s.slice(0, left);
    left -= s.length;
    html += t.c ? `<span class="kv-tk-${t.c}">${esc(s)}</span>` : esc(s);
  }
  if (k < n.codeLen) html += '<span class="kv-caret"></span>';
  n.codeEl.innerHTML = html;
}

// ---------------------------------------------------------------- particles

function canvasFor(n, sp) {
  const w = len(sp.w, W, W), h = len(sp.h, H, H);
  n.w = w; n.h = h;
  const cv = mk('canvas', 'kv-canvas', n.el);
  cv.width = w; cv.height = h;
  n.cv = cv; n.cx = cv.getContext('2d');
  if (sp.blend !== false) n.el.style.mixBlendMode = sp.blend || 'screen';
  return { w, h };
}
function colorsOf(sp) {
  const c = sp.colors || (sp.color ? [sp.color] : [theme.accent, '#ffffff', theme.accent2]);
  return c.map((x) => parseColor(x) || [255, 255, 255, 1]);
}
const rgba = (c, a) => `rgba(${c[0] | 0},${c[1] | 0},${c[2] | 0},${clamp(a, 0, 1).toFixed(3)})`;

const PARTICLES = {
  stars(n, sp) {
    const { w, h } = canvasFor(n, sp);
    const r = rng(hashStr(n.id));
    const cols = colorsOf(Object.assign({ colors: ['#ffffff'] }, sp));
    const ps = Array.from({ length: sp.count || 120 }, () => ({ x: r() * w, y: r() * h, d: r(), ph: r() * TAU, c: cols[(r() * cols.length) | 0] }));
    n.drawParticles = (lt) => {
      const cx = n.cx; cx.clearRect(0, 0, w, h);
      for (const p of ps) {
        const x = (((p.x + lt * (sp.vx !== undefined ? sp.vx : 12) * p.d) % w) + w) % w;
        const y = (((p.y + lt * (sp.vy || 0) * p.d) % h) + h) % h;
        const a = (0.3 + 0.7 * p.d) * (0.6 + 0.4 * Math.sin(lt * 2 + p.ph));
        cx.fillStyle = rgba(p.c, a);
        cx.beginPath(); cx.arc(x, y, (sp.size || 1.6) * (0.4 + p.d), 0, TAU); cx.fill();
      }
    };
  },
  dust(n, sp) {
    const { w, h } = canvasFor(n, sp);
    const r = rng(hashStr(n.id));
    const cols = colorsOf(sp);
    const ps = Array.from({ length: sp.count || 60 }, () => ({ x: r() * w, y: r() * h, d: r(), ph: r() * TAU, s: 1 + r() * 3, c: cols[(r() * cols.length) | 0] }));
    n.drawParticles = (lt) => {
      const cx = n.cx; cx.clearRect(0, 0, w, h);
      for (const p of ps) {
        const x = (((p.x + Math.sin(lt * 0.4 + p.ph) * 30 + lt * 8 * p.d) % w) + w) % w;
        const y = (((p.y - lt * (10 + 20 * p.d)) % h) + h) % h;
        const g = cx.createRadialGradient(x, y, 0, x, y, p.s * 3);
        g.addColorStop(0, rgba(p.c, 0.55 * (0.4 + p.d)));
        g.addColorStop(1, rgba(p.c, 0));
        cx.fillStyle = g; cx.fillRect(x - p.s * 3, y - p.s * 3, p.s * 6, p.s * 6);
      }
    };
  },
  bokeh(n, sp) {
    const { w, h } = canvasFor(n, sp);
    const r = rng(hashStr(n.id));
    const cols = colorsOf(sp);
    const ps = Array.from({ length: sp.count || 18 }, () => ({ x: r() * w, y: r() * h, s: 20 + r() * (sp.size || 90), ph: r() * TAU, c: cols[(r() * cols.length) | 0], a: 0.05 + r() * 0.15 }));
    n.drawParticles = (lt) => {
      const cx = n.cx; cx.clearRect(0, 0, w, h);
      for (const p of ps) {
        const x = p.x + Math.sin(lt * 0.2 + p.ph) * 60, y = p.y + Math.cos(lt * 0.15 + p.ph) * 40;
        const g = cx.createRadialGradient(x, y, p.s * 0.2, x, y, p.s);
        g.addColorStop(0, rgba(p.c, p.a)); g.addColorStop(0.8, rgba(p.c, p.a * 0.6)); g.addColorStop(1, rgba(p.c, 0));
        cx.fillStyle = g; cx.beginPath(); cx.arc(x, y, p.s, 0, TAU); cx.fill();
      }
    };
  },
  // Sparks from a point, starting at `at`: fast, decelerating, fading, with a streak.
  burst(n, sp) {
    const { w, h } = canvasFor(n, sp);
    const r = rng(hashStr(n.id));
    const cols = colorsOf(sp);
    const at = sp.at || 0, life = sp.life || 1.1, speed = sp.speed || 900;
    const ps = Array.from({ length: sp.count || 80 }, () => {
      const a = r() * TAU, v = speed * (0.25 + r() * 0.75);
      return { vx: Math.cos(a) * v, vy: Math.sin(a) * v, l: life * (0.5 + r() * 0.5), s: 1 + r() * (sp.size || 3), c: cols[(r() * cols.length) | 0] };
    });
    const ox = len(sp.ox, w, w / 2), oy = len(sp.oy, h, h / 2);
    n.drawParticles = (lt) => {
      const cx = n.cx; cx.clearRect(0, 0, w, h);
      const e = lt - at;
      if (e < 0 || e > life * 1.05) return;
      cx.lineCap = 'round';
      for (const p of ps) {
        if (e > p.l) continue;
        const k = 4; // drag
        const d = (1 - Math.exp(-k * e)) / k, d2 = (1 - Math.exp(-k * Math.max(0, e - 0.05))) / k;
        const g = (sp.gravity || 0) * e * e * 0.5;
        const x = ox + p.vx * d, y = oy + p.vy * d + g;
        const x2 = ox + p.vx * d2, y2 = oy + p.vy * d2 + (sp.gravity || 0) * Math.max(0, e - 0.05) ** 2 * 0.5;
        const a = 1 - e / p.l;
        cx.strokeStyle = rgba(p.c, a);
        cx.lineWidth = p.s * a + 0.5;
        cx.beginPath(); cx.moveTo(x2, y2); cx.lineTo(x, y); cx.stroke();
      }
    };
  },
  // Hyperspace: streaks rushing out of the centre, faster the further out.
  rays(n, sp) {
    const { w, h } = canvasFor(n, sp);
    const r = rng(hashStr(n.id));
    const cols = colorsOf(Object.assign({ colors: ['#ffffff', theme.accent] }, sp));
    const ps = Array.from({ length: sp.count || 140 }, () => ({ a: r() * TAU, z: r(), sp: 0.5 + r(), c: cols[(r() * cols.length) | 0] }));
    const ox = len(sp.ox, w, w / 2), oy = len(sp.oy, h, h / 2);
    const R = Math.hypot(w, h) / 2;
    n.drawParticles = (lt) => {
      const cx = n.cx; cx.clearRect(0, 0, w, h);
      const at = sp.at || 0, dur = sp.dur || 1.5;
      const e = lt - at;
      if (e < 0 || e > dur) return;
      const intensity = Math.sin(Math.PI * clamp(e / dur, 0, 1));
      cx.lineCap = 'round';
      for (const p of ps) {
        const z = (p.z + e * p.sp * (sp.speed || 1.2)) % 1;
        const r1 = R * Math.pow(z, 2.2), r2 = R * Math.pow(Math.max(0, z - 0.08 - 0.1 * intensity), 2.2);
        const cs = Math.cos(p.a), sn = Math.sin(p.a);
        cx.strokeStyle = rgba(p.c, z * intensity);
        cx.lineWidth = 0.5 + z * 2.5;
        cx.beginPath(); cx.moveTo(ox + cs * r2, oy + sn * r2); cx.lineTo(ox + cs * r1, oy + sn * r1); cx.stroke();
      }
    };
  },
  shockwave(n, sp) {
    const { w, h } = canvasFor(n, sp);
    const cols = colorsOf(Object.assign({ colors: [theme.accent] }, sp));
    const ox = len(sp.ox, w, w / 2), oy = len(sp.oy, h, h / 2);
    const rings = sp.rings || 2;
    n.drawParticles = (lt) => {
      const cx = n.cx; cx.clearRect(0, 0, w, h);
      const at = sp.at || 0, dur = sp.dur || 1.1, maxR = sp.radius || Math.hypot(w, h) * 0.45;
      for (let k = 0; k < rings; k++) {
        const e = (lt - at - k * 0.09) / dur;
        if (e < 0 || e > 1) continue;
        const rr = maxR * EASE.outExpo(e);
        cx.strokeStyle = rgba(cols[k % cols.length], (1 - e) * (k ? 0.5 : 0.9));
        cx.lineWidth = (sp.width || 10) * (1 - e) + 1;
        cx.beginPath(); cx.arc(ox, oy, rr, 0, TAU); cx.stroke();
      }
    };
  },
  confetti(n, sp) {
    const { w, h } = canvasFor(n, sp);
    n.el.style.mixBlendMode = sp.blend || 'normal';
    const r = rng(hashStr(n.id));
    const cols = colorsOf(Object.assign({ colors: [theme.accent, '#ffcf3f', '#ff5d8f', '#34c77b', '#ffffff'] }, sp));
    const ox = len(sp.ox, w, w / 2), oy = len(sp.oy, h, h * 0.35);
    const ps = Array.from({ length: sp.count || 120 }, () => {
      const a = -Math.PI / 2 + (r() - 0.5) * 2.2, v = (sp.speed || 1100) * (0.35 + r() * 0.65);
      return { vx: Math.cos(a) * v, vy: Math.sin(a) * v, rot: r() * TAU, vr: (r() - 0.5) * 18, s: 6 + r() * 8, c: cols[(r() * cols.length) | 0], fl: r() * TAU };
    });
    n.drawParticles = (lt) => {
      const cx = n.cx; cx.clearRect(0, 0, w, h);
      const e = lt - (sp.at || 0);
      if (e < 0 || e > (sp.life || 3)) return;
      for (const p of ps) {
        const k = 1.6;
        const d = (1 - Math.exp(-k * e)) / k;
        const x = ox + p.vx * d + Math.sin(e * 3 + p.fl) * 20;
        const y = oy + p.vy * d + 520 * e * e * 0.5;
        cx.save(); cx.translate(x, y); cx.rotate(p.rot + p.vr * e);
        cx.scale(1, Math.cos(e * 8 + p.fl));
        cx.fillStyle = rgba(p.c, 1 - clamp((e - (sp.life || 3) + 0.6) / 0.6, 0, 1));
        cx.fillRect(-p.s / 2, -p.s / 4, p.s, p.s / 2);
        cx.restore();
      }
    };
  },
};

// ---------------------------------------------------------------- acts

const ACTS = {
  type(n, a) {
    const el = a.sel ? n.inner.querySelector(a.sel) : n.inner.querySelector('.kv-type');
    if (n.codeEl && !a.sel) {
      const total = n.codeLen;
      const dur = a.dur !== undefined ? a.dur : total / (a.cps || 90);
      n.codeTyped = true;
      renderCode(n, 0);
      return { dom: (lt) => renderCode(n, lt < a.at ? 0 : Math.min(total, Math.floor(((lt - a.at) / dur) * total))) };
    }
    if (!el) { warnings.push(`${n.id}: nothing to type into (no .kv-type${a.sel ? ' or ' + a.sel : ''})`); return {}; }
    const text = String(a.text || '');
    const dur = a.dur !== undefined ? a.dur : text.length / (a.cps || 28);
    const ph = el.parentNode.querySelector('.kv-ph');
    const caret = n.caret || (a.sel ? null : n.inner.querySelector('.kv-caret'));
    const prior = el.textContent;
    const hold = a.hold !== undefined ? a.hold : 1.2;
    return {
      dom: (lt) => {
        if (lt < a.at) return;
        const k = Math.min(text.length, Math.floor(((lt - a.at) / Math.max(dur, 1e-6)) * text.length + 1e-9));
        const shown = (a.append ? prior : '') + text.slice(0, k);
        const v = n.mask ? '•'.repeat(shown.length) : shown;
        if (el.textContent !== v) el.textContent = v;
        if (ph) setStyle(ph, 'display', shown.length ? 'none' : '');
        if (caret) {
          const typing = lt < a.at + dur;
          const blink = typing ? 1 : (lt < a.at + dur + hold ? (Math.floor((lt - a.at - dur) * 2.2) % 2 === 0 ? 1 : 0) : 0);
          setStyle(caret, 'opacity', String(blink));
        }
      },
    };
  },
  stream(n, a) {
    const el = a.sel ? n.inner.querySelector(a.sel) : n.inner.querySelector('.kv-stream');
    if (!el) { warnings.push(`${n.id}: nothing to stream (no text body)`); return {}; }
    // Wrap every word once so revealing is a style change, not a re-layout.
    const walk = (node) => {
      for (const c of [...node.childNodes]) {
        if (c.nodeType === 3) {
          const parts = c.textContent.split(/(\s+)/);
          const frag = document.createDocumentFragment();
          for (const p of parts) {
            if (!p) continue;
            if (/^\s+$/.test(p)) frag.appendChild(document.createTextNode(p));
            else { const s = document.createElement('span'); s.textContent = p; s.className = 'kv-sw'; frag.appendChild(s); }
          }
          c.replaceWith(frag);
        } else walk(c);
      }
    };
    if (!el.__kvWrapped) { walk(el); el.__kvWrapped = true; }
    const words = [...el.querySelectorAll('.kv-sw')];
    const dur = a.dur !== undefined ? a.dur : words.length / (a.wps || 14);
    return {
      dom: (lt) => {
        const k = lt < a.at ? 0 : Math.min(words.length, ((lt - a.at) / Math.max(dur, 1e-6)) * words.length);
        words.forEach((w, i) => {
          const o = clamp(k - i, 0, 1);
          setStyle(w, 'opacity', o.toFixed(2));
          setStyle(w, 'filter', o < 1 ? `blur(${((1 - o) * 4).toFixed(1)}px)` : 'none');
        });
      },
    };
  },
  click(n, a) {
    const d = a.dur || 0.35;
    const isCursor = n.spec.kind === 'cursor';
    const press = (e) => (e < d * 0.3 ? EASE.outQuad(e / (d * 0.3)) : 1 - EASE.outBack((e - d * 0.3) / (d * 0.7)));
    // With a selector, the element inside is what gets pressed and where the ripple starts.
    const sub = a.sel ? n.inner.querySelector(a.sel) : null;
    if (a.sel && !sub) warnings.push(`click on ${n.id}: "${a.sel}" matched nothing`);
    const host = sub || n.el;
    if (sub && getComputedStyle(sub).position === 'static') sub.style.position = 'relative';
    const ripple = mk('div', 'kv-ripple', host);
    ripple.style.opacity = 0;
    if (a.color) ripple.style.borderColor = a.color;
    return {
      mods: sub ? null : (lt) => {
        const e = lt - a.at;
        if (e < 0 || e > d) return null;
        return { s: 1 - (a.depth || (isCursor ? 0.18 : 0.06)) * press(e) };
      },
      dom: (lt) => {
        const e = lt - a.at;
        if (sub) setStyle(sub, 'transform', e >= 0 && e <= d ? `scale(${(1 - (a.depth || 0.1) * press(e)).toFixed(4)})` : 'none');
        const rd = 0.55;
        if (e < 0 || e > rd) { setStyle(ripple, 'opacity', '0'); return; }
        const u = e / rd;
        const bw = sub ? sub.offsetWidth : n.boxW || 80, bh = sub ? sub.offsetHeight : n.boxH || 40;
        const size = (a.size || (isCursor ? 56 : Math.max(bw, bh) * 1.5)) * EASE.outCubic(u);
        const cxp = isCursor ? n.ax * bw : bw / 2;
        const cyp = isCursor ? n.ay * bh : bh / 2;
        setStyle(ripple, 'opacity', (1 - u).toFixed(3));
        setStyle(ripple, 'width', size.toFixed(1) + 'px');
        setStyle(ripple, 'height', size.toFixed(1) + 'px');
        setStyle(ripple, 'left', (cxp - size / 2).toFixed(1) + 'px');
        setStyle(ripple, 'top', (cyp - size / 2).toFixed(1) + 'px');
      },
    };
  },
  toggle(n, a) { return stateAct(n, a, 'toggles'); },
  check(n, a) { return stateAct(n, a, 'checks'); },
  select(n, a) {
    n.selectActs = (n.selectActs || []).concat([a]).sort((x, y) => x.at - y.at);
    return {};
  },
  count(n, a) {
    n.countActs = (n.countActs || []).concat([a]).sort((x, y) => x.at - y.at);
    return {};
  },
  highlight(n, a) {
    const d = a.dur || 0.8;
    const c = a.color || theme.accent;
    return {
      dom: (lt) => {
        const e = lt - a.at;
        const hold = a.hold !== undefined ? a.hold : d;
        let v = 0;
        if (e >= 0) v = e < 0.25 ? EASE.outCubic(e / 0.25) : e < hold ? 1 : clamp(1 - (e - hold) / 0.4, 0, 1);
        if (a.until !== undefined && lt > a.until) v = clamp(1 - (lt - a.until) / 0.3, 0, 1);
        const target = a.sel ? n.inner.querySelector(a.sel) : n.inner;
        if (!target) return;
        setStyle(target, 'boxShadow', v > 0 ? `0 0 0 ${(2 * v).toFixed(2)}px ${alpha(c, v)}, 0 0 ${(40 * v).toFixed(1)}px ${alpha(c, 0.45 * v)}` : '');
      },
    };
  },
  strike(n, a) {
    n.strikeActs = (n.strikeActs || []).concat([a]);
    return {};
  },
  progress(n, a) {
    n.progActs = (n.progActs || []).concat([a]).sort((x, y) => x.at - y.at);
    return {};
  },
  class(n, a) {
    const els = a.sel ? [...n.inner.querySelectorAll(a.sel)] : [n.inner];
    return {
      dom: (lt) => {
        const on = lt >= a.at && (a.until === undefined || lt < a.until);
        for (const e of els) e.classList.toggle(a.name, on);
      },
    };
  },
  text(n, a) {
    const el = a.sel ? n.inner.querySelector(a.sel) : n.inner;
    if (!el) return {};
    const before = el.innerHTML;
    return { dom: (lt) => { const v = lt >= a.at ? esc(a.text || '') : before; if (el.innerHTML !== v) el.innerHTML = v; } };
  },
};

// Toggles and checks fold every act up to now: on at 1s, off at 3s, on again.
function stateAct(n, a, which) {
  const key = which + 'Acts';
  n[key] = (n[key] || []).concat([a]).sort((x, y) => x.at - y.at);
  if (which === 'checks' && !n.checks && n.checkEl) n.checks = [{ box: n.checkEl, on: 0 }];
  return {};
}
function foldState(acts, idx, initial, lt, dflDur) {
  let v = initial;
  for (const a of acts || []) {
    if ((a.index || 0) !== idx) continue;
    if (lt < a.at) break;
    const target = a.value === false ? 0 : 1;
    const u = clamp((lt - a.at) / (a.dur || dflDur), 0, 1);
    v = lerp(v, target, EASE.outCubic(u));
  }
  return v;
}

// ---------------------------------------------------------------- build

for (const spec of S.nodes || []) {
  const n = new Node(spec);
  n.anims = [];
  nodes.push(n);
  byId[n.id] = n;
}
for (const a of S.acts || []) {
  const n = byId[a.target];
  const A = ACTS[a.do](n, a);
  n.acts.push(A);
}
// `anim` ops: more motion on a layer, or on elements inside it.
for (const a of S.anims || []) {
  const n = byId[a.target];
  if (!a.sel) {
    const inn = normFx(a.in || (a.fx && IN_FX[a.fx] && !a.out ? { fx: a.fx, at: a.at, dur: a.dur, ease: a.ease, stagger: a.stagger } : null), 0.6);
    const out = normFx(a.out || (a.fx && !IN_FX[a.fx] && OUT_FX[a.fx] ? { fx: a.fx, at: a.at, dur: a.dur, ease: a.ease } : null), 0.5);
    const keys = compileKeys(a.keys || (a.to ? [Object.assign({ at: a.at || 0, dur: a.dur || 0.6, ease: a.ease }, a.to)] : []), n.id);
    n.keys = mergeKeys(n.keys, keys);
    if (a.fx && LOOP_FX[a.fx] && !IN_FX[a.fx] && !OUT_FX[a.fx]) n.loops.push(Object.assign({}, a));
    for (const l of [].concat(a.loop || [])) n.loops.push(typeof l === 'string' ? { fx: l } : l);
    if (inn) n.anims.push(fxFn(inn, IN_FX, false, 0, 1));
    if (out) n.anims.push(fxFn(out, OUT_FX, true, 0, 1));
  } else {
    n.pendingSubs = (n.pendingSubs || []).concat([a]);
  }
}
function fxFn(fx, table, isOut, i, cnt) {
  const F = fx.fx ? table[fx.fx] : null;
  return (lt, t, c) => {
    const q = unitProgress(fx, lt, i, cnt, 0.08);
    if (isOut) {
      if (q <= 0) return null;
      if (fx.to) return fromTo(fx.to, 1 - easeFn(fx.ease, 'inExpo')(q));
      return F.f(easeFn(fx.ease, F.ease)(q), c, lt);
    }
    if (fx.from) return fromTo(fx.from, easeFn(fx.ease, 'outExpo')(q));
    return F.f(easeFn(fx.ease, F.ease)(q), c, lt);
  };
}
// A group's `cascade` hands an entrance to each child, one after another.
for (const n of nodes) {
  const cz = n.spec.cascade;
  if (!cz) continue;
  const c = normFx(cz, 0.6);
  const st = c.stagger !== undefined ? c.stagger : 0.08;
  n.kids.forEach((k, i) => {
    if (!k.inFx) k.inFx = Object.assign({}, c, { at: c.at + i * st, stagger: undefined });
  });
}

// ---------------------------------------------------------------- layout pass (after fonts)

function measure() {
  for (const n of nodes) {
    n.boxW = n.el.offsetWidth; n.boxH = n.el.offsetHeight;
    if (n.slot !== n.el) { n.slotW = n.slot.offsetWidth; n.slotH = n.slot.offsetHeight; }
    else if (!n.isGroup) { n.slotW = n.boxW; n.slotH = n.boxH; }
    if (n.chars) {
      const base = n.inner.getBoundingClientRect();
      for (const c of n.chars) {
        const r = c.el.getBoundingClientRect();
        c.bx = r.left - base.left; c.by = r.top - base.top; c.bw = r.width; c.bh = r.height;
      }
      for (const s of n.strikes) {
        const cs = n.chars.filter((c) => c.st === s.run);
        if (!cs.length) continue;
        const x0 = Math.min(...cs.map((c) => c.bx)), x1 = Math.max(...cs.map((c) => c.bx + c.bw));
        const y = cs[0].by + cs[0].bh * 0.56;
        s.el.style.left = x0 - n.fontSize * 0.03 + 'px';
        s.el.style.width = x1 - x0 + n.fontSize * 0.06 + 'px';
        s.el.style.top = y + 'px';
      }
    }
  }
  // Layouts that need sizes: rows, columns, grids and rings.
  for (const n of nodes) {
    const L = n.spec.layout;
    if (!L || L.kind === 'orbit') continue;
    const kids = n.kids;
    const gap = L.gap !== undefined ? L.gap : 24;
    if (L.kind === 'row' || L.kind === 'column') {
      const horiz = L.kind === 'row';
      const sizes = kids.map((k) => (horiz ? k.boxW : k.boxH));
      const total = sizes.reduce((a, b) => a + b, 0) + gap * Math.max(0, kids.length - 1);
      let pos = -total / 2;
      kids.forEach((k, i) => {
        const c = pos + sizes[i] / 2;
        k.layoutPos = horiz ? [c, 0] : [0, c];
        pos += sizes[i] + gap;
      });
    } else if (L.kind === 'grid') {
      const cols = L.cols || Math.ceil(Math.sqrt(kids.length));
      const cw = L.cell ? L.cell[0] : Math.max(...kids.map((k) => k.boxW));
      const ch = L.cell ? L.cell[1] : Math.max(...kids.map((k) => k.boxH));
      const rows = Math.ceil(kids.length / cols);
      kids.forEach((k, i) => {
        const cx = i % cols, cy = Math.floor(i / cols);
        k.layoutPos = [(cx - (cols - 1) / 2) * (cw + gap), (cy - (rows - 1) / 2) * (ch + gap)];
      });
    } else if (L.kind === 'ring') {
      const R = L.r || 300;
      kids.forEach((k, i) => {
        const a = (i / kids.length) * TAU - Math.PI / 2 + ((L.phase || 0) * Math.PI) / 180;
        k.layoutPos = [Math.cos(a) * R, Math.sin(a) * R * (L.squash || 1)];
      });
    } else if (L.kind === 'scatter') {
      const r = rng(hashStr(n.id));
      const w = L.w || W * 0.8, h = L.h || H * 0.7;
      kids.forEach((k) => { k.layoutPos = [(r() - 0.5) * w, (r() - 0.5) * h]; });
    }
  }
  // Sub-element animations, now the elements exist.
  for (const n of nodes) {
    for (const a of n.pendingSubs || []) {
      const els = [...n.inner.querySelectorAll(a.sel)];
      if (!els.length) warnings.push(`anim on ${n.id}: "${a.sel}" matched nothing`);
      const inn = normFx(a.in || (a.fx && IN_FX[a.fx] && !a.out ? { fx: a.fx, at: a.at || 0, dur: a.dur, ease: a.ease, stagger: a.stagger, order: a.order } : null), 0.6);
      const out = normFx(a.out || null, 0.5);
      const keys = compileKeys(a.keys || (a.to ? [Object.assign({ at: a.at || 0, dur: a.dur || 0.6, ease: a.ease }, a.to)] : []), n.id);
      const loops = [].concat(a.loop || []).map((l) => (typeof l === 'string' ? { fx: l } : l));
      if (a.fx && LOOP_FX[a.fx] && !IN_FX[a.fx]) loops.push(Object.assign({}, a));
      els.forEach((el, i) => {
        el.style.display = getComputedStyle(el).display === 'inline' ? 'inline-block' : el.style.display;
        el.style.transformOrigin = a.origin || '50% 50%';
        const sub = {
          el, n, i, keys, loops,
          r: (() => { const g = rng(hashStr(n.id + a.sel) + i * 131); return [g() * 2 - 1, g() * 2 - 1, g() * 2 - 1]; })(),
          fin: inn ? fxFn(Object.assign({}, inn, { stagger: inn.stagger !== undefined ? inn.stagger : 0.07 }), IN_FX, false, i, els.length) : null,
          fout: out ? fxFn(Object.assign({}, out, { stagger: out.stagger !== undefined ? out.stagger : 0.05 }), OUT_FX, true, i, els.length) : null,
          u: Math.max(20, Math.min(el.offsetWidth || 60, el.offsetHeight || 60)),
        };
        n.subs.push(sub);
      });
    }
  }
}

// ---------------------------------------------------------------- per-frame

function transformOf(p, m, ax, ay, flow) {
  const x = p.x + m.x, y = p.y + m.y, z = p.z + m.z;
  const s = p.scale * m.s;
  const sx = s * p.sx * m.sx, sy = s * p.sy * m.sy;
  const r = p.rotate + m.r, rx = p.rx + m.rx, ry = p.ry + m.ry;
  let tr = `translate3d(${x.toFixed(2)}px,${y.toFixed(2)}px,${z.toFixed(1)}px)`;
  if (rx) tr += ` rotateX(${rx.toFixed(2)}deg)`;
  if (ry) tr += ` rotateY(${ry.toFixed(2)}deg)`;
  if (r) tr += ` rotate(${r.toFixed(2)}deg)`;
  if (sx !== 1 || sy !== 1) tr += ` scale(${sx.toFixed(4)},${sy.toFixed(4)})`;
  if (!flow && (ax || ay)) tr += ` translate(${(-ax * 100).toFixed(2)}%,${(-ay * 100).toFixed(2)}%)`;
  return tr;
}
function filterOf(p, m, glowColor) {
  const f = [];
  const b = p.blur + m.b;
  if (b > 0.05) f.push(`blur(${b.toFixed(2)}px)`);
  if (p.bright !== 1) f.push(`brightness(${p.bright.toFixed(3)})`);
  const g = p.glow + m.glow;
  if (g > 0.3) f.push(`drop-shadow(0 0 ${g.toFixed(1)}px ${glowColor})`);
  if (m.rgb > 0.3) f.push(`drop-shadow(${m.rgb.toFixed(1)}px 0 0 rgba(255,0,90,.75)) drop-shadow(${(-m.rgb).toFixed(1)}px 0 0 rgba(0,220,255,.75))`);
  return f.length ? f.join(' ') : 'none';
}

function lifetime(n) {
  let a = n.inFx ? n.inFx.at + (n.inFx.delay || 0) : -Infinity;
  let b = Infinity;
  if (n.outFx) {
    const cnt = n.chars && n.split !== 'none' ? unitsOf(n).length : 1;
    const st = n.outFx.stagger !== undefined ? n.outFx.stagger : 0.03;
    b = n.outFx.at + (n.outFx.delay || 0) + n.outFx.dur + (cnt - 1) * st + 0.02;
  }
  if (n.spec.from !== undefined) a = Math.max(a, n.spec.from);
  if (n.spec.until !== undefined) b = Math.min(b, n.spec.until);
  return [a, b];
}
function unitsOf(n) {
  if (n.split === 'word') return n.words;
  if (n.split === 'line') return [{ chars: n.chars }];
  return n.chars.map((c) => ({ chars: [c], one: c }));
}

function renderText(n, lt, t) {
  const units = unitsOf(n);
  const cnt = units.length;
  const inF = n.inFx, outF = n.outFx;
  let lastVisible = -1;
  const fs = n.fontSize;
  units.forEach((u, i) => {
    const m = mods();
    const c = { u: fs, r: u.one ? u.one.r : u.chars[0].r };
    if (inF && n.split !== 'none') {
      const dflSt = n.split === 'word' ? 0.09 : inF.fx === 'typewriter' ? 1 / 30 : 0.035;
      const q = unitProgress(inF, lt, i, cnt, dflSt);
      if (inF.from) addMods(m, fromTo(inF.from, easeFn(inF.ease, 'outExpo')(q)));
      else { const F = IN_FX[inF.fx]; addMods(m, F.f(easeFn(inF.ease, F.ease)(q), c, lt + i * 0.013)); }
      if (inF.fx === 'typewriter' && q > 0) lastVisible = i;
    }
    if (outF && n.split !== 'none') {
      const q = unitProgress(outF, lt, i, cnt, n.split === 'word' ? 0.05 : 0.025);
      if (q > 0) {
        if (outF.to) addMods(m, fromTo(outF.to, 1 - easeFn(outF.ease, 'inExpo')(q)));
        else { const F = OUT_FX[outF.fx]; addMods(m, F.f(easeFn(outF.ease, F.ease)(q), c, lt)); }
      }
    }
    for (const ch of u.chars) {
      const el = ch.el;
      const yy = m.yem ? `${(m.y * 100).toFixed(2)}%` : `${m.y.toFixed(2)}px`;
      let tr = `translate3d(${m.x.toFixed(2)}px,${yy},${m.z.toFixed(1)}px)`;
      if (m.rx) tr += ` rotateX(${m.rx.toFixed(2)}deg)`;
      if (m.ry) tr += ` rotateY(${m.ry.toFixed(2)}deg)`;
      if (m.r) tr += ` rotate(${m.r.toFixed(2)}deg)`;
      if (m.s !== 1 || m.sx !== 1 || m.sy !== 1) tr += ` scale(${(m.s * m.sx).toFixed(4)},${(m.s * m.sy).toFixed(4)})`;
      setStyle(el, 'transform', tr);
      setStyle(el, 'opacity', (m.vis === 0 ? 0 : clamp(m.o, 0, 1)).toFixed(3));
      setStyle(el, 'filter', m.b > 0.05 || m.rgb > 0.3 ? filterOf({ blur: 0, bright: 1, glow: 0 }, m, '') : 'none');
      if (inF && inF.fx === 'scramble') {
        const g = m.scr && ch.ch !== ' ' ? SCRAMBLE[(rng(Math.floor(lt * 24) * 131 + ch.bx * 7)() * SCRAMBLE.length) | 0] : ch.ch;
        if (el.textContent !== g) el.textContent = g;
      }
    }
  });
  // The strike lines draw on their own act, or right after the entrance.
  for (const s of n.strikes) {
    let v = 0;
    const acts = (n.strikeActs || []).filter((a) => a.index === undefined || a.index === s.run);
    for (const a of acts) v = Math.max(v, clamp((lt - a.at) / (a.dur || 0.4), 0, 1));
    setStyle(s.el, 'transform', `scaleX(${EASE.inOutCubic(v).toFixed(4)})`);
  }
  if (n.caret) {
    const typing = inF && inF.fx === 'typewriter';
    let x = 0, y = 0, h = fs;
    const ref = lastVisible >= 0 ? n.chars[Math.min(lastVisible, n.chars.length - 1)] : null;
    if (ref) { x = ref.bx + ref.bw; y = ref.by; h = ref.bh; }
    else if (n.chars.length) { const c0 = n.chars[0]; x = c0.bx; y = c0.by; h = c0.bh; }
    const doneAt = inF ? inF.at + (inF.stagger !== undefined ? inF.stagger : 1 / 30) * cnt : 0;
    const blinkOn = lt < doneAt || Math.floor((lt - doneAt) * 2.2) % 2 === 0;
    const visible = typing ? (lt >= inF.at && lt < doneAt + (n.spec.caret_hold !== undefined ? n.spec.caret_hold : 1.2) && blinkOn) : blinkOn;
    setStyle(n.caret, 'left', x.toFixed(1) + 'px');
    setStyle(n.caret, 'top', (y + h * 0.12).toFixed(1) + 'px');
    setStyle(n.caret, 'height', (h * 0.78).toFixed(1) + 'px');
    setStyle(n.caret, 'opacity', visible ? '1' : '0');
  }
}

function renderState(n, lt) {
  if (n.toggles) n.toggles.forEach((tg, i) => {
    if (!tg) return;
    const v = foldState(n.togglesActs, i, tg.on, lt, 0.28);
    setStyle(tg.knob, 'transform', `translateX(${(v * 18).toFixed(2)}px)`);
    setStyle(tg.track, 'background', mixColor(theme.track, theme.accent, v));
  });
  if (n.checks) n.checks.forEach((ck, i) => {
    if (!ck) return;
    const v = foldState(n.checksActs, i, ck.on, lt, 0.35);
    setStyle(ck.box, 'background', v > 0.01 ? alpha(theme.ok, clamp(v * 1.6, 0, 1)) : 'transparent');
    setStyle(ck.box, 'borderColor', mixColor(theme.borderStrong, theme.ok, v));
    const path = ck.box.querySelector('path');
    if (path) { path.setAttribute('pathLength', '1'); setStyle(path, 'strokeDasharray', '1 1'); setStyle(path, 'strokeDashoffset', (1 - clamp(v * 1.3 - 0.3, 0, 1)).toFixed(3)); }
    setStyle(ck.box, 'transform', `scale(${(1 + 0.18 * Math.sin(Math.PI * clamp(v, 0, 1))).toFixed(3)})`);
  });
  if (n.hl) {
    let idx = n.selected;
    let y = idx >= 0 && n.rows[idx] ? n.rows[idx].offsetTop : 0;
    let o = idx >= 0 ? 1 : 0;
    for (const a of n.selectActs || []) {
      if (lt < a.at) break;
      const row = n.rows[a.index || 0];
      if (!row) continue;
      const u = EASE.outExpo(clamp((lt - a.at) / (a.dur || 0.3), 0, 1));
      y = o > 0 ? lerp(y, row.offsetTop, u) : row.offsetTop;
      o = o > 0 ? 1 : u;
    }
    const h = n.rows[0] ? n.rows[0].offsetHeight : 36;
    setStyle(n.hl, 'top', y.toFixed(1) + 'px');
    setStyle(n.hl, 'height', h + 'px');
    setStyle(n.hl, 'opacity', o.toFixed(3));
  }
  if (n.numEl) {
    let v = n.count;
    for (const a of n.countActs || []) {
      if (lt < a.at) break;
      const from = a.from !== undefined ? a.from : v;
      const u = easeFn(a.ease, 'outExpo')(clamp((lt - a.at) / (a.dur || 1.2), 0, 1));
      v = lerp(from, a.to, u);
    }
    const dec = n.spec.decimals || 0;
    const s = v.toLocaleString('en-US', { minimumFractionDigits: dec, maximumFractionDigits: dec });
    if (n.numEl.textContent !== s) n.numEl.textContent = s;
  }
  if (n.progEl) {
    let v = n.prog;
    for (const a of n.progActs || []) {
      if (lt < a.at) break;
      v = lerp(v, a.to !== undefined ? a.to : 1, EASE.inOutCubic(clamp((lt - a.at) / (a.dur || 1), 0, 1)));
    }
    setStyle(n.progEl, 'width', (clamp(v, 0, 1) * 100).toFixed(2) + '%');
  }
  if (n.okEl) {
    const acts = n.checksActs || [];
    const v = foldState(acts, 0, 0, lt, 0.3);
    setStyle(n.okEl, 'opacity', v.toFixed(3));
    setStyle(n.okEl, 'transform', `scale(${(0.5 + 0.5 * EASE.outBack(v)).toFixed(3)})`);
  }
}

// Trails: where a layer was over the last `len` seconds, drawn as a fading ribbon.
const trailCanvases = new Map();
function trailCanvas(host, big) {
  let c = trailCanvases.get(host);
  if (c) return c;
  const cv = mk('canvas', 'kv-canvas');
  const w = big ? W * 2 : W, h = big ? H * 2 : H;
  cv.width = w; cv.height = h;
  cv.style.left = (big ? -W : 0) + 'px'; cv.style.top = (big ? -H : 0) + 'px';
  cv.style.mixBlendMode = 'screen';
  host.insertBefore(cv, host.firstChild);
  c = { cv, cx: cv.getContext('2d'), ox: big ? W : 0, oy: big ? H : 0, drawn: false };
  trailCanvases.set(host, c);
  return c;
}
function drawTrails(t) {
  for (const c of trailCanvases.values()) { if (c.drawn) c.cx.clearRect(0, 0, c.cv.width, c.cv.height); c.drawn = false; }
  for (const n of nodes) {
    const tr = n.trail || (n.parent && n.parent.trail && n.parent.orbit ? n.parent.trail : null);
    if (!tr || n.hidden) continue;
    const host = n.el.parentNode;
    const C = trailCanvas(host, !!n.parent && n.parent.isGroup);
    const lt = n.local(t);
    const steps = 22;
    const pts = [];
    for (let k = 0; k <= steps; k++) {
      const tt = lt - (tr.len * k) / steps;
      const { p, m } = evalNode(n, tt, t - (tr.len * k) / steps);
      pts.push([p.x + m.x + C.ox, p.y + m.y + C.oy, clamp(p.opacity * m.o, 0, 1)]);
    }
    const col = parseColor(tr.color || n.spec.glow_color || theme.accent) || [255, 255, 255, 1];
    const cx = C.cx;
    cx.lineCap = 'round';
    for (let k = 0; k < steps; k++) {
      const a = (1 - k / steps) * pts[k][2] * (tr.opacity || 0.9);
      if (a <= 0.01) continue;
      cx.strokeStyle = rgba(col, a);
      cx.lineWidth = tr.width * (1 - k / steps) + 0.5;
      cx.beginPath(); cx.moveTo(pts[k][0], pts[k][1]); cx.lineTo(pts[k + 1][0], pts[k + 1][1]); cx.stroke();
    }
    C.drawn = true;
  }
}

function sceneVisibility(s, t) {
  const din = s.tin ? s.tin.dur : 0;
  const dout = s.tout ? s.tout.dur : 0;
  const a = s.start - din / 2, b = s.end + dout / 2;
  return t >= a && t < b;
}

function renderScene(s, t) {
  const vis = sceneVisibility(s, t);
  setStyle(s.el, 'display', vis ? 'block' : 'none');
  s.visible = vis;
  if (!vis) return;
  const m = mods();
  if (s.tin && s.tin.dur > 0 && t < s.start + s.tin.dur / 2) {
    const u = clamp((t - (s.start - s.tin.dur / 2)) / s.tin.dur, 0, 1);
    addMods(m, TRANSITIONS[s.tin.kind](u, 'in'));
  }
  if (s.tout && s.tout.dur > 0 && t > s.end - s.tout.dur / 2) {
    const u = clamp((t - (s.end - s.tout.dur / 2)) / s.tout.dur, 0, 1);
    addMods(m, TRANSITIONS[s.tout.kind](u, 'out'));
  }
  if (s.tin && s.tin.dur === 0 && t < s.start) setStyle(s.el, 'display', 'none');
  let tr = `translate(${m.x.toFixed(2)}px,${m.y.toFixed(2)}px)`;
  if (m.r) tr += ` rotate(${m.r.toFixed(2)}deg)`;
  if (m.s !== 1) tr += ` scale(${m.s.toFixed(4)})`;
  setStyle(s.el, 'transform', tr);
  setStyle(s.el, 'opacity', clamp(m.o, 0, 1).toFixed(3));
  const f = [];
  if (m.b > 0.05) f.push(`blur(${m.b.toFixed(2)}px)`);
  if (m.hb > 0.3) {
    if (!s.hblur) s.hblur = hblurFilter(s.hblurId);
    s.hblur.setAttribute('stdDeviation', `${m.hb.toFixed(1)} 0`);
    f.push(`url(#${s.hblurId})`);
  }
  if (m.rgb > 0.3) f.push(`drop-shadow(${m.rgb.toFixed(1)}px 0 0 rgba(255,0,90,.7)) drop-shadow(${(-m.rgb).toFixed(1)}px 0 0 rgba(0,220,255,.7))`);
  setStyle(s.el, 'filter', f.length ? f.join(' ') : 'none');
  setStyle(s.el, 'clipPath', m.clip || 'none');
  // The scene's own camera: a slow push by default, plus any keys.
  const lt = t - s.start;
  const dur = s.end - s.start;
  const push = s.spec.push !== undefined ? s.spec.push : 0.04;
  const k = s.keys;
  const zoom = (k.zoom ? trackValue(k.zoom, lt, 1) : 1) * (1 + push * clamp((t - (s.start - (s.tin ? s.tin.dur / 2 : 0))) / (dur + (s.tin ? s.tin.dur / 2 : 0) + (s.tout ? s.tout.dur / 2 : 0)), 0, 1.2));
  const cx = k.x ? trackValue(k.x, lt, 0) : 0, cy = k.y ? trackValue(k.y, lt, 0) : 0;
  const cr = k.rotate ? trackValue(k.rotate, lt, 0) : 0;
  const crx = k.rx ? trackValue(k.rx, lt, 0) : 0, cry = k.ry ? trackValue(k.ry, lt, 0) : 0;
  const drift = s.spec.drift || [0, 0];
  let ct = `scale(${zoom.toFixed(4)}) translate(${(-cx + drift[0] * lt).toFixed(2)}px,${(-cy + drift[1] * lt).toFixed(2)}px)`;
  if (cr) ct = `rotate(${cr.toFixed(2)}deg) ` + ct;
  if (crx || cry) ct = `perspective(${PERSPECTIVE}px) rotateX(${crx.toFixed(2)}deg) rotateY(${cry.toFixed(2)}deg) ` + ct;
  setStyle(s.cam, 'transform', ct);
}

function renderNode(n, t) {
  const lt = n.local(t);
  if (n.scene && !n.scene.visible) { n.hidden = true; return; }
  const [a, b] = lifetime(n);
  if (lt < a || lt > b) { setStyle(n.el, 'display', 'none'); n.hidden = true; return; }
  if (n.parent && n.parent.hidden) { n.hidden = true; return; }
  setStyle(n.el, 'display', 'block');
  n.hidden = false;
  const { p, m } = evalNode(n, lt, t);
  const flow = false;
  setStyle(n.el, 'transform', transformOf(p, m, n.ax, n.ay, flow));
  setStyle(n.el, 'opacity', (m.vis === 0 ? 0 : clamp(p.opacity * m.o, 0, 1)).toFixed(3));
  setStyle(n.el, 'filter', filterOf(p, m, n.spec.glow_color || theme.accent));
  setStyle(n.el, 'clipPath', m.clip || 'none');
  if (n.parent && n.parent.orbit) setStyle(n.el, 'zIndex', String(orbitOffset(n.parent, n, lt, t).zi));
  if (p.w !== undefined && n.keys.w) setStyle(n.el, 'width', p.w.toFixed(1) + 'px');
  if (p.h !== undefined && n.keys.h) setStyle(n.el, 'height', p.h.toFixed(1) + 'px');
  if (n.keys.color) setStyle(n.inner, 'color', p.color);
  if (n.keys.fill && n.fillProp) setStyle(n.el, n.fillProp, p.fill);
  if (n.drawables) {
    const d = clamp(p.draw * m.draw, 0, 1);
    for (const e of n.drawables) setStyle(e, 'strokeDashoffset', (1 - d).toFixed(4));
  }
  if (n.shineEl) {
    const sh = m.shine !== null ? m.shine : -1;
    n.shineEl.style.setProperty('--sx', (sh < 0 ? -60 : -60 + sh * 220).toFixed(1) + '%');
  }
  if (n.chars) renderText(n, lt, t);
  for (const a2 of n.acts) if (a2.dom) a2.dom(lt);
  renderState(n, lt);
  if (n.drawParticles) n.drawParticles(lt);
  for (const s of n.subs) {
    const mm = mods();
    const c = { u: s.u, r: s.r };
    const pp = { x: 0, y: 0, z: 0, scale: 1, sx: 1, sy: 1, rotate: 0, rx: 0, ry: 0, opacity: 1, blur: 0, glow: 0, bright: 1 };
    for (const k in s.keys) pp[k] = trackValue(s.keys[k], lt, pp[k] !== undefined ? pp[k] : 0);
    if (s.fin) addMods(mm, s.fin(lt, t, c));
    if (s.fout) addMods(mm, s.fout(lt, t, c));
    for (const L of s.loops) if (!(L.at !== undefined && lt < L.at)) addMods(mm, LOOP_FX[L.fx](lt - (L.at || 0), t, c, L));
    setStyle(s.el, 'transform', transformOf(pp, mm, 0, 0, true));
    setStyle(s.el, 'opacity', (mm.vis === 0 ? 0 : clamp(pp.opacity * mm.o, 0, 1)).toFixed(3));
    setStyle(s.el, 'filter', filterOf(pp, mm, theme.accent));
    if (mm.clip) setStyle(s.el, 'clipPath', mm.clip);
  }
}

function renderWorld(t) {
  const k = globalCam;
  let zoom = k.zoom ? trackValue(k.zoom, t, 1) : 1;
  let x = k.x ? trackValue(k.x, t, 0) : 0, y = k.y ? trackValue(k.y, t, 0) : 0;
  let r = k.rotate ? trackValue(k.rotate, t, 0) : 0;
  const pulse = S.video.pulse !== undefined ? S.video.pulse : 0.012;
  if (pulse) zoom *= 1 + pulse * beatEnv(t, BEAT, 9) * energyAt(t);
  for (const sh of S.shakes || []) {
    const base = sh.scene && sceneById[sh.scene] ? sceneById[sh.scene].start : 0;
    const e = t - base - sh.at;
    const d = sh.dur || 0.5;
    if (e < 0 || e > d) continue;
    const amp = (sh.amp || 14) * Math.pow(1 - e / d, 2);
    const f = sh.freq || 22;
    x += noise(e * f, 3) * amp; y += noise(e * f, 7) * amp; r += noise(e * f, 11) * amp * 0.05;
  }
  setStyle(world, 'transform', `scale(${zoom.toFixed(4)}) rotate(${r.toFixed(3)}deg) translate(${(-x).toFixed(2)}px,${(-y).toFixed(2)}px)`);
  // Flashes.
  let fo = 0, fc = '#ffffff';
  for (const f of S.flashes || []) {
    const base = f.scene && sceneById[f.scene] ? sceneById[f.scene].start : 0;
    const e = t - base - f.at;
    const d = f.dur || 0.35;
    if (e < -0.04 || e > d) continue;
    const v = e < 0 ? (e + 0.04) / 0.04 : Math.pow(1 - e / d, 2);
    if (v * (f.peak || 1) > fo) { fo = v * (f.peak || 1); fc = f.color || '#ffffff'; }
  }
  for (const s of scenes) {
    if (s.tin && s.tin.kind === 'flash' && s.tin.dur > 0) {
      const u = (t - (s.start - s.tin.dur / 2)) / s.tin.dur;
      if (u >= 0 && u <= 1) fo = Math.max(fo, Math.pow(1 - Math.abs(u - 0.5) * 2, 1.5));
    }
  }
  setStyle(flashEl, 'opacity', fo.toFixed(3));
  setStyle(flashEl, 'background', fc);
  if (grainTiles.length) {
    const fr = Math.floor(t * FPS);
    setStyle(grainEl, 'backgroundImage', grainTiles[fr % grainTiles.length]);
    setStyle(grainEl, 'backgroundPosition', `${(fr * 37) % 192}px ${(fr * 71) % 192}px`);
  }
}

function seek(t) {
  try {
    if (bgState.draw) bgState.draw(t);
    renderWorld(t);
    for (const s of scenes) renderScene(s, t);
    for (const n of nodes) renderNode(n, t);
    if (trailCanvases.size || nodes.some((n) => n.trail || (n.parent && n.parent.trail))) drawTrails(t);
    // CSS animations in an html layer run on this clock, not the browser's, so they
    // come out the same in every frame of every render.
    if (document.getAnimations) {
      for (const an of document.getAnimations()) {
        an.pause();
        an.currentTime = t * 1000;
      }
    }
    return '';
  } catch (e) {
    return String(e && e.stack ? e.stack : e);
  }
}

// ---------------------------------------------------------------- ready

const ready = (async () => {
  await document.fonts.ready;
  await Promise.all([...document.images].map((i) => (i.decode ? i.decode().catch(() => 0) : 0)));
  // Measure with every layer on and untransformed, then hand over to seek.
  measure();
  seek(0);
  await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
})();

window.KV = {
  ready,
  seek,
  duration: S.video.duration,
  info: () => ({ warnings, layers: nodes.length, scenes: scenes.length }),
};

// ---------------------------------------------------------------- preview player

if (S.preview) {
  const fit = () => {
    const k = Math.min(window.innerWidth / W, (window.innerHeight - 44) / H);
    root.style.transform = `translate(${((window.innerWidth - W * k) / 2).toFixed(1)}px,0) scale(${k})`;
  };
  window.addEventListener('resize', fit);
  fit();
  const bar = mk('div', '', document.body); bar.id = 'kv-player';
  bar.innerHTML = '<button>Play</button><input type="range" min="0" step="0.001"><span>0.00s</span>';
  const btn = bar.querySelector('button'), rg = bar.querySelector('input'), lab = bar.querySelector('span');
  rg.max = S.video.duration;
  const audio = S.audio ? new Audio(S.audio) : null;
  let playing = false, t0 = 0, wall = 0, cur = 0;
  const show = (t) => { cur = t; seek(t); rg.value = t; lab.textContent = t.toFixed(2) + 's'; };
  const loop = () => {
    if (!playing) return;
    const t = audio ? audio.currentTime : t0 + (performance.now() - wall) / 1000;
    if (t >= S.video.duration) { playing = false; btn.textContent = 'Play'; if (audio) audio.pause(); show(S.video.duration); return; }
    show(t);
    requestAnimationFrame(loop);
  };
  const play = () => {
    if (playing) { playing = false; btn.textContent = 'Play'; if (audio) audio.pause(); return; }
    if (cur >= S.video.duration - 0.01) cur = 0;
    playing = true; btn.textContent = 'Pause'; t0 = cur; wall = performance.now();
    if (audio) { audio.currentTime = cur; audio.play(); }
    requestAnimationFrame(loop);
  };
  btn.onclick = play;
  window.addEventListener('keydown', (e) => { if (e.code === 'Space') { e.preventDefault(); play(); } });
  rg.oninput = () => { if (audio) audio.currentTime = +rg.value; t0 = +rg.value; wall = performance.now(); show(+rg.value); };
  ready.then(() => show(0));
}
})();
