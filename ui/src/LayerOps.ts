// What can be done to layers, as plain functions over the item list: duplicate, split, trim, order, align,
// distribute, keyframes per property, motion presets, text looks, a clipboard that works across films.
import { tx } from "./i18n";
import { COLORS, evalItem, groupsOf, Item, Key, STYLE_NUM, styled } from "./Motion";
import { Json } from "./api";

export const r2 = (x: number) => Math.round(x * 100) / 100;
export const PROPS = ["x", "y", "w", "h", "rot", "scale", "opacity"] as const;
export type Prop = (typeof PROPS)[number];
/** Everything a key can hold: the place, and style numbers and colours. */
export const KEYABLE: string[] = [...PROPS, ...STYLE_NUM, ...COLORS];
export const uid = (p = "l") => p + Date.now().toString(36) + Math.random().toString(36).slice(2, 5);
const NEAR = 0.02;

/** A layer's name for lists and the timeline. */
export function layerName(it: Item): string {
  return it.label || (it.text ? String(it.text).split("\n")[0] : "") || it.title || it.puppet || kindLabel(it.kind);
}
export function kindLabel(k: string): string {
  return ({ group: tx("Group"), video: tx("Video"), text: tx("Text"), image: tx("Picture"), rect: tx("Box"), ellipse: tx("Circle"), counter: tx("Counter"), marker: tx("Handwriting"), arrow: tx("Arrow"), phone: tx("Phone"), lower: tx("Lower third"), progress: tx("Progress"), puppet: tx("Puppet") } as Json)[k] || k;
}

// ---------- the list ----------
export const replace = (items: Item[], ids: string[], f: (it: Item) => Item) => items.map((x) => (ids.includes(x.id) ? f(x) : x));

/** The layers inside groups among ids (all the way down), after the ids themselves. */
export function withInside(items: Item[], ids: string[]): string[] {
  const out = [...ids];
  for (let i = 0; i < out.length; i++) for (const x of items) if (x.parent === out[i] && !out.includes(x.id) && items.find((g) => g.id === out[i])?.kind === "group") out.push(x.id);
  return out;
}

export function duplicate(items: Item[], ids: string[], dx = 24, dy = 24): { items: Item[]; ids: string[] } {
  // a group comes with what is in it; the copies inside follow the copied group
  const all = withInside(items, ids);
  const map = new Map(all.map((id) => [id, uid(items.find((x) => x.id === id)?.kind === "puppet" ? "p" : "l")]));
  const out = [...items];
  const made: string[] = [];
  for (const id of all) {
    const i = out.findIndex((x) => x.id === id);
    if (i < 0) continue;
    const src = out[i];
    const c: Item = JSON.parse(JSON.stringify(src));
    c.id = map.get(id)!;
    const inner = !!c.parent && map.has(c.parent);
    if (inner) c.parent = map.get(c.parent!);
    const ddx = inner ? 0 : dx, ddy = inner ? 0 : dy;
    c.x = (c.x || 0) + ddx; c.y = (c.y || 0) + ddy;
    if (c.keys) c.keys = c.keys.map((k: Key) => ({ ...k, ...(k.x !== undefined ? { x: k.x + ddx } : {}), ...(k.y !== undefined ? { y: k.y + ddy } : {}) }));
    if (c.path) c.path = c.path.map((p: number[]) => [p[0] + ddx, p[1] + ddy]);
    if (c.label && !inner) c.label = c.label + " " + tx("copy");
    out.splice(i + 1, 0, c);
    if (ids.includes(id)) made.push(c.id);
  }
  return { items: out, ids: made };
}

export function remove(items: Item[], ids: string[]): Item[] {
  ids = withInside(items, ids);
  return items.filter((x) => !ids.includes(x.id)).map((x) => (x.parent && ids.includes(x.parent) ? { ...x, parent: undefined } : x));
}

/** Cut a layer in two at t: the left keeps the keys before t, the right gets those after. */
export function split(items: Item[], id: string, t: number): { items: Item[]; id?: string } {
  const i = items.findIndex((x) => x.id === id);
  const it = items[i];
  if (!it || t <= it.t0 + 0.05 || t >= it.t1 - 0.05) return { items };
  const s = evalItem(items, it, t);
  const here: Key = { t: r2(t), x: Math.round(s.x), y: Math.round(s.y), w: Math.round(s.w), h: Math.round(s.h), rot: r2(s.rot), scale: r2(s.scale), opacity: r2(s.opacity) };
  const ks = it.keys || [];
  const a: Item = { ...it, t1: r2(t), keys: ks.length ? [...ks.filter((k) => k.t < t), here] : undefined };
  const b: Item = { ...JSON.parse(JSON.stringify(it)), id: uid(it.kind === "puppet" ? "p" : "l"), t0: r2(t), keys: ks.length ? [here, ...ks.filter((k) => k.t > t)] : undefined };
  if (it.kind === "puppet" && it.states?.length) {
    // states are in the layer's own seconds
    const d = t - it.t0;
    const before = [...it.states].filter((x: Json) => x.t0 <= d).sort((p: Json, q: Json) => p.t0 - q.t0).pop();
    b.states = [...(before ? [{ ...before, t0: 0, blend: 0 }] : []), ...it.states.filter((x: Json) => x.t0 > d).map((x: Json) => ({ ...x, t0: r2(x.t0 - d) }))];
    a.states = it.states.filter((x: Json) => x.t0 <= d);
  }
  if (it.fade) { a.animOut = a.animOut || "none"; b.anim = "none"; }
  const out = [...items];
  out.splice(i, 1, a, b);
  return { items: out, id: b.id };
}

export function trim(items: Item[], ids: string[], t: number, side: "start" | "end"): Item[] {
  return replace(items, ids, (x) => (side === "start" ? (t < x.t1 - 0.05 ? { ...x, t0: r2(t) } : x) : t > x.t0 + 0.05 ? { ...x, t1: r2(t) } : x));
}

/** Move layers in the drawing order (the list is drawn first to last: the last is on top). */
export function reorder(items: Item[], ids: string[], to: "up" | "down" | "top" | "bottom"): Item[] {
  const picked = items.filter((x) => ids.includes(x.id));
  const rest = items.filter((x) => !ids.includes(x.id));
  if (to === "top") return [...rest, ...picked];
  if (to === "bottom") return [...picked, ...rest];
  const out = [...items];
  const order = to === "up" ? [...ids].reverse() : ids;
  for (const id of order) {
    const i = out.findIndex((x) => x.id === id);
    const j = to === "up" ? i + 1 : i - 1;
    if (i < 0 || j < 0 || j >= out.length || ids.includes(out[j].id)) continue;
    [out[i], out[j]] = [out[j], out[i]];
  }
  return out;
}

/** Put a layer at position `to` (0 = bottom) in the drawing order. */
export function moveTo(items: Item[], id: string, to: number): Item[] {
  const it = items.find((x) => x.id === id);
  if (!it) return items;
  const rest = items.filter((x) => x.id !== id);
  rest.splice(Math.max(0, Math.min(rest.length, to)), 0, it);
  return rest;
}

// ---------- keyframes, one property at a time ----------
export const keyAt = (it: Item, t: number) => (it.keys || []).find((k) => Math.abs(k.t - t) <= NEAR);
export const animated = (it: Item, p: string) => (it.keys || []).some((k) => (k as Json)[p] !== undefined);
export const keyTimes = (it: Item) => [...new Set((it.keys || []).map((k) => k.t).concat((it.pkeys || []).map((k: Json) => k.t)))].sort((a, b) => a - b);

/** Set one property: on an animated property that is a key at t (auto-key), otherwise the base value. */
export function setProp(items: Item[], it: Item, t: number, p: string, v: any): Item {
  if (!KEYABLE.includes(p) || !animated(it, p)) return { ...it, [p]: v };
  const keys = (it.keys || []).filter((k) => Math.abs(k.t - t) > NEAR);
  const old = keyAt(it, t) || { t: r2(t), ease: "inOut" };
  return { ...it, keys: [...keys, { ...old, [p]: v }].sort((a, b) => a.t - b.t) };
}

/** A key for property p at t (with the value it has now), or take it away if there is one. */
export function toggleKey(items: Item[], it: Item, t: number, p: string, fallback?: any): Item {
  const k = keyAt(it, t) as Json | undefined;
  if (k && k[p] !== undefined) {
    const nk = { ...k };
    delete nk[p];
    const rest = (it.keys || []).filter((x) => x !== k);
    const keep = KEYABLE.some((q) => nk[q] !== undefined);
    return { ...it, keys: keep ? [...rest, nk].sort((a, b) => a.t - b.t) : rest.length ? rest : undefined };
  }
  let v: any;
  if ((PROPS as readonly string[]).includes(p)) {
    const s = evalItem(items, { ...it, parent: undefined }, t);
    v = p === "rot" || p === "scale" || p === "opacity" ? r2(s[p]) : Math.round(s[p]);
  } else v = styled(it, t)[p] ?? it[p] ?? fallback ?? (COLORS.includes(p) ? "#ffffff" : 0);
  const keys = (it.keys || []).filter((x) => x !== k);
  // the first key of a property also pins its value where it was: the base becomes the start
  return { ...it, keys: [...keys, { ...(k || { t: r2(t), ease: "inOut" }), [p]: v }].sort((a, b) => a.t - b.t) };
}

/** Keys of all transform properties at t (a "snapshot" key). */
export function keyAll(items: Item[], it: Item, t: number): Item {
  const s = evalItem(items, { ...it, parent: undefined }, t);
  const keys = (it.keys || []).filter((k) => Math.abs(k.t - t) > NEAR);
  return { ...it, keys: [...keys, { ...(keyAt(it, t) || { ease: "inOut" }), t: r2(t), x: Math.round(s.x), y: Math.round(s.y), w: Math.round(s.w), h: Math.round(s.h), rot: r2(s.rot), scale: r2(s.scale), opacity: r2(s.opacity) }].sort((a, b) => a.t - b.t) };
}

/** Move every key (and puppet param key) at time `from` to `to`. */
export function moveKeyTime(it: Item, from: number, to: number): Item {
  const mv = (k: Json) => (Math.abs(k.t - from) <= NEAR ? { ...k, t: r2(Math.max(it.t0, Math.min(it.t1, to))) } : k);
  return { ...it, keys: it.keys?.map(mv).sort((a, b) => a.t - b.t), pkeys: it.pkeys?.map(mv).sort((a: Json, b: Json) => a.t - b.t) };
}

// ---------- puppet params keyed over time ----------
export function paramKeyAt(it: Item, t: number) { return (it.pkeys || []).find((k: Json) => Math.abs(k.t - t) <= NEAR); }
export function paramsAt(it: Item, t: number): Json {
  const ks = (it.pkeys || []).slice().sort((a: Json, b: Json) => a.t - b.t);
  const out: Json = { ...(it.params || {}) };
  const names = new Set<string>();
  ks.forEach((k: Json) => Object.keys(k.p || {}).forEach((n) => names.add(n)));
  for (const n of names) {
    const kk = ks.filter((k: Json) => k.p?.[n] !== undefined);
    if (t <= kk[0].t) { out[n] = kk[0].p[n]; continue; }
    if (t >= kk[kk.length - 1].t) { out[n] = kk[kk.length - 1].p[n]; continue; }
    for (let i = 0; i < kk.length - 1; i++) {
      const a = kk[i], b = kk[i + 1];
      if (t >= a.t && t <= b.t) { const va = a.p[n], vb = b.p[n]; out[n] = typeof va === "number" && typeof vb === "number" ? va + (vb - va) * ((t - a.t) / Math.max(1e-6, b.t - a.t)) : va; break; }
    }
  }
  return out;
}
export function setParam(it: Item, t: number, name: string, v: any): Item {
  const keyed = (it.pkeys || []).some((k: Json) => k.p?.[name] !== undefined);
  if (!keyed) return { ...it, params: { ...(it.params || {}), [name]: v } };
  const old = paramKeyAt(it, t) || { t: r2(t), p: {} };
  const rest = (it.pkeys || []).filter((k: Json) => k !== old);
  return { ...it, pkeys: [...rest, { ...old, p: { ...old.p, [name]: v } }].sort((a: Json, b: Json) => a.t - b.t) };
}
export function toggleParamKey(it: Item, t: number, name: string, now: any): Item {
  const k = paramKeyAt(it, t);
  if (k && k.p?.[name] !== undefined) {
    const p = { ...k.p };
    delete p[name];
    const rest = (it.pkeys || []).filter((x: Json) => x !== k);
    const next = Object.keys(p).length ? [...rest, { ...k, p }] : rest;
    return { ...it, pkeys: next.length ? next.sort((a: Json, b: Json) => a.t - b.t) : undefined };
  }
  const rest = (it.pkeys || []).filter((x: Json) => x !== k);
  return { ...it, pkeys: [...rest, { ...(k || { t: r2(t) }), p: { ...(k?.p || {}), [name]: now } }].sort((a: Json, b: Json) => a.t - b.t) };
}

// ---------- arranging ----------
export type Align = "left" | "hcenter" | "right" | "top" | "vcenter" | "bottom";
/** Align layers to each other (two or more) or to the frame (one). */
export function align(items: Item[], ids: string[], how: Align, W: number, H: number, t: number): Item[] {
  const sel = items.filter((x) => ids.includes(x.id));
  if (!sel.length) return items;
  const st = sel.map((x) => evalItem(items, { ...x, parent: undefined }, t));
  const frame = sel.length === 1;
  const L = frame ? 0 : Math.min(...st.map((s) => s.x)), T = frame ? 0 : Math.min(...st.map((s) => s.y));
  const R = frame ? W : Math.max(...st.map((s) => s.x + s.w)), B = frame ? H : Math.max(...st.map((s) => s.y + s.h));
  return items.map((x) => {
    const i = sel.indexOf(x);
    if (i < 0) return x;
    const s = st[i];
    let nx = s.x, ny = s.y;
    if (how === "left") nx = L; else if (how === "right") nx = R - s.w; else if (how === "hcenter") nx = (L + R) / 2 - s.w / 2;
    else if (how === "top") ny = T; else if (how === "bottom") ny = B - s.h; else ny = (T + B) / 2 - s.h / 2;
    return shift(items, x, t, nx - s.x, ny - s.y);
  });
}
export function distribute(items: Item[], ids: string[], axis: "x" | "y", t: number): Item[] {
  const sel = items.filter((x) => ids.includes(x.id));
  if (sel.length < 3) return items;
  const st = new Map(sel.map((x) => [x.id, evalItem(items, { ...x, parent: undefined }, t)]));
  const d = axis === "x" ? "w" : "h";
  const order = [...sel].sort((a, b) => st.get(a.id)![axis] - st.get(b.id)![axis]);
  const first = st.get(order[0].id)!, last = st.get(order[order.length - 1].id)!;
  const total = order.reduce((n, x) => n + st.get(x.id)![d], 0);
  const gap = (last[axis] + last[d] - first[axis] - total) / (order.length - 1);
  let at = first[axis];
  const want = new Map<string, number>();
  for (const x of order) { want.set(x.id, at); at += st.get(x.id)![d] + gap; }
  return items.map((x) => (want.has(x.id) ? shift(items, x, t, axis === "x" ? want.get(x.id)! - st.get(x.id)!.x : 0, axis === "y" ? want.get(x.id)! - st.get(x.id)!.y : 0) : x));
}

/** Move a layer by dx, dy at t: its key at t when x / y are animated, its path when it has one, else its place. */
export function shift(items: Item[], it: Item, t: number, dx: number, dy: number): Item {
  if (!dx && !dy) return it;
  if (Array.isArray(it.path) && it.path.length) return { ...it, path: it.path.map((p: number[]) => [Math.round(p[0] + dx), Math.round(p[1] + dy)]) };
  const own = evalItem(items, { ...it, parent: undefined }, t);
  let n = it;
  if (dx) n = setProp(items, n, t, "x", Math.round(own.x + dx));
  if (dy) n = setProp(items, n, t, "y", Math.round(own.y + dy));
  return n;
}

// ---------- motion presets: keys over the layer's own time ----------
export const MOTIONS: [string, string][] = [
  ["rise", tx("Rise in")], ["drop", tx("Drop in")], ["kenburns", tx("Slow zoom (Ken Burns)")], ["pulse", tx("Pulse")], ["float", tx("Float")],
  ["shake", tx("Shake")], ["wiggle", tx("Wiggle")], ["spin", tx("Spin in")], ["across", tx("Slide across")], ["grow", tx("Grow from zero")], ["popout", tx("Pop and leave")],
];
export function motion(items: Item[], it: Item, name: string, W: number): Item {
  const s = evalItem(items, { ...it, parent: undefined, keys: undefined }, it.t0);
  const a = it.t0, b = it.t1, d = Math.max(0.2, b - a), x = Math.round(s.x), y = Math.round(s.y);
  const K = (t: number, p: Json, ease = "inOut"): Key => ({ t: r2(Math.min(b, Math.max(a, t))), ease, ...p });
  let keys: Key[] = [];
  const steps = (n: number, f: (i: number, k: number) => Json, ease = "inOut") => Array.from({ length: n + 1 }, (_, i) => K(a + (d * i) / n, f(i, i / n), ease));
  switch (name) {
    case "rise": keys = [K(a, { y: y + Math.round(s.h * 0.5), opacity: 0 }), K(a + Math.min(0.6, d / 2), { y, opacity: 1 }, "out")]; break;
    case "drop": keys = [K(a, { y: y - Math.round(s.h * 1.2), opacity: 0 }), K(a + Math.min(0.7, d / 2), { y, opacity: 1 }, "back")]; break;
    case "kenburns": keys = [K(a, { scale: 1 }), K(b, { scale: 1.15 }, "linear")]; break;
    case "pulse": keys = steps(Math.max(2, Math.round(d / 0.5) * 2), (i) => ({ scale: i % 2 ? 1.08 : 1 })); break;
    case "float": keys = steps(Math.max(2, Math.round(d / 1.2)), (i) => ({ y: y + (i % 2 ? -Math.round(s.h * 0.08) : 0) })); break;
    case "shake": { const n = Math.min(12, Math.max(6, Math.round(Math.min(d, 0.8) / 0.06))); keys = Array.from({ length: n + 1 }, (_, i) => K(a + (Math.min(d, 0.8) * i) / n, { x: i === n ? x : x + (i % 2 ? 1 : -1) * Math.round(Math.max(6, s.w * 0.03) * (1 - i / n)) }, "linear")); break; }
    case "wiggle": keys = steps(Math.max(4, Math.round(d / 0.35)), (i, k) => ({ rot: k >= 1 ? 0 : i % 2 ? 6 : -6 })); break;
    case "spin": keys = [K(a, { rot: -180, scale: 0.2, opacity: 0 }), K(a + Math.min(0.8, d / 2), { rot: 0, scale: 1, opacity: 1 }, "out")]; break;
    case "across": keys = [K(a, { x: -Math.round(s.w) }), K(b, { x: W }, "linear")]; break;
    case "grow": keys = [K(a, { scale: 0 }), K(a + Math.min(0.5, d / 2), { scale: 1 }, "back")]; break;
    case "popout": keys = [K(a, { scale: 0 }), K(a + Math.min(0.4, d / 3), { scale: 1 }, "back"), K(b - Math.min(0.4, d / 3), { scale: 1 }), K(b, { scale: 0 }, "in")]; break;
  }
  return { ...it, keys, fade: name === "across" || name === "kenburns" ? it.fade : 0 };
}

// ---------- looks for text ----------
export const LOOKS: [string, string, Json][] = [
  ["bold", tx("Bold title"), { weight: 900, color: "#ffffff", stroke: "#000000", strokeWidth: undefined, shadow: undefined, bg: undefined, upper: true, spacing: 2, font: undefined }],
  ["clean", tx("Clean"), { weight: 600, color: "#ffffff", stroke: undefined, shadow: "rgba(0,0,0,0.55)", shadowBlur: 18, shadowY: 4, bg: undefined, upper: undefined, spacing: undefined, font: undefined }],
  ["caption", tx("Caption box"), { weight: 700, color: "#111111", stroke: undefined, shadow: undefined, bg: "#ffffff", radius: 18, upper: undefined, spacing: undefined }],
  ["neon", tx("Neon"), { weight: 800, color: "#fdf2ff", stroke: undefined, shadow: "#ff3df2", shadowBlur: 40, shadowY: 0, bg: undefined, font: undefined }],
  ["outline", tx("Outline"), { weight: 900, color: "rgba(0,0,0,0)", stroke: "#ffffff", strokeWidth: 6, shadow: undefined, bg: undefined }],
  ["hand", tx("Handwritten"), { weight: 700, font: '"Caveat", "Segoe Print", "Ink Free", cursive', color: "#ffffff", stroke: undefined, upper: undefined }],
  ["serif", tx("Editorial serif"), { weight: 500, italic: true, font: 'Georgia, "Times New Roman", serif', color: "#ffffff", stroke: undefined, upper: undefined, spacing: undefined }],
  ["tag", tx("Accent tag"), { weight: 800, color: "#1a1206", bg: "#e9b84a", radius: 999, upper: true, spacing: 1, stroke: undefined, shadow: undefined }],
];
export const FONTS: [string, string][] = [
  ["", tx("System (Segoe UI)")], ["Inter, system-ui, sans-serif", "Inter"], ["Montserrat, system-ui, sans-serif", "Montserrat"], ["Poppins, system-ui, sans-serif", "Poppins"],
  ["Impact, \"Arial Black\", sans-serif", "Impact"], ["Georgia, \"Times New Roman\", serif", "Georgia"], ["\"Playfair Display\", Georgia, serif", "Playfair Display"],
  ["\"Bebas Neue\", Impact, sans-serif", "Bebas Neue"], ["\"Caveat\", \"Segoe Print\", \"Ink Free\", cursive", "Caveat"], ["\"Cascadia Mono\", Consolas, monospace", tx("Monospace")],
];
export const ANIMS: [string, string][] = [
  ["fade", tx("Fade")], ["none", tx("Cut (none)")], ["slide-up", tx("Slide up")], ["slide-down", tx("Slide down")], ["slide-left", tx("Slide in from right")], ["slide-right", tx("Slide in from left")],
  ["pop", tx("Pop")], ["zoom", tx("Zoom")], ["spin", tx("Spin")], ["blur", tx("Blur")], ["wipe", tx("Wipe")],
];
export const BLENDS: [string, string][] = [
  ["normal", tx("Normal")], ["multiply", tx("Multiply")], ["screen", tx("Screen")], ["overlay", tx("Overlay")], ["darken", tx("Darken")], ["lighten", tx("Lighten")],
  ["color-dodge", tx("Colour dodge")], ["soft-light", tx("Soft light")], ["difference", tx("Difference")],
];
export const EASES = ["inOut", "out", "in", "linear", "back", "sine", "expo", "elastic", "bounce", "hold"];

// ---------- groups ----------
/** Put layers in a new group (a frame that moves, turns, scales and fades them together). */
export function group(items: Item[], ids: string[], t: number): { items: Item[]; id?: string } {
  const picked = items.filter((x) => ids.includes(x.id));
  if (picked.length < 1) return { items };
  // a layer rigged to something outside stays with its rig
  const inGroup = (x: Item) => !x.parent || ids.includes(x.parent) || items.find((p) => p.id === x.parent)?.kind === "group";
  const members = picked.filter(inGroup);
  if (!members.length) return { items };
  const outer = members.map((x) => (x.parent && !ids.includes(x.parent) ? x.parent : "")).filter((p, i, a) => a.indexOf(p) === i);
  const parent = outer.length === 1 && outer[0] ? outer[0] : undefined;
  const st = members.map((x) => evalItem(items, { ...x, parent: undefined }, t));
  const x0 = Math.min(...st.map((s) => s.x)), y0 = Math.min(...st.map((s) => s.y));
  const x1 = Math.max(...st.map((s) => s.x + s.w)), y1 = Math.max(...st.map((s) => s.y + s.h));
  const id = uid("g");
  const g: Item = { id, kind: "group", label: tx("Group"), t0: r2(Math.min(...members.map((x) => x.t0))), t1: r2(Math.max(...members.map((x) => x.t1))), x: Math.round(x0), y: Math.round(y0), w: Math.round(x1 - x0), h: Math.round(y1 - y0), parent, opacity: 1, fade: 0 };
  const first = items.findIndex((x) => members.includes(x));
  const out = items.map((x) => (members.includes(x) && (!x.parent || !ids.includes(x.parent)) ? { ...x, parent: id } : x));
  out.splice(first, 0, g);
  return { items: out, id };
}
/** Take the layers out of a group (they keep their place) and drop the group. */
export function ungroup(items: Item[], id: string): { items: Item[]; ids: string[] } {
  const g = items.find((x) => x.id === id);
  if (!g || g.kind !== "group") return { items, ids: [] };
  const kids = items.filter((x) => x.parent === id).map((x) => x.id);
  return { items: items.filter((x) => x.id !== id).map((x) => (x.parent === id ? { ...x, parent: g.parent } : x)), ids: kids };
}
/** The outermost group a layer is in (or the layer itself). */
export function topOf(items: Item[], it: Item): Item {
  return groupsOf(items, it)[0] || it;
}

// ---------- clipboard (in this window, and across films through local storage) ----------
const CLIP = "linnea.clip2d";
export function copyItems(items: Item[], all?: Item[]) {
  if (all) items = all.filter((x) => withInside(all, items.map((y) => y.id)).includes(x.id));
  try { localStorage.setItem(CLIP, JSON.stringify(items)); } catch { /* private window */ }
}
export function pasteItems(t: number): Item[] {
  let got: Item[] = [];
  try { got = JSON.parse(localStorage.getItem(CLIP) || "[]"); } catch { return []; }
  if (!Array.isArray(got) || !got.length) return [];
  const t0 = Math.min(...got.map((x) => x.t0));
  const ids = new Map(got.map((x) => [x.id, uid(x.kind === "puppet" ? "p" : "l")]));
  return got.map((x) => {
    const d = t - t0;
    const mv = (k: Json) => ({ ...k, t: r2(k.t + d) });
    return { ...x, id: ids.get(x.id)!, parent: x.parent && ids.has(x.parent) ? ids.get(x.parent) : undefined, t0: r2(x.t0 + d), t1: r2(x.t1 + d), keys: x.keys?.map(mv), pkeys: x.pkeys?.map(mv) };
  });
}
export function hasClip(): boolean {
  try { return (localStorage.getItem(CLIP) || "[]").length > 2; } catch { return false; }
}
const STYLE = "linnea.style2d";
const STYLE_KEYS = ["fx", "fy", "fit", "font", "size", "weight", "italic", "align", "spacing", "lineHeight", "upper", "color", "stroke", "strokeWidth", "bg", "bg2", "gradAngle", "radius", "border", "borderWidth", "shadow", "shadowBlur", "shadowX", "shadowY", "blend", "blur", "anim", "animOut", "fade", "typing", "reveal"];
export function copyStyle(it: Item) {
  const s: Json = {};
  for (const k of STYLE_KEYS) s[k] = it[k];
  try { localStorage.setItem(STYLE, JSON.stringify(s)); } catch { /* private window */ }
}
export function pasteStyle(it: Item): Item {
  try { return { ...it, ...JSON.parse(localStorage.getItem(STYLE) || "{}") }; } catch { return it; }
}

// ---------- your own presets: a layer kept to add again ----------
const MINE = "linnea.presets2d";
export function myPresets(): { name: string; item: Item }[] {
  try { const v = JSON.parse(localStorage.getItem(MINE) || "[]"); return Array.isArray(v) ? v : []; } catch { return []; }
}
export function savePreset(name: string, it: Item) {
  const { id, t0, t1, parent, ...rest } = it;
  const d = t1 - t0;
  const keys = it.keys?.map((k) => ({ ...k, t: r2(k.t - t0) }));
  const pkeys = it.pkeys?.map((k: Json) => ({ ...k, t: r2(k.t - t0) }));
  const list = myPresets().filter((p) => p.name !== name);
  list.push({ name, item: { ...rest, id: "", t0: 0, t1: r2(d), keys, pkeys } as Item });
  try { localStorage.setItem(MINE, JSON.stringify(list.slice(-40))); } catch { /* private window */ }
}
export function deletePreset(name: string) {
  try { localStorage.setItem(MINE, JSON.stringify(myPresets().filter((p) => p.name !== name))); } catch { /* private window */ }
}
export function fromPreset(p: Item, t: number, dur: number): Item {
  const mv = (k: Json) => ({ ...k, t: r2(k.t + t) });
  const len = p.t1 - p.t0;
  return { ...JSON.parse(JSON.stringify(p)), id: uid(p.kind === "puppet" ? "p" : "l"), t0: r2(t), t1: r2(Math.min(dur, t + len)), keys: p.keys?.map(mv), pkeys: p.pkeys?.map(mv) };
}
