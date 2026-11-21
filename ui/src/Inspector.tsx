// The properties of the selected layer, in sections: content, timing, place and size (each with its own
// keyframe diamond), look, effects, motion presets, keyframes, motion path and rig. Numbers can be typed,
// nudged with the arrow keys or scrubbed by dragging their label (Shift: ×10, Alt: ×0.1).
import { tx } from "./i18n";
import { useRef, useState } from "react";
import {
  AlignCenterHorizontal, AlignCenterVertical, AlignEndHorizontal, AlignEndVertical, AlignHorizontalSpaceAround, AlignStartHorizontal, AlignStartVertical,
  AlignVerticalSpaceAround, Group, ChevronDown, ChevronLeft, ChevronRight, Copy, Diamond, Eye, EyeOff, FlipHorizontal2, FlipVertical2, Lock, Maximize, Trash2, Unlock, Wand2, Bookmark,
} from "lucide-react";
import { Json, tool } from "./api";
import { evalItem, Item, Layers } from "./Layers";
import {
  align, ANIMS, animated, BLENDS, copyStyle, distribute, duplicate, EASES, FONTS, keyAll, keyAt, keyTimes, kindLabel, layerName, LOOKS, motion, MOTIONS, moveKeyTime,
  paramsAt, pasteStyle, PROPS, r2, remove, replace, savePreset, setParam, setProp, toggleKey, toggleParamKey, ungroup, group,
} from "./LayerOps";
import { PuppetFields, PuppetInfo } from "./Puppets";
import { COLORS, hexOf, STYLE_NUM, styled } from "./Motion";
import { CurveEditor, CurveIcon, curveName } from "./Curve";

// ---------- small fields ----------
export function NumField({ label, value, onChange, onLive, step = 1, min, max, title, keyState, onKey, wide }: {
  label: string; value: number | undefined; onChange: (v: number | undefined) => void; onLive?: (v: number) => void; step?: number; min?: number; max?: number; title?: string;
  keyState?: "" | "anim" | "on"; onKey?: () => void; wide?: boolean;
}) {
  const [txt, setTxt] = useState<string | null>(null);
  const clamp = (v: number) => Math.max(min ?? -Infinity, Math.min(max ?? Infinity, v));
  const dec = step < 0.1 ? 2 : step < 1 ? 1 : 0;
  const fmt = (v: number | undefined) => (v === undefined || v === null || !isFinite(v) ? "" : String(Math.round(v * 10 ** dec) / 10 ** dec));
  const commit = (s: string) => {
    setTxt(null);
    if (s.trim() === "") return onChange(undefined);
    // simple sums are fine: "100+20", "1080/2"
    let v = Number(s);
    if (!isFinite(v) && /^[\d+\-*/. ()]+$/.test(s)) { try { v = Function(`return (${s})`)(); } catch { return; } }
    if (isFinite(v) && v !== value) onChange(clamp(v));
  };
  const scrub = (e: React.PointerEvent) => {
    if (e.button !== 0) return;
    e.preventDefault();
    const x0 = e.clientX, v0 = value ?? 0;
    let v = v0, moved = false;
    const mv = (ev: PointerEvent) => {
      const f = ev.shiftKey ? 10 : ev.altKey ? 0.1 : 1;
      const n = clamp(Math.round((v0 + ((ev.clientX - x0) / 2) * step * f) / (step * (ev.altKey ? 0.1 : 1))) * step * (ev.altKey ? 0.1 : 1));
      if (Math.abs(ev.clientX - x0) > 2) moved = true;
      if (moved && n !== v) { v = n; setTxt(fmt(n)); onLive ? onLive(n) : undefined; }
    };
    const up = () => {
      window.removeEventListener("pointermove", mv);
      window.removeEventListener("pointerup", up);
      document.body.style.cursor = "";
      setTxt(null);
      if (moved && v !== v0) onChange(r2(v));
    };
    document.body.style.cursor = "ew-resize";
    window.addEventListener("pointermove", mv);
    window.addEventListener("pointerup", up);
  };
  return (
    <div className={`nf ${wide ? "wide" : ""}`} title={title}>
      <span className="nf-l" onPointerDown={scrub}>{label}</span>
      <input className="num" value={txt ?? fmt(value)} onFocus={(e) => e.target.select()} onChange={(e) => setTxt(e.target.value)} onBlur={(e) => txt !== null && commit(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") (e.target as HTMLInputElement).blur();
          else if (e.key === "Escape") { setTxt(null); (e.target as HTMLInputElement).blur(); }
          else if (e.key === "ArrowUp" || e.key === "ArrowDown") { e.preventDefault(); const f = e.shiftKey ? 10 : e.altKey ? 0.1 : 1; const n = clamp(r2((value ?? 0) + (e.key === "ArrowUp" ? 1 : -1) * step * f)); setTxt(null); onChange(n); }
        }} />
      {onKey && <button className={`nf-k ${keyState}`} title={keyState === "on" ? tx("Remove the key at this second") : tx("Add a key at this second")} onClick={onKey}><Diamond /></button>}
    </div>
  );
}

export function ColorField({ label, value, onChange, fallback = "#ffffff", keyState, onKey }: { label: string; value?: string; onChange: (v: string | undefined) => void; fallback?: string; keyState?: "" | "anim" | "on"; onKey?: () => void }) {
  const hex = hexOf(value) || fallback;
  return (
    <div className="nf wide cf">
      <span className="nf-l">{label}</span>
      <input type="color" value={hex} onChange={(e) => onChange(e.target.value)} />
      <input className="num" key={value} defaultValue={value || ""} placeholder={tx("none")} onBlur={(e) => e.target.value !== (value || "") && onChange(e.target.value || undefined)} onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()} />
      {value && !keyState && <button className="nf-k" title={tx("None")} onClick={() => onChange(undefined)}><Trash2 /></button>}
      {onKey && <button className={`nf-k ${keyState}`} title={keyState === "on" ? tx("Remove the key at this second") : tx("Key this colour at this second: it changes smoothly between keys")} onClick={onKey}><Diamond /></button>}
    </div>
  );
}

export function SelectField({ label, value, options, onChange }: { label: string; value: string; options: [string, string][]; onChange: (v: string) => void }) {
  return (
    <div className="nf wide">
      <span className="nf-l">{label}</span>
      <select value={value} onChange={(e) => onChange(e.target.value)}>{options.map(([v, l]) => <option key={v} value={v}>{l}</option>)}</select>
    </div>
  );
}

export function Section({ name, title, children, right, open: dflt = true }: { name: string; title: string; children: any; right?: any; open?: boolean }) {
  const key = "linnea.sec." + name;
  const [open, setOpen] = useState(() => { try { const v = localStorage.getItem(key); return v === null ? dflt : v === "1"; } catch { return dflt; } });
  const toggle = () => { setOpen(!open); try { localStorage.setItem(key, open ? "0" : "1"); } catch { /* private window */ } };
  return (
    <div className={`sec2 ${open ? "open" : ""}`}>
      <div className="sec2-h" onClick={toggle}>
        <ChevronDown className="chev" />
        <span className="grow">{title}</span>
        <span onClick={(e) => e.stopPropagation()} className="rowx" style={{ gap: 2 }}>{right}</span>
      </div>
      {open && <div className="sec2-b">{children}</div>}
    </div>
  );
}

const IconBtn = ({ title, onClick, on, children, disabled }: { title: string; onClick: () => void; on?: boolean; children: any; disabled?: boolean }) => (
  <button className={`btn icon sm ${on ? "" : "ghost"}`} title={title} onClick={onClick} disabled={disabled} style={on ? { color: "var(--accent)" } : undefined}>{children}</button>
);

// ---------- the inspector of one layer ----------
export function LayerInspector({ L, id, t, W, H, dur, seek, pathFor, setPathFor, puppetInfo, onLipsync, say, select }: {
  L: Layers; id: string; t: number; W: number; H: number; dur: number; seek?: (t: number) => void; pathFor?: string; setPathFor?: (s: string) => void;
  puppetInfo?: (name: string) => PuppetInfo | null; onLipsync?: () => void; say?: (s: string) => void; select?: (ids: string[]) => void;
}) {
  const items = L.items;
  const it = items.find((x) => x.id === id);
  const [motionPick, setMotionPick] = useState("rise");
  const [curveAt, setCurveAt] = useState(-1);
  const picRef = useRef<HTMLInputElement>(null);
  if (!it) return null;
  const put = (f: (x: Item) => Item) => L.update(items.map((x) => (x.id === id ? f(x) : x)));
  const set = (p: Json) => put((x) => ({ ...x, ...p }));
  const s = evalItem(items, it, t);
  const own = evalItem(items, { ...it, parent: undefined }, t);
  const on = t >= it.t0 && t < it.t1;
  const prop = (p: (typeof PROPS)[number], label: string, step = 1, extra: Json = {}) => {
    const k = keyAt(it, t) as Json | undefined;
    const ks: "" | "anim" | "on" = k && k[p] !== undefined ? "on" : animated(it, p) ? "anim" : "";
    return <NumField key={p} label={label} value={own[p]} step={step} {...extra} keyState={ks} onKey={() => put((x) => toggleKey(items, x, t, p))}
      onLive={(v) => L.preview(items.map((x) => (x.id === id ? setProp(items, x, t, p, v) : x)))}
      onChange={(v) => v !== undefined && put((x) => setProp(items, x, t, p, v))} />;
  };
  // style numbers and colours can be keyed too: a diamond each, and typing keys them once they are animated
  const look = styled(it, t);
  const ksOf = (k: string): "" | "anim" | "on" => { const kk = keyAt(it, t) as Json | undefined; return kk && kk[k] !== undefined ? "on" : animated(it, k) ? "anim" : ""; };
  const num = (k: string, label: string, step = 1, extra: Json = {}) => {
    const keyable = STYLE_NUM.includes(k);
    return <NumField key={k} label={label} value={(keyable ? look[k] : it[k]) ?? extra.dflt} step={step} {...extra} onLive={(v) => L.preview(items.map((x) => (x.id === id ? setProp(items, x, t, k, v) : x)))} onChange={(v) => put((x) => setProp(items, x, t, k, v))}
      {...(keyable ? { keyState: ksOf(k), onKey: () => put((x) => toggleKey(items, x, t, k, extra.fallback ?? 0)) } : {})} />;
  };
  const col = (k: string, label: string, fb?: string) => <ColorField key={k} label={label} value={COLORS.includes(k) ? look[k] : it[k]} fallback={fb} onChange={(v) => put((x) => setProp(items, x, t, k, v))}
    {...(COLORS.includes(k) ? { keyState: ksOf(k), onKey: () => put((x) => toggleKey(items, x, t, k, fb)) } : {})} />;
  const [curveFor, setCurveFor] = [curveAt, setCurveAt];
  const str = (k: string, label: string) => (
    <div className="nf wide" key={k}><span className="nf-l">{label}</span><input key={`${id}${k}`} defaultValue={it[k] ?? ""} onChange={(e) => set({ [k]: e.target.value })} /></div>
  );
  const textual = ["text", "marker", "counter", "lower"].includes(it.kind);
  const shape = ["rect", "ellipse", "text", "image"].includes(it.kind);
  const times = keyTimes(it);
  const prevKey = times.filter((x) => x < t - 0.02).pop(), nextKey = times.find((x) => x > t + 0.02);
  const info = it.kind === "puppet" ? puppetInfo?.(it.puppet) || null : null;
  const pickPicture = async () => {
    const r = await tool("pick_file");
    if (!r.path) return;
    set({ src: await L.addImage(r.path) });
  };
  return (
    <div className="insp">
      <div className="insp-h">
        <span className="tag">{kindLabel(it.kind)}</span>
        <input className="insp-name" key={id} defaultValue={it.label || ""} placeholder={layerName(it)} onBlur={(e) => e.target.value !== (it.label || "") && set({ label: e.target.value || undefined })} onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()} />
        <IconBtn title={it.hidden ? tx("Show (H)") : tx("Hide (H)")} onClick={() => set({ hidden: !it.hidden || undefined })}>{it.hidden ? <EyeOff /> : <Eye />}</IconBtn>
        <IconBtn title={it.locked ? tx("Unlock (Ctrl+L)") : tx("Lock on the frame (Ctrl+L)")} on={!!it.locked} onClick={() => set({ locked: !it.locked || undefined })}>{it.locked ? <Lock /> : <Unlock />}</IconBtn>
        <IconBtn title={tx("Duplicate (Ctrl+D)")} onClick={() => { const r = duplicate(items, [id]); L.update(r.items); select?.(r.ids); }}><Copy /></IconBtn>
        <IconBtn title={tx("Delete (Delete)")} onClick={() => { L.update(remove(items, [id])); select?.([]); }}><Trash2 /></IconBtn>
      </div>
      {!on && <div className="hint warn">{tx("Not on screen at this second.")} <button className="linkish" onClick={() => seek?.(it.t0 + 0.05)}>{tx("Go to its start")}</button></div>}

      {/* ---- a group: what is in it ---- */}
      {it.kind === "group" && (
        <Section name="group" title={tx("In this group")}>
          {items.filter((x) => x.parent === id).map((x) => <button key={x.id} className="lrow" style={{ width: "100%" }} onClick={() => select?.([x.id])}><span className="lrow-name">{layerName(x)}</span><span className="lrow-t num">{kindLabel(x.kind)}</span></button>)}
          <span className="muted" style={{ fontSize: 12, lineHeight: "18px" }}>{tx("Moving, turning, scaling or fading the group does it to everything in it; its time range and its eye hide them too. Click a layer in it on the frame a second time to change that layer alone.")}</span>
          <button className="btn sm" onClick={() => { const r = ungroup(items, id); L.update(r.items); select?.(r.ids); }}>{tx("Ungroup (Ctrl+Shift+G)")}</button>
        </Section>
      )}
      {/* ---- a video ---- */}
      {it.kind === "video" && (
        <Section name="video" title={tx("Video")} right={it.fileDur ? <span className="muted num" style={{ fontSize: 11 }}>{tx("file {0} s", it.fileDur)}</span> : null}>
          <div className="rowx">
            <button className="btn sm grow" onClick={async () => { const r = await tool("pick_file"); if (r.path) set({ src: await L.addImage(r.path) }); }}>{tx("Replace video…")}</button>
          </div>
          <div className="g2">
            {num("in", tx("From s"), 0.1, { min: 0, dflt: 0, title: tx("Where in the file the layer starts") })}
            {num("speed", tx("Speed ×"), 0.05, { min: 0.1, max: 4, dflt: 1 })}
          </div>
          <SelectField label={tx("Fit")} value={it.fit || "fill"} options={[["fill", tx("Stretch")], ["cover", tx("Fill (crop)")], ["contain", tx("Fit inside")]]} onChange={(v) => set({ fit: v === "fill" ? undefined : v })} />
          <div className="rowx" style={{ gap: 12, flexWrap: "wrap" }}>
            <label className="rowx" style={{ fontSize: 12, gap: 6 }}><input type="checkbox" checked={!!it.loop} onChange={(e) => set({ loop: e.target.checked || undefined })} />{tx("Loop")}</label>
            <label className="rowx" style={{ fontSize: 12, gap: 6 }}><input type="checkbox" checked={it.sound !== false} onChange={(e) => set({ sound: e.target.checked })} />{tx("Its sound in the film")}</label>
          </div>
          {it.sound !== false && num("gain", tx("Volume dB"), 1, { min: -60, max: 20, wide: true, dflt: 0 })}
          {it.fileDur ? <button className="btn sm ghost" onClick={() => set({ t1: r2(Math.min(dur, it.t0 + Math.max(0.1, (it.fileDur - (it.in || 0)) / (it.speed || 1)))) })}>{tx("Length to the end of the video")}</button> : null}
          <span className="muted" style={{ fontSize: 11, lineHeight: "16px" }}>{tx("Renders show the exact frame of the video at every frame of the film. Its sound is a linked clip in the Sound tab and follows the layer.")}</span>
        </Section>
      )}

      {/* ---- content ---- */}
      {(textual || ["phone", "progress", "image", "puppet"].includes(it.kind)) && (
        <Section name="content" title={tx("Content")}>
          {textual && it.kind !== "counter" && <textarea className="input" style={{ fontFamily: "var(--font)", fontSize: 13, height: "auto" }} rows={2} key={`${id}text`} defaultValue={it.text} onChange={(e) => set({ text: e.target.value })} />}
          {it.kind === "lower" && str("sub", tx("Second line"))}
          {it.kind === "phone" && <>{str("title", tx("Title"))}{str("text", tx("Notification"))}</>}
          {it.kind === "counter" && <div className="g2">{num("from", tx("From"))}{num("to", tx("To"))}{num("decimals", tx("Decimals"), 1, { min: 0, max: 4 })}<span />{str("prefix", tx("Before"))}{str("suffix", tx("After"))}</div>}
          {it.kind === "progress" && num("value", tx("Fill 0–1"), 0.05, { min: 0, max: 1, wide: true })}
          {(it.kind === "image" || it.kind === "phone") && (
            <div className="rowx">
              <button className="btn sm grow" onClick={pickPicture}>{it.src ? tx("Replace picture…") : tx("Choose picture…")}</button>
              <input ref={picRef} type="file" accept="image/*" hidden onChange={async (e) => { const f = e.target.files?.[0]; e.target.value = ""; if (f) set({ src: await L.upload(f) }); }} />
              <button className="btn sm ghost" onClick={() => picRef.current?.click()}>{tx("Upload")}</button>
            </div>
          )}
          {it.kind === "image" && <SelectField label={tx("Fit")} value={it.fit || "fill"} options={[["fill", tx("Stretch")], ["cover", tx("Fill (crop)")], ["contain", tx("Fit inside")]]} onChange={(v) => set({ fit: v === "fill" ? undefined : v })} />}
          {it.kind === "puppet" && (
            <>
              <PuppetFields info={info} params={paramsAt(it, t)} states={it.states || []} localT={t - it.t0} onLipsync={onLipsync}
                onParams={(p) => put((x) => { let n = x; for (const [k2, v] of Object.entries(p)) if (paramsAt(x, t)[k2] !== v) n = setParam(n, t, k2, v); return n; })}
                onStates={(states) => set({ states })}
                keyed={(k2) => { const pk = (it.pkeys || []).find((q: Json) => Math.abs(q.t - t) <= 0.02); return pk?.p?.[k2] !== undefined ? "on" : (it.pkeys || []).some((q: Json) => q.p?.[k2] !== undefined) ? "anim" : ""; }}
                onKey={(k2) => put((x) => toggleParamKey(x, t, k2, paramsAt(x, t)[k2] ?? info?.params?.[k2]?.default))} />
              <div className="g2">
                {num("speed", tx("Speed ×"), 0.05, { min: 0.05, max: 8, dflt: 1 })}
                <div className="rowx" style={{ gap: 2 }}>
                  <IconBtn title={tx("Mirror left–right")} on={!!it.fx} onClick={() => set({ fx: !it.fx || undefined })}><FlipHorizontal2 /></IconBtn>
                  <IconBtn title={tx("Mirror up–down")} on={!!it.fy} onClick={() => set({ fy: !it.fy || undefined })}><FlipVertical2 /></IconBtn>
                </div>
              </div>
            </>
          )}
        </Section>
      )}

      {/* ---- timing ---- */}
      <Section name="timing" title={tx("Timing")} right={<span className="muted num" style={{ fontSize: 11 }}>{r2(it.t1 - it.t0)} {tx("s")}</span>}>
        <div className="g3">
          <NumField label={tx("In")} value={it.t0} step={0.1} min={0} max={it.t1 - 0.05} onChange={(v) => v !== undefined && set({ t0: r2(v) })} />
          <NumField label={tx("Out")} value={it.t1} step={0.1} min={it.t0 + 0.05} max={dur} onChange={(v) => v !== undefined && set({ t1: r2(v) })} />
          <NumField label={tx("Len")} value={r2(it.t1 - it.t0)} step={0.1} min={0.05} onChange={(v) => v !== undefined && set({ t1: r2(Math.min(dur, it.t0 + v)) })} />
        </div>
        <div className="rowx" style={{ gap: 4, flexWrap: "wrap" }}>
          <button className="btn sm ghost" title="Alt+[" onClick={() => t < it.t1 - 0.05 && set({ t0: r2(t) })}>{tx("Start here")}</button>
          <button className="btn sm ghost" title="Alt+]" onClick={() => t > it.t0 + 0.05 && set({ t1: r2(t) })}>{tx("End here")}</button>
          <button className="btn sm ghost" onClick={() => set({ t0: 0, t1: r2(dur) })}>{tx("Whole film")}</button>
        </div>
        <div className="g2">
          <SelectField label={tx("Enter")} value={it.anim || "fade"} options={ANIMS} onChange={(v) => set({ anim: v === "fade" ? undefined : v })} />
          <SelectField label={tx("Leave")} value={it.animOut || ""} options={[["", tx("Same as enter")], ...ANIMS]} onChange={(v) => set({ animOut: v || undefined })} />
          <NumField label={tx("Ease s")} value={it.fade ?? 0.25} step={0.05} min={0} title={tx("How long the way in and the way out take")} onChange={(v) => set({ fade: v })} />
          {(["counter", "marker", "arrow", "phone", "lower", "progress"].includes(it.kind) || it.typing) && num("reveal", tx("Draw s"), 0.1, { min: 0.05, title: tx("How long it takes to draw on") })}
        </div>
        {it.kind === "text" && <SelectField label={tx("Typing")} value={it.typing || ""} options={[["", tx("All at once")], ["chars", tx("Letter by letter")], ["words", tx("Word by word")]]} onChange={(v) => set({ typing: v || undefined, reveal: v && !it.reveal ? 1.2 : it.reveal })} />}
      </Section>

      {/* ---- transform ---- */}
      <Section name="transform" title={tx("Place and size")} right={
        <>
          <button className="btn icon sm ghost" title={tx("Previous key")} disabled={prevKey === undefined} onClick={() => prevKey !== undefined && seek?.(prevKey)}><ChevronLeft /></button>
          <button className="btn icon sm ghost" title={tx("Key everything at this second")} onClick={() => put((x) => keyAll(items, x, t))}><Diamond /></button>
          <button className="btn icon sm ghost" title={tx("Next key")} disabled={nextKey === undefined} onClick={() => nextKey !== undefined && seek?.(nextKey)}><ChevronRight /></button>
        </>
      }>
        {Array.isArray(it.path) && it.path.length >= 2 && <div className="hint">{tx("It follows a motion path, so X and Y come from the path.")}</div>}
        <div className="g2">
          {prop("x", "X")}{prop("y", "Y")}
          {it.kind !== "group" && prop("w", "W", 1, { min: 1 })}{it.kind !== "group" && prop("h", "H", 1, { min: 1 })}
          {prop("rot", tx("Angle"), 1)}{prop("scale", tx("Scale"), 0.01, { min: 0 })}
          {prop("opacity", tx("Opacity"), 0.01, { min: 0, max: 1 })}
          <div className="rowx" style={{ gap: 2, justifyContent: "flex-end" }}>
            {it.kind !== "puppet" && <IconBtn title={tx("Mirror left–right")} on={!!it.fx} onClick={() => set({ fx: !it.fx || undefined })}><FlipHorizontal2 /></IconBtn>}
            {it.kind !== "puppet" && <IconBtn title={tx("Mirror up–down")} on={!!it.fy} onClick={() => set({ fy: !it.fy || undefined })}><FlipVertical2 /></IconBtn>}
            <IconBtn title={tx("Fill the frame")} onClick={() => put((x) => ["x", "y", "w", "h"].reduce((n, p) => setProp(items, n, t, p, ({ x: 0, y: 0, w: W, h: H } as Json)[p]), x))}><Maximize /></IconBtn>
          </div>
        </div>
        <AlignRow onAlign={(how) => L.update(align(items, [id], how, W, H, t))} frame />
        {s.x !== own.x || s.y !== own.y ? <div className="hint">{tx("Moves with its parent: values are before the parent's motion.")}</div> : null}
      </Section>

      {/* ---- look ---- */}
      {(textual || shape || ["arrow", "progress", "phone"].includes(it.kind)) && (
        <Section name="look" title={tx("Look")} right={
          <>
            <button className="btn sm ghost" title={tx("Copy the look")} onClick={() => { copyStyle(it); say?.(tx("Look copied")); }}>{tx("Copy")}</button>
            <button className="btn sm ghost" title={tx("Paste the look")} onClick={() => put((x) => pasteStyle(x))}>{tx("Paste")}</button>
          </>
        }>
          {textual && (
            <>
              <SelectField label={tx("Font")} value={it.font || ""} options={FONTS.some(([f]) => f === (it.font || "")) ? FONTS : [...FONTS, [it.font, it.font]]} onChange={(v) => set({ font: v || undefined })} />
              <div className="g2">
                {it.kind !== "lower" && num("size", tx("Size"), 1, { min: 4 })}
                {it.kind !== "lower" && num("weight", tx("Weight"), 100, { min: 100, max: 900 })}
                {num("spacing", tx("Spacing"), 0.5)}
                {num("lineHeight", tx("Line h."), 0.05, { min: 0.5, max: 3 })}
              </div>
              <div className="rowx" style={{ gap: 4 }}>
                <div className="seg">
                  {(["left", "center", "right"] as const).map((a) => <button key={a} className={(it.align || "center") === a ? "on" : ""} onClick={() => set({ align: a === "center" ? undefined : a })}>{a === "left" ? tx("Left") : a === "center" ? tx("Centre") : tx("Right")}</button>)}
                </div>
                <span className="grow" />
                <IconBtn title={tx("Italic")} on={!!it.italic} onClick={() => set({ italic: !it.italic || undefined })}><i style={{ fontWeight: 700, fontFamily: "Georgia, serif" }}>I</i></IconBtn>
                <IconBtn title={tx("Capitals")} on={!!it.upper} onClick={() => set({ upper: !it.upper || undefined })}><b style={{ fontSize: 11 }}>AA</b></IconBtn>
              </div>
              {col("color", tx("Text"))}
              {it.kind !== "lower" && <div className="g2">{col("stroke", tx("Outline"), "#000000")}{num("strokeWidth", tx("Outline px"), 1, { min: 0 })}</div>}
            </>
          )}
          {it.kind === "lower" && <>{col("bg", tx("Bar"), "#121212")}{col("accent", tx("Accent"), "#e9b84a")}{col("subColor", tx("Second line"), "#b0b0b0")}</>}
          {(it.kind === "rect" || it.kind === "ellipse") && col(it.kind === "rect" ? "bg" : "color", tx("Fill"), "#e9b84a")}
          {(it.kind === "text" || it.kind === "marker" || it.kind === "counter" || it.kind === "image") && col("bg", tx("Background"), "#000000")}
          {(it.kind === "rect" || it.kind === "ellipse" || ((it.kind === "text") && it.bg)) && <div className="g2">{col("bg2", tx("Gradient to"), "#000000")}{it.bg2 ? num("gradAngle", tx("Angle"), 5) : <span />}</div>}
          {(shape || it.kind === "progress") && it.kind !== "ellipse" && num("radius", tx("Corners"), 1, { min: 0, wide: true })}
          {shape && <div className="g2">{col("border", tx("Border"), "#ffffff")}{it.border ? num("borderWidth", tx("Border px"), 1, { min: 0 }) : <span />}</div>}
          {it.kind === "arrow" && <div className="g2">{col("color", tx("Colour"), "#e9b84a")}{num("width", tx("Line px"), 1, { min: 1 })}</div>}
          {it.kind === "progress" && <>{col("color", tx("Fill"), "#e9b84a")}{col("bg", tx("Track"), "#333333")}</>}
          {it.kind === "phone" && <>{col("frame", tx("Frame"), "#0d0d0d")}{col("screen", tx("Screen"), "#f2f2f2")}</>}
          {it.kind === "text" && (
            <div className="looks">
              {LOOKS.map(([k2, l, p]) => <button key={k2} className="btn sm" onClick={() => set(p)}>{l}</button>)}
            </div>
          )}
        </Section>
      )}

      {/* ---- effects ---- */}
      {it.kind !== "group" && <Section name="effects" title={tx("Effects")} open={false}>
        <div className="g2">{col("shadow", tx("Shadow"), "#000000")}{it.shadow ? num("shadowBlur", tx("Softness"), 1, { min: 0 }) : <span />}</div>
        {it.shadow && <div className="g2">{num("shadowX", tx("Shadow X"))}{num("shadowY", tx("Shadow Y"))}</div>}
        <div className="g2">
          {num("blur", tx("Blur px"), 0.5, { min: 0 })}
          <SelectField label={tx("Blend")} value={it.blend || "normal"} options={BLENDS} onChange={(v) => set({ blend: v === "normal" ? undefined : v })} />
        </div>
      </Section>}

      {/* ---- animation ---- */}
      <Section name="animate" title={tx("Animation")} right={times.length ? <span className="muted num" style={{ fontSize: 11 }}>{tx("{0} keys", times.length)}</span> : null}>
        <div className="rowx" style={{ gap: 4 }}>
          <select className="select grow" style={{ height: 28 }} value={motionPick} onChange={(e) => setMotionPick(e.target.value)}>{MOTIONS.map(([k2, l]) => <option key={k2} value={k2}>{l}</option>)}</select>
          <button className="btn sm" onClick={() => { put((x) => motion(items, x, motionPick, W)); say?.(tx("Motion added over the layer's time")); }}><Wand2 />{tx("Apply")}</button>
        </div>
        {(it.keys || []).length === 0 && <span className="muted" style={{ fontSize: 12, lineHeight: "18px" }}>{tx("Click a diamond next to X, Y, Angle… to key that value at this second, then move the playhead and change it. Dragging on the frame then keys it too.")}</span>}
        {(it.keys || []).map((k2, i) => (
          <div key={i} className={`keyrow ${Math.abs(k2.t - t) <= 0.02 ? "on" : ""}`}>
            <button className="linkish num" style={{ width: 52 }} onClick={() => seek?.(k2.t)}>{k2.t} {tx("s")}</button>
            <span className="muted num grow" style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }} title={Object.keys(k2).filter((p) => !["t", "ease", "bez"].includes(p)).join(", ")}>{Object.keys(k2).filter((p) => !["t", "ease", "bez"].includes(p)).map((p) => (typeof (k2 as Json)[p] === "number" ? `${p} ${(k2 as Json)[p]}` : p)).join(" · ")}</span>
            <span style={{ position: "relative" }}>
              <button className="btn sm ghost curve-btn" title={tx("The curve into this key")} onClick={() => setCurveFor(curveFor === i ? -1 : i)}><CurveIcon k={k2} />{curveName(k2)}</button>
              {curveFor === i && <CurveEditor value={k2} close={() => setCurveFor(-1)}
                onChange={(c) => set({ keys: it.keys!.map((x, j) => (j === i ? { ...x, ease: c.ease, bez: c.bez } : x)) })}
                onAll={() => { set({ keys: it.keys!.map((x) => ({ ...x, ease: k2.ease, bez: k2.bez })) }); setCurveFor(-1); }} />}
            </span>
            <button className="btn icon sm ghost" title={tx("Move this key to the playhead")} onClick={() => put((x) => moveKeyTime(x, k2.t, t))}><ChevronDown style={{ transform: "rotate(-90deg)" }} /></button>
            <button className="btn icon sm ghost" onClick={() => set({ keys: it.keys!.filter((_, j) => j !== i).length ? it.keys!.filter((_, j) => j !== i) : undefined })}><Trash2 /></button>
          </div>
        ))}
        {(it.keys?.length || it.pkeys?.length) ? <button className="btn sm ghost" onClick={() => set({ keys: undefined, pkeys: undefined })}><Trash2 />{tx("Remove all keys")}</button> : null}
        {setPathFor && (
          <div className="stack" style={{ gap: 6, marginTop: 4 }}>
            <div className="rowx">
              <span className="grow sub-h">{tx("Motion path")}</span>
              <button className={`btn sm ${pathFor === id ? "primary" : ""}`} onClick={() => setPathFor(pathFor === id ? "" : id)}>{pathFor === id ? tx("Finish") : it.path?.length ? tx("Add points") : tx("Draw path")}</button>
              {it.path?.length ? <button className="btn icon sm ghost" title={tx("Delete path")} onClick={() => { set({ path: undefined }); setPathFor(""); }}><Trash2 /></button> : null}
            </div>
            {pathFor === id && <span className="muted" style={{ fontSize: 12 }}>{tx("Click the path's points on the frame in order; press Done when finished.")}</span>}
            {it.path?.length >= 2 && (
              <div className="g2">
                <SelectField label={tx("Speed")} value={it.pathEase || "inOut"} options={EASES.map((e) => [e, e])} onChange={(v) => set({ pathEase: v })} />
                <label className="rowx" style={{ fontSize: 12, gap: 6 }}><input type="checkbox" checked={!!it.pathTurn} onChange={(e) => set({ pathTurn: e.target.checked || undefined })} />{tx("Face its direction")}</label>
              </div>
            )}
          </div>
        )}
        <SelectField label={tx("Parent")} value={it.parent || ""} options={[["", tx("None")], ...items.filter((x) => x.id !== id && x.parent !== id).map((x) => [x.id, layerName(x)] as [string, string])]} onChange={(v) => set({ parent: v || undefined })} />
      </Section>

      <button className="btn sm ghost" onClick={() => { const n = window.prompt(tx("Name of the preset"), layerName(it)); if (n) { savePreset(n, it); say?.(tx("Saved to your presets in Add")); } }}><Bookmark />{tx("Save as my preset")}</button>
    </div>
  );
}

export function AlignRow({ onAlign, frame }: { onAlign: (how: "left" | "hcenter" | "right" | "top" | "vcenter" | "bottom") => void; frame?: boolean }) {
  const p = frame ? tx(" (to the frame)") : "";
  return (
    <div className="rowx" style={{ gap: 0, justifyContent: "space-between" }}>
      <button className="btn icon sm ghost" title={tx("Align left") + p} onClick={() => onAlign("left")}><AlignStartVertical /></button>
      <button className="btn icon sm ghost" title={tx("Centre horizontally") + p} onClick={() => onAlign("hcenter")}><AlignCenterVertical /></button>
      <button className="btn icon sm ghost" title={tx("Align right") + p} onClick={() => onAlign("right")}><AlignEndVertical /></button>
      <button className="btn icon sm ghost" title={tx("Align top") + p} onClick={() => onAlign("top")}><AlignStartHorizontal /></button>
      <button className="btn icon sm ghost" title={tx("Centre vertically") + p} onClick={() => onAlign("vcenter")}><AlignCenterHorizontal /></button>
      <button className="btn icon sm ghost" title={tx("Align bottom") + p} onClick={() => onAlign("bottom")}><AlignEndHorizontal /></button>
    </div>
  );
}

/** Several layers at once: line them up, spread them, time them together. */
export function MultiInspector({ L, ids, t, W, H, dur, select }: { L: Layers; ids: string[]; t: number; W: number; H: number; dur: number; select: (ids: string[]) => void }) {
  const items = L.items;
  const picked = items.filter((x) => ids.includes(x.id));
  const all = (p: Json) => L.update(replace(items, ids, (x) => ({ ...x, ...p })));
  return (
    <div className="insp">
      <div className="insp-h"><span className="grow" style={{ fontWeight: 600 }}>{tx("{0} layers selected", picked.length)}</span>
        <IconBtn title={tx("Duplicate (Ctrl+D)")} onClick={() => { const r = duplicate(items, ids); L.update(r.items); select(r.ids); }}><Copy /></IconBtn>
        <IconBtn title={tx("Delete (Delete)")} onClick={() => { L.update(remove(items, ids)); select([]); }}><Trash2 /></IconBtn>
      </div>
      <button className="btn sm primary" onClick={() => { const r = group(items, ids, t); if (r.id) { L.update(r.items); select([r.id]); } }}><Group />{tx("Group them (Ctrl+G)")}</button>
      <Section name="m-align" title={tx("Arrange")}>
        <AlignRow onAlign={(how) => L.update(align(items, ids, how, W, H, t))} />
        <div className="rowx" style={{ gap: 4 }}>
          <button className="btn sm grow" disabled={picked.length < 3} onClick={() => L.update(distribute(items, ids, "x", t))}><AlignHorizontalSpaceAround />{tx("Spread across")}</button>
          <button className="btn sm grow" disabled={picked.length < 3} onClick={() => L.update(distribute(items, ids, "y", t))}><AlignVerticalSpaceAround />{tx("Spread down")}</button>
        </div>
      </Section>
      <Section name="m-time" title={tx("Timing")}>
        <div className="rowx" style={{ gap: 4, flexWrap: "wrap" }}>
          <button className="btn sm" onClick={() => all({ t0: r2(t) })}>{tx("All start here")}</button>
          <button className="btn sm" onClick={() => L.update(replace(items, ids, (x) => (t > x.t0 + 0.05 ? { ...x, t1: r2(t) } : x)))}>{tx("All end here")}</button>
          <button className="btn sm" onClick={() => {
            // one after another, each 0.15 s after the one before (a stagger)
            const order = [...picked].sort((a, b) => a.t0 - b.t0);
            const base = order[0]?.t0 ?? t;
            L.update(items.map((x) => { const i = order.indexOf(x); if (i < 0) return x; const d = base + i * 0.15 - x.t0; return { ...x, t0: r2(x.t0 + d), t1: r2(Math.min(dur, x.t1 + d)), keys: x.keys?.map((k) => ({ ...k, t: r2(k.t + d) })) }; }));
          }}>{tx("Stagger")}</button>
        </div>
        <SelectField label={tx("Enter")} value={picked.every((x) => (x.anim || "fade") === (picked[0]?.anim || "fade")) ? picked[0]?.anim || "fade" : ""} options={[["", "—"], ...ANIMS]} onChange={(v) => v && all({ anim: v === "fade" ? undefined : v })} />
      </Section>
      <div className="rowx" style={{ gap: 4 }}>
        <button className="btn sm grow" onClick={() => all({ hidden: !picked.every((x) => x.hidden) || undefined })}><EyeOff />{tx("Hide / show")}</button>
        <button className="btn sm grow" onClick={() => all({ locked: !picked.every((x) => x.locked) || undefined })}><Lock />{tx("Lock / unlock")}</button>
      </div>
    </div>
  );
}
