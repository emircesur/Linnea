// Timeline lanes for what was added to a reel or film: cards and titles, sounds, layers.
// Drag a bar to move it, drag its edges to change its length (both snap to the playhead, scene cuts and the
// other bars; Alt: no snapping), click to select, Delete to remove. Diamonds on a bar are its keyframes and
// puppet states: drag one to retime it, click it to go there.
import { tx } from "./i18n";
import { useEffect, useState } from "react";
import { X } from "lucide-react";

export type Mark = { t: number; kind: "key" | "state"; label?: string };
export type TItem = { id: string; s: number; e: number; label: string; tick?: boolean; locked?: boolean; marks?: Mark[]; dim?: boolean; color?: string };
export type Lane = {
  key: string;
  label: string;
  color: string;
  items: TItem[];
  sel?: string;
  selSet?: string[];
  /** pack the items into as many rows as they need (sounds, the film's own elements) */
  stack?: boolean;
  /** listen for Delete itself (the film editor handles keys in one place) */
  ownKeys?: boolean;
  onSelect?: (id: string, e?: { shiftKey: boolean; ctrlKey: boolean; metaKey: boolean }) => void;
  onChange?: (id: string, s: number, e: number) => void;
  onDelete?: (id: string) => void;
  onMark?: (id: string, from: number, to: number) => void;
  onSeek?: (t: number) => void;
  onMenu?: (id: string, x: number, y: number) => void;
};

export const LANE_H = 26;

/** Which row each item goes on so that none overlap (first fit, by start). */
export function packRows(items: TItem[]): Map<string, number> {
  const ends: number[] = [];
  const out = new Map<string, number>();
  for (const it of [...items].sort((a, b) => a.s - b.s)) {
    let r = ends.findIndex((e) => e <= it.s + 1e-3);
    if (r < 0) { r = ends.length; ends.push(0); }
    ends[r] = it.e;
    out.set(it.id, r);
  }
  return out;
}
export const laneRows = (lane: Lane) => (lane.stack ? Math.max(1, ...[...packRows(lane.items).values()].map((r) => r + 1)) : 1);

export function TrackLane({ lane, top, sx, pad, total, snaps, onSnap, rowH = LANE_H }: { lane: Lane; top: number; sx: number; pad: number; total: number; snaps?: number[]; onSnap?: (t: number | null) => void; rowH?: number }) {
  const [drag, setDrag] = useState<{ id: string; s: number; e: number } | null>(null);
  const [mdrag, setMdrag] = useState<{ id: string; from: number; to: number } | null>(null);
  useEffect(() => {
    if (!lane.sel || !lane.onDelete || lane.ownKeys === false) return;
    const key = (e: KeyboardEvent) => {
      if (/INPUT|TEXTAREA|SELECT/.test((e.target as HTMLElement).tagName)) return;
      if ((e.key === "Delete" || e.key === "Backspace") && lane.items.some((x) => x.id === lane.sel && !x.locked)) {
        e.preventDefault();
        lane.onDelete!(lane.sel!);
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [lane.sel, lane.items]);
  const rows = lane.stack ? packRows(lane.items) : null;
  const snapTo = (x: number, alt: boolean, extra: number[] = []) => {
    if (alt || !snaps) return { x, hit: null as number | null };
    const th = 8 / sx;
    let best: number | null = null;
    for (const c of [...snaps, ...extra]) if (Math.abs(c - x) <= th && (best === null || Math.abs(c - x) < Math.abs(best - x))) best = c;
    return best === null ? { x, hit: null } : { x: best, hit: best };
  };
  const start = (e: React.PointerEvent, it: TItem, mode: "move" | "l" | "r") => {
    if (e.button !== 0) return;
    e.stopPropagation();
    lane.onSelect?.(it.id, e);
    if (it.locked || !lane.onChange || e.shiftKey || e.ctrlKey || e.metaKey) return;
    const x0 = e.clientX;
    const others = (snaps || []).filter((v) => Math.abs(v - it.s) > 1e-3 && Math.abs(v - it.e) > 1e-3);
    let cur = { id: it.id, s: it.s, e: it.e };
    const mv = (ev: PointerEvent) => {
      const d = (ev.clientX - x0) / sx;
      const len = it.e - it.s;
      let hit: number | null = null;
      if (mode === "move") {
        let s = Math.max(0, Math.min(total - len, it.s + d));
        const a = snapTo(s, ev.altKey, others.filter(() => true)), b = snapTo(s + len, ev.altKey, others);
        if (a.hit !== null && (b.hit === null || Math.abs(a.x - s) <= Math.abs(b.x - s - len))) { s = a.x; hit = a.hit; }
        else if (b.hit !== null) { s = b.x - len; hit = b.hit; }
        s = Math.max(0, Math.min(total - len, s));
        cur = { id: it.id, s, e: s + len };
      } else if (mode === "l") { const a = snapTo(Math.max(0, Math.min(it.e - 0.1, it.s + d)), ev.altKey, others); hit = a.hit; cur = { id: it.id, s: Math.min(it.e - 0.1, a.x), e: it.e }; }
      else { const a = snapTo(Math.min(total, Math.max(it.s + 0.1, it.e + d)), ev.altKey, others); hit = a.hit; cur = { id: it.id, s: it.s, e: Math.max(it.s + 0.1, a.x) }; }
      onSnap?.(hit);
      setDrag(cur);
    };
    const up = () => {
      window.removeEventListener("pointermove", mv);
      window.removeEventListener("pointerup", up);
      setDrag(null);
      onSnap?.(null);
      if (Math.abs(cur.s - it.s) > 1e-3 || Math.abs(cur.e - it.e) > 1e-3) lane.onChange!(it.id, Math.round(cur.s * 100) / 100, Math.round(cur.e * 100) / 100);
    };
    window.addEventListener("pointermove", mv);
    window.addEventListener("pointerup", up);
  };
  const markDown = (e: React.PointerEvent, it: TItem, m: Mark) => {
    e.stopPropagation();
    if (e.button !== 0) return;
    const x0 = e.clientX;
    let to = m.t, moved = false;
    const mv = (ev: PointerEvent) => {
      if (Math.abs(ev.clientX - x0) > 2) moved = true;
      if (!moved || !lane.onMark) return;
      const a = snapTo(Math.max(it.s, Math.min(it.e, m.t + (ev.clientX - x0) / sx)), ev.altKey);
      to = a.x;
      onSnap?.(a.hit);
      setMdrag({ id: it.id, from: m.t, to });
    };
    const up = () => {
      window.removeEventListener("pointermove", mv);
      window.removeEventListener("pointerup", up);
      setMdrag(null);
      onSnap?.(null);
      if (moved && lane.onMark && Math.abs(to - m.t) > 1e-3) lane.onMark(it.id, m.t, Math.round(to * 100) / 100);
      else lane.onSeek?.(m.t);
    };
    window.addEventListener("pointermove", mv);
    window.addEventListener("pointerup", up);
  };
  return (
    <>
      {lane.items.map((it) => {
        const d = drag && drag.id === it.id ? drag : it;
        const on = lane.sel === it.id || !!lane.selSet?.includes(it.id);
        const y = top + (rows?.get(it.id) || 0) * rowH;
        const color = it.color || lane.color;
        if (it.tick)
          return <span key={it.id} title={it.label} style={{ position: "absolute", left: pad + d.s * sx, top: y + 4, width: 2, height: rowH - 8, background: color, opacity: 0.55, pointerEvents: "none" }} />;
        const w = Math.max(6, (d.e - d.s) * sx);
        return (
          <div key={it.id} className={`tbar ${on ? "on" : ""} ${it.dim ? "dim" : ""}`} title={tx("{0} {1}–{2} s", it.label, d.s.toFixed(2), d.e.toFixed(2))}
            onPointerDown={(e) => start(e, it, "move")} onMouseDown={(e) => e.stopPropagation()}
            onContextMenu={(e) => { if (!lane.onMenu) return; e.preventDefault(); e.stopPropagation(); if (!on) lane.onSelect?.(it.id); lane.onMenu(it.id, e.clientX, e.clientY); }}
            style={{ left: pad + d.s * sx, top: y + 1, width: w, height: rowH - 4, borderColor: on ? color : undefined, borderLeftColor: color, cursor: it.locked ? "default" : "grab", background: on ? `color-mix(in srgb, ${color} 22%, var(--raised))` : undefined }}>
            {!it.locked && <span className="tbar-h l" onPointerDown={(e) => start(e, it, "l")} />}
            <span className="tbar-t">{it.label}</span>
            {(it.marks || []).map((m, i) => {
              const mt = mdrag && mdrag.id === it.id && Math.abs(mdrag.from - m.t) < 1e-3 ? mdrag.to : m.t;
              return <span key={i} className={`tmark ${m.kind}`} title={(m.label ? m.label + " · " : "") + mt.toFixed(2) + " s"} style={{ left: (mt - d.s) * sx - 5 }} onPointerDown={(e) => markDown(e, it, m)} />;
            })}
            {!it.locked && <span className="tbar-h r" onPointerDown={(e) => start(e, it, "r")} />}
            {on && lane.onDelete && !it.locked && w > 40 && (
              <button title={tx("Delete (Delete)")} onPointerDown={(e) => { e.stopPropagation(); lane.onDelete!(it.id); }} style={{ position: "absolute", right: 6, top: 2, border: 0, background: "none", color: "var(--text-2)", cursor: "pointer", padding: 0, zIndex: 2 }}><X size={12} /></button>
            )}
          </div>
        );
      })}
    </>
  );
}
