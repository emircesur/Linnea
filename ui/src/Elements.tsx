// Elements the film's own code draws (through KE, or any text): select them on the picture, move,
// resize, turn, re-type, recolour, hide, retime and keyframe them. Edits go to tweaks.json `edits`.
import { tx } from "./i18n";
import { useEffect, useRef, useState } from "react";
import { ChevronDown, ChevronUp, Diamond, Eye, EyeOff, RotateCcw, Trash2 } from "lucide-react";
import { Json, tool } from "./api";
import type { Layers } from "./Layers";

export type Box = { id: string; label: string; x: number; y: number; w: number; h: number; t0?: number; t1?: number; kind: string; text?: string; edited?: boolean; hidden?: boolean; puppet?: string; params?: Json; states?: Json[]; pt?: number };
export type Reg = { id: string; label: string; t0: number; t1: number; kind: string; text?: string };
const r2 = (x: number) => Math.round(x * 100) / 100;

export function useElements(toolName: string, args: Json, getWin: () => any, redraw: () => void, paint: () => void, before?: (prev: Json) => void) {
  const [edits, setEdits] = useState<Json>({});
  const [loaded, setLoaded] = useState(false);
  const timer = useRef<any>(0);
  const cur = useRef<Json>({});
  cur.current = edits;
  const key = JSON.stringify(args);
  useEffect(() => { tool(toolName, args).then((r) => { setEdits(r.edits || {}); setLoaded(true); }).catch(() => {}); }, [toolName, key]);
  /** Draw these edits now (while dragging) and return the boxes they give; nothing is saved. */
  const preview = (next: Json): Box[] => {
    try { getWin()?.KE?.setEdits(next); paint(); return [...(getWin()?.KE_BOXES || [])]; } catch { return []; }
  };
  const update = (next: Json, silent = false) => {
    if (!silent) before?.(cur.current);
    setEdits(next);
    try { getWin()?.KE?.setEdits(next); } catch { /* reloading */ }
    redraw();
    clearTimeout(timer.current);
    timer.current = setTimeout(() => tool(toolName, { ...args, edits: next }), 300);
  };
  const set = (id: string, p: Json) => update({ ...edits, [id]: { ...(edits[id] || {}), ...p } });
  const reset = (id: string) => { const n = { ...edits }; delete n[id]; update(n); };
  return { edits, update, set, reset, preview, loaded };
}
export type Elements = ReturnType<typeof useElements>;

/** Every element the film draws, with the seconds it is on screen: the page is drawn every `step` seconds.
 *  Run it on a page nobody watches (it jumps through the film). It works in short slices so the app stays
 *  smooth, hands over what it has found so far through `part`, and stops when `alive` turns false. */
export async function scanRegistry(w: any, dur: number, step = 0.5, part?: (r: Reg[]) => void, alive: () => boolean = () => true): Promise<Reg[]> {
  const reg = new Map<string, Reg>();
  let slice = performance.now();
  for (let t = 0; t <= dur + 1e-6; t += step) {
    if (!alive()) break;
    try {
      const r = w.renderOut(t);
      if (r && r.then) await r;
    } catch { continue; }
    for (const b of (w.KE_BOXES || []) as Box[]) {
      const x = reg.get(b.id);
      if (x) x.t1 = Math.min(dur, t + step);
      else reg.set(b.id, { id: b.id, label: b.label, kind: b.kind, t0: b.t0 ?? t, t1: b.t1 ?? Math.min(dur, t + step), text: (b as any).src ?? (b.kind === "auto" ? b.label : undefined) });
    }
    // about one frame's worth of work at a time, then the app gets the thread back
    if (performance.now() - slice > 12) {
      part?.([...reg.values()].map((x) => ({ ...x })));
      await new Promise((r) => setTimeout(r, 0));
      slice = performance.now();
    }
  }
  return [...reg.values()];
}

function upsertKey(e: Json, t: number, p: Json) {
  const keys = (e.keys || []).filter((k: Json) => Math.abs(k.t - t) > 0.02);
  const old = (e.keys || []).find((k: Json) => Math.abs(k.t - t) <= 0.02) || { t: r2(t), ease: "inOut" };
  return [...keys, { dx: e.dx || 0, dy: e.dy || 0, scale: e.scale ?? 1, rot: e.rot || 0, opacity: e.opacity ?? 1, ...old, ...p }].sort((a: Json, b: Json) => a.t - b.t);
}

export function ElementOverlay({ boxes, E, sel, setSel, t, W, H, L }: { boxes: Box[]; E: Elements; sel: string; setSel: (s: string) => void; t: number; W: number; H: number; L?: Layers }) {
  const ref = useRef<HTMLDivElement>(null);
  const [editing, setEditing] = useState("");
  const [live, setLive] = useState<Box[] | null>(null);
  const frame = useRef(0);
  const start = (ev: React.PointerEvent, b: Box, mode: "move" | "size") => {
    ev.stopPropagation();
    setSel(b.id);
    const r = ref.current!.getBoundingClientRect();
    const f = W / r.width;
    const [x0, y0] = [ev.clientX, ev.clientY];
    const e = E.edits[b.id] || {};
    let p: Json | null = null;
    const mv = (ev: PointerEvent) => {
      const dx = (ev.clientX - x0) * f, dy = (ev.clientY - y0) * f;
      if (!p && Math.abs(dx) + Math.abs(dy) < 3 * f) return;   // a click, not a drag
      // resizing follows the corner: the scale grows with the box's diagonal
      const k = Math.max(0.1, Math.hypot(b.w + dx, b.h + dy) / Math.max(1, Math.hypot(b.w, b.h)));
      p = mode === "move" ? { dx: Math.round((e.dx || 0) + dx), dy: Math.round((e.dy || 0) + dy) } : { scale: r2((e.scale ?? 1) * k) };
      const next = { ...E.edits, [b.id]: { ...e, ...(e.keys?.length ? { keys: upsertKey(e, t, p) } : p) } };
      cancelAnimationFrame(frame.current);
      frame.current = requestAnimationFrame(() => setLive(E.preview(next)));
    };
    const up = () => {
      window.removeEventListener("pointermove", mv);
      window.removeEventListener("pointerup", up);
      cancelAnimationFrame(frame.current);
      setLive(null);
      if (p) E.set(b.id, e.keys?.length ? { keys: upsertKey(e, t, p) } : p);
    };
    window.addEventListener("pointermove", mv);
    window.addEventListener("pointerup", up);
  };
  const drop = async (e: React.DragEvent) => {
    e.preventDefault();
    const f = e.dataTransfer.files?.[0];
    if (!f || !L || !f.type.startsWith("image/")) return;
    const r = ref.current!.getBoundingClientRect();
    const px = ((e.clientX - r.left) / r.width) * W, py = ((e.clientY - r.top) / r.height) * H;
    const src = await L.upload(f);
    const w = W * 0.4;
    L.update([...L.items, { id: "l" + Date.now().toString(36), kind: "image", src, t0: r2(t), t1: r2(t + 3), x: Math.round(px - w / 2), y: Math.round(py - w / 2), w, h: w, opacity: 1, fade: 0.25 }]);
  };
  const lw = W / 400, hs = W / 60;
  return (
    <div ref={ref} style={{ position: "absolute", inset: 0 }} onPointerDown={() => { setSel(""); setEditing(""); }} onDragOver={(e) => e.preventDefault()} onDrop={drop}>
      {(live || boxes).map((b) => {
        const on = b.id === sel;
        const hid = !!E.edits[b.id]?.hidden || !!b.hidden;
        const { w, h } = b;
        return (
          <div key={b.id} title={hid ? tx("{0} (hidden)", b.label) : b.label} onPointerDown={(e) => start(e, b, "move")} onDoubleClick={() => b.text !== undefined && setEditing(b.id)}
            style={{ position: "absolute", left: b.x, top: b.y, width: w, height: h, cursor: "move",
              outline: `${lw}px ${hid ? "dotted" : on ? "solid" : "dashed"} ${hid ? "rgba(160,160,160,.75)" : on ? "#7fb0ff" : "rgba(127,176,255,.45)"}`,
              background: hid ? "repeating-linear-gradient(45deg, rgba(160,160,160,.14) 0 8px, transparent 8px 16px)" : on ? "rgba(127,176,255,.08)" : undefined }}>
            {on && <div onPointerDown={(e) => start(e, b, "size")} style={{ position: "absolute", right: -hs / 2, bottom: -hs / 2, width: hs, height: hs, background: "#7fb0ff", borderRadius: hs / 4, cursor: "nwse-resize" }} />}
            {editing === b.id && (
              <textarea autoFocus defaultValue={b.text} onPointerDown={(e) => e.stopPropagation()}
                onBlur={(e) => { if (e.target.value !== b.text) E.set(b.id, { text: e.target.value }); setEditing(""); }}
                onKeyDown={(e) => { if (e.key === "Escape" || (e.key === "Enter" && !e.shiftKey)) (e.target as HTMLTextAreaElement).blur(); }}
                style={{ position: "absolute", inset: 0, width: "100%", height: "100%", minWidth: 200, font: `700 ${Math.max(24, h * 0.6)}px "Segoe UI", sans-serif`, color: "#fff", background: "rgba(0,0,0,.7)", border: 0, outline: `${lw * 2}px solid #7fb0ff`, resize: "none", textAlign: "center" }} />
            )}
          </div>
        );
      })}
    </div>
  );
}

export function ElementPanel({ id, box, reg, E, t, close }: { id: string; box?: Box; reg?: Reg; E: Elements; t: number; close: () => void }) {
  const e = E.edits[id] || {};
  const textual = box?.text !== undefined || reg?.kind === "text" || reg?.kind === "auto";
  const num = (k: string, label: string, step: number, def: number) => (
    <div className="field"><label>{label}</label><input className="input num" type="number" step={step} key={`${id}${k}${e[k]}`} defaultValue={e[k] ?? def} onBlur={(ev) => +ev.target.value !== (e[k] ?? def) && E.set(id, { [k]: +ev.target.value })} /></div>
  );
  return (
    <div className="stack" style={{ gap: 8 }}>
      <div className="rowx">
        <h2 className="grow" style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{box?.label || reg?.label || id}</h2>
        <button className="btn icon sm ghost" title={e.hidden ? tx("Show") : tx("Hide")} onClick={() => E.set(id, { hidden: !e.hidden })}>{e.hidden ? <EyeOff /> : <Eye />}</button>
        <button className="btn icon sm ghost" title={tx("Reset to the code")} onClick={() => E.reset(id)}><RotateCcw /></button>
        <button className="btn sm ghost" onClick={close}>{tx("Close")}</button>
      </div>
      <span className="muted" style={{ fontSize: 12 }}>{reg?.kind === "auto" || id.startsWith("txt:") ? tx("Text from the code: moves, scales, turns, and its text and colour change; the box behind it stays.") : tx("An element from the code: moves, scales and turns as one piece, and can be retimed.")}</span>
      {textual && <textarea className="input" style={{ fontFamily: "var(--font)", fontSize: 13 }} rows={2} key={`${id}text${e.text}`} defaultValue={e.text ?? box?.text ?? ""} onBlur={(ev) => ev.target.value !== (e.text ?? box?.text) && E.set(id, { text: ev.target.value })} />}
      <div className="grid2">
        {textual && <div className="field"><label>{tx("Colour")}</label><div className="rowx"><input className="input" type="color" style={{ padding: 2, width: 40 }} value={e.color || "#ffffff"} onChange={(ev) => E.set(id, { color: ev.target.value })} /><button className="btn icon sm ghost" title={tx("Colour from the code")} onClick={() => E.set(id, { color: undefined })}><Trash2 /></button></div></div>}
        {num("scale", tx("Scale"), 0.05, 1)}
        {num("rot", tx("Angle"), 1, 0)}
        {num("opacity", tx("Opacity"), 0.05, 1)}
        {num("dx", tx("Horizontal offset, px"), 1, 0)}
        {num("dy", tx("Vertical offset, px"), 1, 0)}
      </div>
      <div className="grid2">
        {num("t0", tx("Appears, s"), 0.1, reg?.t0 ?? 0)}
        {num("t1", tx("Disappears, s"), 0.1, reg?.t1 ?? 0)}
      </div>
      <div className="stack" style={{ gap: 6 }}>
        <div className="rowx"><h2 className="grow">{tx("Keyframes")}</h2><button className="btn sm" onClick={() => E.set(id, { keys: upsertKey(e, t, {}) })}><Diamond />{r2(t)} {tx("s")}</button></div>
        {(e.keys || []).map((k: Json, i: number) => (
          <div key={i} className="rowx" style={{ fontSize: 12 }}>
            <span className="num" style={{ width: 52 }}>{k.t} {tx("s")}</span>
            <span className="muted num grow">{Math.round(k.dx || 0)},{Math.round(k.dy || 0)} ×{k.scale ?? 1}{k.rot ? ` ${k.rot}°` : ""}</span>
            <select className="select" style={{ width: 96, height: 28 }} value={k.ease || "inOut"} onChange={(ev) => E.set(id, { keys: e.keys.map((x: Json, j: number) => (j === i ? { ...x, ease: ev.target.value } : x)) })}>
              {["inOut", "out", "in", "linear", "back"].map((x) => <option key={x}>{x}</option>)}
            </select>
            <button className="btn icon sm ghost" onClick={() => E.set(id, { keys: e.keys.filter((_: Json, j: number) => j !== i) })}><Trash2 /></button>
          </div>
        ))}
        {!e.keys?.length && <span className="muted" style={{ fontSize: 12 }}>{tx("Add keys to put the element somewhere else at each second; dragging saves to that second.")}</span>}
      </div>
    </div>
  );
}

/** Every layer of the film in one list, top first: the film's own elements (blue) and hand-placed layers
 *  (green). Hidden ones stay here; the eye brings them back. Layers can change order. */
export function LayerList({ reg, E, L, t, pick, sel }: { reg: Reg[]; E: Elements; L: Layers; t: number; pick: (kind: "e" | "l", id: string, t0: number) => void; sel: string }) {
  const items = L.items;
  const move = (i: number, d: number) => {
    const j = i + d;
    if (j < 0 || j >= items.length) return;
    const n = [...items];
    [n[i], n[j]] = [n[j], n[i]];
    L.update(n);
  };
  const row = (kind: "e" | "l", id: string, label: string, t0: number, t1: number, hidden: boolean, toggle: () => void, extra?: React.ReactNode) => {
    const now = t >= t0 && t < t1;
    return (
      <div key={kind + id} className="rowx" style={{ fontSize: 12, gap: 6, padding: "2px 4px", borderRadius: 6, background: sel === id ? "var(--hover)" : undefined, opacity: hidden ? 0.55 : 1 }}>
        <span style={{ width: 8, height: 8, borderRadius: 4, flex: "none", background: kind === "e" ? "#7fb0ff" : "#86b991" }} />
        <button className="grow" onClick={() => pick(kind, id, t0)} style={{ all: "unset", cursor: "pointer", minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", fontWeight: now ? 600 : 400, textDecoration: hidden ? "line-through" : undefined }}>{label}</button>
        <span className="muted num" style={{ flex: "none" }}>{r2(t0)}–{r2(t1)}</span>
        {extra}
        <button className="btn icon sm ghost" title={hidden ? tx("Show") : tx("Hide")} onClick={toggle}>{hidden ? <EyeOff /> : <Eye />}</button>
      </div>
    );
  };
  return (
    <div className="stack" style={{ gap: 2 }}>
      <h2 style={{ marginBottom: 4 }}>{tx("Layers in the scene")}</h2>
      {items.map((it, i) => ({ it, i })).reverse().map(({ it, i }) => row("l", it.id, it.label || it.text || it.puppet || it.kind, it.t0, it.t1, !!it.hidden,
        () => L.update(items.map((x) => (x.id === it.id ? { ...x, hidden: !x.hidden } : x))),
        <>
          <button className="btn icon sm ghost" title={tx("Bring forward")} onClick={() => move(i, 1)}><ChevronUp /></button>
          <button className="btn icon sm ghost" title={tx("Send backward")} onClick={() => move(i, -1)}><ChevronDown /></button>
        </>))}
      {reg.map((r) => {
        const e = E.edits[r.id] || {};
        return row("e", r.id, e.text ?? r.label, e.t0 ?? r.t0, e.t1 ?? r.t1, !!e.hidden, () => E.set(r.id, { hidden: !e.hidden }));
      })}
      {!items.length && !reg.length && <span className="muted" style={{ fontSize: 12 }}>{tx("No layers yet.")}</span>}
    </div>
  );
}
