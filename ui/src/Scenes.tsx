// Ready scenes: a few layers and puppets that belong together (a presenter with a speech bubble, a like
// button being clicked, a countdown…), placed as one group at the playhead. Each is ordinary layers after
// that: change, retime, ungroup at will.
import { tx } from "./i18n";
import { Clapperboard, Hand, ListChecks, MessageCircle, PartyPopper, Quote, Timer, Target, Columns2 } from "lucide-react";
import { Json, tool } from "./api";
import { Item, Layers } from "./Layers";
import { group, r2, uid } from "./LayerOps";

type Ctx = { t: number; W: number; H: number; dur: number };
type Scene = { id: string; label: string; icon: any; desc: string; puppets: string[]; len: number; make: (c: Ctx) => Item[] };

const P = (puppet: string, label: string, t0: number, t1: number, x: number, y: number, w: number, h: number, more: Json = {}): Item =>
  ({ id: uid("p"), kind: "puppet", puppet, label, t0: r2(t0), t1: r2(t1), x: Math.round(x), y: Math.round(y), w: Math.round(w), h: Math.round(h), opacity: 1, fade: 0.2, params: {}, states: [], ...more });
const T = (text: string, t0: number, t1: number, x: number, y: number, w: number, h: number, more: Json = {}): Item =>
  ({ id: uid(), kind: "text", text, t0: r2(t0), t1: r2(t1), x: Math.round(x), y: Math.round(y), w: Math.round(w), h: Math.round(h), opacity: 1, fade: 0.25, color: "#ffffff", weight: 800, ...more });
const R = (t0: number, t1: number, x: number, y: number, w: number, h: number, more: Json = {}): Item =>
  ({ id: uid(), kind: "rect", t0: r2(t0), t1: r2(t1), x: Math.round(x), y: Math.round(y), w: Math.round(w), h: Math.round(h), opacity: 1, fade: 0.25, ...more });

export const SCENES: Scene[] = [
  {
    id: "presenter", label: tx("Presenter with a speech bubble"), icon: MessageCircle, len: 6, puppets: ["stickman", "bubble"],
    desc: tx("A stick figure waves, then points at what it says in a bubble, with its name below."),
    make: ({ t, W, H }) => {
      const m = Math.min(W, H), fh = m * 0.55, fw = fh * (400 / 600);
      const fx = W * 0.08, fy = H - fh - H * 0.08;
      return [
        P("stickman", tx("Presenter"), t, t + 6, fx, fy, fw, fh, { states: [{ state: "wave", t0: 0 }, { state: "point", t0: 1.6, blend: 0.2 }, { state: "idle", t0: 4.5, blend: 0.3 }], anim: "slide-right" }),
        P("bubble", tx("Speech bubble"), t + 0.8, t + 6, fx + fw * 0.85, fy - fh * 0.25, m * 0.6, m * 0.6 * (360 / 700), { params: { text: tx("Here is the idea!") }, anim: "pop" }),
        { id: uid(), kind: "lower", t0: r2(t + 1.2), t1: r2(t + 6), x: Math.round(W * 0.06), y: Math.round(H * 0.05), w: Math.round(W * 0.6), h: Math.round(m * 0.12), text: tx("Full Name"), sub: tx("Job title"), reveal: 0.6, opacity: 1, fade: 0.25 },
      ];
    },
  },
  {
    id: "subscribe", label: tx("Like and subscribe"), icon: Hand, len: 4, puppets: ["cursor", "heart"],
    desc: tx("A pointer moves to a button, clicks it, and a heart bursts out."),
    make: ({ t, W, H }) => {
      const m = Math.min(W, H), bw = m * 0.5, bh = m * 0.13, bx = (W - bw) / 2, by = H * 0.62;
      const btn = R(t, t + 4, bx, by, bw, bh, { bg: "#ff3b3b", radius: Math.round(bh / 2), anim: "pop", label: tx("Button") });
      const lab = T(tx("SUBSCRIBE"), t, t + 4, bx, by, bw, bh, { size: Math.round(bh * 0.42), spacing: 2, anim: "pop" });
      const cs = m * 0.28;
      const cur = P("cursor", tx("Pointer"), t + 0.3, t + 4, W * 0.8, H * 0.9, cs, cs, { states: [{ state: "idle", t0: 0 }, { state: "click", t0: 1.3 }],
        keys: [{ t: r2(t + 0.3), x: Math.round(W * 0.8), y: Math.round(H * 0.9) }, { t: r2(t + 1.2), x: Math.round(bx + bw * 0.6 - cs * (110 / 300)), y: Math.round(by + bh * 0.55 - cs * (70 / 300)), ease: "out" }] });
      const hs = m * 0.3;
      const heart = P("heart", tx("Heart"), t + 1.45, t + 4, bx + bw - hs * 0.2, by - hs * 0.85, hs, hs, { states: [{ state: "burst", t0: 0 }, { state: "beat", t0: 1.3 }], fade: 0 });
      return [btn, lab, heart, cur];
    },
  },
  {
    id: "celebrate", label: tx("Celebration"), icon: PartyPopper, len: 4, puppets: ["confetti", "sparkle"],
    desc: tx("Confetti bursts over the whole frame behind a big title that pops in."),
    make: ({ t, W, H }) => {
      const m = Math.min(W, H);
      return [
        P("confetti", tx("Confetti"), t, t + 3, 0, 0, W, H, { fade: 0 }),
        T(tx("WE DID IT!"), t + 0.2, t + 4, W * 0.05, H * 0.4, W * 0.9, m * 0.2, { size: Math.round(m * 0.12), weight: 900, stroke: "#000000", anim: "pop", animOut: "zoom" }),
        P("sparkle", tx("Sparkles"), t + 0.4, t + 3.5, W * 0.62, H * 0.3, m * 0.3, m * 0.3),
      ];
    },
  },
  {
    id: "countdown", label: tx("Countdown"), icon: Timer, len: 5.5, puppets: ["timer"],
    desc: tx("A ring counts down five seconds under a title."),
    make: ({ t, W, H }) => {
      const m = Math.min(W, H), s = m * 0.42;
      return [
        T(tx("Starting in"), t, t + 5.5, W * 0.1, H / 2 - s / 2 - m * 0.16, W * 0.8, m * 0.12, { size: Math.round(m * 0.06), weight: 600, anim: "slide-down" }),
        P("timer", tx("Countdown"), t, t + 5.5, (W - s) / 2, (H - s) / 2, s, s, { params: { seconds: 5 }, states: [{ state: "run", t0: 0 }, { state: "done", t0: 5 }], anim: "zoom" }),
      ];
    },
  },
  {
    id: "checklist", label: tx("Checklist"), icon: ListChecks, len: 5, puppets: ["check"],
    desc: tx("Three lines, each ticked off one after another."),
    make: ({ t, W, H }) => {
      const m = Math.min(W, H), row = m * 0.14, cs = row * 0.9, x0 = W * 0.12, y0 = H / 2 - row * 1.5;
      return [0, 1, 2].flatMap((i) => [
        P("check", tx("Tick {0}", i + 1), t + 0.6 * i, t + 5, x0, y0 + i * row * 1.15, cs, cs, { anim: "pop" }),
        T([tx("First point"), tx("Second point"), tx("Third point")][i], t + 0.6 * i + 0.2, t + 5, x0 + cs * 1.2, y0 + i * row * 1.15, W * 0.65, cs, { size: Math.round(row * 0.42), align: "left", weight: 700, anim: "slide-right" }),
      ]);
    },
  },
  {
    id: "callout", label: tx("Callout"), icon: Target, len: 4, puppets: [],
    desc: tx("A ring draws round a spot, an arrow points at it and a caption explains it."),
    make: ({ t, W, H }) => {
      const m = Math.min(W, H), r = m * 0.22, cx = W * 0.62, cy = H * 0.42;
      return [
        { id: uid(), kind: "ellipse", t0: r2(t), t1: r2(t + 4), x: Math.round(cx - r / 2), y: Math.round(cy - r / 2), w: Math.round(r), h: Math.round(r), color: "rgba(0,0,0,0)", border: "#e9b84a", borderWidth: Math.round(m * 0.01), anim: "zoom", opacity: 1, fade: 0.3 },
        { id: uid(), kind: "arrow", t0: r2(t + 0.4), t1: r2(t + 4), x: Math.round(cx - r * 1.6), y: Math.round(cy - r * 1.3), w: Math.round(r * 1.0), h: Math.round(r * 0.75), color: "#e9b84a", width: Math.round(m * 0.012), reveal: 0.5, opacity: 1, fade: 0.25 },
        T(tx("Look here"), t + 0.7, t + 4, cx - r * 2.6, cy - r * 1.9, r * 2.2, m * 0.09, { size: Math.round(m * 0.045), color: "#111111", bg: "#ffffff", radius: Math.round(m * 0.03), anim: "pop" }),
      ];
    },
  },
  {
    id: "quote", label: tx("Quote card"), icon: Quote, len: 6, puppets: [],
    desc: tx("A gradient card, a quote typed out in a serif and who said it."),
    make: ({ t, W, H }) => {
      const m = Math.min(W, H), cw = W * 0.84, ch = m * 0.6, cx = (W - cw) / 2, cy = (H - ch) / 2;
      return [
        R(t, t + 6, cx, cy, cw, ch, { bg: "#1d2b64", bg2: "#6a3093", gradAngle: 135, radius: Math.round(m * 0.04), shadow: "rgba(0,0,0,0.5)", shadowBlur: 40, shadowY: 12, anim: "zoom" }),
        T(tx("“Simple things should be simple.”"), t + 0.4, t + 6, cx + cw * 0.08, cy + ch * 0.12, cw * 0.84, ch * 0.55, { size: Math.round(m * 0.06), weight: 500, italic: true, font: 'Georgia, "Times New Roman", serif', typing: "chars", reveal: 1.6 }),
        T(tx("— Somebody wise"), t + 2, t + 6, cx + cw * 0.08, cy + ch * 0.7, cw * 0.84, ch * 0.15, { size: Math.round(m * 0.035), weight: 600, color: "#e7c6ff", align: "right", anim: "slide-left" }),
      ];
    },
  },
  {
    id: "beforeafter", label: tx("Before / after"), icon: Columns2, len: 5, puppets: [],
    desc: tx("The frame split in two halves with a label on each and a line between them."),
    make: ({ t, W, H }) => {
      const m = Math.min(W, H), tag = { size: Math.round(m * 0.04), color: "#111111", radius: 999, upper: true, spacing: 1, anim: "pop" };
      return [
        R(t, t + 5, W / 2 - m * 0.004, 0, m * 0.008, H, { bg: "#ffffff", anim: "wipe", fade: 0.4 }),
        T(tx("Before"), t + 0.3, t + 5, W * 0.25 - m * 0.13, H * 0.06, m * 0.26, m * 0.08, { ...tag, bg: "#ffffff" }),
        T(tx("After"), t + 0.5, t + 5, W * 0.75 - m * 0.13, H * 0.06, m * 0.26, m * 0.08, { ...tag, bg: "#e9b84a" }),
      ];
    },
  },
  {
    id: "intro", label: tx("Title intro"), icon: Clapperboard, len: 4, puppets: [],
    desc: tx("A shaded strip slides in, the title rises, a small line under it draws on."),
    make: ({ t, W, H }) => {
      const m = Math.min(W, H);
      return [
        R(t, t + 4, 0, H * 0.36, W, m * 0.3, { bg: "rgba(0,0,0,0.65)", anim: "wipe", fade: 0.5 }),
        T(tx("Your title"), t + 0.35, t + 4, W * 0.08, H * 0.36 + m * 0.03, W * 0.84, m * 0.16, { size: Math.round(m * 0.1), weight: 900, anim: "slide-up", upper: true }),
        { id: uid(), kind: "progress", t0: r2(t + 0.7), t1: r2(t + 4), x: Math.round(W * 0.35), y: Math.round(H * 0.36 + m * 0.22), w: Math.round(W * 0.3), h: Math.round(m * 0.012), value: 1, color: "#e9b84a", bg: "rgba(0,0,0,0)", reveal: 0.6, fade: 0, opacity: 1 },
      ];
    },
  },
];

/** Place a scene at t (its puppets linked into the film first), as one group. */
export async function placeScene(sc: Scene, film: string | null, L: Layers, c: Ctx): Promise<string> {
  for (const p of sc.puppets) if (film) await tool("puppet_link", { film, name: p }).catch(() => {});
  const t = Math.max(0, Math.min(c.t, c.dur - 0.5));
  const made = sc.make({ ...c, t }).map((x) => ({ ...x, t1: r2(Math.min(c.dur, x.t1)) }));
  const all = [...L.items, ...made];
  const r = group(all, made.map((x) => x.id), t);
  const items = r.items.map((x) => (x.id === r.id ? { ...x, label: sc.label } : x));
  L.update(items);
  return r.id || "";
}

export function SceneGrid({ film, L, t, W, H, dur, say, select }: { film: string | null; L: Layers; t: number; W: number; H: number; dur: number; say: (s: string) => void; select: (ids: string[]) => void }) {
  return (
    <div className="stack" style={{ gap: 6 }}>
      <span className="sub-h">{tx("Scenes")}</span>
      <div className="tiles">
        {SCENES.map((sc) => {
          const I = sc.icon;
          return (
            <button key={sc.id} className="tile" title={sc.desc} onClick={async () => { const id = await placeScene(sc, film, L, { t, W, H, dur }); if (id) select([id]); say(tx("\"{0}\" placed as a group: click a part of it again to change that part alone", sc.label)); }}>
              <I /><span>{sc.label}</span>
            </button>
          );
        })}
      </div>
    </div>
  );
}

