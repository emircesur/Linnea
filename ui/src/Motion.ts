// How a layer moves: keyframes, a motion path, a parent. Mirrors linnea2d.js exactly.
import type { Json } from "./api";

export type Key = { t: number; x?: number; y?: number; w?: number; h?: number; rot?: number; scale?: number; opacity?: number; ease?: string; bez?: number[]; [k: string]: any };
export type Item = { id: string; kind: string; t0: number; t1: number; x: number; y: number; w: number; h: number; keys?: Key[]; parent?: string; hidden?: boolean; [k: string]: any };

const PROPS = ["x", "y", "w", "h", "rot", "scale", "opacity"] as const;
export const EASE: Record<string, (k: number) => number> = {
  linear: (k) => k,
  out: (k) => 1 - Math.pow(1 - k, 3),
  in: (k) => k * k * k,
  inOut: (k) => (k < 0.5 ? 4 * k * k * k : 1 - Math.pow(-2 * k + 2, 3) / 2),
  back: (k) => { const c = 1.7; return 1 + (c + 1) * Math.pow(k - 1, 3) + c * Math.pow(k - 1, 2); },
  sine: (k) => -(Math.cos(Math.PI * k) - 1) / 2,
  expo: (k) => (k >= 1 ? 1 : 1 - Math.pow(2, -10 * k)),
  elastic: (k) => (k <= 0 ? 0 : k >= 1 ? 1 : Math.pow(2, -10 * k) * Math.sin(((k * 10 - 0.75) * (2 * Math.PI)) / 3) + 1),
  bounce: (k) => { const n = 7.5625, d = 2.75; if (k < 1 / d) return n * k * k; if (k < 2 / d) return n * (k -= 1.5 / d) * k + 0.75; if (k < 2.5 / d) return n * (k -= 2.25 / d) * k + 0.9375; return n * (k -= 2.625 / d) * k + 0.984375; },
  hold: (k) => (k >= 1 ? 1 : 0),
};

/** A cubic Bezier ease (x1, y1, x2, y2), as CSS cubic-bezier(). */
export function bezier([x1, y1, x2, y2]: number[]) {
  const A = (a: number, c: number) => 1 - 3 * c + 3 * a, B = (a: number, c: number) => 3 * c - 6 * a, C = (a: number) => 3 * a;
  const X = (u: number) => ((A(x1, x2) * u + B(x1, x2)) * u + C(x1)) * u, Y = (u: number) => ((A(y1, y2) * u + B(y1, y2)) * u + C(y1)) * u, D = (u: number) => (3 * A(x1, x2) * u + 2 * B(x1, x2)) * u + C(x1);
  return (x: number) => {
    if (x <= 0) return 0;
    if (x >= 1) return 1;
    let u = x;
    for (let i = 0; i < 8; i++) { const e = X(u) - x; if (Math.abs(e) < 1e-6) break; const d = D(u); if (Math.abs(d) < 1e-6) break; u -= e / d; }
    if (!(u >= 0 && u <= 1) || Math.abs(X(u) - x) > 1e-4) { let lo = 0, hi = 1; u = x; for (let i = 0; i < 40; i++) { const v = X(u); if (Math.abs(v - x) < 1e-6) break; if (v < x) lo = u; else hi = u; u = (lo + hi) / 2; } }
    return Y(u);
  };
}
/** The curve into key b: its own Bezier, or its named ease. */
export const curve = (b: Json) => (Array.isArray(b.bez) && b.bez.length === 4 ? bezier(b.bez) : EASE[b.ease || "inOut"] || EASE.inOut);

// style values keys can animate besides the place
export const STYLE_NUM = ["size", "blur", "shadowBlur", "shadowX", "shadowY", "strokeWidth", "borderWidth", "radius", "spacing", "value", "gradAngle", "lineHeight"];
export const COLORS = ["color", "bg", "bg2", "stroke", "shadow", "border", "accent", "subColor"];
function rgba(c: any): number[] | null {
  if (typeof c !== "string") return null;
  c = c.trim();
  if (c[0] === "#") {
    let h = c.slice(1);
    if (h.length === 3 || h.length === 4) h = h.split("").map((x: string) => x + x).join("");
    const n = parseInt(h, 16);
    if (isNaN(n)) return null;
    if (h.length === 6) return [(n >> 16) & 255, (n >> 8) & 255, n & 255, 1];
    if (h.length === 8) return [(n >>> 24) & 255, (n >> 16) & 255, (n >> 8) & 255, (n & 255) / 255];
    return null;
  }
  const m = c.match(/^rgba?\(([^)]+)\)$/i);
  if (m) { const p = m[1].split(/[\s,/]+/).filter(Boolean).map(parseFloat); return p.length >= 3 ? [p[0], p[1], p[2], p[3] ?? 1] : null; }
  return null;
}
export function mixColor(a: any, b: any, k: number) {
  const A = rgba(a), B = rgba(b);
  if (!A || !B) return k < 1 ? a : b;
  const v = (i: number) => A[i] + (B[i] - A[i]) * k;
  return `rgba(${Math.round(v(0))},${Math.round(v(1))},${Math.round(v(2))},${Math.round(v(3) * 1000) / 1000})`;
}
/** A colour as #rrggbb (for colour pickers), when it can be read. */
export function hexOf(c: any): string | null {
  const v = rgba(c);
  return v ? "#" + v.slice(0, 3).map((x) => Math.round(x).toString(16).padStart(2, "0")).join("") : null;
}
function keyed(ks: Json[], p: string, t: number, color: boolean) {
  const kk = ks.filter((k) => k[p] !== undefined);
  if (!kk.length) return undefined;
  if (t <= kk[0].t) return kk[0][p];
  if (t >= kk[kk.length - 1].t) return kk[kk.length - 1][p];
  for (let i = 0; i < kk.length - 1; i++) {
    const a = kk[i], b = kk[i + 1];
    if (t >= a.t && t <= b.t) {
      const k = curve(b)((t - a.t) / Math.max(1e-6, b.t - a.t));
      return color ? mixColor(a[p], b[p], k) : typeof a[p] === "number" && typeof b[p] === "number" ? a[p] + (b[p] - a[p]) * k : k < 1 ? a[p] : b[p];
    }
  }
}
/** The layer's keyed style values at t (as linnea2d.js draws them). */
export function styled(it: Item, t: number): Item {
  if (!it.keys?.length) return it;
  const ks = it.keys.slice().sort((a, b) => a.t - b.t);
  let out: Item | null = null;
  for (const p of STYLE_NUM) { const v = keyed(ks, p, t, false); if (v !== undefined) (out = out || { ...it })[p] = v; }
  for (const p of COLORS) { const v = keyed(ks, p, t, true); if (v !== undefined) (out = out || { ...it })[p] = v; }
  return out || it;
}

/** A point k (0..1) of the way along a polyline, by length, and the heading there (as linnea2d.js). */
function alongPath(P: number[][], k: number) {
  const L: number[] = [];
  let tot = 0;
  for (let i = 1; i < P.length; i++) { const d = Math.hypot(P[i][0] - P[i - 1][0], P[i][1] - P[i - 1][1]); L.push(d); tot += d; }
  let want = k * tot, i = 0;
  while (i < L.length - 1 && want > L[i]) { want -= L[i]; i++; }
  const a = P[i], b = P[i + 1], f = L[i] > 0 ? Math.max(0, Math.min(1, want / L[i])) : 0;
  return { x: a[0] + (b[0] - a[0]) * f, y: a[1] + (b[1] - a[1]) * f, a: Math.atan2(b[1] - a[1], b[0] - a[0]) };
}

export function evalItem(items: Item[], it: Item, t: number, depth = 0): Json {
  const s: Json = { x: it.x || 0, y: it.y || 0, w: it.w || 200, h: it.h || 100, rot: it.rot || 0, scale: it.scale ?? 1, opacity: it.opacity ?? 1 };
  const keys = (it.keys || []).slice().sort((a, b) => a.t - b.t);
  for (const p of PROPS) {
    const ks = keys.filter((k) => (k as Json)[p] !== undefined);
    if (!ks.length) continue;
    if (t <= ks[0].t) { s[p] = (ks[0] as Json)[p]; continue; }
    if (t >= ks[ks.length - 1].t) { s[p] = (ks[ks.length - 1] as Json)[p]; continue; }
    for (let i = 0; i < ks.length - 1; i++) {
      const a = ks[i] as Json, b = ks[i + 1] as Json;
      if (t >= a.t && t <= b.t) {
        const k = curve(b)((t - a.t) / Math.max(1e-6, b.t - a.t));
        s[p] = a[p] + (b[p] - a[p]) * k;
        break;
      }
    }
  }
  if (Array.isArray(it.path) && it.path.length >= 2) {
    const span = Math.max(0.01, (it.t1 ?? t) - (it.t0 ?? 0));
    const pt = alongPath(it.path, (EASE[it.pathEase || "inOut"] || EASE.inOut)(Math.max(0, Math.min(1, (t - (it.t0 || 0)) / span))));
    s.x = pt.x - s.w / 2;
    s.y = pt.y - s.h / 2;
    if (it.pathTurn) s.rot += (pt.a * 180) / Math.PI;
  }
  if (it.parent && depth < 8) {
    const par = items.find((p) => p.id === it.parent);
    if (par) {
      const now = evalItem(items, par, t, depth + 1);
      const bcx = (par.x || 0) + (par.w || 200) / 2, bcy = (par.y || 0) + (par.h || 100) / 2;
      const dr = ((now.rot - (par.rot || 0)) * Math.PI) / 180;
      // a group also scales and fades what is in it
      const grp = par.kind === "group", ds = grp ? now.scale / (par.scale ?? 1) : 1;
      if (grp) { s.scale *= ds; s.opacity *= now.opacity; }
      const ox = (s.x + s.w / 2 - bcx) * ds, oy = (s.y + s.h / 2 - bcy) * ds;
      s.x = now.x + now.w / 2 + ox * Math.cos(dr) - oy * Math.sin(dr) - s.w / 2;
      s.y = now.y + now.h / 2 + ox * Math.sin(dr) + oy * Math.cos(dr) - s.h / 2;
      s.rot += now.rot - (par.rot || 0);
    }
  }
  return s;
}


/** Whether a layer is on screen at t: its own time, and every group it sits in. */
export function shownAt(items: Item[], it: Item, t: number, depth = 0): boolean {
  if (t < it.t0 || t >= it.t1 || it.hidden) return false;
  if (it.parent && depth < 8) { const par = items.find((p) => p.id === it.parent); if (par?.kind === "group") return shownAt(items, par, t, depth + 1); }
  return true;
}
/** The group a layer is in, outermost first (empty when it is in none). */
export function groupsOf(items: Item[], it: Item): Item[] {
  const out: Item[] = [];
  let p = it.parent ? items.find((x) => x.id === it.parent) : undefined;
  for (let i = 0; p && p.kind === "group" && i < 8; i++) { out.unshift(p); p = p.parent ? items.find((x) => x.id === p!.parent) : undefined; }
  return out;
}
