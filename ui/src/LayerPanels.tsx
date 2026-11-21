// The panels around the picture: Add (everything that can go on the frame, drag a tile onto it or click it),
// the layer stack (one list, top first: your layers, then what the film's code draws), menus and the command
// palette.
import { tx } from "./i18n";
import { useEffect, useMemo, useRef, useState } from "react";
import {
  ArrowUpRight, Bookmark, Circle, CircleDot, Diamond, Eye, EyeOff, Gauge, GripVertical, Hash, Image as ImageIcon, Keyboard, Link2, Lock, PenLine, Plus, Rows3, Search,
  Smartphone, Sparkles, Square, Subtitles, FolderClosed, Film, Music, ChevronRight, Type, TypeOutline, Unlock, X, Blend, PanelBottom, MessageSquareText, Clapperboard,
} from "lucide-react";
import { Json, tool } from "./api";
import { DROP, evalItem, Item, Layers, newItem, videoItem } from "./Layers";
import { deletePreset, fromPreset, kindLabel, layerName, moveTo, myPresets, r2, uid } from "./LayerOps";
import { LayerInspector, MultiInspector } from "./Inspector";
import type { Elements, Reg } from "./Elements";
import { PuppetInfo } from "./Puppets";
import { PuppetPreview } from "./PuppetsPage";
import { RigEditor } from "./RigEditor";
import { puppetFromFrames } from "./Rig";
import { Bone as BoneIcon, Images } from "lucide-react";

export const KIND_ICON: Record<string, any> = {
  group: FolderClosed, video: Film, text: Type, image: ImageIcon, rect: Square, ellipse: Circle, counter: Hash, marker: PenLine, arrow: ArrowUpRight, phone: Smartphone, lower: Rows3, progress: Gauge, puppet: Sparkles,
};

type Tile = [string, string, any, string?];
const GROUPS: [string, Tile[]][] = [
  [tx("Text"), [["text", tx("Title"), Type], ["subtitle", tx("Subtitle"), Subtitles], ["caption", tx("Caption box"), MessageSquareText], ["typing", tx("Typewriter"), TypeOutline],
    ["marker", tx("Handwriting"), PenLine], ["counter", tx("Counter"), Hash], ["lower", tx("Lower third"), Rows3]]],
  [tx("Shapes"), [["rect", tx("Box"), Square], ["ellipse", tx("Circle"), Circle], ["ring", tx("Ring"), CircleDot], ["arrow", tx("Arrow"), ArrowUpRight], ["progress", tx("Progress bar"), Gauge],
    ["gradient", tx("Gradient background"), Blend], ["scrim", tx("Shade for text"), PanelBottom]]],
  [tx("Media"), [["video", tx("Video…"), Film], ["image", tx("Picture…"), ImageIcon], ["sound", tx("Sound…"), Music], ["phone", tx("Phone"), Smartphone]]],
];

/** Where a new layer goes: centred on (x, y) when dropped, as made otherwise; t0 at the playhead. */
function placeAt(it: Item, x?: number, y?: number): Item {
  if (x === undefined || y === undefined) return it;
  return { ...it, x: Math.round(x - it.w / 2), y: Math.round(y - it.h / 2) };
}

export function AddPanel({ L, t, W, H, dur, select, say, puppets, addSound, scenes }: { L: Layers; t: number; W: number; H: number; dur: number; select: (ids: string[]) => void; say: (s: string) => void; puppets?: any; addSound?: (path: string, at: number) => void; scenes?: any }) {
  const [mine, setMine] = useState(myPresets);
  const st = useRef({ L, t });
  st.current = { L, t };
  const add = async (kind: string, x?: number, y?: number) => {
    const { L, t } = st.current;
    if (kind === "sound") {
      const r = await tool("pick_file");
      if (r.path) addSound?.(r.path, t);
      return;
    }
    if (kind === "video") {
      const r = await tool("pick_file");
      if (!r.path) return;
      const src = await L.addImage(r.path);
      let v = await videoItem(src, String(r.path).split(/[\\/]/).pop()?.replace(/\.[^.]+$/, "") || tx("Video"), t, dur, W, H, L.url.replace(/[^/]*$/, ""));
      v = placeAt(v, x, y);
      st.current.L.update([...st.current.L.items, v]);
      select([v.id]);
      return;
    }
    let it = newItem(kind, t, dur, W, H);
    if (kind === "image") {
      const r = await tool("pick_file");
      if (!r.path) return;
      it.src = await L.addImage(r.path);
      it.label = String(r.path).split(/[\\/]/).pop()?.replace(/\.[^.]+$/, "");
    }
    it = placeAt(it, x, y);
    L.update([...L.items, it]);
    select([it.id]);
  };
  const addPreset = (i: number, x?: number, y?: number) => {
    const p = myPresets()[i];
    if (!p) return;
    const it = placeAt(fromPreset(p.item, st.current.t, dur), x, y);
    st.current.L.update([...st.current.L.items, it]);
    select([it.id]);
  };
  const puppetDrop = useRef<null | ((name: string, x: number, y: number) => void)>(null);
  useEffect(() => {
    DROP.fn = (p: Json, x: number, y: number) => {
      if (p.kind) add(p.kind, x, y);
      else if (p.preset !== undefined) addPreset(p.preset, x, y);
      else if (p.puppet) puppetDrop.current?.(p.puppet, x, y);
    };
    return () => { DROP.fn = null; };
  }, [W, H, dur]);
  const drag = (payload: Json) => (e: React.DragEvent) => { e.dataTransfer.setData("application/x-linnea", JSON.stringify(payload)); e.dataTransfer.effectAllowed = "copy"; };
  return (
    <div className="stack" style={{ gap: 16 }}>
      <p className="muted" style={{ fontSize: 12, lineHeight: "18px", margin: 0 }}>{tx("Click to add at the playhead, or drag onto the frame to place it.")}</p>
      {GROUPS.map(([g, tiles]) => (
        <div key={g} className="stack" style={{ gap: 6 }}>
          <span className="sub-h">{g}</span>
          <div className="tiles">
            {tiles.map(([k, l, I]) => (
              <button key={k} data-add={k} className="tile" draggable onDragStart={drag({ kind: k })} onClick={() => add(k)} title={l}><I /><span>{l}</span></button>
            ))}
          </div>
        </div>
      ))}
      {mine.length > 0 && (
        <div className="stack" style={{ gap: 6 }}>
          <span className="sub-h">{tx("My presets")}</span>
          <div className="tiles">
            {mine.map((p, i) => {
              const I = KIND_ICON[p.item.kind] || Bookmark;
              return (
                <div key={p.name} className="tile" role="button" draggable onDragStart={drag({ preset: i })} onClick={() => addPreset(i)} title={p.name}>
                  <I /><span>{p.name}</span>
                  <button className="tile-x" title={tx("Delete")} onClick={(e) => { e.stopPropagation(); deletePreset(p.name); setMine(myPresets()); }}><X size={11} /></button>
                </div>
              );
            })}
          </div>
        </div>
      )}
      {scenes}
      {puppets && puppets(puppetDrop)}
    </div>
  );
}

/** One list of everything on the frame, top first. Drag a row to change the order; click to select
 *  (Shift / Ctrl: several), double-click to rename, the eye hides, the lock keeps it from being moved. */
export function LayerStack({ L, t, sel, multi, select, seek, reg, E, selEl, pickEl, scanning, onMenu, onAdd, askClaude }: {
  L: Layers; t: number; sel: string; multi: string[]; select: (ids: string[], primary?: string) => void; seek: (t: number) => void;
  reg?: Reg[]; E?: Elements; selEl?: string; pickEl?: (id: string, t0: number, t1: number) => void; scanning?: boolean; onMenu?: (x: number, y: number, id: string) => void; onAdd?: () => void; askClaude?: () => void;
}) {
  const [q, setQ] = useState("");
  const [now, setNow] = useState(false);
  const [renaming, setRenaming] = useState("");
  const [dragId, setDragId] = useState("");
  const [over, setOver] = useState<number | null>(null);
  const [codeOpen, setCodeOpen] = useState(true);
  const items = L.items;
  const match = (s: string) => !q || s.toLowerCase().includes(q.toLowerCase());
  const [shut, setShut] = useState<Record<string, boolean>>({});
  const isGroup = (id?: string) => !!id && items.find((x) => x.id === id)?.kind === "group";
  // top first; what is in a group comes under it, indented, unless searching
  const rows: { it: Item; i: number; depth: number }[] = [];
  const walk = (parent: string | undefined, depth: number) => {
    for (let i = items.length - 1; i >= 0; i--) {
      const it = items[i];
      const inG = isGroup(it.parent);
      if (depth === 0 ? inG : it.parent !== parent) continue;
      rows.push({ it, i, depth });
      if (it.kind === "group" && !shut[it.id] && depth < 6) walk(it.id, depth + 1);
    }
  };
  if (q || now) items.forEach((it, i) => { if (match(layerName(it) + " " + it.kind) && (!now || (t >= it.t0 && t < it.t1))) rows.unshift({ it, i, depth: 0 }); });
  else walk(undefined, 0);
  const code = (reg || []).filter((r) => { const e = E?.edits[r.id] || {}; const t0 = e.t0 ?? r.t0, t1 = e.t1 ?? r.t1; return match((e.text ?? r.label) + " " + r.kind) && (!now || (t >= t0 && t < t1)); });
  const chosen = multi.length ? multi : sel ? [sel] : [];
  const click = (e: React.MouseEvent, it: Item) => {
    if (e.shiftKey && chosen.length) {
      // a run from the last chosen to this one, in list order
      const order = items.map((x) => x.id).reverse();
      const a = order.indexOf(chosen[chosen.length - 1]), b = order.indexOf(it.id);
      const run = order.slice(Math.min(a, b), Math.max(a, b) + 1);
      select([...new Set([...chosen, ...run])], it.id);
    } else if (e.ctrlKey || e.metaKey) select(chosen.includes(it.id) ? chosen.filter((x) => x !== it.id) : [...chosen, it.id], it.id);
    else select([it.id], it.id);
    if (t < it.t0 || t >= it.t1) seek(it.t0 + 0.05);
  };
  const put = (id: string, p: Json) => L.update(items.map((x) => (x.id === id ? { ...x, ...p } : x)));
  const fmt = (x: number) => `${Math.floor(x / 60)}:${(x % 60).toFixed(1).padStart(4, "0")}`;
  return (
    <div className="stack" style={{ gap: 8 }}>
      <div className="rowx" style={{ gap: 4 }}>
        <div className="search grow"><Search /><input placeholder={tx("Find a layer")} value={q} onChange={(e) => setQ(e.target.value)} />{q && <button onClick={() => setQ("")}><X size={12} /></button>}</div>
        <button className={`btn sm ${now ? "" : "ghost"}`} title={tx("Only what is on screen now")} onClick={() => setNow(!now)} style={now ? { color: "var(--accent)" } : undefined}><Clapperboard />{tx("Now")}</button>
        {onAdd && <button className="btn icon sm" title={tx("Add (A)")} onClick={onAdd}><Plus /></button>}
      </div>
      <div className="stack-list">
        <div className="stack-g">{tx("Your layers")} <span className="muted">{items.length}</span></div>
        {rows.map(({ it, i, depth }) => {
          const I = KIND_ICON[it.kind] || Square;
          const live = t >= it.t0 && t < it.t1;
          const on = chosen.includes(it.id);
          return (
            <div key={it.id} className={`lrow ${on ? "on" : ""} ${it.id === sel ? "prim" : ""} ${over === i ? "over" : ""}`} style={{ opacity: it.hidden ? 0.5 : 1 }}
              draggable={!renaming} onDragStart={(e) => { setDragId(it.id); e.dataTransfer.effectAllowed = "move"; e.dataTransfer.setData("text/plain", it.id); }}
              onDragOver={(e) => { if (dragId) { e.preventDefault(); setOver(i); } }} onDragLeave={() => setOver(null)}
              onDrop={(e) => {
                e.preventDefault();
                if (dragId && dragId !== it.id) {
                  // dropped among a group's layers it joins the group; dropped outside every group it leaves its own
                  const moved = moveTo(items, dragId, i);
                  const into = isGroup(it.parent) ? it.parent : undefined;
                  const d = moved.find((x) => x.id === dragId)!;
                  L.update(into !== d.parent && (into || isGroup(d.parent)) && dragId !== into ? moved.map((x) => (x.id === dragId ? { ...x, parent: into } : x)) : moved);
                }
                setDragId(""); setOver(null);
              }} onDragEnd={() => { setDragId(""); setOver(null); }}
              onClick={(e) => click(e, it)} onDoubleClick={() => setRenaming(it.id)} onContextMenu={(e) => { if (!onMenu) return; e.preventDefault(); if (!on) select([it.id], it.id); onMenu(e.clientX, e.clientY, it.id); }}>
              <GripVertical className="grip" />
              {depth > 0 && <span style={{ width: depth * 14, flex: "none" }} />}
              {it.kind === "group" ? <button className="lrow-b keep" style={{ width: 16 }} onClick={(e) => { e.stopPropagation(); setShut({ ...shut, [it.id]: !shut[it.id] }); }}><ChevronRight style={{ transform: shut[it.id] ? undefined : "rotate(90deg)" }} /></button> : null}
              <I className="kind" />
              {renaming === it.id
                ? <input autoFocus className="lrow-name" defaultValue={layerName(it)} onClick={(e) => e.stopPropagation()} onKeyDown={(e) => { e.stopPropagation(); if (e.key === "Enter" || e.key === "Escape") (e.target as HTMLInputElement).blur(); }} onBlur={(e) => { put(it.id, { label: e.target.value.trim() || undefined }); setRenaming(""); }} />
                : <span className="lrow-name" style={{ fontWeight: live ? 600 : 400, textDecoration: it.hidden ? "line-through" : undefined }}>{layerName(it)}</span>}
              {it.parent && <Link2 className="flag" aria-label={tx("Attached to a parent")} />}
              {(it.keys?.length || it.pkeys?.length) ? <Diamond className="flag acc" aria-label={tx("Animated")} /> : null}
              <span className="lrow-t num">{fmt(it.t0)}</span>
              <button className={`lrow-b ${it.locked ? "keep" : ""}`} title={it.locked ? tx("Unlock") : tx("Lock")} onClick={(e) => { e.stopPropagation(); put(it.id, { locked: !it.locked || undefined }); }}>{it.locked ? <Lock /> : <Unlock />}</button>
              <button className={`lrow-b ${it.hidden ? "keep" : ""}`} title={it.hidden ? tx("Show") : tx("Hide")} onClick={(e) => { e.stopPropagation(); put(it.id, { hidden: !it.hidden || undefined }); }}>{it.hidden ? <EyeOff /> : <Eye />}</button>
            </div>
          );
        })}
        {!items.length && (
          <div className="empty">
            <span>{tx("No layers yet. Add a title, a shape, a picture or a puppet; drop a picture onto the frame.")}</span>
            {onAdd && <button className="btn sm" onClick={onAdd}><Plus />{tx("Add something")}</button>}
          </div>
        )}
        {reg && E && (
          <>
            <div className="stack-g" onClick={() => setCodeOpen(!codeOpen)} style={{ cursor: "pointer" }}>{tx("From the film's code")} <span className="muted">{scanning ? tx("scanning…") : reg.length}</span><span className="grow" />{codeOpen ? "−" : "+"}</div>
            {codeOpen && code.map((r) => {
              const e = E.edits[r.id] || {};
              const t0 = e.t0 ?? r.t0, t1 = e.t1 ?? r.t1;
              const live = t >= t0 && t < t1;
              return (
                <div key={r.id} className={`lrow el ${selEl === r.id ? "on prim" : ""}`} style={{ opacity: e.hidden ? 0.5 : 1 }} onClick={() => pickEl?.(r.id, t0, t1)}>
                  <span className="dot" />
                  {r.kind === "puppet" ? <Sparkles className="kind" /> : r.kind === "auto" || r.kind === "text" ? <Type className="kind" /> : <Square className="kind" />}
                  <span className="lrow-name" style={{ fontWeight: live ? 600 : 400, textDecoration: e.hidden ? "line-through" : undefined }}>{e.text ?? r.label}</span>
                  {Object.keys(e).length > 0 && <span className="flag acc" title={tx("Edited by hand")}>•</span>}
                  <span className="lrow-t num">{fmt(t0)}</span>
                  <button className={`lrow-b ${e.hidden ? "keep" : ""}`} title={e.hidden ? tx("Show") : tx("Hide")} onClick={(ev) => { ev.stopPropagation(); E.set(r.id, { hidden: !e.hidden }); }}>{e.hidden ? <EyeOff /> : <Eye />}</button>
                </div>
              );
            })}
            {codeOpen && !reg.length && !scanning && (
              <div className="empty">
                <span>{tx("The film's code draws nothing Linnea can pick out yet.")}</span>
                {askClaude && <button className="btn sm" onClick={askClaude}>{tx("Have Claude make everything editable")}</button>}
              </div>
            )}
          </>
        )}
      </div>
    </div>
  );
}

/** Layers of a reel (or anything without the film's code elements): the stack, Add, and the inspector. */
export function LayersPanel({ L, t, W, H, dur, sel, setSel, pathFor, setPathFor, say }: { L: Layers; t: number; W: number; H: number; dur: number; sel: string; setSel: (s: string) => void; pathFor?: string; setPathFor?: (s: string) => void; say?: (s: string) => void }) {
  const [adding, setAdding] = useState(false);
  const select = (ids: string[], p?: string) => setSel(p ?? ids[ids.length - 1] ?? "");
  return (
    <div className="stack" style={{ gap: 12 }}>
      {!L.installed && (
        <div className="card" style={{ padding: 12 }}>
          <p className="sec" style={{ fontSize: 12, marginBottom: 8 }}>{tx("This page has no layer hook. Once installed, layers are drawn in the preview and in renders.")}</p>
          <button className="btn sm" onClick={L.install}>{tx("Install hook")}</button>
        </div>
      )}
      <div className="seg" style={{ alignSelf: "flex-start" }}>
        <button className={!adding ? "on" : ""} onClick={() => setAdding(false)}>{tx("Layers")}</button>
        <button className={adding ? "on" : ""} onClick={() => setAdding(true)}>{tx("Add")}</button>
      </div>
      {adding ? <AddPanel L={L} t={t} W={W} H={H} dur={dur} say={say || (() => {})} select={(ids) => { select(ids); setAdding(false); }} />
        : <LayerStack L={L} t={t} sel={sel} multi={sel ? [sel] : []} select={select} seek={() => {}} onAdd={() => setAdding(true)} />}
      {sel && !adding && <LayerInspector L={L} id={sel} t={t} W={W} H={H} dur={dur} pathFor={pathFor} setPathFor={setPathFor} say={say} select={(ids) => select(ids)} />}
    </div>
  );
}

// ---------- menus ----------
export type MenuItem = { label: string; keys?: string; run: () => void; disabled?: boolean; danger?: boolean } | "-";

export function Menu({ x, y, items, close }: { x: number; y: number; items: MenuItem[]; close: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ x, y });
  useEffect(() => {
    const r = ref.current?.getBoundingClientRect();
    if (r) setPos({ x: Math.min(x, window.innerWidth - r.width - 8), y: Math.min(y, window.innerHeight - r.height - 8) });
    const off = (e: Event) => { if (!ref.current?.contains(e.target as Node)) close(); };
    const key = (e: KeyboardEvent) => { if (e.key === "Escape") { e.stopImmediatePropagation(); close(); } };
    setTimeout(() => window.addEventListener("pointerdown", off, true));
    window.addEventListener("keydown", key, true);
    return () => { window.removeEventListener("pointerdown", off, true); window.removeEventListener("keydown", key, true); };
  }, [x, y]);
  return (
    <div ref={ref} className="menu" style={{ left: pos.x, top: pos.y }} onContextMenu={(e) => e.preventDefault()}>
      {items.map((m, i) => m === "-" ? <div key={i} className="menu-sep" /> : (
        <button key={i} disabled={m.disabled} className={m.danger ? "danger" : ""} onClick={() => { close(); m.run(); }}>
          <span className="grow">{m.label}</span>{m.keys && <span className="menu-k">{m.keys}</span>}
        </button>
      ))}
    </div>
  );
}

export function MenuButton({ label, items, icon }: { label: string; items: () => MenuItem[]; icon?: any }) {
  const [at, setAt] = useState<{ x: number; y: number } | null>(null);
  return (
    <>
      <button className={`btn sm ghost ${at ? "pressed" : ""}`} onClick={(e) => { const r = (e.currentTarget as HTMLElement).getBoundingClientRect(); setAt(at ? null : { x: r.left, y: r.bottom + 4 }); }}>{icon}{label}</button>
      {at && <Menu x={at.x} y={at.y} items={items()} close={() => setAt(null)} />}
    </>
  );
}

// ---------- the command palette ----------
export type Command = { id: string; label: string; group: string; keys?: string; run: () => void; when?: () => boolean };

export function Palette({ commands, close }: { commands: Command[]; close: () => void }) {
  const [q, setQ] = useState("");
  const [i, setI] = useState(0);
  const list = useMemo(() => {
    const words = q.toLowerCase().split(/\s+/).filter(Boolean);
    return commands.filter((c) => (!c.when || c.when()) && words.every((w) => (c.label + " " + c.group).toLowerCase().includes(w))).slice(0, 60);
  }, [q, commands]);
  useEffect(() => setI(0), [q]);
  const go = (c?: Command) => { if (!c) return; close(); setTimeout(c.run, 0); };
  return (
    <div className="palette-bg" onPointerDown={close}>
      <div className="palette" onPointerDown={(e) => e.stopPropagation()}>
        <div className="search big"><Keyboard /><input autoFocus placeholder={tx("Type a command: split, duplicate, render, add title…")} value={q} onChange={(e) => setQ(e.target.value)}
          onKeyDown={(e) => {
            e.stopPropagation();
            if (e.key === "Escape") close();
            else if (e.key === "ArrowDown") { e.preventDefault(); setI(Math.min(list.length - 1, i + 1)); }
            else if (e.key === "ArrowUp") { e.preventDefault(); setI(Math.max(0, i - 1)); }
            else if (e.key === "Enter") go(list[i]);
          }} /></div>
        <div className="palette-list">
          {list.map((c, j) => (
            <button key={c.id} className={j === i ? "on" : ""} onMouseEnter={() => setI(j)} onClick={() => go(c)}>
              <span className="muted" style={{ width: 84, flex: "none", fontSize: 11 }}>{c.group}</span>
              <span className="grow">{c.label}</span>
              {c.keys && <span className="menu-k">{c.keys}</span>}
            </button>
          ))}
          {!list.length && <div className="muted" style={{ padding: 12, fontSize: 12 }}>{tx("No command matches.")}</div>}
        </div>
      </div>
    </div>
  );
}

// ---------- puppets in Add: a grid with moving previews ----------
const changedHere = (st?: string) => st === "changed in this film" || String(st || "").startsWith("both changed");

export function PuppetGrid({ film, L, t, W, H, dur, say, select, dropRef, makeFromImage }: {
  film: string; L: Layers; t: number; W: number; H: number; dur: number; say: (s: string) => void; select: (ids: string[]) => void;
  dropRef: React.MutableRefObject<null | ((name: string, x: number, y: number) => void)>; makeFromImage: (f: File, names: string[]) => Promise<string>;
}) {
  const [list, setList] = useState<Json[]>([]);
  const [defs, setDefs] = useState<Record<string, Json>>({});
  const [sync, setSync] = useState<Json>({});
  const [own, setOwn] = useState<string[]>([]);
  const [q, setQ] = useState("");
  const [hover, setHover] = useState("");
  const [rig, setRig] = useState(false);
  const load = () => {
    tool("puppet_list", { film }).then((r) => {
      setList(r.puppets || []);
      const names = new Set((r.puppets || []).map((p: Json) => p.name));
      setOwn((r.local || []).map((f: string) => f.replace(/\.r?ppet$/, "")).filter((n: string) => !names.has(n) && !(r.linked || {})[n]));
      for (const p of r.puppets || []) tool("puppet_get", { name: p.name, film }).then((g) => setDefs((d) => ({ ...d, [p.name]: g.puppet }))).catch(() => {});
    }).catch(() => {});
    tool("puppet_sync", { film, check: true }).then((r) => setSync(r.puppets || {})).catch(() => {});
  };
  useEffect(load, [film]);
  const st = useRef({ L, t, list, sync });
  st.current = { L, t, list, sync };
  const add = async (name: string, x?: number, y?: number) => {
    const { L, t, list, sync } = st.current;
    const p = list.find((q) => q.name === name);
    if (!p) return;
    if (!changedHere(sync[p.name])) {
      try { await tool("puppet_link", { film, name: p.name }); } catch (e: any) { return say(e.message); }
    }
    const [pw, ph] = p.size || [400, 400];
    const s = Math.min(1, (W * 0.45) / pw, (H * 0.45) / ph);
    const w = Math.round(pw * s), h = Math.round(ph * s);
    const t0 = r2(Math.max(0, Math.min(t, dur - 0.5)));
    const it: Item = { id: uid("p"), kind: "puppet", puppet: p.name, label: p.label, t0, t1: r2(Math.min(dur, t0 + 4)), x: Math.round(x !== undefined ? x - w / 2 : (W - w) / 2), y: Math.round(y !== undefined ? y - h / 2 : (H - h) / 2), w, h, opacity: 1, fade: 0.2, params: {}, states: [] };
    L.update([...L.items, it]);
    select([it.id]);
    say(tx("{0} added to the film", p.label || p.name));
  };
  dropRef.current = (name, x, y) => add(name, x, y);
  const act = (p: Promise<any>, msg: string) => p.then(() => { say(msg); load(); }).catch((e) => say(e.message));
  const shown = list.filter((p) => !q || JSON.stringify([p.name, p.label, p.desc, p.tags]).toLowerCase().includes(q.toLowerCase()));
  return (
    <div className="stack" style={{ gap: 6 }}>
      {rig && <RigEditor taken={list.map((p) => p.name)} close={() => setRig(false)} say={say} saved={(n) => { setRig(false); load(); setTimeout(() => add(n), 600); }} />}
      <div className="rowx">
        <span className="sub-h grow">{tx("Puppets")}</span>
        <button className="btn icon sm ghost" title={tx("Build a puppet from bones and pose it")} onClick={() => setRig(true)}><BoneIcon /></button>
        <label className="btn icon sm ghost" title={tx("Pick several pictures: they play in turn, like a flip book")}><Images /><input type="file" accept="image/*" multiple hidden onChange={(e) => { const fs = [...(e.target.files || [])]; e.target.value = ""; if (fs.length) puppetFromFrames(fs, list.map((p) => p.name)).then((n) => { say(tx("\"{0}\" saved to the library", n)); load(); }).catch((er) => say(er.message)); }} /></label>
        <label className="btn sm ghost" title={tx("Make a puppet from a picture")}><ImageIcon />{tx("From a picture")}<input type="file" accept="image/*" hidden onChange={(e) => { const f = e.target.files?.[0]; e.target.value = ""; if (f) makeFromImage(f, list.map((p) => p.name)).then((n) => { say(tx("\"{0}\" saved to the library", n)); load(); }).catch((er) => say(er.message)); }} /></label>
      </div>
      {own.length > 0 && (
        <div className="card" style={{ padding: 8, fontSize: 12 }}>
          <div style={{ marginBottom: 6 }}>{tx("This film has {0} puppets of its own that the library does not have: {1}", own.length, own.join(", "))}</div>
          <button className="btn sm" onClick={() => act(tool("puppet_harvest", { film }), tx("Saved to the library"))}>{tx("Save them to the library")}</button>
        </div>
      )}
      {list.length > 6 && <div className="search"><Search /><input placeholder={tx("Search: pencil, bubble, arrow…")} value={q} onChange={(e) => setQ(e.target.value)} /></div>}
      <div className="ptiles">
        {shown.map((p) => {
          const s = sync[p.name];
          return (
            <div key={p.name} className="ptile" draggable onDragStart={(e) => { e.dataTransfer.setData("application/x-linnea", JSON.stringify({ puppet: p.name })); e.dataTransfer.effectAllowed = "copy"; }}
              onClick={() => add(p.name)} onMouseEnter={() => setHover(p.name)} onMouseLeave={() => setHover("")} title={p.desc || p.label}>
              <div className="ptile-pic">{defs[p.name] ? <PuppetPreview def={defs[p.name]} state={hover === p.name ? "" : defs[p.name].initial || ""} style={{ pointerEvents: "none", borderRadius: 8 }} /> : <Sparkles />}</div>
              <div className="ptile-n">{p.label || p.name}</div>
              <div className="ptile-s">{p.kind === "rppet" ? tx("{0} states", p.states?.length || 0) : tx("still")}{s && s !== "current" ? " · " : ""}{s && s !== "current" && <b style={{ color: "var(--accent)" }}>{s === "library has a newer version" ? tx("newer") : changedHere(s) ? tx("changed") : "!"}</b>}</div>
              {s && s !== "current" && (
                <div className="ptile-act" onClick={(e) => e.stopPropagation()}>
                  {(s === "library has a newer version" || s === "missing in this film") && <button className="btn sm" onClick={() => act(tool("puppet_sync", { film }), tx("Puppet updated"))}>{tx("Get")}</button>}
                  {changedHere(s) && <button className="btn sm" onClick={() => act(tool("puppet_save", { film, name: p.name }), tx("The film's version saved to the library"))}>{tx("Send to library")}</button>}
                </div>
              )}
            </div>
          );
        })}
      </div>
      {!shown.length && <span className="muted" style={{ fontSize: 12 }}>{tx("The library is empty. Ask Claude to \"save this as a puppet\", or upload a picture.")}</span>}
    </div>
  );
}

export type { PuppetInfo };
export { kindLabel, evalItem };
