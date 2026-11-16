// Timeline lanes for what was added to a reel or film: cards and titles, sounds, layers.
// Drag a bar to move it, drag its edges to change its length, click to select, Delete to remove.
import { tx } from "./i18n";
import { useEffect, useState } from "react";
import { X } from "lucide-react";

export type TItem = { id: string; s: number; e: number; label: string; tick?: boolean; locked?: boolean };
export type Lane = {
  key: string;
  label: string;
  color: string;
  items: TItem[];
  sel?: string;
  onSelect?: (id: string) => void;
  onChange?: (id: string, s: number, e: number) => void;
  onDelete?: (id: string) => void;
};

export const LANE_H = 26;

export function TrackLane({ lane, top, sx, pad, total }: { lane: Lane; top: number; sx: number; pad: number; total: number }) {
  const [drag, setDrag] = useState<{ id: string; s: number; e: number } | null>(null);
  useEffect(() => {
    if (!lane.sel || !lane.onDelete) return;
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
  const start = (e: React.PointerEvent, it: TItem, mode: "move" | "l" | "r") => {
    e.stopPropagation();
    lane.onSelect?.(it.id);
    if (it.locked || !lane.onChange) return;
    const x0 = e.clientX;
    let cur = { id: it.id, s: it.s, e: it.e };
    const mv = (ev: PointerEvent) => {
      const d = (ev.clientX - x0) / sx;
      const len = it.e - it.s;
      if (mode === "move") {
        const s = Math.max(0, Math.min(total - len, it.s + d));
        cur = { id: it.id, s, e: s + len };
      } else if (mode === "l") cur = { id: it.id, s: Math.max(0, Math.min(it.e - 0.1, it.s + d)), e: it.e };
      else cur = { id: it.id, s: it.s, e: Math.min(total, Math.max(it.s + 0.1, it.e + d)) };
      setDrag(cur);
    };
    const up = () => {
      window.removeEventListener("pointermove", mv);
      window.removeEventListener("pointerup", up);
      setDrag(null);
      if (Math.abs(cur.s - it.s) > 1e-3 || Math.abs(cur.e - it.e) > 1e-3) lane.onChange!(it.id, Math.round(cur.s * 100) / 100, Math.round(cur.e * 100) / 100);
    };
    window.addEventListener("pointermove", mv);
    window.addEventListener("pointerup", up);
  };
  return (
    <>
      <span style={{ position: "absolute", left: 2, top: top + 6, fontSize: 10, color: "var(--text-3)", pointerEvents: "none", width: pad - 2, overflow: "hidden" }} title={lane.label} />
      {lane.items.map((it) => {
        const d = drag && drag.id === it.id ? drag : it;
        const on = lane.sel === it.id;
        if (it.tick)
          return <span key={it.id} title={it.label} style={{ position: "absolute", left: pad + d.s * sx, top: top + 4, width: 2, height: LANE_H - 8, background: lane.color, opacity: 0.55, pointerEvents: "none" }} />;
        return (
          <div key={it.id} title={tx("{0} {1}–{2} s", it.label, d.s.toFixed(2), d.e.toFixed(2))}
            onPointerDown={(e) => start(e, it, "move")} onMouseDown={(e) => e.stopPropagation()}
            style={{ position: "absolute", left: pad + d.s * sx, top, width: Math.max(6, (d.e - d.s) * sx), height: LANE_H - 4, borderRadius: 6,
              background: on ? "var(--raised)" : "var(--take)", border: `1px solid ${on ? lane.color : "var(--line-2)"}`, borderLeft: `3px solid ${lane.color}`,
              fontSize: 11, lineHeight: `${LANE_H - 6}px`, padding: "0 6px", overflow: "hidden", whiteSpace: "nowrap", textOverflow: "ellipsis", color: "var(--text-2)",
              cursor: it.locked ? "default" : "grab", userSelect: "none" }}>
            {!it.locked && <span onPointerDown={(e) => start(e, it, "l")} style={{ position: "absolute", left: 0, top: 0, bottom: 0, width: 6, cursor: "ew-resize" }} />}
            {it.label}
            {!it.locked && <span onPointerDown={(e) => start(e, it, "r")} style={{ position: "absolute", right: 0, top: 0, bottom: 0, width: 6, cursor: "ew-resize" }} />}
            {on && lane.onDelete && !it.locked && (
              <button title={tx("Delete (Delete)")} onPointerDown={(e) => { e.stopPropagation(); lane.onDelete!(it.id); }} style={{ position: "absolute", right: 6, top: 2, border: 0, background: "none", color: "var(--text-2)", cursor: "pointer", padding: 0 }}><X size={12} /></button>
            )}
          </div>
        );
      })}
    </>
  );
}
