// The edits made without code: sound layers, 2D layers over a film, and the Thumbnail Studio.
import { tx } from "./i18n";
import { useEffect, useMemo, useRef, useState } from "react";
import { Copy, Download, Headphones, Image as ImageIcon, Pencil, Play, Plus, Square, Trash2, Type, Volume2, VolumeX } from "lucide-react";
import { CardActions, deleteItem, renameItem } from "./Manage";
import { fileUrl, Json, tool, useLoad } from "./api";

const r2 = (x: number) => Math.round(x * 100) / 100;

// ---------- sound layers ----------

/** A volume slider in dB that shows its value while it moves and saves when it is let go. */
function Fader({ db, disabled, onSet }: { db: number; disabled?: boolean; onSet: (db: number) => void }) {
  const [v, setV] = useState(db);
  useEffect(() => setV(db), [db]);
  const done = () => { if (v !== db) onSet(v); };
  return (
    <div className="rowx" style={{ gap: 8 }}>
      <input type="range" className="grow" min={-30} max={12} step={1} value={v} disabled={disabled} onChange={(e) => setV(+e.target.value)} onPointerUp={done} onKeyUp={done} onBlur={done} onDoubleClick={() => { setV(0); onSet(0); }} title={tx("Double-click: back to 0 dB")} />
      <span className="num muted" style={{ fontSize: 12, width: 48, textAlign: "right" }}>{v > 0 ? "+" : ""}{v} dB</span>
    </div>
  );
}

/** Plays one sound file by itself, from the start; a second press stops it. */
function useListen() {
  const [on, setOn] = useState("");
  const el = useRef<HTMLAudioElement | null>(null);
  useEffect(() => () => el.current?.pause(), []);
  const toggle = (key: string, path: string, db = 0) => {
    el.current?.pause();
    if (on === key) { setOn(""); return; }
    const a = new Audio(fileUrl(path));
    a.volume = Math.max(0, Math.min(1, Math.pow(10, db / 20)));
    a.onended = () => setOn("");
    a.play().catch(() => setOn(""));
    el.current = a;
    setOn(key);
  };
  return { on, toggle };
}

export function SoundPanel({ film, t, range, onApplied, say, rev, sel, seek }: { film: string; t: number; range: [number, number] | null; onApplied: () => void; say: (s: string) => void; rev?: number; sel?: string; seek?: (t: number) => void }) {
  const s = useLoad(() => tool("hub_sound", { film }), [film, rev]);
  const listen = useListen();
  const lib = useLoad(() => tool("library"), []);
  const [busy, setBusy] = useState(false);
  const [pick, setPick] = useState("");
  const sound: Json = s.data?.sound || {};
  const apply = async (patch?: Json, clips?: Json[]) => {
    setBusy(true);
    try {
      if (patch || clips) await tool("hub_sound", { film, ...(patch ? { set: patch } : {}), ...(clips ? { clips } : {}) });
      await tool("hub_sound_apply", { film });
      s.reload();
      onApplied();
    } catch (e: any) {
      say(e.message);
    }
    setBusy(false);
  };
  const mute: string[] = sound.mute || [];
  const clips: Json[] = s.data?.clips || [];
  // one clip changed by its id; the mix is rebuilt in the same call
  const setClip = async (id: string, p: Json) => {
    setBusy(true);
    try {
      await tool("hub_sound", { film, clip: { id, ...p }, apply: true });
      s.reload();
      onApplied();
    } catch (e: any) {
      say(e.message);
    }
    setBusy(false);
  };
  const allMuted = clips.length > 0 && clips.every((c) => c.mute);
  if (s.error) return <p className="err" style={{ fontSize: 12 }}>{s.error}</p>;
  return (
    <div className="stack" style={{ gap: 12 }}>
      {!s.data?.layers?.length && <p className="sec" style={{ fontSize: 12 }}>{tx("The film has no sound.")} <span className="num">audio/mix.wav</span> {tx("or")} <span className="num">stems/*.wav</span> {tx("or put a sound clip below.")}</p>}
      {(s.data?.layers || []).map((l: Json) => {
        const db = sound.levels?.[l.id] ?? 0;
        const off = mute.includes(l.id);
        return (
          <div key={l.id} className="stack" style={{ gap: 4 }}>
            <div className="rowx">
              <b className="grow" style={{ fontWeight: 500 }}>{({ voice: tx("Sound"), music: tx("Music"), sfx: tx("Effects"), mix: tx("Mix") } as Json)[l.kind] || l.id}</b>
              <button className={`btn icon sm ${off ? "" : "ghost"}`} title={off ? tx("Unmute") : tx("Mute")} onClick={() => apply({ mute: off ? mute.filter((m) => m !== l.id) : [...mute, l.id] })}>{off ? <VolumeX /> : <Volume2 />}</button>
              <button className={`btn sm ${sound.solo === l.id ? "primary" : "ghost"}`} onClick={() => apply({ solo: sound.solo === l.id ? null : l.id })}>{tx("Solo")}</button>
            </div>
            <Fader db={db} disabled={busy || off} onSet={(v) => apply({ levels: { [l.id]: v } })} />
          </div>
        );
      })}
      {(s.data?.layers || []).some((l: Json) => l.kind === "music") && (
        <label className="rowx sec" style={{ fontSize: 12 }}><input type="checkbox" checked={!!sound.removeMusic} onChange={(e) => apply({ removeMusic: e.target.checked })} />{tx("Remove music")}</label>
      )}
      <div className="stack" style={{ gap: 4 }}>
        <div className="rowx">
          <h2 className="grow">{tx("Ducking")}</h2>
          <button className="btn sm" disabled={!range || busy} title={tx("Select a range on the timeline with Shift")} onClick={() => range && apply({ duck: [...(sound.duck || []), { from: r2(range[0]), to: r2(range[1]), db: -10 }] })}><Plus />{tx("Selected range")}</button>
        </div>
        {(sound.duck || []).map((d: Json, i: number) => (
          <div key={i} className="rowx" style={{ fontSize: 12 }}>
            <span className="num grow">{d.from}–{d.to} {tx("s")}</span>
            <span className="num muted">{d.db} dB</span>
            <button className="btn icon sm ghost" onClick={() => apply({ duck: (sound.duck || []).filter((_: Json, k: number) => k !== i) })}><Trash2 /></button>
          </div>
        ))}
        {!sound.duck?.length && <span className="muted" style={{ fontSize: 12 }}>{tx("Music (or the single mix) is turned down in these ranges")}</span>}
      </div>
      <div className="stack" style={{ gap: 8 }}>
        <div className="rowx">
          <h2 className="grow">{tx("Sounds")}{clips.length ? <span className="muted num" style={{ marginLeft: 6, fontWeight: 400 }}>{clips.length}</span> : null}</h2>
          {clips.length > 1 && <button className="btn sm ghost" disabled={busy} onClick={() => apply(undefined, clips.map((c) => ({ ...c, mute: !allMuted })))}>{allMuted ? <Volume2 /> : <VolumeX />}{allMuted ? tx("Turn all on") : tx("Mute all")}</button>}
        </div>
        <div className="rowx">
          <select className="select grow" value={pick} onChange={(e) => setPick(e.target.value)}>
            <option value="">{tx("Sound from the library")}</option>
            {(lib.data?.sounds || []).filter((x: Json) => x.path).map((x: Json) => <option key={x.id} value={x.path}>{x.id}</option>)}
          </select>
          <button className="btn sm" disabled={!pick || busy} onClick={async () => { await tool("hub_sound_clip", { film, file: pick, at: r2(t) }); apply(); }}><Plus />{r2(t)} {tx("s")}</button>
          <button className="btn sm ghost" disabled={busy} title={tx("From file")} onClick={async () => { const r = await tool("pick_file"); if (r.path) { await tool("hub_sound_clip", { film, file: r.path, at: r2(t) }); apply(); } }}>{tx("File")}</button>
        </div>
        {!clips.length && <span className="muted" style={{ fontSize: 12, lineHeight: "18px" }}>{tx("Each sound added here (by you or by Claude) gets its own volume, mute and solo. Sounds already mixed into the film's audio file can only be changed together, as a layer above.")}</span>}
        {clips.map((c, i) => {
          const id = c.id || `c${i}`;
          const solo = sound.solo === c.id;
          const here = t >= c.at && t < c.at + (c.dur || 1);
          return (
            <div key={id} className="stack" style={{ gap: 2, padding: "6px 8px", margin: "0 -8px", borderRadius: 8, background: sel === id ? "var(--bg-2, rgba(127,167,217,.12))" : undefined, opacity: c.mute ? 0.55 : 1 }}>
              <div className="rowx" style={{ fontSize: 12, gap: 4 }}>
                <button className="grow" onClick={() => seek?.(c.at)} title={tx("Go to {0} s", r2(c.at))} style={{ textAlign: "left", background: "none", border: 0, padding: 0, cursor: "pointer", color: here ? "var(--accent)" : "var(--text)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", fontWeight: 500 }}>{c.name || c.file}</button>
                <input className="input num" style={{ width: 62, height: 26 }} type="number" step={0.1} defaultValue={c.at} key={`${id}@${c.at}`} title={tx("Starts at (s)")} onBlur={(e) => +e.target.value !== c.at && setClip(c.id, { at: r2(+e.target.value) })} />
                <button className={`btn icon sm ${listen.on === id ? "" : "ghost"}`} title={tx("Listen to this sound alone")} onClick={() => listen.toggle(id, c.path, c.gain || 0)}>{listen.on === id ? <Square /> : <Play />}</button>
                <button className={`btn icon sm ${c.mute ? "" : "ghost"}`} disabled={busy} title={c.mute ? tx("Unmute") : tx("Mute")} onClick={() => setClip(c.id, { mute: !c.mute })}>{c.mute ? <VolumeX /> : <Volume2 />}</button>
                <button className={`btn icon sm ${solo ? "primary" : "ghost"}`} disabled={busy} title={tx("Solo: only this sound in the film")} onClick={() => apply({ solo: solo ? null : c.id })}><Headphones /></button>
                <button className="btn icon sm ghost" disabled={busy} title={tx("Remove")} onClick={() => apply(undefined, clips.filter((_, k) => k !== i))}><Trash2 /></button>
              </div>
              <Fader db={c.gain || 0} disabled={busy || c.mute} onSet={(v) => setClip(c.id, { gain: v })} />
            </div>
          );
        })}
      </div>
      <p className="muted" style={{ fontSize: 12, lineHeight: "18px" }}>{tx("Every change rebuilds")} <span className="num">audio/mix.wav</span> {tx("and keeps the original as")} <span className="num">mix.vhorig.wav</span> {tx("— the preview and renders use the new mix.")}</p>
    </div>
  );
}

// ---------- 2D layers ----------

export type Item = { anim?: string; id: string; kind: "text" | "image" | "rect"; t0: number; t1: number; x: number; y: number; w: number; h: number; text?: string; size?: number; weight?: number; color?: string; bg?: string; stroke?: string; radius?: number; opacity?: number; fade?: number; rot?: number; src?: string; align?: string; font?: string };

export function useLayers(film: string, push: (items: Item[]) => void) {
  const [items, setItems] = useState<Item[]>([]);
  const [installed, setInstalled] = useState(true);
  const timer = useRef<any>(0);
  const load = () => tool("hub_2d", { film }).then((r) => { setItems(r.items || []); setInstalled(r.installed); });
  useEffect(() => {
    load();
  }, [film]);
  const update = (next: Item[]) => {
    setItems(next);
    push(next);
    clearTimeout(timer.current);
    timer.current = setTimeout(() => tool("hub_2d", { film, items: next }), 350);
  };
  const install = () => tool("hub_2d", { film, install: true }).then(load);
  return { items, update, installed, install, reload: load };
}

export function LayersPanel({ film, L, t, W, H, sel, setSel, dur }: Json) {
  const items: Item[] = L.items;
  const it = items.find((x) => x.id === sel);
  const set = (p: Partial<Item>) => L.update(items.map((x) => (x.id === sel ? { ...x, ...p } : x)));
  const add = async (kind: Item["kind"]) => {
    const id = "l" + Date.now().toString(36);
    const base: Item = { id, kind, t0: r2(t), t1: r2(Math.min(dur, t + 3)), x: W * 0.1, y: H * 0.7, w: W * 0.8, h: H * 0.1, opacity: 1, fade: 0.25 };
    if (kind === "text") Object.assign(base, { text: tx("Title"), size: Math.round(Math.min(W, H) * 0.08), color: "#ffffff", weight: 800, stroke: "#000000" });
    if (kind === "rect") Object.assign(base, { bg: "#e9b84a", radius: 24 });
    if (kind === "image") {
      const r = await tool("pick_file");
      if (!r.path) return;
      const c = await tool("hub_2d", { film, add_image: r.path });
      Object.assign(base, { src: c.src, h: W * 0.5, w: W * 0.5, x: W * 0.25 });
    }
    L.update([...items, base]);
    setSel(id);
  };
  const num = (k: keyof Item, label: string, step = 1) => (
    <div className="field"><label>{label}</label><input className="input num" type="number" step={step} key={`${sel}${k}${(it as any)?.[k]}`} defaultValue={(it as any)?.[k] ?? ""} onBlur={(e) => set({ [k]: e.target.value === "" ? undefined : +e.target.value } as any)} /></div>
  );
  const color = (k: keyof Item, label: string) => (
    <div className="field"><label>{label}</label><div className="rowx"><input className="input" type="color" style={{ padding: 2, width: 40 }} value={((it as any)?.[k] as string) || "#000000"} onChange={(e) => set({ [k]: e.target.value } as any)} /><button className="btn icon sm ghost" title={tx("None")} onClick={() => set({ [k]: undefined } as any)}><Trash2 /></button></div></div>
  );
  return (
    <div className="stack" style={{ gap: 12 }}>
      {!L.installed && (
        <div className="card" style={{ padding: 12 }}>
          <p className="sec" style={{ fontSize: 12, marginBottom: 8 }}>{tx("This film's page has no layer hook. Once installed, layers are drawn in the preview and in renders.")}</p>
          <button className="btn sm" onClick={L.install}>{tx("Install hook")}</button>
        </div>
      )}
      <div className="rowx">
        <button className="btn sm" onClick={() => add("text")}><Type />{tx("Text")}</button>
        <button className="btn sm" onClick={() => add("image")}><ImageIcon />{tx("Picture")}</button>
        <button className="btn sm" onClick={() => add("rect")}><Square />{tx("Box")}</button>
      </div>
      <div className="list-rows">
        {items.map((x) => (
          <div key={x.id} className="rowx" style={{ padding: "6px 0", cursor: "pointer", color: x.id === sel ? "var(--text)" : "var(--text-2)" }} onClick={() => setSel(x.id)}>
            <span className="tag">{x.kind === "text" ? tx("Text") : x.kind === "image" ? tx("Picture") : tx("Box")}</span>
            <span className="grow" style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", fontSize: 12 }}>{x.text || x.src || x.bg}</span>
            <span className="num muted" style={{ fontSize: 11 }}>{x.t0}–{x.t1}</span>
          </div>
        ))}
      </div>
      {it && (
        <div className="stack" style={{ gap: 8 }}>
          {it.kind === "text" && <textarea className="input" style={{ fontFamily: "var(--font)", fontSize: 13 }} rows={2} key={sel} defaultValue={it.text} onChange={(e) => set({ text: e.target.value })} />}
          <div className="grid2">{num("t0", tx("Start, s"), 0.1)}{num("t1", tx("End, s"), 0.1)}</div>
          {it.kind === "text" && <div className="grid2">{num("size", tx("Size, px"))}{num("weight", tx("Weight"), 100)}</div>}
          <div className="grid2">{it.kind === "text" && color("color", tx("Colour"))}{color("bg", tx("Background"))}{it.kind === "text" && color("stroke", tx("Outline"))}</div>
          <div className="grid2">{num("radius", tx("Corner"))}{num("rot", tx("Angle"))}</div>
          <div className="grid2">{num("opacity", tx("Opacity"), 0.05)}{num("fade", tx("Transition, s"), 0.05)}</div>
          <div className="field"><label>{tx("In / out")}</label><select className="select" value={it.anim || "fade"} onChange={(e) => set({ anim: e.target.value })}><option value="fade">{tx("Fade")}</option><option value="slide-up">{tx("Slide up")}</option><option value="slide-left">{tx("Slide in")}</option><option value="pop">{tx("Pop")}</option></select></div>
          <button className="btn sm ghost" onClick={() => { L.update(items.filter((x) => x.id !== sel)); setSel(""); }}><Trash2 />{tx("Delete layer")}</button>
        </div>
      )}
      <p className="muted" style={{ fontSize: 12, lineHeight: "18px" }}>{tx("Layers")} <span className="num">tweaks.json</span> {tx("keeps them. Drag a layer to move it, resize it from the bottom-right corner. Claude can also change them with")} <span className="num">hub_2d</span> {tx("too.")}</p>
    </div>
  );
}

/** Boxes over the preview for the layers shown at t: drag to move, corner to resize. */
export function LayerOverlay({ items, t, sel, setSel, update, W }: { items: Item[]; t: number; sel: string; setSel: (s: string) => void; update: (i: Item[]) => void; W: number }) {
  const ref = useRef<HTMLDivElement>(null);
  const start = (e: React.PointerEvent, it: Item, mode: "move" | "size") => {
    e.stopPropagation();
    setSel(it.id);
    const r = ref.current!.getBoundingClientRect();
    const k = W / r.width;
    const [x0, y0] = [e.clientX, e.clientY];
    const orig = { ...it };
    const move = (ev: PointerEvent) => {
      const dx = (ev.clientX - x0) * k, dy = (ev.clientY - y0) * k;
      const next = mode === "move" ? { x: Math.round(orig.x + dx), y: Math.round(orig.y + dy) } : { w: Math.max(20, Math.round(orig.w + dx)), h: Math.max(20, Math.round(orig.h + dy)) };
      update(items.map((x) => (x.id === it.id ? { ...x, ...next } : x)));
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };
  const hs = W / 60;
  return (
    <div ref={ref} style={{ position: "absolute", inset: 0 }} onPointerDown={() => setSel("")}>
      {items.filter((x) => t >= x.t0 && t < x.t1).map((x) => (
        <div key={x.id} onPointerDown={(e) => start(e, x, "move")} style={{ position: "absolute", left: x.x, top: x.y, width: x.w, height: x.h, outline: `${W / 400}px ${x.id === sel ? "solid" : "dashed"} ${x.id === sel ? "var(--accent)" : "rgba(255,255,255,.5)"}`, cursor: "move", transform: x.rot ? `rotate(${x.rot}deg)` : undefined }}>
          {x.id === sel && <div onPointerDown={(e) => start(e, x, "size")} style={{ position: "absolute", right: -hs / 2, bottom: -hs / 2, width: hs, height: hs, background: "var(--accent)", borderRadius: hs / 4, cursor: "nwse-resize" }} />}
        </div>
      ))}
    </div>
  );
}

// ---------- Thumbnail Studio ----------

const TYPE_COLOR: Record<string, string> = { background: "#2a2f3a", character: "#4f7cac", face: "#4f7cac", object: "#7a6a4f", effect: "#c0392b", text: "#e9b84a", prop: "#6b8f71" };
const fillSlots = (s: string, v: Json) => String(s || "").replace(/\{\{(\w+)\}\}/g, (_, k) => v?.[k] ?? "");

function drawThumb(g: CanvasRenderingContext2D, doc: Json, W: number, H: number, bg: HTMLImageElement | null, sketch: boolean) {
  g.clearRect(0, 0, W, H);
  if (bg) {
    const s = Math.max(W / bg.width, H / bg.height);
    g.drawImage(bg, (W - bg.width * s) / 2, (H - bg.height * s) / 2, bg.width * s, bg.height * s);
  } else {
    g.fillStyle = "#16181d";
    g.fillRect(0, 0, W, H);
  }
  const pal: string[] = doc.style?.palette || [];
  for (const l of doc.layers || []) {
    const [x, y, w, h] = [(l.x ?? 0) * W, (l.y ?? 0) * H, (l.w ?? 1) * W, (l.h ?? 1) * H];
    if (l.type === "text") {
      const text = fillSlots(l.text || l.content || doc.values?.TITLE || l.desc, doc.values);
      let px = h * 0.62;
      g.font = `900 ${px}px "Segoe UI", system-ui, sans-serif`;
      const tw = g.measureText(text).width;
      if (tw > w) px *= w / tw;
      g.font = `900 ${px}px "Segoe UI", system-ui, sans-serif`;
      g.textAlign = "center";
      g.textBaseline = "middle";
      g.lineJoin = "round";
      g.lineWidth = px * 0.16;
      g.strokeStyle = l.stroke || "#000";
      g.strokeText(text, x + w / 2, y + h / 2);
      g.fillStyle = l.color || pal[2] || "#fff";
      g.fillText(text, x + w / 2, y + h / 2);
    } else if (sketch && l.type !== "background") {
      g.fillStyle = (TYPE_COLOR[l.type] || "#555") + "55";
      g.strokeStyle = l.color || TYPE_COLOR[l.type] || "#888";
      g.lineWidth = Math.max(2, W / 400);
      g.beginPath();
      if (l.shape === "circle") g.ellipse(x + w / 2, y + h / 2, w / 2, h / 2, 0, 0, Math.PI * 2);
      else g.rect(x, y, w, h);
      g.fill();
      g.stroke();
      g.fillStyle = "#fff";
      g.font = `600 ${Math.max(14, W / 70)}px "Segoe UI", sans-serif`;
      g.textAlign = "left";
      g.textBaseline = "top";
      g.fillText(l.name || l.type, x + 8, y + 6);
    }
  }
}

export function ThumbStudio({ say }: { say: (s: string) => void }) {
  const all = useLoad(() => tool("hub_thumbs"), []);
  const [id, setId] = useState("");
  const [tpl, setTpl] = useState("");
  const [title, setTitle] = useState("");
  const create = async () => {
    const r = await tool("hub_thumb_new", { title: title || tx("Thumbnail"), template: tpl || undefined });
    all.reload();
    setId(r.id);
  };
  if (id) return <ThumbEditor id={id} back={() => { setId(""); all.reload(); }} say={say} />;
  const tpls: Json[] = all.data?.templates || [];
  return (
    <div className="page">
      <div className="wrap">
        <div className="page-head"><h1>{tx("Thumbnail studio")}</h1></div>
        <p className="sec" style={{ marginBottom: 16, maxWidth: 760 }}>{tx("Build a layout from a template, rough in the layers and give the brief to Claude. Claude makes the images and adds them with")} <span className="num">hub_thumb_variant</span> {tx("and they show up here at once. Pick one and add the title last yourself.")}</p>
        <div className="card" style={{ padding: 16, marginBottom: 24, display: "grid", gridTemplateColumns: "1fr 1fr auto", gap: 12, alignItems: "end" }}>
          <div className="field"><label>{tx("Title")}</label><input className="input" value={title} onChange={(e) => setTitle(e.target.value)} placeholder={tx("Video title")} /></div>
          <div className="field"><label>{tx("Template")}</label><select className="select" value={tpl} onChange={(e) => setTpl(e.target.value)}><option value="">{tx("Blank")}</option>{tpls.map((x) => <option key={x.id} value={x.id}>{x.name} · {x.format}</option>)}</select></div>
          <button className="btn primary" onClick={create}><Plus />{tx("Create")}</button>
        </div>
        {tpl && <p className="sec" style={{ marginTop: -12, marginBottom: 24, fontSize: 12 }}>{tpls.find((x) => x.id === tpl)?.desc}</p>}
        <div className="proj-grid">
          {(all.data?.thumbnails || []).map((th: Json) => (
            <button key={th.id} className="card proj" onClick={() => setId(th.id)}>
              <CardActions actions={[{ icon: Pencil, label: tx("Rename"), run: () => renameItem("thumb", th.id, th.title || th.id, {}, say, all.reload) }, { icon: Trash2, label: tx("Delete"), danger: true, run: () => deleteItem("thumb", th.id, th.title || th.id, {}, say, all.reload) }]} />
              <div className="thumb" style={{ aspectRatio: "16/9" }}>{(th.final || th.variants?.[0]) && <img src={fileUrl(th.final || th.variants[0])} alt="" />}</div>
              <div className="meta"><b>{th.title}</b><span>{[th.format, th.variants?.length ? tx("{0} variants", th.variants.length) : tx("No variants"), th.final ? tx("Final ready") : ""].filter(Boolean).join(" · ")}</span></div>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}

function ThumbEditor({ id, back, say }: { id: string; back: () => void; say: (s: string) => void }) {
  const [data, setData] = useState<Json | null>(null);
  const [pick, setPick] = useState("");
  const [sel, setSel] = useState(-1);
  const [note, setNote] = useState("");
  const cv = useRef<HTMLCanvasElement>(null);
  const [bg, setBg] = useState<HTMLImageElement | null>(null);
  const saveT = useRef<any>(0);
  const load = () => tool("hub_thumb", { id }).then(setData);
  useEffect(() => {
    load();
    const iv = setInterval(load, 3000); // variants an agent adds show up by themselves
    return () => clearInterval(iv);
  }, [id]);
  const doc = data?.doc;
  const [W, H] = doc?.format === "9:16" ? [1080, 1920] : [1280, 720];
  useEffect(() => {
    if (!pick) return setBg(null);
    const im = new Image();
    im.onload = () => setBg(im);
    im.src = fileUrl(pick);
  }, [pick]);
  useEffect(() => {
    const g = cv.current?.getContext("2d");
    if (g && doc) drawThumb(g, doc, W, H, bg, !bg);
  }, [doc, bg, W, H]);
  const save = (next: Json) => {
    setData({ ...data, doc: next });
    clearTimeout(saveT.current);
    saveT.current = setTimeout(() => {
      const c = document.createElement("canvas");
      c.width = W;
      c.height = H;
      drawThumb(c.getContext("2d")!, next, W, H, null, true);
      tool("hub_thumb", { id, set: { values: next.values, layers: next.layers, format: next.format }, layout: c.toDataURL("image/png") });
    }, 500);
  };
  const drag = (e: React.PointerEvent, i: number, mode: "move" | "size") => {
    e.stopPropagation();
    setSel(i);
    const r = cv.current!.getBoundingClientRect();
    const [x0, y0] = [e.clientX, e.clientY];
    const o = { ...doc.layers[i] };
    const mv = (ev: PointerEvent) => {
      const dx = (ev.clientX - x0) / r.width, dy = (ev.clientY - y0) / r.height;
      const l = mode === "move" ? { x: r2((o.x ?? 0) + dx), y: r2((o.y ?? 0) + dy) } : { w: Math.max(0.03, r2((o.w ?? 1) + dx)), h: Math.max(0.03, r2((o.h ?? 1) + dy)) };
      save({ ...doc, layers: doc.layers.map((x: Json, k: number) => (k === i ? { ...x, ...l } : x)) });
    };
    const up = () => {
      window.removeEventListener("pointermove", mv);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", mv);
    window.addEventListener("pointerup", up);
  };
  const exportFinal = async () => {
    if (!bg) return say(tx("Choose a variant first"));
    const c = document.createElement("canvas");
    c.width = W;
    c.height = H;
    drawThumb(c.getContext("2d")!, doc, W, H, bg, false);
    await tool("hub_thumb", { id, final: c.toDataURL("image/png"), set: { status: "final" } });
    load();
    say(tx("final.png saved"));
  };
  if (!doc) return <div className="page"><span className="muted">{tx("Loading")}</span></div>;
  const L = doc.layers?.[sel];
  return (
    <div className="ed" style={{ gridTemplateColumns: "300px minmax(0, 1fr) 320px", gridTemplateRows: "48px minmax(0, 1fr)" }}>
      <div className="ed-top">
        <button className="btn ghost" onClick={back}>{tx("Thumbnails")}</button>
        <span className="title">{doc.title}</span>
        <span className="muted" style={{ fontSize: 12 }}>{doc.name || tx("Blank template")} · {doc.format}</span>
        <span className="grow" />
        <button className="btn" onClick={() => navigator.clipboard.writeText(tx("Make variants for \"{0}\" (id {1}) in the Linnea thumbnail studio: read the brief with hub_thumb_brief id={2}, look at layout_png, make the images and add each with hub_thumb_variant. No text in the images.", doc.title, id, id)).then(() => say(tx("Message for Claude copied")))}><Copy />{tx("Send the brief to Claude")}</button>
        <button className="btn primary" disabled={!bg} onClick={exportFinal}><Download />{tx("Save final")}</button>
      </div>
      <div className="ed-left" style={{ overflow: "auto", padding: 16, gap: 12 }}>
        <h2>{tx("Slots")}</h2>
        {Object.entries(doc.slots || {}).map(([k, s]: [string, any]) => (
          <div className="field" key={k}><label>{s.label}</label><input className="input" defaultValue={doc.values?.[k]} onBlur={(e) => save({ ...doc, values: { ...doc.values, [k]: e.target.value } })} /></div>
        ))}
        <div className="rowx"><h2 className="grow">{tx("Layers")}</h2><button className="btn sm" onClick={() => save({ ...doc, layers: [...(doc.layers || []), { type: "text", name: tx("Title"), text: "{{TITLE}}", x: 0.06, y: 0.72, w: 0.88, h: 0.2 }] })}><Type />{tx("Text")}</button><button className="btn sm" onClick={() => save({ ...doc, layers: [...(doc.layers || []), { type: "object", name: tx("Object"), desc: "", x: 0.35, y: 0.3, w: 0.3, h: 0.35 }] })}><Square />{tx("Object")}</button></div>
        <div className="list-rows">
          {(doc.layers || []).map((l: Json, i: number) => (
            <div key={i} className="rowx" style={{ padding: "6px 0", cursor: "pointer", color: i === sel ? "var(--text)" : "var(--text-2)" }} onClick={() => setSel(i)}>
              <span className="tag" style={{ background: (TYPE_COLOR[l.type] || "#555") + "44" }}>{l.type}</span>
              <span className="grow" style={{ fontSize: 12, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{l.name}</span>
            </div>
          ))}
        </div>
        {L && (
          <div className="stack" style={{ gap: 8 }}>
            <div className="field"><label>{tx("Name")}</label><input className="input" key={`n${sel}`} defaultValue={L.name} onBlur={(e) => save({ ...doc, layers: doc.layers.map((x: Json, k: number) => (k === sel ? { ...x, name: e.target.value } : x)) })} /></div>
            <div className="field"><label>{L.type === "text" ? tx("Text") : tx("Description (for Claude)")}</label><textarea className="input" style={{ fontFamily: "var(--font)", fontSize: 13 }} rows={3} key={`d${sel}`} defaultValue={L.type === "text" ? L.text : L.desc} onBlur={(e) => save({ ...doc, layers: doc.layers.map((x: Json, k: number) => (k === sel ? { ...x, [L.type === "text" ? "text" : "desc"]: e.target.value } : x)) })} /></div>
            <button className="btn sm ghost" onClick={() => { save({ ...doc, layers: doc.layers.filter((_: Json, k: number) => k !== sel) }); setSel(-1); }}><Trash2 />{tx("Delete")}</button>
          </div>
        )}
      </div>
      <div className="stage">
        <div style={{ position: "relative", maxWidth: "100%", maxHeight: "100%", aspectRatio: `${W}/${H}`, height: doc.format === "9:16" ? "100%" : undefined, width: doc.format === "9:16" ? undefined : "100%" }}>
          <canvas ref={cv} width={W} height={H} style={{ width: "100%", height: "100%", borderRadius: 10, boxShadow: "0 0 0 1px var(--line-2)" }} onPointerDown={() => setSel(-1)} />
          {(doc.layers || []).map((l: Json, i: number) => l.type !== "background" && (
            <div key={i} onPointerDown={(e) => drag(e, i, "move")} style={{ position: "absolute", left: `${(l.x ?? 0) * 100}%`, top: `${(l.y ?? 0) * 100}%`, width: `${(l.w ?? 1) * 100}%`, height: `${(l.h ?? 1) * 100}%`, outline: i === sel ? "2px solid var(--accent)" : "1px dashed rgba(255,255,255,.35)", cursor: "move" }}>
              {i === sel && <div onPointerDown={(e) => drag(e, i, "size")} style={{ position: "absolute", right: -6, bottom: -6, width: 12, height: 12, background: "var(--accent)", borderRadius: 3, cursor: "nwse-resize" }} />}
            </div>
          ))}
        </div>
        <span className="muted" style={{ fontSize: 12 }}>{bg ? tx("Selected variant + title layers (a preview of the final)") : tx("Layout sketch: Claude sees this as layout.png")}</span>
      </div>
      <div className="ed-right">
        <h2>{tx("Variants")}</h2>
        {!data.variants?.length && <p className="sec" style={{ fontSize: 12 }}>{tx("No variants yet. Send the brief to Claude; what it makes lands here by itself. You can also add a picture you have.")}</p>}
        <button className="btn sm" onClick={async () => { const r = await tool("pick_file"); if (r.path) { await tool("hub_thumb_variant", { id, file: r.path }); load(); } }}><Plus />{tx("Add image")}</button>
        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 8 }}>
          {(data.variants || []).map((v: string) => (
            <button key={v} className="card" style={{ padding: 0, overflow: "hidden", outline: pick === v ? "2px solid var(--accent)" : undefined, cursor: "pointer" }} onClick={() => setPick(pick === v ? "" : v)}>
              <img src={fileUrl(v)} alt="" style={{ width: "100%", display: "block", aspectRatio: `${W}/${H}`, objectFit: "cover" }} />
            </button>
          ))}
        </div>
        {pick && (
          <div className="stack" style={{ gap: 8 }}>
            <textarea className="input" style={{ fontFamily: "var(--font)", fontSize: 13 }} rows={2} placeholder={tx("What should change in this variant")} value={note} onChange={(e) => setNote(e.target.value)} />
            <button className="btn sm" disabled={!note} onClick={async () => { await tool("hub_thumb_note", { id, variant: pick, text: note }); setNote(""); load(); }}>{tx("Leave a note")}</button>
          </div>
        )}
        {(data.notes || []).filter((n: Json) => n.status === "open").map((n: Json) => (
          <div key={n.id} className="sec" style={{ fontSize: 12 }}>{n.variant?.split("/").pop()}: {n.text}</div>
        ))}
        {data.final && <><h2>{tx("Final")}</h2><img src={fileUrl(data.final, Date.now())} alt="" style={{ width: "100%", borderRadius: 8 }} /><button className="btn sm ghost" onClick={() => tool("reveal", { path: data.final })}>{tx("Show in folder")}</button></>}
      </div>
    </div>
  );
}
