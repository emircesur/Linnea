// Colour of a picture or a video layer: exposure, white balance, contrast, saturation and hue, three wheels
// (lift, gamma, gain) and a .cube LUT. linnea2d.js grades it in one WebGL pass, the same in the preview and
// in renders (it.grade).
import { tx } from "./i18n";
import { useRef } from "react";
import { Eye, EyeOff, RotateCcw, Trash2 } from "lucide-react";
import { Json } from "./api";
import { Item, Layers } from "./Layers";
import { NumField, Section, SelectField } from "./Inspector";

export type Wheel = { x?: number; y?: number; l?: number };
export type Grade = {
  exposure?: number; contrast?: number; saturation?: number; temp?: number; tint?: number; hue?: number;
  lift?: Wheel; gamma?: Wheel; gain?: Wheel; lut?: string; lutMix?: number; off?: boolean; look?: string;
};

/** Ready looks, as plain numbers: they can be changed after. */
export const GRADE_LOOKS: [string, string, Grade][] = [
  ["warm", tx("Warm"), { temp: 0.35, saturation: 0.08, gain: { x: 0.12, y: 0.08 } }],
  ["cool", tx("Cool"), { temp: -0.35, tint: 0.05, saturation: -0.05 }],
  ["teal", tx("Teal and orange"), { contrast: 0.1, saturation: 0.15, lift: { x: -0.3, y: 0 }, gain: { x: 0.24, y: 0.14 } }],
  ["bleach", tx("Bleach bypass"), { contrast: 0.35, saturation: -0.55 }],
  ["faded", tx("Faded film"), { contrast: -0.15, saturation: -0.2, lift: { l: 0.35, x: 0.05, y: 0.05 }, gain: { l: -0.1 } }],
  ["punchy", tx("Punchy"), { contrast: 0.25, saturation: 0.3 }],
  ["mono", tx("Black and white"), { saturation: -1, contrast: 0.15 }],
  ["night", tx("Day for night"), { exposure: -1.2, temp: -0.55, saturation: -0.35, lift: { x: -0.1, y: -0.2 } }],
];

const clean = (g: Grade): Grade | undefined => {
  const out: Json = {};
  for (const [k, v] of Object.entries(g)) {
    if (v === undefined || v === 0 || v === "" || v === false) continue;
    if (typeof v === "object" && !Object.values(v).some((x) => x)) continue;
    out[k] = v;
  }
  return Object.keys(out).length ? out : undefined;
};

/** A colour wheel: drag the dot towards a colour; double-click to put it back in the middle. */
function ColourWheel({ label, value, onLive, onChange }: { label: string; value?: Wheel; onLive: (w: Wheel) => void; onChange: (w: Wheel) => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const x = value?.x || 0, y = value?.y || 0;
  const at = (e: { clientX: number; clientY: number }, fine: boolean, from: Wheel): Wheel => {
    const r = ref.current!.getBoundingClientRect();
    let nx = ((e.clientX - r.left) / r.width) * 2 - 1, ny = -(((e.clientY - r.top) / r.height) * 2 - 1);
    if (fine) { nx = (from.x || 0) + (nx - (from.x || 0)) * 0.25; ny = (from.y || 0) + (ny - (from.y || 0)) * 0.25; }
    const d = Math.hypot(nx, ny);
    if (d > 1) { nx /= d; ny /= d; }
    return { ...value, x: Math.round(nx * 100) / 100, y: Math.round(ny * 100) / 100 };
  };
  const down = (e: React.PointerEvent) => {
    if (e.button !== 0) return;
    e.preventDefault();
    const from = { ...value };
    let cur = at(e, e.shiftKey, from);
    onLive(cur);
    const mv = (ev: PointerEvent) => { cur = at(ev, ev.shiftKey, from); onLive(cur); };
    const up = () => { window.removeEventListener("pointermove", mv); window.removeEventListener("pointerup", up); onChange(cur); };
    window.addEventListener("pointermove", mv);
    window.addEventListener("pointerup", up);
  };
  return (
    <div className="gw">
      <div ref={ref} className="gw-disc" onPointerDown={down} onDoubleClick={() => onChange({ ...value, x: 0, y: 0 })} title={tx("Drag towards a colour (Shift: finer). Double-click: back to the middle.")}>
        <span className="gw-dot" style={{ left: `${50 + x * 50}%`, top: `${50 - y * 50}%` }} />
      </div>
      <span className="gw-l">{label}</span>
      <input type="range" className="gw-r" min={-1} max={1} step={0.01} key={String(value?.l || 0)} defaultValue={value?.l || 0} title={tx("Brightness")}
        onChange={(e) => onLive({ ...value, l: +e.target.value })} onPointerUp={(e) => onChange({ ...value, l: +(e.target as HTMLInputElement).value })}
        onKeyUp={(e) => onChange({ ...value, l: +(e.target as HTMLInputElement).value })} onDoubleClick={() => onChange({ ...value, l: 0 })} />
    </div>
  );
}

export function GradeSection({ L, it }: { L: Layers; it: Item }) {
  const lutRef = useRef<HTMLInputElement>(null);
  const g: Grade = it.grade || {};
  const with_ = (p: Partial<Grade>) => clean({ ...g, ...p });
  const live = (p: Partial<Grade>) => L.preview(L.items.map((x) => (x.id === it.id ? { ...x, grade: with_(p) } : x)));
  const set = (p: Partial<Grade>) => L.update(L.items.map((x) => (x.id === it.id ? { ...x, grade: with_(p) } : x)));
  const n = (k: keyof Grade, label: string, step: number, min: number, max: number, title?: string) => (
    <NumField key={k} label={label} value={(g[k] as number) ?? 0} step={step} min={min} max={max} title={title} onLive={(v) => live({ [k]: v })} onChange={(v) => set({ [k]: v ?? 0 })} />
  );
  const any = !!clean({ ...g, off: undefined, look: undefined });
  const others = L.items.filter((x) => x.id !== it.id && (x.kind === "video" || x.kind === "image"));
  return (
    <Section name="grade" title={tx("Colour")} right={
      <>
        <button className="btn icon sm ghost" title={g.off ? tx("Show the grade") : tx("Show the original (compare)")} disabled={!any} onClick={() => set({ off: !g.off || undefined })}>{g.off ? <EyeOff /> : <Eye />}</button>
        <button className="btn icon sm ghost" title={tx("Reset the colour")} disabled={!any} onClick={() => L.update(L.items.map((x) => (x.id === it.id ? { ...x, grade: undefined } : x)))}><RotateCcw /></button>
      </>
    }>
      <SelectField label={tx("Look")} value={g.look || ""} options={[["", tx("Custom")], ...GRADE_LOOKS.map(([k, l]) => [k, l] as [string, string])]}
        onChange={(v) => { const lk = GRADE_LOOKS.find((x) => x[0] === v); L.update(L.items.map((x) => (x.id === it.id ? { ...x, grade: clean({ ...(lk ? lk[2] : {}), lut: g.lut, lutMix: g.lutMix, look: v || undefined }) } : x))); }} />
      <div className="g2">
        {n("exposure", tx("Exposure"), 0.05, -4, 4, tx("Stops of light: +1 is twice as bright"))}
        {n("contrast", tx("Contrast"), 0.01, -1, 1)}
        {n("temp", tx("Warmth"), 0.01, -1, 1, tx("Blue to orange"))}
        {n("tint", tx("Tint"), 0.01, -1, 1, tx("Green to magenta"))}
        {n("saturation", tx("Saturation"), 0.01, -1, 1)}
        {n("hue", tx("Hue °"), 1, -180, 180)}
      </div>
      <div className="gw-row">
        <ColourWheel label={tx("Shadows")} value={g.lift} onLive={(w) => live({ lift: w })} onChange={(w) => set({ lift: w })} />
        <ColourWheel label={tx("Midtones")} value={g.gamma} onLive={(w) => live({ gamma: w })} onChange={(w) => set({ gamma: w })} />
        <ColourWheel label={tx("Highlights")} value={g.gain} onLive={(w) => live({ gain: w })} onChange={(w) => set({ gain: w })} />
      </div>
      <div className="rowx" style={{ gap: 4 }}>
        <button className="btn sm grow" onClick={() => lutRef.current?.click()}>{g.lut ? tx("Replace LUT…") : tx("Load a LUT (.cube)…")}</button>
        {g.lut && <button className="btn icon sm ghost" title={tx("Remove the LUT")} onClick={() => set({ lut: undefined, lutMix: undefined })}><Trash2 /></button>}
        <input ref={lutRef} type="file" accept=".cube" hidden onChange={async (e) => { const f = e.target.files?.[0]; e.target.value = ""; if (f) set({ lut: await L.upload(f), lutMix: g.lutMix ?? 1 }); }} />
      </div>
      {g.lut && (
        <>
          <span className="muted" style={{ fontSize: 11, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{g.lut.split("/").pop()}</span>
          {n("lutMix", tx("LUT amount"), 0.01, 0, 1)}
        </>
      )}
      {any && others.length > 0 && (
        <button className="btn sm ghost" title={tx("Give every other picture and video this colour, to match them")}
          onClick={() => L.update(L.items.map((x) => (others.includes(x) ? { ...x, grade: clean({ ...g, off: undefined }) } : x)))}>{tx("Use on all {0} other pictures and videos", others.length)}</button>
      )}
      <span className="muted" style={{ fontSize: 11, lineHeight: "16px" }}>{tx("The grade is drawn the same in the preview and in renders. A LUT is applied last, after the numbers and the wheels.")}</span>
    </Section>
  );
}
