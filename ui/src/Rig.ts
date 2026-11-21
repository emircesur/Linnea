// Puppets built from bones (a rig) and puppets built from frames (a sprite), both saved as ordinary rich
// puppets: their `draw` is generated here and carries its own data, so the engine, the library previews and
// renders need nothing new. The rig itself is kept on the puppet (`rig`) so it can be edited again.
import { tx } from "./i18n";
import { Json, tool } from "./api";
import { puppetName } from "./Puppets";

export type Bone = {
  id: string; label?: string; parent?: string;
  x: number; y: number;      // the joint, at rest, in the puppet's box
  a: number; len: number;    // rest angle in degrees (0 points down, 90 right, 180 up) and length
  shape: "capsule" | "ellipse" | "rect" | "circle" | "line" | "image" | "none";
  w: number;                 // thickness (an image's width)
  color?: string;            // a colour, or $param
  z?: number; face?: boolean; img?: string; stroke?: string;
};
export type RigState = { dur: number; loop: boolean; label?: string; keys: { t: number; pose: Record<string, number>; ease?: string }[] };
export type Rig = { size: [number, number]; bones: Bone[]; params: Json; states: Record<string, RigState>; initial: string; outline?: string };

/** The runtime every rig puppet's draw carries: solve the bones for a pose, and paint them. */
export const RUNTIME = String.raw`
function solve(R, pose) {
  const W = {}, by = {};
  for (const b of R.bones) by[b.id] = b;
  const one = b => {
    if (W[b.id]) return W[b.id];
    const d = (pose[b.id] || 0) * Math.PI / 180;
    const P = b.parent && by[b.parent] ? one(by[b.parent]) : null;
    if (P) {
      const pb = by[b.parent], ox = b.x - pb.x, oy = b.y - pb.y, c = Math.cos(P.d), s = Math.sin(P.d);
      return (W[b.id] = { x: P.x + ox * c + oy * s, y: P.y - ox * s + oy * c, d: P.d + d });
    }
    return (W[b.id] = { x: b.x + (pose.$x || 0), y: b.y + (pose.$y || 0), d });
  };
  R.bones.forEach(one);
  return W;
}
function poseAt(st, T) {
  const ks = (st && st.keys || []).slice().sort((a, b) => a.t - b.t);
  if (!ks.length) return {};
  if (ks.length === 1) return ks[0].pose;
  const dur = st.dur || 1;
  let t = st.loop ? ((T % dur) + dur) % dur : Math.min(T, dur);
  const list = st.loop ? ks.concat([{ t: dur + ks[0].t, pose: ks[0].pose, ease: ks[0].ease }]) : ks;
  if (st.loop && t < list[0].t) t += dur;
  if (t <= list[0].t) return list[0].pose;
  if (t >= list[list.length - 1].t) return list[list.length - 1].pose;
  for (let i = 0; i < list.length - 1; i++) {
    const a = list[i], b = list[i + 1];
    if (t >= a.t && t <= b.t) {
      let k = (t - a.t) / Math.max(1e-6, b.t - a.t);
      const e = b.ease || 'inOut';
      k = e === 'linear' ? k : e === 'in' ? k * k * k : e === 'out' ? 1 - Math.pow(1 - k, 3) : e === 'hold' ? (k >= 1 ? 1 : 0) : (k < 0.5 ? 4 * k * k * k : 1 - Math.pow(-2 * k + 2, 3) / 2);
      const o = {}, names = new Set(Object.keys(a.pose).concat(Object.keys(b.pose)));
      for (const n of names) o[n] = (a.pose[n] || 0) + ((b.pose[n] || 0) - (a.pose[n] || 0)) * k;
      return o;
    }
  }
  return list[0].pose;
}
function paintRig(g, p, t, s, u, R, pose, sel) {
  const W = solve(R, pose);
  const col = c => (typeof c === 'string' && c[0] === '$' ? p[c.slice(1)] : c) || '#888888';
  const list = R.bones.slice().sort((a, b) => (a.z || 0) - (b.z || 0));
  for (const b of list) {
    if (b.shape === 'none') continue;
    const w = W[b.id], A = (b.a * Math.PI / 180) + w.d, L = b.len, T = b.w;
    g.save(); g.translate(w.x, w.y); g.rotate(-A);
    g.fillStyle = col(b.color); g.strokeStyle = b.stroke ? col(b.stroke) : R.outline ? col(R.outline) : 'transparent'; g.lineWidth = R.outlineWidth || 4; g.lineJoin = 'round';
    g.beginPath();
    if (b.shape === 'capsule') g.roundRect(-T / 2, -T / 2, T, L + T, T / 2);
    else if (b.shape === 'ellipse') g.ellipse(0, L / 2, T / 2, L / 2, 0, 0, Math.PI * 2);
    else if (b.shape === 'rect') g.rect(-T / 2, 0, T, L);
    else if (b.shape === 'circle') g.arc(0, L / 2, Math.max(T, L) / 2, 0, Math.PI * 2);
    if (b.shape === 'line') { g.strokeStyle = col(b.color); g.lineWidth = T; g.lineCap = 'round'; g.moveTo(0, 0); g.lineTo(0, L); g.stroke(); }
    else if (b.shape === 'image') { const im = u.img && u.img[b.img]; if (im && (im.complete === undefined || im.complete)) g.drawImage(im, -T / 2, 0, T, L); }
    else { g.fill(); if (g.strokeStyle !== 'transparent' && (b.stroke || R.outline)) g.stroke(); }
    if (b.face) {
      const r = Math.max(T, L) / 2;
      g.translate(0, L / 2); g.rotate(b.a * Math.PI / 180);
      g.fillStyle = col(R.face || '$face');
      const bl = 1 - Math.max(0, Math.min(1, p.blink || 0)) * 0.9;
      for (const ex of [-0.34, 0.34]) { g.beginPath(); g.ellipse(ex * r, -0.08 * r, r * 0.09, r * 0.09 * bl + 0.6, 0, 0, Math.PI * 2); g.fill(); }
      const m = Math.max(0, Math.min(1, p.mouth || 0));
      g.beginPath();
      if (m > 0.04) { g.ellipse(0, r * 0.38, r * 0.2, r * (0.04 + m * 0.2), 0, 0, Math.PI * 2); g.fill(); }
      else { g.strokeStyle = col(R.face || '$face'); g.lineWidth = Math.max(2, r * 0.07); g.lineCap = 'round'; g.moveTo(-r * 0.22, r * 0.32); g.quadraticCurveTo(0, r * 0.5, r * 0.22, r * 0.32); g.stroke(); }
    }
    g.restore();
  }
  if (sel !== undefined) {
    for (const b of R.bones) {
      const w = W[b.id], A = (b.a * Math.PI / 180) + w.d;
      g.save(); g.strokeStyle = b.id === sel ? '#ff4fa3' : 'rgba(255,255,255,0.7)'; g.fillStyle = g.strokeStyle; g.lineWidth = b.id === sel ? 4 : 2;
      g.beginPath(); g.moveTo(w.x, w.y); g.lineTo(w.x + Math.sin(A) * b.len, w.y + Math.cos(A) * b.len); g.stroke();
      g.beginPath(); g.arc(w.x, w.y, b.id === sel ? 8 : 5, 0, Math.PI * 2); g.fill(); g.restore();
    }
  }
  return W;
}`;

const rt = new Function(RUNTIME + "; return { solve, poseAt, paintRig };")() as { solve: (R: Rig, pose: Json) => Record<string, { x: number; y: number; d: number }>; poseAt: (st: RigState, t: number) => Record<string, number>; paintRig: (g: CanvasRenderingContext2D, p: Json, t: number, s: Json, u: Json, R: Rig, pose: Json, sel?: string) => Json };
export const solve = rt.solve, poseAt = rt.poseAt, paintRig = rt.paintRig;

/** The draw function of a rig puppet: the rig's data and the runtime inside it. */
export function compileRig(R: Rig): string {
  const data = JSON.stringify({ bones: R.bones, states: R.states, outline: R.outline, face: "$face" });
  return `function (g, p, t, s, u) {\n  const R = ${data};\n${RUNTIME}\n  const st = R.states[s.name] || R.states[Object.keys(R.states)[0]];\n  paintRig(g, p, t, s, u, R, poseAt(st, s.t));\n}`;
}

export function puppetFromRig(R: Rig, name: string, label: string, assets?: Json) {
  const states: Json = {};
  for (const [k, v] of Object.entries(R.states)) states[k] = { dur: v.dur, loop: v.loop, label: v.label || k, blend: 0.2 };
  return {
    name, label, desc: tx("A puppet built from bones. States: {0}.", Object.keys(R.states).join(", ")), tags: ["rig", "bones", "character"],
    size: R.size, params: R.params, states, initial: R.initial in R.states ? R.initial : Object.keys(R.states)[0], draw: compileRig(R), rig: R, assets,
  };
}

// ---------- templates ----------
const deg = (a: number) => (a * Math.PI) / 180;
function tip(b: Bone): [number, number] { return [b.x + Math.sin(deg(b.a)) * b.len, b.y + Math.cos(deg(b.a)) * b.len]; }
function chainBone(parent: Bone, more: Partial<Bone> & { id: string }): Bone {
  const [x, y] = tip(parent);
  return { x, y, a: parent.a, len: parent.len, shape: parent.shape, w: parent.w, color: parent.color, z: parent.z, parent: parent.id, ...more } as Bone;
}

export function presenterRig(): Rig {
  const hips: Bone = { id: "hips", label: tx("Hips"), x: 200, y: 410, a: 180, len: 0, shape: "none", w: 0 };
  const torso: Bone = { id: "torso", label: tx("Body"), parent: "hips", x: 200, y: 410, a: 180, len: 172, shape: "capsule", w: 92, color: "$shirt", z: 2 };
  const head: Bone = { id: "head", label: tx("Head"), parent: "torso", x: 200, y: 236, a: 180, len: 112, shape: "circle", w: 112, color: "$skin", z: 6, face: true };
  const hair: Bone = { id: "hair", label: tx("Hair"), parent: "head", x: 200, y: 228, a: 180, len: 122, shape: "circle", w: 122, color: "$hair", z: 5 };
  const armL: Bone = { id: "armL", label: tx("Left arm"), parent: "torso", x: 156, y: 264, a: -14, len: 96, shape: "capsule", w: 34, color: "$shirt", z: 3 };
  const armR: Bone = { id: "armR", label: tx("Right arm"), parent: "torso", x: 244, y: 264, a: 14, len: 96, shape: "capsule", w: 34, color: "$shirt", z: 3 };
  const foreL = chainBone(armL, { id: "foreL", label: tx("Left forearm"), len: 86, w: 28, color: "$skin", a: -6 });
  const foreR = chainBone(armR, { id: "foreR", label: tx("Right forearm"), len: 86, w: 28, color: "$skin", a: 6 });
  const thighL: Bone = { id: "thighL", label: tx("Left thigh"), parent: "hips", x: 178, y: 406, a: -4, len: 124, shape: "capsule", w: 42, color: "$pants", z: 1 };
  const thighR: Bone = { id: "thighR", label: tx("Right thigh"), parent: "hips", x: 222, y: 406, a: 4, len: 124, shape: "capsule", w: 42, color: "$pants", z: 1 };
  const shinL = chainBone(thighL, { id: "shinL", label: tx("Left shin"), len: 118, w: 36, a: 0 });
  const shinR = chainBone(thighR, { id: "shinR", label: tx("Right shin"), len: 118, w: 36, a: 0 });
  const K = (t: number, pose: Json, ease?: string) => ({ t, pose, ease });
  return {
    size: [400, 700], outline: "$line", initial: "idle",
    bones: [hips, thighL, thighR, shinL, shinR, torso, armL, armR, foreL, foreR, hair, head],
    params: {
      skin: { type: "color", default: "#f2c7a5", label: tx("Skin") }, hair: { type: "color", default: "#3b2a20", label: tx("Hair") },
      shirt: { type: "color", default: "#3e7be6", label: tx("Shirt") }, pants: { type: "color", default: "#2b2f3a", label: tx("Trousers") },
      line: { type: "color", default: "#1d1d1f", label: tx("Outline") }, face: { type: "color", default: "#1d1d1f", label: tx("Face") },
      mouth: { type: "number", default: 0, min: 0, max: 1, step: 0.05, label: tx("Mouth (follows the voice if left alone)") },
      blink: { type: "number", default: 0, min: 0, max: 1, step: 0.05, label: tx("Eyes (blink on their own if left alone)") },
    },
    states: {
      idle: { dur: 3, loop: true, label: tx("Idle"), keys: [K(0, {}), K(1.5, { torso: 1.5, head: -3, armL: -3, armR: 3, $y: -3 })] },
      talk: { dur: 2, loop: true, label: tx("Talk"), keys: [K(0, { armR: 30, foreR: -70, armL: -20, foreL: 40, head: 4 }), K(1, { armR: 45, foreR: -45, armL: -32, foreL: 62, head: -4, torso: 1 })] },
      wave: { dur: 1.2, loop: true, label: tx("Wave"), keys: [K(0, { armR: 150, foreR: 25, head: 6 }), K(0.3, { armR: 150, foreR: -25, head: 6 }), K(0.6, { armR: 150, foreR: 25, head: 6 }), K(0.9, { armR: 150, foreR: -25, head: 6 })] },
      point: { dur: 0.6, loop: false, label: tx("Point"), keys: [K(0, {}), K(0.6, { armR: 80, foreR: 8, head: 10, torso: -3 }, "out")] },
      cheer: { dur: 0.8, loop: true, label: tx("Cheer"), keys: [K(0, { armL: -150, armR: 150, foreL: -12, foreR: 12 }), K(0.4, { armL: -158, armR: 158, foreL: -25, foreR: 25, $y: -22 })] },
      shrug: { dur: 0.8, loop: false, label: tx("Shrug"), keys: [K(0, {}), K(0.5, { armL: -35, foreL: 105, armR: 35, foreR: -105, head: 10, $y: -6 }, "out")] },
      think: { dur: 0.7, loop: false, label: tx("Think"), keys: [K(0, {}), K(0.7, { armR: 22, foreR: -150, head: -9 }, "out")] },
      walk: { dur: 1, loop: true, label: tx("Walk"), keys: [K(0, { armL: 12, armR: 12 }), K(0.25, { thighL: -22, shinL: 45, armL: -18, armR: -18, $y: -8 }), K(0.5, { armL: -12, armR: -12 }), K(0.75, { thighR: 22, shinR: -45, armL: 18, armR: 18, $y: -8 })] },
    },
  };
}

/** A chain of bones that sways like a tail, a tentacle or a plant. */
export function chainRig(n = 7): Rig {
  const bones: Bone[] = [{ id: "b0", label: tx("Bone {0}", 1), x: 200, y: 580, a: 180, len: 72, shape: "capsule", w: 56, color: "$color", z: 0 }];
  for (let i = 1; i < n; i++) bones.push(chainBone(bones[i - 1], { id: "b" + i, label: tx("Bone {0}", i + 1), w: Math.max(12, 56 - i * 7), len: 72 - i * 4, z: i }));
  const sway = (amp: number, ph: number) => Object.fromEntries(bones.map((b, i) => [b.id, Math.sin(ph + i * 0.7) * amp * (0.4 + i * 0.15)]));
  return {
    size: [400, 600], initial: "sway", bones,
    params: { color: { type: "color", default: "#7bd389", label: tx("Colour") } },
    states: {
      sway: { dur: 2.4, loop: true, label: tx("Sway"), keys: [0, 1, 2, 3].map((i) => ({ t: i * 0.6, pose: sway(9, (i * Math.PI) / 2) })) },
      curl: { dur: 1, loop: false, label: tx("Curl up"), keys: [{ t: 0, pose: {} }, { t: 1, pose: Object.fromEntries(bones.map((b, i) => [b.id, 18 + i * 2])), ease: "out" }] },
      still: { dur: 1, loop: true, label: tx("Still"), keys: [{ t: 0, pose: {} }] },
    },
  };
}

export function blankRig(): Rig {
  return {
    size: [400, 400], initial: "idle", bones: [{ id: "root", label: tx("Root"), x: 200, y: 300, a: 180, len: 160, shape: "capsule", w: 60, color: "$color", z: 0 }],
    params: { color: { type: "color", default: "#e9b84a", label: tx("Colour") } },
    states: { idle: { dur: 2, loop: true, label: tx("Idle"), keys: [{ t: 0, pose: {} }] } },
  };
}

export const RIG_TEMPLATES: [string, string, () => Rig][] = [
  ["presenter", tx("Presenter (a person, 8 states)"), presenterRig],
  ["chain", tx("Tail / tentacle / plant (a chain)"), chainRig],
  ["blank", tx("One bone, to build your own"), blankRig],
];

// ---------- a puppet from frames ----------
/** A puppet that flips through pictures: one state plays them all, one holds the first. */
export async function puppetFromFrames(files: File[], taken: string[], fps = 12): Promise<string> {
  const read = (f: File) => new Promise<string>((ok, no) => { const r = new FileReader(); r.onload = () => ok(String(r.result)); r.onerror = () => no(new Error(tx("The picture could not be read"))); r.readAsDataURL(f); });
  const load = (src: string) => new Promise<HTMLImageElement>((ok, no) => { const im = new Image(); im.onload = () => ok(im); im.onerror = () => no(new Error(tx("This file could not be opened as a picture"))); im.src = src; });
  const sorted = [...files].sort((a, b) => a.name.localeCompare(b.name, undefined, { numeric: true }));
  const first = await load(await read(sorted[0]));
  const k = Math.min(1, 1024 / Math.max(first.naturalWidth, first.naturalHeight, 1));
  const w = Math.max(1, Math.round(first.naturalWidth * k)), h = Math.max(1, Math.round(first.naturalHeight * k));
  const assets: Json = {};
  const keys: string[] = [];
  for (let i = 0; i < sorted.length && i < 120; i++) {
    const im = await load(await read(sorted[i]));
    const c = document.createElement("canvas");
    c.width = w; c.height = h;
    c.getContext("2d")!.drawImage(im, 0, 0, w, h);
    assets["f" + i] = c.toDataURL("image/png");
    keys.push("f" + i);
  }
  const n = keys.length;
  const draw = `function (g, p, t, s, u) {\n  const F = ${JSON.stringify(keys)}, fps = p.fps || ${fps};\n  let i = s.name === 'still' ? 0 : s.name === 'once' ? Math.min(F.length - 1, Math.floor(s.t * fps)) : Math.floor(s.t * fps) % F.length;\n  const im = u.img[F[i]];\n  if (im && (im.complete === undefined || im.complete)) g.drawImage(im, 0, 0, u.W, u.H);\n}`;
  const label = sorted[0].name.replace(/\.[^.]+$/, "").replace(/[_-]?\d+$/, "") || tx("Frames");
  const name = puppetName(label, taken);
  await tool("puppet_save", {
    name, label, desc: tx("{0} frames that play in turn. play: loops them; once: plays them once and holds the last; still: the first frame.", n), tags: ["frames", "sprite", "picture"],
    size: [w, h], assets, params: { fps: { type: "number", default: fps, min: 1, max: 60, step: 1, label: tx("Frames a second") } },
    states: { play: { dur: n / fps, loop: true, label: tx("Play") }, once: { dur: n / fps, loop: false, label: tx("Play once") }, still: { dur: 1, loop: true, label: tx("Still") } },
    initial: "play", draw,
  });
  return name;
}
