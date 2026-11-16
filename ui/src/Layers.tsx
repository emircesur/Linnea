// Layers over a film or a reel: drag, resize, edit text in place, drop pictures, keyframe and rig.
// The same item format as kesit2d.js draws (tweaks.json items2d); evalItem mirrors its motion exactly.
import { tx } from "./i18n";
import { useEffect, useRef, useState } from "react";
import { ArrowUpRight, Diamond, Eye, EyeOff, Gauge, Hash, Image as ImageIcon, PenLine, Smartphone, Square, Trash2, Type, Rows3 } from "lucide-react";
import { Json, tool } from "./api";

export type Key = { t: number; x?: number; y?: number; w?: number; h?: number; rot?: number; scale?: number; opacity?: number; ease?: string };
export type Item = { id: string; kind: string; t0: number; t1: number; x: number; y: number; w: number; h: number; keys?: Key[]; parent?: string; hidden?: boolean; [k: string]: any };

const r2 = (x: number) => Math.round(x * 100) / 100;
const PROPS = ["x", "y", "w", "h", "rot", "scale", "opacity"] as const;
const EASE: Record<string, (k: number) => number> = {
  linear: (k) => k,
  out: (k) => 1 - Math.pow(1 - k, 3),
  in: (k) => k * k * k,
  inOut: (k) => (k < 0.5 ? 4 * k * k * k : 1 - Math.pow(-2 * k + 2, 3) / 2),
  back: (k) => { const c = 1.7; return 1 + (c + 1) * Math.pow(k - 1, 3) + c * Math.pow(k - 1, 2); },
};

/** A point k (0..1) of the way along a polyline, by length, and the heading there (as kesit2d.js). */
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

const KINDS: [string, string, any][] = [
  ["text", tx("Text"), Type], ["image", tx("Picture"), ImageIcon], ["rect", tx("Box"), Square], ["counter", tx("Counter"), Hash], ["marker", tx("Handwriting"), PenLine],
  ["arrow", "Ok", ArrowUpRight], ["phone", tx("Phone"), Smartphone], ["lower", tx("Lower third"), Rows3], ["progress", tx("Progress"), Gauge],
];

function newItem(kind: string, t: number, dur: number, W: number, H: number): Item {
  const id = "l" + Date.now().toString(36);
  const m = Math.min(W, H);
  const b: Item = { id, kind, t0: r2(t), t1: r2(Math.min(dur, t + 3)), x: W * 0.1, y: H * 0.68, w: W * 0.8, h: m * 0.12, opacity: 1, fade: 0.25 };
  switch (kind) {
    case "text": return { ...b, text: tx("Title"), size: Math.round(m * 0.08), color: "#ffffff", weight: 800, stroke: "#000000" };
    case "rect": return { ...b, bg: "#e9b84a", radius: 24 };
    case "counter": return { ...b, from: 0, to: 1000, prefix: "", suffix: "", size: Math.round(m * 0.12), color: "#ffffff", weight: 800, reveal: 1.2 };
    case "marker": return { ...b, text: tx("Note"), size: Math.round(m * 0.09), color: "#e9b84a", reveal: 1.0 };
    case "arrow": return { ...b, x: W * 0.3, y: H * 0.4, w: W * 0.3, h: H * 0.1, color: "#e9b84a", width: Math.round(m * 0.012), reveal: 0.6 };
    case "phone": return { ...b, x: W * 0.25, y: H * 0.2, w: W * 0.5, h: W * 1.05, title: tx("Linnea"), text: tx("New notification"), reveal: 0.8 };
    case "lower": return { ...b, x: W * 0.06, y: H * 0.72, w: W * 0.7, h: m * 0.13, text: tx("Full Name"), sub: tx("Job title"), reveal: 0.6 };
    case "progress": return { ...b, y: H * 0.9, h: m * 0.025, value: 1, color: "#e9b84a", reveal: Math.max(0.5, b.t1 - b.t0), fade: 0 };
    default: return b;
  }
}

export function LayersPanel({ L, t, W, H, dur, sel, setSel, pathFor, setPathFor }: { L: Layers; t: number; W: number; H: number; dur: number; sel: string; setSel: (s: string) => void; pathFor?: string; setPathFor?: (s: string) => void }) {
  const items = L.items;
  const it = items.find((x) => x.id === sel);
  const set = (p: Partial<Item>) => L.update(items.map((x) => (x.id === sel ? { ...x, ...p } : x)));
  const add = async (kind: string) => {
    const b = newItem(kind, t, dur, W, H);
    if (kind === "image") {
      const r = await tool("pick_file");
      if (!r.path) return;
      b.src = await L.addImage(r.path);
      b.w = W * 0.5; b.h = W * 0.5; b.x = W * 0.25; b.y = H * 0.3;
    }
    L.update([...items, b]);
    setSel(b.id);
  };
  const keyHere = () => {
    if (!it) return;
    const s = evalItem(items, it, t);
    const keys = (it.keys || []).filter((k) => Math.abs(k.t - t) > 0.02);
    set({ keys: [...keys, { t: r2(t), x: Math.round(s.x), y: Math.round(s.y), w: Math.round(s.w), h: Math.round(s.h), rot: r2(s.rot), scale: r2(s.scale), opacity: r2(s.opacity), ease: "inOut" }].sort((a, b) => a.t - b.t) });
  };
  const num = (k: string, label: string, step = 1) => (
    <div className="field"><label>{label}</label><input className="input num" type="number" step={step} key={`${sel}${k}${it?.[k]}`} defaultValue={it?.[k] ?? ""} onBlur={(e) => set({ [k]: e.target.value === "" ? undefined : +e.target.value })} /></div>
  );
  const str = (k: string, label: string) => (
    <div className="field"><label>{label}</label><input className="input" key={`${sel}${k}`} defaultValue={it?.[k] ?? ""} onChange={(e) => set({ [k]: e.target.value })} /></div>
  );
  const color = (k: string, label: string) => (
    <div className="field"><label>{label}</label><div className="rowx"><input className="input" type="color" style={{ padding: 2, width: 40 }} value={it?.[k] || "#000000"} onChange={(e) => set({ [k]: e.target.value })} /><button className="btn icon sm ghost" title={tx("None")} onClick={() => set({ [k]: undefined })}><Trash2 /></button></div></div>
  );
  const textual = it && ["text", "marker", "counter", "lower"].includes(it.kind);
  return (
    <div className="stack" style={{ gap: 12 }}>
      {!L.installed && (
        <div className="card" style={{ padding: 12 }}>
          <p className="sec" style={{ fontSize: 12, marginBottom: 8 }}>{tx("This page has no layer hook. Once installed, layers are drawn in the preview and in renders.")}</p>
          <button className="btn sm" onClick={L.install}>{tx("Install hook")}</button>
        </div>
      )}
      <div style={{ display: "grid", gridTemplateColumns: "repeat(3, 1fr)", gap: 6 }}>
        {KINDS.map(([k, l, I]) => <button key={k} className="btn sm" onClick={() => add(k)}><I />{l}</button>)}
      </div>
      <div className="list-rows">
        {items.map((x) => (
          <div key={x.id} className="rowx" style={{ padding: "6px 0", cursor: "pointer", color: x.id === sel ? "var(--text)" : "var(--text-2)" }} onClick={() => setSel(x.id)}>
            <span className="tag">{KINDS.find((k) => k[0] === x.kind)?.[1] || x.kind}</span>
            <span className="grow" style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", fontSize: 12 }}>{x.text || x.title || x.src || ""}</span>
            {x.keys?.length ? <Diamond size={12} style={{ color: "var(--accent-ink)" }} /> : null}
            <span className="num muted" style={{ fontSize: 11 }}>{x.t0}–{x.t1}</span>
            <button className="btn icon sm ghost" title={x.hidden ? tx("Show") : tx("Hide")} onClick={(e) => { e.stopPropagation(); L.update(items.map((y) => (y.id === x.id ? { ...y, hidden: !y.hidden } : y))); }}>{x.hidden ? <EyeOff /> : <Eye />}</button>
          </div>
        ))}
      </div>
      {!items.length && <p className="muted" style={{ fontSize: 12, lineHeight: "18px" }}>{tx("Pick a kind, or drop a picture onto the frame. Drag a box to move it, resize from the corner, double-click text to change it in place.")}</p>}
      {it && (
        <div className="stack" style={{ gap: 8 }}>
          {textual && it.kind !== "counter" && <textarea className="input" style={{ fontFamily: "var(--font)", fontSize: 13 }} rows={2} key={sel} defaultValue={it.text} onChange={(e) => set({ text: e.target.value })} />}
          {it.kind === "lower" && str("sub", tx("Second line"))}
          {it.kind === "phone" && <div className="grid2">{str("title", tx("Notification title"))}{str("text", tx("Notification"))}</div>}
          {it.kind === "counter" && <div className="grid2">{num("from", tx("Start"))}{num("to", tx("End"))}{str("prefix", tx("Prefix"))}{str("suffix", tx("Suffix"))}</div>}
          {it.kind === "progress" && num("value", tx("Fill 0–1"), 0.05)}
          <div className="grid2">{num("t0", tx("Start, s"), 0.1)}{num("t1", tx("End, s"), 0.1)}</div>
          {(textual || it.kind === "phone") && <div className="grid2">{num("size", tx("Size, px"))}{num("weight", tx("Weight"), 100)}</div>}
          <div className="grid2">{color("color", tx("Colour"))}{color("bg", tx("Background"))}{textual && color("stroke", tx("Outline"))}</div>
          <div className="grid2">{num("radius", tx("Corner"))}{num("rot", tx("Angle"))}{num("scale", tx("Scale"), 0.05)}{num("opacity", tx("Opacity"), 0.05)}</div>
          <div className="grid2">
            {num("fade", tx("Transition, s"), 0.05)}
            {["counter", "marker", "arrow", "phone", "lower", "progress"].includes(it.kind) && num("reveal", tx("Draw-on, s"), 0.1)}
          </div>
          <div className="grid2">
            <div className="field"><label>{tx("In / out")}</label><select className="select" value={it.anim || "fade"} onChange={(e) => set({ anim: e.target.value })}><option value="fade">{tx("Fade")}</option><option value="slide-up">{tx("Slide up")}</option><option value="slide-left">{tx("Slide in")}</option><option value="pop">{tx("Pop")}</option></select></div>
            <div className="field"><label>{tx("Attached to (rig)")}</label><select className="select" value={it.parent || ""} onChange={(e) => set({ parent: e.target.value || undefined })}><option value="">{tx("None")}</option>{items.filter((x) => x.id !== sel).map((x) => <option key={x.id} value={x.id}>{x.text || x.kind} · {x.id.slice(-4)}</option>)}</select></div>
          </div>
          <div className="stack" style={{ gap: 6 }}>
            <div className="rowx"><h2 className="grow">{tx("Keyframes")}</h2><button className="btn sm" onClick={keyHere}><Diamond />{r2(t)} {tx("s")}</button></div>
            {!it.keys?.length && <span className="muted" style={{ fontSize: 12 }}>{tx("Move the playhead, place the box and add a key. With keys, dragging saves to that second.")}</span>}
            {(it.keys || []).map((k, i) => (
              <div key={i} className="rowx" style={{ fontSize: 12 }}>
                <span className="num" style={{ width: 52 }}>{k.t} {tx("s")}</span>
                <span className="muted num grow">{Math.round(k.x ?? 0)},{Math.round(k.y ?? 0)}{k.rot ? ` ${k.rot}°` : ""}{k.scale !== undefined && k.scale !== 1 ? ` ×${k.scale}` : ""}</span>
                <select className="select" style={{ width: 96, height: 28 }} value={k.ease || "inOut"} onChange={(e) => set({ keys: it.keys!.map((x, j) => (j === i ? { ...x, ease: e.target.value } : x)) })}>
                  {["inOut", "out", "in", "linear", "back"].map((e) => <option key={e}>{e}</option>)}
                </select>
                <button className="btn icon sm ghost" onClick={() => set({ keys: it.keys!.filter((_, j) => j !== i) })}><Trash2 /></button>
              </div>
            ))}
          </div>
          {setPathFor && (
            <div className="stack" style={{ gap: 6 }}>
              <div className="rowx">
                <h2 className="grow">{tx("Motion path")}</h2>
                <button className={`btn sm ${pathFor === sel ? "primary" : ""}`} onClick={() => setPathFor(pathFor === sel ? "" : sel)}>{pathFor === sel ? tx("Finish") : it.path?.length ? tx("Add points") : tx("Draw path")}</button>
                {it.path?.length ? <button className="btn icon sm ghost" title={tx("Delete path")} onClick={() => { set({ path: undefined }); setPathFor(""); }}><Trash2 /></button> : null}
              </div>
              <span className="muted" style={{ fontSize: 12, lineHeight: "18px" }}>{pathFor === sel ? tx("Click the path's points on the frame in order; press Done when finished.") : it.path?.length >= 2 ? tx("{0} points: the layer follows this path from {1} to {2} s.", it.path.length, it.t0, it.t1) : tx("The layer follows the path you draw, from its start to its end.")}</span>
              {it.path?.length >= 2 && (
                <div className="grid2">
                  <div className="field"><label>{tx("Speed curve")}</label><select className="select" value={it.pathEase || "inOut"} onChange={(e) => set({ pathEase: e.target.value })}>{["inOut", "out", "in", "linear", "back"].map((x) => <option key={x}>{x}</option>)}</select></div>
                  <div className="field"><label>{tx("Face its direction")}</label><input type="checkbox" checked={!!it.pathTurn} onChange={(e) => set({ pathTurn: e.target.checked || undefined })} /></div>
                </div>
              )}
            </div>
          )}
          <button className="btn sm ghost" onClick={() => { L.update(items.filter((x) => x.id !== sel).map((x) => (x.parent === sel ? { ...x, parent: undefined } : x))); setSel(""); }}><Trash2 />{tx("Delete layer")}</button>
        </div>
      )}
    </div>
  );
}

/** Edit boxes over the picture: drag to move, corner to resize, double-click to edit text, drop a picture. */
export function LayerOverlay({ L, t, sel, setSel, W, H, passThrough, pathFor }: { L: Layers; t: number; sel: string; setSel: (s: string) => void; W: number; H: number; passThrough?: boolean; pathFor?: string }) {
  const ref = useRef<HTMLDivElement>(null);
  const [editing, setEditing] = useState("");
  const [ghost, setGhost] = useState<Item | null>(null);
  const items = ghost ? L.items.map((x) => (x.id === ghost.id ? ghost : x)) : L.items;
  const write = (it: Item, p: Json) => {
    // with keyframes, a move becomes a key at this second
    if (it.keys?.length) {
      const s = evalItem(items, it, t);
      const keys = it.keys.filter((k) => Math.abs(k.t - t) > 0.02);
      const old = it.keys.find((k) => Math.abs(k.t - t) <= 0.02);
      return { ...it, keys: [...keys, { ...(old || { t: r2(t), ease: "inOut" }), x: Math.round(s.x), y: Math.round(s.y), w: Math.round(s.w), h: Math.round(s.h), ...p }].sort((a, b) => a.t - b.t) };
    }
    return { ...it, ...p };
  };
  const start = (e: React.PointerEvent, it: Item, mode: "move" | "size") => {
    e.stopPropagation();
    setSel(it.id);
    const r = ref.current!.getBoundingClientRect();
    const k = W / r.width;
    const [x0, y0] = [e.clientX, e.clientY];
    const s0 = evalItem(items, it, t);
    let last: Item | null = null;
    const mv = (ev: PointerEvent) => {
      const dx = (ev.clientX - x0) * k, dy = (ev.clientY - y0) * k;
      if (!last && Math.abs(dx) + Math.abs(dy) < 3 * k) return;   // a click, not a drag
      const p = mode === "move" ? { x: Math.round(s0.x + dx), y: Math.round(s0.y + dy) } : { w: Math.max(16, Math.round(s0.w + dx)), h: Math.max(16, Math.round(s0.h + dy)) };
      last = write(it, p);
      setGhost(last);
      L.preview(L.items.map((x) => (x.id === it.id ? last! : x)));
    };
    const up = () => {
      window.removeEventListener("pointermove", mv);
      window.removeEventListener("pointerup", up);
      if (last) L.update(L.items.map((x) => (x.id === it.id ? last! : x)));
      setGhost(null);
    };
    window.addEventListener("pointermove", mv);
    window.addEventListener("pointerup", up);
  };
  const drop = async (e: React.DragEvent) => {
    e.preventDefault();
    const f = e.dataTransfer.files?.[0];
    if (!f) return;
    const r = ref.current!.getBoundingClientRect();
    const px = ((e.clientX - r.left) / r.width) * W, py = ((e.clientY - r.top) / r.height) * H;
    if (f.type.startsWith("image/")) {
      const src = await L.upload(f);
      const w = W * 0.4;
      const it: Item = { id: "l" + Date.now().toString(36), kind: "image", src, t0: r2(t), t1: r2(t + 3), x: Math.round(px - w / 2), y: Math.round(py - w / 2), w, h: w, opacity: 1, fade: 0.25 };
      L.update([...items, it]);
      setSel(it.id);
    } else if (f.type.startsWith("text/")) {
      const text = (await f.text()).slice(0, 200);
      const it = { ...newItem("text", t, t + 3, W, H), text, x: Math.round(px - W * 0.4), y: Math.round(py - 60) };
      L.update([...items, it]);
      setSel(it.id);
    }
  };
  const hs = W / 60, lw = W / 400;
  // drawing a motion path: every click on the picture is the next point
  const down = (e: React.PointerEvent) => {
    if (pathFor) {
      const r = ref.current!.getBoundingClientRect();
      const pt = [Math.round(((e.clientX - r.left) / r.width) * W), Math.round(((e.clientY - r.top) / r.height) * H)];
      L.update(L.items.map((x) => (x.id === pathFor ? { ...x, path: [...(x.path || []), pt] } : x)));
      return;
    }
    setSel("");
    setEditing("");
  };
  const shown = items.filter((x) => Array.isArray(x.path) && x.path.length && (x.id === sel || x.id === pathFor));
  return (
    <div ref={ref} style={{ position: "absolute", inset: 0, pointerEvents: passThrough && !pathFor ? "none" : undefined, cursor: pathFor ? "crosshair" : undefined }} onPointerDown={down} onDragOver={(e) => e.preventDefault()} onDrop={drop}>
      {shown.length > 0 && (
        <svg width={W} height={H} viewBox={`0 0 ${W} ${H}`} style={{ position: "absolute", inset: 0, pointerEvents: "none" }}>
          {shown.map((x) => (
            <g key={x.id}>
              <polyline points={x.path.map((p: number[]) => p.join(",")).join(" ")} fill="none" style={{ stroke: "var(--accent)" }} strokeWidth={lw * 2} strokeDasharray={`${lw * 6} ${lw * 4}`} />
              {x.path.map((p: number[], i: number) => <circle key={i} cx={p[0]} cy={p[1]} r={lw * 5} style={{ fill: i === 0 ? "#86b991" : "var(--accent)" }} />)}
            </g>
          ))}
        </svg>
      )}
      {items.filter((x) => t >= x.t0 && t < x.t1).map((x) => {
        const s = evalItem(items, x, t);
        const on = x.id === sel;
        const col = x.hidden ? "rgba(160,160,160,.7)" : on ? "var(--accent)" : "rgba(255,255,255,.5)";
        return (
          <div key={x.id} onPointerDown={(e) => start(e, x, "move")} onDoubleClick={() => ["text", "marker", "lower"].includes(x.kind) && setEditing(x.id)}
            style={{ pointerEvents: pathFor ? "none" : "auto", position: "absolute", left: s.x, top: s.y, width: s.w, height: s.h, outline: `${lw}px ${x.hidden ? "dotted" : on ? "solid" : "dashed"} ${col}`, background: x.hidden ? "repeating-linear-gradient(45deg, rgba(160,160,160,.12) 0 8px, transparent 8px 16px)" : undefined, cursor: "move", transform: `rotate(${s.rot}deg) scale(${s.scale})` }}>
            {on && <div onPointerDown={(e) => start(e, x, "size")} style={{ position: "absolute", right: -hs / 2, bottom: -hs / 2, width: hs, height: hs, background: "var(--accent)", borderRadius: hs / 4, cursor: "nwse-resize" }} />}
            {editing === x.id && (
              <textarea autoFocus defaultValue={x.text} onPointerDown={(e) => e.stopPropagation()}
                onBlur={(e) => { L.update(items.map((y) => (y.id === x.id ? { ...y, text: e.target.value } : y))); setEditing(""); }}
                onKeyDown={(e) => { if (e.key === "Escape" || (e.key === "Enter" && !e.shiftKey)) (e.target as HTMLTextAreaElement).blur(); }}
                style={{ position: "absolute", inset: 0, width: "100%", height: "100%", font: `${x.weight || 800} ${x.size || 72}px "Segoe UI", sans-serif`, textAlign: (x.align as any) || "center", color: "#fff", background: "rgba(0,0,0,.55)", border: 0, outline: `${lw * 2}px solid var(--accent)`, resize: "none", padding: 0 }} />
            )}
          </div>
        );
      })}
    </div>
  );
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
