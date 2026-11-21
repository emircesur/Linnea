// How a layer moves: keyframes, a motion path, a parent. Mirrors linnea2d.js exactly.
import type { Json } from "./api";

export type Key = { t: number; x?: number; y?: number; w?: number; h?: number; rot?: number; scale?: number; opacity?: number; ease?: string };
export type Item = { id: string; kind: string; t0: number; t1: number; x: number; y: number; w: number; h: number; keys?: Key[]; parent?: string; hidden?: boolean; [k: string]: any };

const PROPS = ["x", "y", "w", "h", "rot", "scale", "opacity"] as const;
export const EASE: Record<string, (k: number) => number> = {
  linear: (k) => k,
  out: (k) => 1 - Math.pow(1 - k, 3),
  in: (k) => k * k * k,
  inOut: (k) => (k < 0.5 ? 4 * k * k * k : 1 - Math.pow(-2 * k + 2, 3) / 2),
  back: (k) => { const c = 1.7; return 1 + (c + 1) * Math.pow(k - 1, 3) + c * Math.pow(k - 1, 2); },
};

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
        const k = (EASE[b.ease || "inOut"] || EASE.inOut)((t - a.t) / Math.max(1e-6, b.t - a.t));
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
      const ox = s.x + s.w / 2 - bcx, oy = s.y + s.h / 2 - bcy;
      s.x = now.x + now.w / 2 + ox * Math.cos(dr) - oy * Math.sin(dr) - s.w / 2;
      s.y = now.y + now.h / 2 + ox * Math.sin(dr) + oy * Math.cos(dr) - s.h / 2;
      s.rot += now.rot - (par.rot || 0);
    }
  }
  return s;
}

