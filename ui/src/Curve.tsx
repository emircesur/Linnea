// The curve into a keyframe: pick a named ease, or drag the two handles of a cubic Bézier.
// The same curves linnea2d.js draws with (`ease`, or the key's own `bez: [x1, y1, x2, y2]`).
import { tx } from "./i18n";
import { useEffect, useRef, useState } from "react";
import { curve, EASE } from "./Motion";
import { Json } from "./api";

const PRESETS: [string, string, number[] | null][] = [
  ["linear", tx("Even"), null], ["inOut", tx("Smooth"), null], ["out", tx("Slow end"), null], ["in", tx("Slow start"), null], ["sine", tx("Gentle"), null],
  ["expo", tx("Snap"), null], ["back", tx("Overshoot"), null], ["elastic", tx("Spring"), null], ["bounce", tx("Bounce"), null], ["hold", tx("Hold, then jump"), null],
  ["bez:anticip", tx("Wind up"), [0.36, -0.45, 0.6, 1]], ["bez:swift", tx("Swift"), [0.55, 0, 0.1, 1]], ["bez:ease", "CSS ease", [0.25, 0.1, 0.25, 1]],
];

/** A small picture of a curve (for buttons). */
export function CurveIcon({ k, size = 18 }: { k: Json; size?: number }) {
  const f = curve(k);
  const pts = Array.from({ length: 25 }, (_, i) => { const x = i / 24; return `${2 + x * (size - 4)},${size - 2 - f(x) * (size - 4) * 0.8 - (size - 4) * 0.1}`; }).join(" ");
  return <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} style={{ flex: "none" }}><polyline points={pts} fill="none" stroke="currentColor" strokeWidth={1.5} /></svg>;
}

export function curveName(k: Json): string {
  if (Array.isArray(k.bez)) return PRESETS.find((p) => p[2] && p[2].join() === k.bez.join())?.[1] || tx("Custom");
  return PRESETS.find((p) => p[0] === (k.ease || "inOut"))?.[1] || k.ease;
}

export function CurveEditor({ value, onChange, close, onAll }: { value: Json; onChange: (k: { ease?: string; bez?: number[] }) => void; close: () => void; onAll?: () => void }) {
  const S = 180, P = 28;   // the square, and room above and below for overshoot
  const ref = useRef<SVGSVGElement>(null);
  const box = useRef<HTMLDivElement>(null);
  const start = Array.isArray(value.bez) ? value.bez : null;
  const [b, setB] = useState<number[]>(start || [0.42, 0, 0.58, 1]);
  const custom = !!start;
  useEffect(() => {
    const off = (e: Event) => { if (!box.current?.contains(e.target as Node)) close(); };
    const key = (e: KeyboardEvent) => { if (e.key === "Escape") { e.stopImmediatePropagation(); close(); } };
    setTimeout(() => window.addEventListener("pointerdown", off, true));
    window.addEventListener("keydown", key, true);
    return () => { window.removeEventListener("pointerdown", off, true); window.removeEventListener("keydown", key, true); };
  }, []);
  const X = (x: number) => P + x * S, Y = (y: number) => P + (1 - y) * S;
  const f = custom || b.join() !== (start || [0.42, 0, 0.58, 1]).join() ? curve({ bez: b }) : EASE[value.ease || "inOut"] || EASE.inOut;
  const path = Array.from({ length: 61 }, (_, i) => { const x = i / 60; return `${X(x)},${Y(f(x))}`; }).join(" ");
  const drag = (which: 0 | 1) => (e: React.PointerEvent) => {
    e.preventDefault();
    const r = ref.current!.getBoundingClientRect();
    const sc = r.width / (S + P * 2);
    const mv = (ev: PointerEvent) => {
      const x = Math.max(0, Math.min(1, ((ev.clientX - r.left) / sc - P) / S));
      const y = Math.max(-0.6, Math.min(1.6, 1 - ((ev.clientY - r.top) / sc - P) / S));
      const n = [...b];
      n[which * 2] = Math.round(x * 100) / 100;
      n[which * 2 + 1] = Math.round(y * 100) / 100;
      setB(n);
      b.splice(0, 4, ...n);
      moved = true;
    };
    let moved = false;
    // the key changes once, when the handle is let go (one step to undo)
    const up = () => { window.removeEventListener("pointermove", mv); window.removeEventListener("pointerup", up); if (moved) onChange({ ease: undefined, bez: [...b] }); };
    window.addEventListener("pointermove", mv);
    window.addEventListener("pointerup", up);
  };
  return (
    <div className="curve-pop" ref={box} onPointerDown={(e) => e.stopPropagation()}>
      <svg ref={ref} viewBox={`0 0 ${S + P * 2} ${S + P * 2}`} width={S + P * 2} height={S + P * 2} style={{ display: "block", touchAction: "none" }}>
        <rect x={X(0)} y={Y(1)} width={S} height={S} fill="var(--sunken)" stroke="var(--line-2)" />
        {[0.25, 0.5, 0.75].map((g) => <g key={g}><line x1={X(g)} x2={X(g)} y1={Y(0)} y2={Y(1)} stroke="var(--line)" /><line y1={Y(g)} y2={Y(g)} x1={X(0)} x2={X(1)} stroke="var(--line)" /></g>)}
        <polyline points={path} fill="none" stroke="var(--accent)" strokeWidth={2.5} />
        {(
          <>
            <line x1={X(0)} y1={Y(0)} x2={X(b[0])} y2={Y(b[1])} stroke="var(--text-3)" strokeDasharray="3 3" opacity={custom ? 1 : 0.4} />
            <line x1={X(1)} y1={Y(1)} x2={X(b[2])} y2={Y(b[3])} stroke="var(--text-3)" strokeDasharray="3 3" opacity={custom ? 1 : 0.4} />
            <circle cx={X(b[0])} cy={Y(b[1])} r={7} fill="#fff" stroke="var(--accent)" strokeWidth={2} style={{ cursor: "grab", opacity: custom ? 1 : 0.5 }} onPointerDown={drag(0)} />
            <circle cx={X(b[2])} cy={Y(b[3])} r={7} fill="#fff" stroke="var(--accent)" strokeWidth={2} style={{ cursor: "grab", opacity: custom ? 1 : 0.5 }} onPointerDown={drag(1)} />
          </>
        )}
      </svg>
      <div className="num muted" style={{ fontSize: 10, textAlign: "center" }}>{custom ? `cubic-bezier(${b.join(", ")})` : tx("Drag a handle for your own curve")}</div>
      <div className="curve-presets">
        {PRESETS.map(([k, l, bz]) => {
          const on = bz ? custom && b.join() === bz.join() : !custom && (value.ease || "inOut") === k;
          return (
            <button key={k} className={`chip ${on ? "on" : ""}`} onClick={() => { if (bz) { setB(bz); onChange({ ease: undefined, bez: bz }); } else onChange({ ease: k, bez: undefined }); }}>
              <CurveIcon k={bz ? { bez: bz } : { ease: k }} size={14} />{l}
            </button>
          );
        })}
      </div>
      {onAll && <button className="btn sm ghost" onClick={onAll}>{tx("Use this curve for every key of the layer")}</button>}
    </div>
  );
}
