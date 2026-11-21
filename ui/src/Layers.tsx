// Layers over a film or a reel: drag, resize from any side, turn, snap to the frame and to each other,
// edit text in place, drop pictures and library items, keyframe and rig.
// The same item format as linnea2d.js draws (tweaks.json items2d); evalItem mirrors its motion exactly.
import { tx } from "./i18n";
import { useEffect, useRef, useState } from "react";
import { Json, tool } from "./api";
import { evalItem, Item } from "./Motion";
import { copyItems, duplicate, hasClip, layerName, pasteItems, r2, remove, reorder, replace, setProp, shift, split, trim, uid } from "./LayerOps";

export { evalItem } from "./Motion";
export type { Item, Key } from "./Motion";

/** Layers of a film (`hub_2d`, {film}) or of a reel (`reel_layers`, {project, reel}). */
export function useLayers(toolName: string, args: Json, push: (items: Item[]) => void, before?: (prev: Item[]) => void) {
  const [items, setItems] = useState<Item[]>([]);
  const [installed, setInstalled] = useState(true);
  const [url, setUrl] = useState("");
  const timer = useRef<any>(0);
  const cur = useRef<Item[]>([]);
  cur.current = items;
  const frame = useRef(0);
  const key = JSON.stringify(args);
  const [loaded, setLoaded] = useState(false);
  const load = () => tool(toolName, args).then((r) => { setItems(r.items || []); setInstalled(r.installed !== false); setUrl(r.url || ""); setLoaded(true); }).catch(() => {});
  useEffect(() => { load(); }, [toolName, key]);
  // live picture while dragging: no React state, no save, at most once per frame
  const preview = (next: Item[]) => {
    cancelAnimationFrame(frame.current);
    frame.current = requestAnimationFrame(() => push(next));
  };
  const update = (next: Item[], silent = false) => {
    if (!silent) before?.(cur.current);
    cancelAnimationFrame(frame.current);
    setItems(next);
    push(next);
    clearTimeout(timer.current);
    timer.current = setTimeout(() => tool(toolName, { ...args, items: next }), 350);
  };
  const addImage = async (path: string) => (await tool(toolName, { ...args, add_image: path })).src as string;
  const upload = (file: File) => new Promise<string>((ok, no) => {
    const r = new FileReader();
    r.onload = () => tool(toolName, { ...args, upload: { name: file.name, data: r.result } }).then((x) => ok(x.src)).catch(no);
    r.readAsDataURL(file);
  });
  const install = () => tool(toolName, { ...args, install: true }).then(load);
  return { items, update, preview, installed, install, addImage, upload, url, reload: load, loaded };
}
export type Layers = ReturnType<typeof useLayers>;

/** What can be added: [kind, label, make(t, dur, W, H)]. */
export function newItem(kind: string, t: number, dur: number, W: number, H: number): Item {
  const id = uid(kind === "puppet" ? "p" : "l");
  const m = Math.min(W, H);
  const b: Item = { id, kind, t0: r2(t), t1: r2(Math.min(dur, t + 3)), x: Math.round(W * 0.1), y: Math.round(H * 0.68), w: Math.round(W * 0.8), h: Math.round(m * 0.14), opacity: 1, fade: 0.25 };
  if (b.t1 - b.t0 < 0.5) b.t0 = r2(Math.max(0, b.t1 - 3));
  switch (kind) {
    case "text": return { ...b, text: tx("Title"), size: Math.round(m * 0.08), color: "#ffffff", weight: 800, stroke: "#000000" };
    case "subtitle": return { ...b, kind: "text", y: Math.round(H * 0.8), h: Math.round(m * 0.1), text: tx("A line of subtitle"), size: Math.round(m * 0.045), color: "#ffffff", weight: 600, shadow: "rgba(0,0,0,0.6)", shadowBlur: 14, shadowY: 3 };
    case "caption": return { ...b, kind: "text", x: Math.round(W * 0.2), w: Math.round(W * 0.6), h: Math.round(m * 0.11), text: tx("Caption"), size: Math.round(m * 0.05), color: "#111111", weight: 700, bg: "#ffffff", radius: Math.round(m * 0.03), anim: "pop" };
    case "typing": return { ...b, kind: "text", text: tx("Typed out letter by letter"), size: Math.round(m * 0.06), color: "#ffffff", weight: 700, typing: "chars", reveal: 1.5, align: "left" };
    case "rect": return { ...b, bg: "#e9b84a", radius: 24 };
    case "ellipse": return { ...b, kind: "ellipse", x: Math.round(W / 2 - m * 0.15), y: Math.round(H / 2 - m * 0.15), w: Math.round(m * 0.3), h: Math.round(m * 0.3), bg: undefined, color: "#e9b84a", anim: "pop" };
    case "ring": return { ...b, kind: "ellipse", x: Math.round(W / 2 - m * 0.15), y: Math.round(H / 2 - m * 0.15), w: Math.round(m * 0.3), h: Math.round(m * 0.3), color: "rgba(0,0,0,0)", border: "#e9b84a", borderWidth: Math.round(m * 0.012), anim: "zoom" };
    case "gradient": return { ...b, kind: "rect", x: 0, y: 0, w: W, h: H, bg: "#1d2b64", bg2: "#f8cdda", gradAngle: 135, radius: 0, t0: 0, t1: r2(dur), fade: 0 };
    case "scrim": return { ...b, kind: "rect", x: 0, y: Math.round(H * 0.55), w: W, h: Math.round(H * 0.45), bg: "rgba(0,0,0,0)", bg2: "rgba(0,0,0,0.75)", gradAngle: 180, radius: 0 };
    case "counter": return { ...b, from: 0, to: 1000, prefix: "", suffix: "", size: Math.round(m * 0.12), color: "#ffffff", weight: 800, reveal: 1.2 };
    case "marker": return { ...b, text: tx("Note"), size: Math.round(m * 0.09), color: "#e9b84a", reveal: 1.0 };
    case "arrow": return { ...b, x: Math.round(W * 0.3), y: Math.round(H * 0.4), w: Math.round(W * 0.3), h: Math.round(H * 0.1), color: "#e9b84a", width: Math.round(m * 0.012), reveal: 0.6 };
    case "phone": return { ...b, x: Math.round(W * 0.25), y: Math.round(H * 0.2), w: Math.round(W * 0.5), h: Math.round(W * 1.05), title: tx("Linnea"), text: tx("New notification"), reveal: 0.8 };
    case "lower": return { ...b, x: Math.round(W * 0.06), y: Math.round(H * 0.72), w: Math.round(W * 0.7), h: Math.round(m * 0.13), text: tx("Full Name"), sub: tx("Job title"), reveal: 0.6 };
    case "progress": return { ...b, y: Math.round(H * 0.9), h: Math.round(m * 0.025), value: 1, color: "#e9b84a", reveal: Math.max(0.5, b.t1 - b.t0), fade: 0 };
    case "image": return { ...b, x: Math.round(W * 0.25), y: Math.round(H * 0.3), w: Math.round(W * 0.5), h: Math.round(W * 0.5), fit: "cover" };
    default: return b;
  }
}

/** Something dragged from the Add panel onto the frame: the panel puts its handler here. */
export const DROP: { fn: null | ((payload: Json, x: number, y: number) => void) } = { fn: null };

/** A drop on the frame at (px, py): pictures, text files, or an item from the Add panel. */
export async function dropOnFrame(e: React.DragEvent, px: number, py: number, L: Layers, t: number, W: number, H: number, select?: (id: string) => void) {
  e.preventDefault();
  const payload = e.dataTransfer.getData("application/x-linnea");
  if (payload) {
    try { DROP.fn?.(JSON.parse(payload), px, py); } catch { /* not ours */ }
    return;
  }
  const f = e.dataTransfer.files?.[0];
  if (!f) return;
  if (f.type.startsWith("image/")) {
    const src = await L.upload(f);
    const w = Math.round(W * 0.4);
    const it: Item = { id: uid(), kind: "image", src, label: f.name.replace(/\.[^.]+$/, ""), t0: r2(t), t1: r2(t + 3), x: Math.round(px - w / 2), y: Math.round(py - w / 2), w, h: w, opacity: 1, fade: 0.25, fit: "cover" };
    L.update([...L.items, it]);
    select?.(it.id);
  } else if (f.type.startsWith("text/")) {
    const text = (await f.text()).slice(0, 200);
    const it = { ...newItem("text", t, t + 3, W, H), text, x: Math.round(px - W * 0.4), y: Math.round(py - 60) };
    L.update([...L.items, it]);
    select?.(it.id);
  }
}

type Guide = { x?: number; y?: number };
const HANDLES: [number, number, string][] = [[-1, -1, "nwse"], [0, -1, "ns"], [1, -1, "nesw"], [1, 0, "ew"], [1, 1, "nwse"], [0, 1, "ns"], [-1, 1, "nesw"], [-1, 0, "ew"]];

/** Edit boxes over the picture: drag to move (Shift: one axis), any handle to resize (Shift: keep the shape,
 *  Alt: from the centre), the round handle to turn (Shift: 15° steps). Edges snap to the frame's edges and
 *  centre and to the other layers; Alt while dragging turns snapping off. Shift-click selects several. */
export function LayerOverlay({ L, t, sel, setSel, W, H, passThrough, pathFor, multi, setMulti, snap = true, others, onMenu }: {
  L: Layers; t: number; sel: string; setSel: (s: string) => void; W: number; H: number; passThrough?: boolean; pathFor?: string;
  multi?: string[]; setMulti?: (ids: string[]) => void; snap?: boolean; others?: { x: number; y: number; w: number; h: number }[]; onMenu?: (x: number, y: number, id: string) => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [editing, setEditing] = useState("");
  const [ghost, setGhost] = useState<Item[] | null>(null);
  const [guides, setGuides] = useState<Guide[]>([]);
  const [readout, setReadout] = useState("");
  const [k, setK] = useState(2);
  useEffect(() => {
    const ro = new ResizeObserver(() => { const r = ref.current?.getBoundingClientRect(); if (r?.width) setK(W / r.width); });
    if (ref.current) ro.observe(ref.current);
    return () => ro.disconnect();
  }, [W]);
  const items = ghost || L.items;
  const chosen = multi && multi.length ? multi : sel ? [sel] : [];
  const pick = (id: string, add: boolean) => {
    if (add && setMulti) {
      const base = chosen.includes(id) ? chosen.filter((x) => x !== id) : [...chosen, id];
      setMulti(base);
      setSel(base.includes(id) ? id : base[base.length - 1] || "");
    } else if (!chosen.includes(id) || chosen.length < 2) {
      setMulti?.([id]);
      setSel(id);
    } else setSel(id);
  };
  const begin = (e: React.PointerEvent, it: Item, mode: "move" | "size" | "rot", hx = 0, hy = 0) => {
    if (e.button !== 0) return;
    e.stopPropagation();
    const additive = e.shiftKey && mode === "move";
    pick(it.id, additive);
    if (additive) return;
    const base = L.items;
    const ids = mode === "move" && chosen.includes(it.id) ? chosen : [it.id];
    const movers = base.filter((x) => ids.includes(x.id) && !x.locked);
    if (!movers.length) return;
    const s0 = evalItem(base, it, t);
    const own0 = evalItem(base, { ...it, parent: undefined }, t);
    const [x0, y0] = [e.clientX, e.clientY];
    const box = (e.currentTarget as HTMLElement).closest("[data-layer]")?.getBoundingClientRect();
    const [cx, cy] = box ? [box.left + box.width / 2, box.top + box.height / 2] : [x0, y0];
    const a0 = Math.atan2(y0 - cy, x0 - cx);
    // what the moving edges can snap to
    const vis = base.filter((x) => !ids.includes(x.id) && !x.hidden && t >= x.t0 && t < x.t1).map((x) => evalItem(base, x, t)).concat(others || []);
    const VX = [0, W / 2, W, ...vis.flatMap((b) => [b.x, b.x + b.w / 2, b.x + b.w])];
    const VY = [0, H / 2, H, ...vis.flatMap((b) => [b.y, b.y + b.h / 2, b.y + b.h])];
    let last: Item[] | null = null;
    const mv = (ev: PointerEvent) => {
      let dx = (ev.clientX - x0) * k, dy = (ev.clientY - y0) * k;
      if (!last && Math.abs(dx) + Math.abs(dy) < 3 * k) return;   // a click, not a drag
      const g: Guide[] = [];
      let next: Item[];
      if (mode === "move") {
        if (ev.shiftKey) { if (Math.abs(dx) > Math.abs(dy)) dy = 0; else dx = 0; }
        if (snap && !ev.altKey) {
          const th = 7 * k;
          const best = (vals: number[], at: number[]) => { let d = Infinity, v = 0; for (const a of at) for (const c of vals) if (Math.abs(c - a) < Math.abs(d)) { d = c - a; v = c; } return Math.abs(d) <= th ? { d, v } : null; };
          const bx = best(VX, [s0.x + dx, s0.x + dx + s0.w / 2, s0.x + dx + s0.w]), by = best(VY, [s0.y + dy, s0.y + dy + s0.h / 2, s0.y + dy + s0.h]);
          if (bx && !(ev.shiftKey && dx === 0)) { dx += bx.d; g.push({ x: bx.v }); }
          if (by && !(ev.shiftKey && dy === 0)) { dy += by.d; g.push({ y: by.v }); }
        }
        next = base.map((x) => (movers.includes(x) ? shift(base, x, t, Math.round(dx), Math.round(dy)) : x));
        setReadout(`${Math.round(s0.x + dx)}, ${Math.round(s0.y + dy)}`);
      } else if (mode === "size") {
        const th = (s0.rot * Math.PI) / 180, sc = s0.scale || 1, cs = Math.cos(th), sn = Math.sin(th);
        const lx = (dx * cs + dy * sn) / sc, ly = (-dx * sn + dy * cs) / sc;
        let dw = hx * lx, dh = hy * ly;
        const corner = hx !== 0 && hy !== 0;
        const keep = (it.kind === "image" || it.kind === "puppet" || it.kind === "ellipse" && it.w === it.h) !== ev.shiftKey;
        if (keep) {
          if (corner) { const f = Math.abs(dw / s0.w) > Math.abs(dh / s0.h) ? (s0.w + dw) / s0.w : (s0.h + dh) / s0.h; dw = s0.w * (f - 1); dh = s0.h * (f - 1); }
          else if (hx === 0) dw = s0.w * (dh / s0.h); else dh = s0.h * (dw / s0.w);
        }
        if (ev.altKey) { dw *= 2; dh *= 2; }
        const w = Math.max(8, s0.w + dw), h = Math.max(8, s0.h + dh);
        dw = w - s0.w; dh = h - s0.h;
        const cl = ev.altKey ? 0 : (hx * dw) / 2, cm = ev.altKey ? 0 : (hy * dh) / 2;
        const ncx = s0.x + s0.w / 2 + (cl * cs - cm * sn) * sc, ncy = s0.y + s0.h / 2 + (cl * sn + cm * cs) * sc;
        let n = it;
        const P = (p: string, v: number) => { n = setProp(base, n, t, p, v); };
        P("w", Math.round(w)); P("h", Math.round(h));
        if (!(Array.isArray(it.path) && it.path.length)) { P("x", Math.round(own0.x + (ncx - w / 2 - s0.x))); P("y", Math.round(own0.y + (ncy - h / 2 - s0.y))); }
        next = base.map((x) => (x.id === it.id ? n : x));
        setReadout(`${Math.round(w)} × ${Math.round(h)}`);
      } else {
        const a = Math.atan2(ev.clientY - cy, ev.clientX - cx);
        let rot = own0.rot + ((a - a0) * 180) / Math.PI;
        rot = ev.shiftKey ? Math.round(rot / 15) * 15 : Math.round(rot * 10) / 10;
        if (rot > 180) rot -= 360; else if (rot < -180) rot += 360;
        next = base.map((x) => (x.id === it.id ? setProp(base, x, t, "rot", rot) : x));
        setReadout(`${rot}°`);
      }
      setGuides(g);
      last = next;
      setGhost(next);
      L.preview(next);
    };
    const up = () => {
      window.removeEventListener("pointermove", mv);
      window.removeEventListener("pointerup", up);
      if (last) L.update(last);
      setGhost(null);
      setGuides([]);
      setReadout("");
    };
    window.addEventListener("pointermove", mv);
    window.addEventListener("pointerup", up);
  };
  const lw = 1.5 * k;
  // drawing a motion path: every click on the picture is the next point
  const down = (e: React.PointerEvent) => {
    if (pathFor) {
      const r = ref.current!.getBoundingClientRect();
      const pt = [Math.round(((e.clientX - r.left) / r.width) * W), Math.round(((e.clientY - r.top) / r.height) * H)];
      L.update(L.items.map((x) => (x.id === pathFor ? { ...x, path: [...(x.path || []), pt] } : x)));
      return;
    }
    setSel("");
    setMulti?.([]);
    setEditing("");
  };
  const drop = (e: React.DragEvent) => {
    const r = ref.current!.getBoundingClientRect();
    dropOnFrame(e, ((e.clientX - r.left) / r.width) * W, ((e.clientY - r.top) / r.height) * H, L, t, W, H, (id) => { setSel(id); setMulti?.([id]); });
  };
  const shown = items.filter((x) => Array.isArray(x.path) && x.path.length && (x.id === sel || x.id === pathFor));
  const single = chosen.length <= 1;
  return (
    <div ref={ref} style={{ position: "absolute", inset: 0, pointerEvents: passThrough && !pathFor ? "none" : undefined, cursor: pathFor ? "crosshair" : undefined }} onPointerDown={down} onDragOver={(e) => e.preventDefault()} onDrop={drop}>
      {(shown.length > 0 || guides.length > 0) && (
        <svg width={W} height={H} viewBox={`0 0 ${W} ${H}`} style={{ position: "absolute", inset: 0, pointerEvents: "none", overflow: "visible", zIndex: 3 }}>
          {shown.map((x) => (
            <g key={x.id}>
              <polyline points={x.path.map((p: number[]) => p.join(",")).join(" ")} fill="none" style={{ stroke: "var(--accent)" }} strokeWidth={lw * 2} strokeDasharray={`${lw * 6} ${lw * 4}`} />
              {x.path.map((p: number[], i: number) => <circle key={i} cx={p[0]} cy={p[1]} r={lw * 5} style={{ fill: i === 0 ? "#86b991" : "var(--accent)" }} />)}
            </g>
          ))}
          {guides.map((g, i) => (g.x !== undefined ? <line key={i} x1={g.x} x2={g.x} y1={0} y2={H} stroke="#ff4fa3" strokeWidth={lw} /> : <line key={i} y1={g.y} y2={g.y} x1={0} x2={W} stroke="#ff4fa3" strokeWidth={lw} />))}
        </svg>
      )}
      {items.filter((x) => t >= x.t0 && t < x.t1).map((x) => {
        const s = evalItem(items, x, t);
        const on = chosen.includes(x.id);
        const main = x.id === sel;
        const col = x.hidden ? "rgba(160,160,160,.7)" : on ? "var(--accent)" : "rgba(255,255,255,.45)";
        const hs = (11 * k) / (s.scale || 1);
        const bw = lw / (s.scale || 1);
        return (
          <div key={x.id} data-layer={x.id} onPointerDown={(e) => begin(e, x, "move")} onContextMenu={(e) => { if (!onMenu) return; e.preventDefault(); e.stopPropagation(); if (!on) pick(x.id, false); onMenu(e.clientX, e.clientY, x.id); }}
            onDoubleClick={() => ["text", "marker", "lower"].includes(x.kind) && !x.locked && setEditing(x.id)}
            title={layerName(x)}
            style={{ pointerEvents: pathFor || x.locked ? "none" : "auto", position: "absolute", left: s.x, top: s.y, width: s.w, height: s.h, outline: `${bw}px ${x.hidden ? "dotted" : on ? "solid" : "dashed"} ${col}`, background: x.hidden ? "repeating-linear-gradient(45deg, rgba(160,160,160,.12) 0 8px, transparent 8px 16px)" : undefined, cursor: "move", transform: `rotate(${s.rot}deg) scale(${s.scale})`, zIndex: main ? 2 : 1 }}>
            {main && single && !x.locked && HANDLES.map(([hx, hy, c]) => (
              <div key={`${hx}${hy}`} onPointerDown={(e) => begin(e, x, "size", hx, hy)}
                style={{ position: "absolute", left: `calc(${((hx + 1) / 2) * 100}% - ${hs / 2}px)`, top: `calc(${((hy + 1) / 2) * 100}% - ${hs / 2}px)`, width: hs, height: hs, background: "#fff", border: `${bw}px solid var(--accent)`, borderRadius: hx && hy ? hs / 4 : hs / 2, cursor: `${c}-resize`, boxSizing: "border-box" }} />
            ))}
            {main && single && !x.locked && (
              <>
                <div style={{ position: "absolute", left: "50%", top: -hs * 2.4, width: bw, height: hs * 2.4, background: "var(--accent)", pointerEvents: "none" }} />
                <div title={tx("Turn (Shift: 15° steps)")} onPointerDown={(e) => begin(e, x, "rot")} style={{ position: "absolute", left: `calc(50% - ${hs * 0.6}px)`, top: -hs * 3.4, width: hs * 1.2, height: hs * 1.2, borderRadius: "50%", background: "var(--accent)", border: `${bw}px solid #fff`, cursor: "grab", boxSizing: "border-box" }} />
              </>
            )}
            {main && readout && <div style={{ position: "absolute", left: 0, top: "100%", marginTop: hs, transform: `scale(${k / (s.scale || 1)})`, transformOrigin: "0 0", font: "600 11px var(--mono)", background: "var(--accent)", color: "var(--on-accent)", padding: "1px 6px", borderRadius: 4, whiteSpace: "nowrap", pointerEvents: "none" }}>{readout}</div>}
            {editing === x.id && (
              <textarea autoFocus defaultValue={x.text} onPointerDown={(e) => e.stopPropagation()}
                onBlur={(e) => { L.update(L.items.map((y) => (y.id === x.id ? { ...y, text: e.target.value } : y))); setEditing(""); }}
                onKeyDown={(e) => { e.stopPropagation(); if (e.key === "Escape" || (e.key === "Enter" && !e.shiftKey)) (e.target as HTMLTextAreaElement).blur(); }}
                style={{ position: "absolute", inset: 0, width: "100%", height: "100%", font: `${x.weight || 800} ${x.size || 72}px "Segoe UI", sans-serif`, textAlign: (x.align as any) || "center", color: "#fff", background: "rgba(0,0,0,.55)", border: 0, outline: `${lw * 2}px solid var(--accent)`, resize: "none", padding: 0 }} />
            )}
          </div>
        );
      })}
    </div>
  );
}

/** Keys for the selected layers, wherever the picture is being edited: Delete, Ctrl+D duplicate, Ctrl+C / X / V,
 *  arrows nudge (Shift: 10 px), Ctrl+] / [ order, Alt+[ / ] trim to the playhead, S split, H hide, Ctrl+L lock,
 *  Ctrl+A select all at this second, Esc deselect. Runs before the page's own keys, and only when it acts. */
export function useLayerKeys({ L, t, sel, setSel, multi, setMulti, active, say }: { L: Layers; t: number; sel: string; setSel: (s: string) => void; multi?: string[]; setMulti?: (ids: string[]) => void; active: boolean; say?: (s: string) => void }) {
  const st = useRef({ L, t, sel, multi, active });
  st.current = { L, t, sel, multi, active };
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      const { L, t, sel, multi, active } = st.current;
      if (!active || /INPUT|TEXTAREA|SELECT/.test((e.target as HTMLElement).tagName) || (e.target as HTMLElement).isContentEditable) return;
      const ids = multi && multi.length ? multi : sel ? [sel] : [];
      const mod = e.ctrlKey || e.metaKey;
      const items = L.items;
      const done = () => { e.preventDefault(); e.stopImmediatePropagation(); };
      const choose = (list: string[]) => { setSel(list[list.length - 1] || ""); setMulti?.(list); };
      const k = e.key.toLowerCase();
      if (mod && k === "v") {
        const got = pasteItems(t);
        if (!got.length) return;
        done();
        L.update([...items, ...got]);
        choose(got.map((x) => x.id));
        say?.(tx("{0} layers pasted", got.length));
        return;
      }
      if (mod && k === "a" && !e.shiftKey) { done(); choose(items.filter((x) => t >= x.t0 && t < x.t1 && !x.locked).map((x) => x.id)); return; }
      if (!ids.length) return;
      const picked = items.filter((x) => ids.includes(x.id));
      if (e.key === "Delete" || e.key === "Backspace") { done(); L.update(remove(items, ids)); choose([]); }
      else if (e.key === "Escape") { done(); choose([]); }
      else if (mod && k === "d") { done(); const r = duplicate(items, ids); L.update(r.items); choose(r.ids); }
      else if (mod && k === "c") { done(); copyItems(picked); say?.(tx("{0} layers copied", picked.length)); }
      else if (mod && k === "x") { done(); copyItems(picked); L.update(remove(items, ids)); choose([]); }
      else if (mod && k === "l") { done(); const lock = !picked.every((x) => x.locked); L.update(replace(items, ids, (x) => ({ ...x, locked: lock || undefined }))); }
      else if (mod && (e.key === "]" || e.key === "[")) { done(); L.update(reorder(items, ids, e.key === "]" ? (e.shiftKey ? "top" : "up") : e.shiftKey ? "bottom" : "down")); }
      else if (e.altKey && (e.key === "[" || e.key === "]")) { done(); L.update(trim(items, ids, t, e.key === "[" ? "start" : "end")); }
      else if (!mod && !e.altKey && k === "s" && ids.length === 1) { const r = split(items, ids[0], t); if (r.id) { done(); L.update(r.items); choose([r.id]); } }
      else if (!mod && !e.altKey && k === "h") { done(); const hide = !picked.every((x) => x.hidden); L.update(replace(items, ids, (x) => ({ ...x, hidden: hide || undefined }))); }
      else if (!mod && e.key.startsWith("Arrow")) {
        done();
        const d = e.shiftKey ? 10 : 1;
        const dx = e.key === "ArrowLeft" ? -d : e.key === "ArrowRight" ? d : 0, dy = e.key === "ArrowUp" ? -d : e.key === "ArrowDown" ? d : 0;
        L.update(items.map((x) => (ids.includes(x.id) && !x.locked ? shift(items, x, t, dx, dy) : x)));
      }
    };
    window.addEventListener("keydown", key, true);
    return () => window.removeEventListener("keydown", key, true);
  }, []);
  return { canPaste: hasClip };
}

/** A W x H stage scaled to fill its parent's width (for pages drawn over a scaled video). */
export function ScaledStage({ W, H, children }: { W: number; H: number; children: any }) {
  const ref = useRef<HTMLDivElement>(null);
  const [s, setS] = useState(0.3);
  useEffect(() => {
    const ro = new ResizeObserver((e) => setS(e[0].contentRect.width / W));
    if (ref.current) ro.observe(ref.current);
    return () => ro.disconnect();
  }, [W]);
  return (
    <div ref={ref} style={{ position: "absolute", inset: 0, overflow: "hidden" }}>
      <div style={{ position: "absolute", left: 0, top: 0, width: W, height: H, transform: `scale(${s})`, transformOrigin: "0 0" }}>{children}</div>
    </div>
  );
}
