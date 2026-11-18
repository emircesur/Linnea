// Films written as code: live preview from the page itself, notes drawn on frames, the check tour, manual renders.
import { tx } from "./i18n";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Redo2, SendHorizontal } from "lucide-react";
import { ArrowLeft, Check, ChevronLeft, ChevronRight, Circle, Copy, FolderOpen, Image as ImageIcon, MousePointer2, Pause, Pen, Play, Plus, RefreshCw, Repeat, Square, Trash2, Undo2, X } from "lucide-react";
import { Activity, dur, fetchJson, fileUrl, Json, runJob, tc, tool, useLoad } from "./api";
import { SoundPanel } from "./Studio";
import { LayerOverlay, LayersPanel, useLayers } from "./Layers";
import { createPortal } from "react-dom";
import { Lane, LANE_H, TrackLane } from "./Tracks";
import { PuppetFields, PuppetLibrary } from "./Puppets";
import { LayerList } from "./Elements";
import { Box, ElementOverlay, ElementPanel, Reg, scanRegistry, useElements } from "./Elements";
import { arrange, CardActions, deleteItem, ListTools, renameItem, useStars } from "./Manage";
import { Pencil } from "lucide-react";
import { Keyboard, LayoutGrid } from "lucide-react";
import { LangsPanel, ViewLang } from "./Langs";
import { Shortcuts, Storyboard } from "./Storyboard";

/** The elements found in each film's code (film id @ revision), so opening Layers again needs no new scan. */
const REGISTRY = new Map<string, Reg[]>();

const STATUS: Record<string, string> = { open: tx("Open"), claude: "Claude'da", fixed: tx("Fixed"), still: tx("Still there"), done: tx("Done"), wontfix: tx("Won't fix") };
const formats: [string, number, number][] = [["9:16", 1080, 1920], ["16:9", 1920, 1080], ["1:1", 1080, 1080], ["4:5", 1080, 1350]];

export function Films({ act, open, say }: { act: Activity; open: (id: string) => void; say: (s: string) => void }) {
  const films = useLoad(() => tool("hub_films"), [act.events.length]);
  const [title, setTitle] = useState("");
  const [fmt, setFmt] = useState(0);
  const [len, setLen] = useState(15);
  const make = async (args: Json) => {
    try {
      const r = await tool("hub_new_film", args);
      open(r.film);
    } catch (e: any) {
      say(e.message);
    }
  };
  const all: Json[] = films.data?.films || [];
  const stars = useStars("films");
  const [q, setQ] = useState("");
  const [sort, setSort] = useState("recent");
  const list = arrange(all, q, sort, stars.has, (f) => f.rev || 0);
  const duplicate = async (f: Json) => { try { const r = await tool("film_duplicate", { film: f.id }); say(tx("\"{0}\" created", r.title)); films.reload(); } catch (e: any) { say(e.message); } };
  return (
    <div className="page">
      <div className="wrap">
        <div className="page-head">
          <h1>{tx("Films")}</h1>
          <span className="grow" />
          <button className="btn ghost" onClick={() => make({ title: "Hello film", example: "hello-film" })}>{tx("Example film")}</button>
        </div>
        <div className="card" style={{ padding: 16, marginBottom: 24, display: "grid", gridTemplateColumns: "1fr auto auto auto", gap: 12, alignItems: "end" }}>
          <div className="field"><label>{tx("New film")}</label><input className="input" placeholder={tx("Title")} value={title} onChange={(e) => setTitle(e.target.value)} onKeyDown={(e) => e.key === "Enter" && title && len >= 0.5 && make({ title, width: formats[fmt][1], height: formats[fmt][2], dur: len })} /></div>
          <div className="field"><label>{tx("Format")}</label><div className="seg">{formats.map((f, i) => <button key={f[0]} className={fmt === i ? "on" : ""} onClick={() => setFmt(i)}>{f[0]}</button>)}</div></div>
          <div className="field"><label>{tx("Length, s")}</label><input className="input num" style={{ width: 80 }} type="number" min={1} step={1} value={len} onChange={(e) => setLen(+e.target.value)} /></div>
          <button className="btn primary" disabled={!title || !(len >= 0.5)} onClick={() => make({ title, width: formats[fmt][1], height: formats[fmt][2], dur: len })}><Plus />{tx("Create")}</button>
        </div>
        <p className="sec" style={{ marginBottom: 16, maxWidth: 760 }}>{tx("Films are written as code: Claude draws every second in")} <span className="num">film.js</span> {tx("and Linnea plays it live without rendering. You leave notes on frames, Claude fixes them and the preview reloads by itself. You render when you are ready.")}</p>
        {!all.length && !films.loading && <div className="empty">{tx("No films yet. Ask Claude to \"make a film in Linnea\", or open a blank template here. Folder:")} <span className="num">{films.data?.folder}</span></div>}
        {all.length > 0 && <div className="rowx" style={{ marginBottom: 16 }}><ListTools q={q} setQ={setQ} sort={sort} setSort={setSort} placeholder={tx("Search films")} /></div>}
        {all.length > 0 && !list.length && <div className="empty">{tx("Nothing matches")}</div>}
        <div className="proj-grid">
          {list.map((f) => (
            <button key={f.id} className="card proj" onClick={() => open(f.id)}>
              <CardActions starred={stars.has(f.id)} onStar={() => stars.toggle(f.id)} actions={[
                { icon: Pencil, label: tx("Rename"), run: () => renameItem("film", f.id, f.title, {}, say, films.reload) },
                { icon: Copy, label: tx("Duplicate"), run: () => duplicate(f) },
                { icon: FolderOpen, label: tx("Show in folder"), run: () => { tool("reveal", { path: f.dir }).catch((e) => say(e.message)); } },
                { icon: Trash2, label: tx("Delete"), danger: true, run: () => deleteItem("film", f.id, f.title, {}, say, films.reload) },
              ]} />
              <div className="thumb" style={{ aspectRatio: "16/10" }}>
                {f.cover && <img src={fileUrl(f.cover, f.rev)} alt="" style={{ objectFit: "contain", background: "#000" }} />}
                <span className="tc">{dur(f.dur)}</span>
              </div>
              <div className="meta">
                <b>{f.title}</b>
                <span>{[`${f.width}×${f.height}`, f.open_notes ? tx("{0} open notes", f.open_notes) : "", f.fixed_notes ? tx("{0} fixed", f.fixed_notes) : "", f.versions ? `v${f.versions}` : tx("Not rendered")].filter(Boolean).join(" · ")}</span>
              </div>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}

type Stroke = { kind: "pen" | "ring" | "box"; pts: [number, number][]; color: string };

export function FilmReview({ id, act, say, back }: { id: string; act: Activity; say: (s: string) => void; back: () => void }) {
  const [info, setInfo] = useState<Json | null>(null);
  const [notes, setNotes] = useState<Json[]>([]);
  const [versions, setVersions] = useState<Json[]>([]);
  const [err, setErr] = useState("");
  const [t, setT] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [loop, setLoop] = useState(true);
  const [ready, setReady] = useState(false);
  const [pageErr, setPageErr] = useState("");
  const [scenes, setScenes] = useState<Json[]>([]);
  const scenesRef = useRef<Json[]>([]);
  const [rev, setRev] = useState(0);
  const [range, setRange] = useState<[number, number] | null>(null);
  const [mode, setMode] = useState<"live" | "note" | "check">("live");
  const [watch, setWatch] = useState<Json | null>(null);
  const [busy, setBusy] = useState<{ label: string; p: number } | null>(null);
  const frame = useRef<HTMLIFrameElement>(null);
  const audio = useRef<HTMLAudioElement>(null);
  const tRef = useRef(0);

  const load = useCallback(async () => {
    try {
      const p = await tool("hub_project", { film: id });
      setInfo(p);
      setVersions(p.versions || []);
      const n = await tool("hub_notes", { film: id, status: "all" });
      setNotes(n.notes);
      tool("hub_sound", { film: id }).then((r) => setClips(r.clips || [])).catch(() => {});
      setErr("");
    } catch (e: any) {
      setErr(e.message);
    }
  }, [id]);
  useEffect(() => {
    load();
  }, [load]);
  // The preview reloads when the film's code changes (an agent saving film.js, or you).
  useEffect(() => {
    let stop = false;
    let last = 0;
    const tick = async () => {
      try {
        const r = await (await fetch(`/api/film-rev?id=${encodeURIComponent(id)}`)).json();
        if (!stop && r.rev && last && r.rev !== last) {
          setRev(r.rev);
          load();
        }
        last = r.rev || last;
      } catch {}
      if (!stop) setTimeout(tick, 900);
    };
    tick();
    return () => {
      stop = true;
    };
  }, [id, load]);
  const lastEv = useRef(Date.now());
  useEffect(() => {
    const ev = act.events.filter((e) => e.project === id && e.ts > lastEv.current);
    if (ev.length) {
      lastEv.current = ev[ev.length - 1].ts;
      load();
    }
  }, [act.events, id, load]);

  const W = info?.width || 1920, H = info?.height || 1080, FPS = info?.fps || 30, DUR = info?.dur || 10;
  scenesRef.current = scenes.length ? scenes : info?.scenes || [];
  const win = () => frame.current?.contentWindow as any;
  const draw = useCallback((x: number) => {
    const w = win();
    if (!w || !w.renderOut) return;
    try {
      const r = w.renderOut(x);
      if (r && r.catch) r.catch((e: any) => setPageErr(String(e?.message || e)));
    } catch (e: any) {
      setPageErr(String(e?.message || e));
    }
  }, []);
  const [side, setSide] = useState<"versions" | "sound" | "layers" | "langs" | "ai">("versions");
  // the language shown on the frame (null: the film's own texts), and the extra views
  const [viewLang, setViewLang] = useState<ViewLang>(null);
  const [board, setBoard] = useState(() => { try { return localStorage.getItem("linnea.board") === "1"; } catch { return false; } });
  const [keysOpen, setKeysOpen] = useState(false);
  useEffect(() => { try { localStorage.setItem("linnea.board", board ? "1" : "0"); } catch { /* private window */ } }, [board]);
  const [audioV, setAudioV] = useState(0);
  const [selTrack, setSelTrack] = useState("");
  const [boxes, setBoxes] = useState<Box[]>([]);
  const [reg, setReg] = useState<Reg[]>([]);
  const [selEl, setSelEl] = useState("");
  const refreshBoxes = () => setTimeout(() => { try { setBoxes([...((frame.current?.contentWindow as any)?.KE_BOXES || [])]); } catch { /* loading */ } }, 40);
  // undo / redo over both the element edits and the layers
  const hist = useRef<{ past: Json[]; future: Json[] }>({ past: [], future: [] });
  const snap = useRef<{ edits: Json; items: Json[]; ok?: boolean }>({ edits: {}, items: [] });
  const [, setHistV] = useState(0);
  const remember = (s: Json) => {
    hist.current.past.push(s);
    if (hist.current.past.length > 200) hist.current.past.shift();
    hist.current.future = [];
    setHistV((v) => v + 1);
  };
  const E = useElements("hub_2d", { film: id }, () => frame.current?.contentWindow, () => { draw(tRef.current); refreshBoxes(); }, () => draw(tRef.current),
    (prev) => remember({ edits: prev, items: snap.current.items }));
  const [clips, setClips] = useState<Json[]>([]);
  const [sel2d, setSel2d] = useState("");
  const [pathFor, setPathFor] = useState("");
  const L = useLayers("hub_2d", { film: id }, (items) => {
    const w = win();
    if (w?.VH2D) w.VH2D.set(items).then(() => draw(tRef.current));
  }, (prev) => remember({ edits: snap.current.edits, items: prev }));
  snap.current = { edits: E.edits, items: L.items, ok: E.loaded && L.loaded };
  const travel = (from: Json[], to: Json[]) => {
    const s = from.pop();
    if (!s) return;
    to.push({ edits: E.edits, items: L.items });
    if (JSON.stringify(s.edits) !== JSON.stringify(E.edits)) E.update(s.edits, true);
    if (JSON.stringify(s.items) !== JSON.stringify(L.items)) L.update(s.items, true);
    setHistV((v) => v + 1);
  };
  const undo = () => travel(hist.current.past, hist.current.future);
  const redo = () => travel(hist.current.future, hist.current.past);
  const undoRef = useRef({ undo, redo });
  undoRef.current = { undo, redo };
  useEffect(() => {
    const w = win();
    if (!ready || !w?.KE?.setLang) return;
    w.KE.setLang(viewLang?.code || "", viewLang?.texts);
    draw(tRef.current);
    refreshBoxes();
  }, [viewLang, ready]);
  // element editing: boxes of the frame on screen, and once per page load the list of all elements
  useEffect(() => {
    if (side === "layers" && !playing) refreshBoxes();
  }, [t, side, playing, ready]);
  // The list of all elements comes from drawing the whole film once. That runs on a hidden copy of the page,
  // so the preview on screen never jumps through the film, and it is kept for the film's current code.
  const [scanning, setScanning] = useState(false);
  const scanFrame = useRef<HTMLIFrameElement>(null);
  const scanKey = `${id}@${rev}`;
  useEffect(() => {
    if ((side !== "layers" && side !== "langs") || !ready || !info) return;
    const w = frame.current?.contentWindow as any;
    if (!w?.KE) {
      if (!L.installed) L.install().then(() => say(tx("Editing hook added to the film")));
      return;
    }
    const known = REGISTRY.get(scanKey);
    if (known) setReg(known);
    else setScanning(true);
  }, [side, ready, rev]);
  const onScanLoad = () => {
    const w = scanFrame.current?.contentWindow as any;
    const key = scanKey;
    const alive = () => scanFrame.current?.contentWindow === w && key === scanKeyRef.current;
    const t0 = Date.now();
    const wait = () => {
      if (!alive()) return;
      if (w?.READY !== true) {
        if (Date.now() - t0 < 30000) setTimeout(wait, 100);
        else setScanning(false);
        return;
      }
      try { w.KE?.setEdits(snap.current.edits); } catch { /* an older page */ }
      scanRegistry(w, info?.dur || 10, 0.5, (r) => alive() && setReg((prev) => (r.length >= prev.length ? r : prev)), alive).then((r) => {
        if (!alive()) return;
        REGISTRY.set(key, r);
        if (REGISTRY.size > 20) REGISTRY.delete(REGISTRY.keys().next().value!);
        setReg(r);
        setScanning(false);
      });
    };
    wait();
  };
  const scanKeyRef = useRef(scanKey);
  scanKeyRef.current = scanKey;
  const seek = useCallback((x: number) => {
    x = Math.max(0, Math.min(DUR, Math.round(x * FPS) / FPS));
    tRef.current = x;
    setT(x);
    if (audio.current) audio.current.currentTime = x;
    draw(x);
  }, [DUR, FPS, draw]);
  const onLoad = () => {
    setReady(false);
    setPageErr("");
    const w = win();
    if (!w) return;
    w.addEventListener("error", (e: any) => setPageErr(`${e.message} (${String(e.filename || "").split("/").pop()}:${e.lineno})`));
    const t0 = Date.now();
    const wait = () => {
      if (w.READY === true) {
        setReady(true);
        // the page read tweaks.json when it loaded; a layer or edit made in the last moment may not be saved
        // yet, so the app's own state (once it has loaded) goes in again
        if (snap.current.ok) {
          try {
            w.KE?.setEdits(snap.current.edits);
            const p = w.VH2D?.set(snap.current.items);
            if (p?.then) p.then(() => draw(tRef.current));
          } catch { /* an older page without the hook */ }
        }
        let s = w.VH_SCENES;
        if (typeof s === "function") s = s();
        setScenes(Array.isArray(s) ? s : []);
        draw(tRef.current);
      } else if (Date.now() - t0 < 30000) setTimeout(wait, 60);
      else setPageErr(typeof w.renderOut === "function" ? tx("window.READY never became true") : tx("window.renderOut is not defined"));
    };
    wait();
  };
  // Playback: the audio is the clock when there is sound.
  useEffect(() => {
    if (!playing) return;
    const a = audio.current;
    let start = performance.now() - tRef.current * 1000;
    if (a && info?.audio) {
      a.currentTime = tRef.current;
      a.play().catch(() => {});
    }
    let raf = 0;
    let pending = false;
    const loopFn = () => {
      let x = a && info?.audio && !a.paused ? a.currentTime : (performance.now() - start) / 1000;
      const end = range ? range[1] : DUR;
      if (x >= end) {
        if (loop) {
          x = range ? range[0] : 0;
          start = performance.now() - x * 1000;
          if (a) a.currentTime = x;
        } else {
          setPlaying(false);
          return;
        }
      }
      tRef.current = x;
      setT(x);
      const w = win();
      if (w?.renderOut && !pending) {
        try {
          const r = w.renderOut(x);
          if (r && r.then) {
            pending = true;
            r.finally(() => (pending = false));
          }
        } catch (e: any) {
          setPageErr(String(e?.message || e));
        }
      }
      raf = requestAnimationFrame(loopFn);
    };
    raf = requestAnimationFrame(loopFn);
    return () => {
      cancelAnimationFrame(raf);
      a?.pause();
    };
    // eslint-disable-next-line
  }, [playing, loop, range, info?.audio, DUR]);
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (/INPUT|TEXTAREA|SELECT/.test((e.target as HTMLElement).tagName)) return;
      if ((e.ctrlKey || e.metaKey) && (e.key === "z" || e.key === "Z")) {
        e.preventDefault();
        if (e.shiftKey) undoRef.current.redo(); else undoRef.current.undo();
        return;
      }
      if ((e.ctrlKey || e.metaKey) && (e.key === "y" || e.key === "Y")) {
        e.preventDefault();
        undoRef.current.redo();
        return;
      }
      if (e.ctrlKey || e.metaKey || e.altKey) return;
      const sc = (scenesRef.current || []).map((x: Json) => x.t0).filter((x: any) => typeof x === "number").sort((a: number, b: number) => a - b);
      if (e.code === "Space") {
        e.preventDefault();
        setPlaying((p) => !p);
      } else if (e.code === "ArrowRight") seek(tRef.current + (e.shiftKey ? 1 : 1 / FPS));
      else if (e.code === "ArrowLeft") seek(tRef.current - (e.shiftKey ? 1 : 1 / FPS));
      else if (e.key === ".") seek(tRef.current + 1 / FPS);
      else if (e.key === ",") seek(tRef.current - 1 / FPS);
      else if (e.code === "Home") { e.preventDefault(); seek(0); }
      else if (e.code === "End") { e.preventDefault(); seek(DUR); }
      else if (e.code === "PageDown") { e.preventDefault(); const n = sc.find((x: number) => x > tRef.current + 0.01); if (n !== undefined) seek(n); }
      else if (e.code === "PageUp") { e.preventDefault(); const p = sc.filter((x: number) => x < tRef.current - 0.25).pop(); seek(p ?? 0); }
      else if (e.key === "?") setKeysOpen((o) => !o);
      else if (e.key === "b" || e.key === "B") setBoard((b) => !b);
      else if (/^[1-5]$/.test(e.key)) { const k = (["versions", "sound", "layers", "langs", "ai"] as const)[+e.key - 1]; setSide(k); if (k === "layers") setPlaying(false); }
      else if (e.key === "n" || e.key === "N") {
        setPlaying(false);
        setMode("note");
      } else if (e.key === "Escape") setMode("live");
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [seek, FPS, DUR]);

  const run = async (label: string, name: string, args: Json) => {
    setBusy({ label, p: 0 });
    try {
      const r = await runJob(name, { film: id, ...args }, (p) => setBusy({ label, p }));
      setBusy(null);
      load();
      return r;
    } catch (e: any) {
      say(e.message);
      setBusy(null);
      return null;
    }
  };
  const copy = (s: string, msg: string) => navigator.clipboard.writeText(s).then(() => say(msg)).catch((e) => say(e.message));
  const lip = () => tool("hub_lipsync", { film: id }).then(() => say(tx("Mouth movement made from the film's sound; the preview reloads"))).catch((e) => say(e.message));
  const sendFrame = async () => {
    const r = await tool("hub_frame", { film: id, ...(range ? { from: range[0], to: range[1], step: Math.max((range[1] - range[0]) / 8, 1 / FPS) } : { t: tRef.current }) });
    const where = range ? tx("range {0}–{1}", tc(range[0], FPS), tc(range[1], FPS)) : tx("{0} (frame {1})", tc(tRef.current, FPS), Math.round(tRef.current * FPS));
    const files = r.sheet ? tx("Contact sheet: {0}\nFrames: {1}", r.sheet, r.frames.map((f: Json) => `${f.t.toFixed(2)}s ${f.file}`).join(", ")) : tx("Frame: {0}", r.frames[0].file);
    copy(tx("Linnea film \"{0}\" ({1}), {2}.\n{3}\nLook at this frame:", info?.title, id, where, files), tx("Frame path copied; paste it to Claude"));
  };
  const openNotes = notes.filter((n) => ["open", "claude", "still"].includes(n.status));
  const toCheck = notes.filter((n) => n.status === "fixed");
  const latest = versions[versions.length - 1];
  const saveClips = async (next: Json[]) => {
    setClips(next);
    try {
      await tool("hub_sound", { film: id, clips: next });
      await tool("hub_sound_apply", { film: id });
      setAudioV(Date.now());
    } catch (e: any) {
      say(e.message);
    }
  };
  const lanes: Lane[] = [
    {
      key: "elements", label: tx("Code elements"), color: "#7fb0ff", sel: selEl ? `e:${selEl}` : "",
      items: reg.map((r) => ({ id: `e:${r.id}`, s: E.edits[r.id]?.t0 ?? r.t0, e: E.edits[r.id]?.t1 ?? r.t1, label: (E.edits[r.id]?.hidden ? "(gizli) " : "") + (E.edits[r.id]?.text ?? r.label) })),
      onSelect: (eid) => { setSelEl(eid.slice(2)); setSide("layers"); },
      onChange: (eid, s, e) => E.set(eid.slice(2), { t0: s, t1: e }),
      onDelete: (eid) => E.set(eid.slice(2), { hidden: true }),
    },
    {
      key: "layers", label: tx("Layers"), color: "#86b991", sel: selTrack,
      items: L.items.map((it) => ({ id: `l:${it.id}`, s: it.t0, e: it.t1, label: (it.hidden ? "(gizli) " : "") + (it.label || it.text || it.title || it.puppet || it.kind) })),
      onSelect: (lid) => { setSelTrack(lid); setSel2d(lid.slice(2)); setSide("layers"); },
      onChange: (lid, s, e) => L.update(L.items.map((it) => (`l:${it.id}` === lid ? { ...it, t0: s, t1: e } : it))),
      onDelete: (lid) => { L.update(L.items.filter((it) => `l:${it.id}` !== lid)); setSelTrack(""); },
    },
    {
      key: "sound", label: tx("Sound clips"), color: "#7fa7d9", sel: selTrack,
      // each sound at its real length; a muted one says so, and selecting one opens it in the Sound tab
      items: clips.map((c, i) => ({ id: `c:${c.id || i}`, s: c.at, e: c.at + Math.max(0.3, c.dur || 1), label: (c.mute ? tx("(muted) ") : "") + (c.name || c.file) })),
      onSelect: (cid) => { setSelTrack(cid); setSide("sound"); },
      onChange: (cid, s) => saveClips(clips.map((c, i) => (`c:${c.id || i}` === cid ? { ...c, at: Math.round(s * 100) / 100 } : c))),
      onDelete: (cid) => { saveClips(clips.filter((c, i) => `c:${c.id || i}` !== cid)); setSelTrack(""); },
    },
  ];

  if (err) return <div className="page"><button className="btn ghost" onClick={back}><ArrowLeft />{tx("Films")}</button><p className="err" style={{ marginTop: 16 }}>{err}</p></div>;
  if (!info) return <div className="page"><span className="muted">{tx("Loading")}</span></div>;
  return (
    <div className="ed" style={{ gridTemplateColumns: "320px minmax(0, 1fr) 320px" }}>
      <div className="ed-top">
        <button className="btn icon ghost" onClick={back} title={tx("Films")}><ArrowLeft /></button>
        <span className="title">{info.title}</span>
        <span className="muted num" style={{ fontSize: 12 }}>{W}×{H} · {FPS} {tx("fps ·")} {dur(DUR)}</span>
        <span className="grow" />
        {busy && (
          <div className="rowx" style={{ width: 260 }}>
            <span className="sec" style={{ fontSize: 12, whiteSpace: "nowrap" }}>{busy.label}</span>
            <div className="meter grow"><i style={{ width: `${busy.p * 100}%` }} /></div>
            <button className="btn icon sm ghost" title={tx("Stop")} onClick={() => act.jobs.filter((j) => j.project === id && j.status === "running").forEach((j) => tool("hub_job", { id: j.id, cancel: true }))}><Square /></button>
          </div>
        )}
        <button className="btn ghost" title={tx("Film folder")} onClick={() => tool("reveal", { path: info.dir })}><FolderOpen /></button>
        {toCheck.length > 0 && <button className={`btn ${mode === "check" ? "primary" : ""}`} onClick={() => { setMode(mode === "check" ? "live" : "check"); setPlaying(false); }}>{tx("Check the fixes")} {toCheck.length}</button>}
        {openNotes.length > 0 && (
          <button className="btn" disabled={!!busy} title={tx("Let the film assistant fix all open notes")} onClick={async () => {
            setSide("ai");
            const r = await run(tx("Assistant fixing notes"), "hub_ask", { prompt: tx("Fix all open notes one by one: look at each note's picture, make the change in film.js, check that frame and write a short reply with hub_note_update status=fixed. Leave a note open if you cannot fix it, and say why.") });
            if (r) say(tx("The assistant is done: review with \"Check the fixes\""));
          }}><SendHorizontal />{tx("Fix with assistant")}</button>
        )}
        {openNotes.length > 0 && (
          <button className="btn" onClick={() => copy(tx("The film \"{0}\" ({1}) in Linnea has {2} notes. Read them with hub_notes (look at images.crop first), fix them, and for each one write hub_note_update status=fixed with a short reply. Do not render.", info.title, id, openNotes.length), tx("Message for Claude copied"))}><Copy />{tx("Send to Claude")}</button>
        )}
        <RenderMenu disabled={!!busy} latest={latest} fixed={toCheck.length + notes.filter((n) => n.status === "done" && !n.version).length} run={run} lang={viewLang?.code || ""} />
      </div>

      <div className="ed-left">
        <div className="tabs"><button className="on">{tx("Notes")} {openNotes.length ? openNotes.length : ""}</button></div>
        <div className="tab-body">
          <NotesList notes={notes} fps={FPS} seek={(n: Json) => { seek(n.t); if (n.t1) setRange([n.t, n.t1]); }} update={async (n: Json, p: Json) => { await tool("hub_note_update", { film: id, id: n.id, ...p }); load(); }} />
        </div>
      </div>

      <div className="stage">
        <Viewer W={W} H={H}>
          {watch ? (
            <VersionPlayer v={watch} t={t} onTime={(x: number) => setT(x)} close={() => setWatch(null)} note={async (tt: number, text: string) => { await tool("hub_note_add", { film: id, t: tt, text, video: watch.file }); load(); say(tx("Note left on the version")); }} />
          ) : (
            <>
              <iframe ref={frame} key={rev} src={`${info.url}?r=${rev}`} onLoad={onLoad} style={{ width: W, height: H, border: 0, display: "block", background: "#000", pointerEvents: "none" }} title="film" />
              {mode === "note" && <NoteLayer W={W} H={H} t={t} range={range} fps={FPS} cancel={() => setMode("live")} save={async (p: Json) => { setBusy({ label: tx("Saving the note"), p: 0.5 }); try { await tool("hub_note_add", { film: id, ...p }); setMode("live"); setRange(null); load(); } catch (e: any) { say(e.message); } setBusy(null); }} />}
              {mode === "live" && side === "layers" && !playing && <ElementOverlay boxes={boxes} E={E} sel={selEl} setSel={(x) => { setSelEl(x); if (x) setSel2d(""); }} t={t} W={W} H={H} L={L} />}
              {mode === "live" && side === "layers" && !playing && <LayerOverlay L={L} t={t} sel={sel2d} setSel={(x) => { setSel2d(x); if (x) setSelEl(""); }} W={W} H={H} passThrough pathFor={pathFor} />}
              {mode === "check" && <CheckLayer notes={toCheck} seek={seek} update={async (n: Json, p: Json) => { await tool("hub_note_update", { film: id, id: n.id, ...p }); load(); }} done={() => setMode("live")} />}
            </>
          )}
        </Viewer>
        {scanning && info && <iframe ref={scanFrame} key={scanKey} src={`${info.url}?r=${rev}&scan=1`} onLoad={onScanLoad} title="scan" aria-hidden tabIndex={-1} style={{ position: "fixed", left: -20000, top: 0, width: W, height: H, border: 0, visibility: "hidden", pointerEvents: "none" }} />}
        <div id="note-host" style={{ width: "100%", maxWidth: 680 }} />
        {!ready && !watch && <div className="muted" style={{ fontSize: 12 }}>{pageErr ? <span className="err">{pageErr}</span> : tx("Loading the page")}</div>}
        {ready && pageErr && <div className="err" style={{ fontSize: 12 }}>{pageErr}</div>}
        <div className="transport">
          <button className="btn icon sm ghost" onClick={() => seek(t - 1 / FPS)} title={tx("Previous frame")}><ChevronLeft /></button>
          <button className="btn icon" onClick={() => setPlaying(!playing)} disabled={mode !== "live"} title={tx("Play / pause")}>{playing ? <Pause /> : <Play />}</button>
          <button className="btn icon sm ghost" onClick={() => seek(t + 1 / FPS)} title={tx("Next frame")}><ChevronRight /></button>
          <span className="tc"><b>{tc(t, FPS)}</b> <span>/ {tc(DUR, FPS)}</span></span>
          <span className="muted num" style={{ fontSize: 12 }}>{tx("frame")} {Math.round(t * FPS)}</span>
          <button className={`btn icon sm ${loop ? "" : "ghost"}`} onClick={() => setLoop(!loop)} title={tx("Loop")}><Repeat /></button>
          <span style={{ width: 8 }} />
          <button className={`btn sm ${mode === "note" ? "primary" : ""}`} onClick={() => { setPlaying(false); setMode(mode === "note" ? "live" : "note"); }}><Pen />{tx("Note")} <span className="kbd">N</span></button>
          <button className="btn sm" onClick={sendFrame} title={tx("Capture the frame (or the selected range) for Claude")}><ImageIcon />{range ? tx("Range to Claude") : tx("Frame to Claude")}</button>
          {range && <button className="btn icon sm ghost" title={tx("Clear range")} onClick={() => setRange(null)}><X /></button>}
          <span className="grow" />
          {viewLang && <button className="btn sm ghost" title={tx("Back to the original texts")} onClick={() => setViewLang(null)} style={{ color: "var(--accent)" }}>{viewLang.code} <X /></button>}
          <button className={`btn icon sm ${board ? "" : "ghost"}`} onClick={() => setBoard(!board)} title={tx("Storyboard (B)")}><LayoutGrid /></button>
          <button className="btn icon sm ghost" onClick={() => setKeysOpen(true)} title={tx("Keyboard shortcuts (?)")}><Keyboard /></button>
        </div>
        {board && !watch && info && <Storyboard url={info.url} rev={rev} lang={viewLang?.code || ""} W={W} H={H} dur={DUR} scenes={scenesRef.current} t={t} seek={(x) => { setPlaying(false); seek(x); }} />}
        {keysOpen && <Shortcuts close={() => setKeysOpen(false)} />}
        {info.audio && <audio ref={audio} src={fileUrl(info.audio, audioV)} preload="auto" />}
      </div>

      <div className="ed-right" style={{ padding: 0, gap: 0 }}>
        <div className="tabs">
          {([["versions", tx("Versions")], ["sound", tx("Sound")], ["layers", tx("Layers")], ["langs", tx("Languages")], ["ai", tx("Assistant")]] as const).map(([k, l]) => (
            <button key={k} className={side === k ? "on" : ""} onClick={() => { setSide(k); if (k === "layers") setPlaying(false); }}>{l}{k === "layers" && L.items.length ? ` ${L.items.length}` : ""}</button>
          ))}
          <span style={{ flex: 1 }} />
          <button className="btn icon sm ghost" title={tx("Undo (Ctrl+Z)")} disabled={!hist.current.past.length} onClick={undo}><Undo2 /></button>
          <button className="btn icon sm ghost" title={tx("Redo (Ctrl+Y)")} disabled={!hist.current.future.length} onClick={redo}><Redo2 /></button>
        </div>
        <div style={{ padding: 16, display: "flex", flexDirection: "column", gap: 24, overflow: "auto", flex: 1 }}>
        {side === "langs" && <LangsPanel film={id} reg={reg} scanning={scanning} t={t} seek={(x) => { setPlaying(false); seek(x); }} say={say} view={viewLang} setView={setViewLang} run={run} copy={copy} />}
        {side === "ai" && <FilmAssistant id={id} dir={info.dir} t={t} fps={FPS} sel={selEl} run={run} busy={!!busy} say={say} />}
        {side === "sound" && <SoundPanel film={id} t={t} range={range} say={say} rev={audioV} sel={selTrack.startsWith("c:") ? selTrack.slice(2) : ""} seek={(x) => { setPlaying(false); seek(x); }} onApplied={() => { setAudioV(Date.now()); load(); }} />}
        {side === "layers" && selEl && <ElementPanel id={selEl} box={boxes.find((b) => b.id === selEl)} reg={reg.find((r) => r.id === selEl)} E={E} t={t} close={() => setSelEl("")} />}
        {side === "layers" && selEl && (() => {
          const b = boxes.find((x) => x.id === selEl);
          if (b?.kind === "puppet") return <PuppetFields info={win()?.KE_PUPPET?.(b.puppet) || null} params={E.edits[selEl]?.params ?? b.params ?? {}} states={E.edits[selEl]?.states ?? b.states ?? []} localT={b.pt ?? 0} onParams={(params) => E.set(selEl, { params })} onStates={(states) => E.set(selEl, { states })} onLipsync={lip} />;
          return <button className="btn sm" onClick={() => navigator.clipboard.writeText(tx("In the film \"{0}\" in Linnea, turn the element \"{1}\" (id {2}) into a puppet: save its drawing code to the library with puppet_save (things that may change, like colour or text, become params; if it animates, make it a rich puppet with states), draw it in the film with KE.puppet, keep the current edits, then hub_live.", id, b?.label || selEl, selEl)).then(() => say(tx("Message for Claude copied")))}>{tx("Have Claude make it a puppet")}</button>;
        })()}
        {side === "layers" && sel2d && (() => {
          const it = L.items.find((x: Json) => x.id === sel2d);
          if (it?.kind !== "puppet") return null;
          const put = (p: Json) => L.update(L.items.map((x: Json) => (x.id === sel2d ? { ...x, ...p } : x)));
          return (<div className="stack" style={{ gap: 6 }}><h2>{it.label || it.puppet}</h2><PuppetFields info={win()?.KE_PUPPET?.(it.puppet) || null} params={it.params || {}} states={it.states || []} localT={t - it.t0} onParams={(params) => put({ params })} onStates={(states) => put({ states })} onLipsync={lip} /></div>);
        })()}
        {side === "layers" && !selEl && !sel2d && <LayerList reg={reg} E={E} L={L} t={t} sel={selEl || sel2d} pick={(k, x, t0) => {
          if (k === "e") { setSelEl(x); setSel2d(""); } else { setSel2d(x); setSelEl(""); }
          const it = k === "l" ? L.items.find((i) => i.id === x) : null;
          const r = k === "e" ? reg.find((i) => i.id === x) : null;
          const t1 = it ? it.t1 : (E.edits[x]?.t1 ?? r?.t1 ?? DUR);
          if (tRef.current < t0 || tRef.current >= t1) seek(t0 + 0.05);
        }} />}
        {side === "layers" && !selEl && !sel2d && <PuppetLibrary film={id} L={L} t={t} W={W} H={H} dur={DUR} say={say} />}
        {side === "layers" && !selEl && (
          <div className="stack" style={{ gap: 6 }}>
            <h2>{tx("Code elements")}</h2>
            <p className="sec" style={{ fontSize: 12, lineHeight: "18px" }}>{tx("Text and elements the film draws itself show as blue boxes: click, drag, resize from the corner, double-click text to change it. The code stays as it is; edits go to tweaks.json and into renders.")}</p>
            {!reg.length && <span className="muted" style={{ fontSize: 12 }}>{tx("Scanning elements…")}</span>}
            <button className="btn sm" onClick={() => navigator.clipboard.writeText(tx("Make the film \"{0}\" ({1}) in Linnea editable: draw every text and element on screen (title frames, caption boxes, characters, objects) with KE.text / KE.group so I can move and resize each as one piece in the app. Keep the current tweaks.json edits. Then hub_live.", info.title, id)).then(() => say(tx("Message for Claude copied")))}>{tx("Have Claude make everything editable")}</button>
          </div>
        )}
        {side === "layers" && <LayersPanel L={L} t={t} W={W} H={H} dur={DUR} sel={sel2d} setSel={(x) => { setSel2d(x); if (x !== pathFor) setPathFor(""); }} pathFor={pathFor} setPathFor={setPathFor} />}
        {side === "versions" && <>
        <Versions versions={versions} watch={watch} setWatch={setWatch} />
        <Cleanup film={id} say={say} />
        <div className="stack" style={{ gap: 8 }}>
          <h2>{tx("Working with Claude")}</h2>
          <p className="sec" style={{ fontSize: 12, lineHeight: "18px" }}>{tx("Just tell Claude Code \"in Linnea, in the film")} {info.title} {tx("change this\"; the preview reloads when the change is saved. To point at a place in the frame, draw with")} <b>N</b> {tx("and leave a note; you can also capture a moment or a range (Shift-drag on the timeline) and paste it.")}</p>
          <div className="kv">
            <span>{tx("Page")}</span><span className="num" style={{ color: "var(--text-2)" }}>{info.page}</span>
            <span>{tx("Sound")}</span><span className="num" style={{ color: "var(--text-2)" }}>{info.audio ? info.audio.split("/").slice(-2).join("/") : "yok"}</span>
            <span>{tx("Chunks")}</span><span className="num" style={{ color: "var(--text-2)" }}>{info.chunks} × {Math.round(info.chunk_frames / FPS * 10) / 10} {tx("s")}</span>
          </div>
          <button className="btn sm" onClick={() => copy(tx("claude: open the film \"{0}\" ({1}) in Linnea: look at its state with hub_project and hub_live.", info.title, id), tx("Copied"))}><Copy />{tx("Introduce the film to Claude")}</button>
        </div>
        </>}
        </div>
      </div>

      <div className="dock">
        <FilmTimeline dur={DUR} fps={FPS} t={t} seek={(x: number) => { setPlaying(false); seek(x); }} scenes={scenes.length ? scenes : info.scenes || []} notes={notes} range={range} setRange={setRange} changed={latest?.changedRanges || []} chunkSec={info.chunk_frames / FPS} lanes={lanes} />
      </div>
    </div>
  );
}

/** Lines of a and b as a short diff: only the changed stretches, with two lines around each. */
function lineDiff(a: string, b: string): { t: " " | "+" | "-" | "…"; s: string }[] {
  const A = a.split("\n"), B = b.split("\n");
  let p = 0;
  while (p < A.length && p < B.length && A[p] === B[p]) p++;
  let q = 0;
  while (q < A.length - p && q < B.length - p && A[A.length - 1 - q] === B[B.length - 1 - q]) q++;
  const a2 = A.slice(p, A.length - q), b2 = B.slice(p, B.length - q);
  const n = a2.length, m = b2.length;
  const out: { t: " " | "+" | "-" | "…"; s: string }[] = A.slice(Math.max(0, p - 2), p).map((s) => ({ t: " ", s }));
  if (n * m > 4_000_000) {
    out.push(...a2.map((s) => ({ t: "-" as const, s })), ...b2.map((s) => ({ t: "+" as const, s })));
  } else {
    const L = new Uint32Array((n + 1) * (m + 1));
    for (let i = n - 1; i >= 0; i--) for (let j = m - 1; j >= 0; j--) L[i * (m + 1) + j] = a2[i] === b2[j] ? L[(i + 1) * (m + 1) + j + 1] + 1 : Math.max(L[(i + 1) * (m + 1) + j], L[i * (m + 1) + j + 1]);
    let i = 0, j = 0, same: string[] = [];
    const flush = () => { if (same.length > 5) out.push(...same.slice(0, 2).map((s) => ({ t: " " as const, s })), { t: "…", s: tx("{0} lines unchanged", same.length - 4) }, ...same.slice(-2).map((s) => ({ t: " " as const, s }))); else out.push(...same.map((s) => ({ t: " " as const, s }))); same = []; };
    while (i < n || j < m) {
      if (i < n && j < m && a2[i] === b2[j]) { same.push(a2[i]); i++; j++; }
      else { flush(); if (j < m && (i >= n || L[i * (m + 1) + j + 1] >= L[(i + 1) * (m + 1) + j])) { out.push({ t: "+", s: b2[j] }); j++; } else { out.push({ t: "-", s: a2[i] }); i++; } }
    }
    flush();
  }
  out.push(...A.slice(A.length - q, Math.min(A.length, A.length - q + 2)).map((s) => ({ t: " " as const, s })));
  return out;
}

function DiffView({ files, close }: { files: Json[]; close: () => void }) {
  return (
    <div className="card" style={{ padding: 10, display: "flex", flexDirection: "column", gap: 8 }}>
      <div className="rowx"><b className="grow" style={{ fontWeight: 500, fontSize: 12 }}>{tx("Changes")}</b><button className="btn icon sm ghost" onClick={close}><X /></button></div>
      {files.map((f) => (
        <div key={f.path} className="stack" style={{ gap: 4 }}>
          <span className="num" style={{ fontSize: 11 }}>{f.path}{f.existed ? "" : tx(" (new file)")}</span>
          <pre style={{ margin: 0, maxHeight: 320, overflow: "auto", fontSize: 11, lineHeight: "16px", background: "var(--sunken)", borderRadius: 6, padding: 6 }}>
            {lineDiff(f.before || "", f.after || "").map((l, i) => (
              <div key={i} style={{ whiteSpace: "pre", color: l.t === "+" ? "var(--good)" : l.t === "-" ? "var(--bad)" : l.t === "…" ? "var(--text-3)" : "var(--text-2)" }}>{l.t === "…" ? `   ⋯ ${l.s}` : `${l.t} ${l.s}`}</div>
            ))}
          </pre>
        </div>
      ))}
    </div>
  );
}

/** Change the film in plain words: the configured model edits film.js, checks the page and looks at its frames.
 *  Its steps show while it works; every request can be looked at as a diff and taken back. */
function FilmAssistant({ id, dir, t, fps, sel, run, busy, say }: Json) {
  const s = useLoad(() => tool("settings"), []);
  const [prompt, setPrompt] = useState("");
  const [hist, setHist] = useState<Json[]>([]);
  const [live, setLive] = useState<Json | null>(null);
  const [diff, setDiff] = useState<Json[] | null>(null);
  const off = !!s.data && (s.data.llm?.provider || "none") === "none";
  const loadHist = () => fetchJson(`${dir}/.videohub/assistant.json`).then((h) => setHist(Array.isArray(h) ? h : []));
  useEffect(() => { loadHist(); }, [dir, busy]);
  // while a request runs, the steps it takes come from the file the assistant writes as it goes
  useEffect(() => {
    if (!busy) return;
    let stop = false;
    const tick = async () => {
      const l = await fetchJson(`${dir}/.videohub/assistant-live.json`);
      if (stop) return;
      setLive(l?.running ? l : null);
      setTimeout(tick, 700);
    };
    tick();
    return () => { stop = true; setLive(null); };
  }, [busy, dir]);
  const send = async () => {
    const q = prompt.trim();
    if (!q) return;
    const r = await run(tx("Assistant working"), "hub_ask", { prompt: q, t, ...(sel ? { element: sel } : {}) });
    if (r) setPrompt("");
    loadHist();
  };
  const undo = (h: Json, latest: boolean) => {
    if (!window.confirm(latest ? tx("Undo this request's changes?") : tx("Return the film to how it was before this request? Changes from later requests are undone too."))) return;
    tool("hub_restore", { film: id, since: h.since, action: "undo" }).then(() => { say(tx("Undone; the preview reloads")); setDiff(null); loadHist(); }).catch((e) => say(e.message));
  };
  const showDiff = (h: Json) => tool("hub_restore", { film: id, since: h.since, action: "diff" }).then((r) => setDiff(r.files || [])).catch((e) => say(e.message));
  const list = hist.slice().reverse();
  const latest = list.find((h) => h.since && h.changed?.length && !h.undone);
  const lastImg = live ? [...(live.steps || [])].reverse().find((x: Json) => x.image)?.image : null;
  return (
    <div className="stack" style={{ gap: 8 }}>
      <h2>{tx("Film assistant")}</h2>
      {off && <div className="warn">{tx("No model chosen: pick Claude, OpenAI or a local server in Settings > Assistant.")}</div>}
      <p className="sec" style={{ fontSize: 12, lineHeight: "18px" }}>{tx("Say what should change. The assistant reads and edits film.js, checks the page and looks at the frames it changed; the preview reloads by itself. The second on screen")}{sel ? tx(" and the selected element") : ""} {tx("are added to the request. It remembers recent requests and never renders.")}</p>
      <textarea className="input" style={{ fontFamily: "var(--font)", fontSize: 13 }} rows={3} placeholder={tx("Make the title yellow and slide it up at 2 s")} value={prompt} onChange={(e) => setPrompt(e.target.value)} onKeyDown={(e) => e.key === "Enter" && (e.ctrlKey || e.metaKey) && !busy && !off && send()} />
      <div className="rowx">
        <span className="grow muted num" style={{ fontSize: 12 }}>{tc(t, fps)}{sel ? ` · ${sel}` : ""}</span>
        <button className="btn sm primary" disabled={!prompt.trim() || busy || off} onClick={send}><SendHorizontal />{tx("Send")}</button>
      </div>
      {live && (
        <div className="card" style={{ padding: 10, fontSize: 12, lineHeight: "18px", display: "flex", flexDirection: "column", gap: 4 }}>
          <b style={{ fontWeight: 500 }}>{live.prompt}</b>
          {(live.steps || []).map((x: Json, i: number) => (
            <div key={i} className="rowx" style={{ gap: 6 }}>
              <span style={{ width: 12, color: x.error ? "var(--bad)" : i === live.steps.length - 1 ? "var(--accent-ink)" : "var(--good)" }}>{x.error ? "✕" : i === live.steps.length - 1 ? "›" : "✓"}</span>
              <span className="grow">{x.text}{x.note ? <span className="muted"> · {x.note}</span> : null}{x.error ? <span className="err"> · {x.error}</span> : null}</span>
            </div>
          ))}
          {!(live.steps || []).length && <span className="muted">{tx("Thinking…")}</span>}
          {lastImg && <img src={fileUrl(lastImg, (live.steps || []).length)} alt="" style={{ width: "100%", borderRadius: 6, marginTop: 4, background: "#000" }} />}
        </div>
      )}
      {diff && <DiffView files={diff} close={() => setDiff(null)} />}
      {list.map((h, i) => (
        <div key={h.ts || i} className="card" style={{ padding: 10, fontSize: 12, lineHeight: "18px", opacity: h.undone ? 0.6 : 1 }}>
          <b style={{ fontWeight: 500 }}>{h.q}</b>
          <div className="sec" style={{ whiteSpace: "pre-wrap", marginTop: 6 }}>{h.a}</div>
          {h.changed?.length > 0 && <div className="muted num" style={{ marginTop: 6 }}>{tx("Changed:")} {h.changed.join(", ")}</div>}
          {h.undone && <span className="tag" style={{ marginTop: 6, display: "inline-block" }}>{tx("Undone")}</span>}
          {h.since && h.changed?.length > 0 && !h.undone && (
            <div className="rowx" style={{ marginTop: 6 }}>
              <button className="btn sm ghost" onClick={() => showDiff(h)}>{tx("Show diff")}</button>
              <button className="btn sm ghost" disabled={busy} onClick={() => undo(h, h === latest)}><Undo2 />{h === latest ? tx("Undo") : tx("Return to before this request")}</button>
            </div>
          )}
        </div>
      ))}
    </div>
  );
}

function Viewer({ W, H, children }: { W: number; H: number; children: any }) {
  const box = useRef<HTMLDivElement>(null);
  const [s, setS] = useState(0.3);
  useEffect(() => {
    const ro = new ResizeObserver((e) => {
      const r = e[0].contentRect;
      setS(Math.max(0.05, Math.min(r.width / W, r.height / H)));
    });
    if (box.current) ro.observe(box.current);
    return () => ro.disconnect();
  }, [W, H]);
  return (
    <div ref={box} style={{ flex: 1, minHeight: 0, width: "100%", display: "flex", alignItems: "center", justifyContent: "center" }}>
      <div className="monitor" style={{ width: W * s, height: H * s, flex: "0 0 auto" }}>
        <div style={{ width: W, height: H, transform: `scale(${s})`, transformOrigin: "0 0", position: "absolute", left: 0, top: 0 }}>{children}</div>
      </div>
    </div>
  );
}

function NoteLayer({ W, H, t, range, fps, save, cancel }: Json) {
  const cv = useRef<HTMLCanvasElement>(null);
  const [tool_, setTool] = useState<"pen" | "ring" | "box">("pen");
  const [strokes, setStrokes] = useState<Stroke[]>([]);
  const [text, setText] = useState("");
  const cur = useRef<Stroke | null>(null);
  const host = useNoteHost();
  const color = "#ff3b30";
  const lw = Math.max(4, Math.round(Math.min(W, H) / 160));
  const paint = useCallback((list: Stroke[]) => {
    const g = cv.current?.getContext("2d");
    if (!g) return;
    g.clearRect(0, 0, W, H);
    g.lineCap = "round";
    g.lineJoin = "round";
    for (const s of list) {
      g.strokeStyle = s.color;
      g.lineWidth = lw;
      g.beginPath();
      const [a, b] = [s.pts[0], s.pts[s.pts.length - 1]];
      if (s.kind === "pen") s.pts.forEach(([x, y], i) => (i ? g.lineTo(x, y) : g.moveTo(x, y)));
      else if (s.kind === "box") g.rect(Math.min(a[0], b[0]), Math.min(a[1], b[1]), Math.abs(b[0] - a[0]), Math.abs(b[1] - a[1]));
      else g.ellipse((a[0] + b[0]) / 2, (a[1] + b[1]) / 2, Math.max(Math.abs(b[0] - a[0]) / 2, lw * 4), Math.max(Math.abs(b[1] - a[1]) / 2, lw * 4), 0, 0, Math.PI * 2);
      g.stroke();
    }
  }, [W, H, lw]);
  useEffect(() => paint(strokes), [strokes, paint]);
  const pos = (e: React.PointerEvent): [number, number] => {
    const r = cv.current!.getBoundingClientRect();
    return [((e.clientX - r.left) / r.width) * W, ((e.clientY - r.top) / r.height) * H];
  };
  const down = (e: React.PointerEvent) => {
    (e.target as Element).setPointerCapture(e.pointerId);
    cur.current = { kind: tool_, pts: [pos(e)], color };
  };
  const move = (e: React.PointerEvent) => {
    if (!cur.current) return;
    if (cur.current.kind === "pen") cur.current.pts.push(pos(e));
    else cur.current.pts[1] = pos(e);
    paint([...strokes, cur.current]);
  };
  const up = () => {
    const s = cur.current;
    cur.current = null;
    if (!s) return;
    if (s.pts.length < 2 || (s.kind !== "pen" && Math.hypot(s.pts[1][0] - s.pts[0][0], s.pts[1][1] - s.pts[0][1]) < 6)) {
      // A click marks a spot with a ring.
      const [x, y] = s.pts[0];
      const r = Math.min(W, H) * 0.06;
      setStrokes([...strokes, { kind: "ring", pts: [[x - r, y - r], [x + r, y + r]], color }]);
    } else setStrokes([...strokes, s]);
  };
  const submit = () => {
    const pts = strokes.flatMap((s) => s.pts);
    const box = pts.length ? [Math.min(...pts.map((p) => p[0])) - lw, Math.min(...pts.map((p) => p[1])) - lw, Math.max(...pts.map((p) => p[0])) + lw, Math.max(...pts.map((p) => p[1])) + lw] : null;
    save({ t: range ? range[0] : t, t1: range ? range[1] : undefined, text, drawing: strokes.length ? cv.current!.toDataURL("image/png") : undefined, box: box || undefined });
  };
  const ui = Math.max(1, W / 900);
  return (
    <>
      <canvas ref={cv} width={W} height={H} onPointerDown={down} onPointerMove={move} onPointerUp={up} style={{ position: "absolute", inset: 0, width: W, height: H, cursor: "crosshair", touchAction: "none" }} />
      {host && createPortal(<div className="card" style={{ padding: 12, display: "flex", flexDirection: "column", gap: 8, width: "100%" }}>
        <div className="rowx">
          <div className="seg">
            {([["pen", Pen, tx("Pen")], ["ring", Circle, tx("Circle")], ["box", Square, tx("Box")]] as const).map(([k, I, l]) => (
              <button key={k} className={tool_ === k ? "on" : ""} onClick={() => setTool(k)} title={l}><I size={14} /></button>
            ))}
          </div>
          <button className="btn icon sm ghost" title={tx("Undo")} onClick={() => setStrokes(strokes.slice(0, -1))}><Undo2 /></button>
          <span className="grow muted num" style={{ fontSize: 12 }}>{range ? `${tc(range[0], fps)} – ${tc(range[1], fps)}` : tx("{0} · frame {1}", tc(t, fps), Math.round(t * fps))}</span>
          <button className="btn icon sm ghost" title={tx("Cancel (Esc)")} onClick={cancel}><X /></button>
        </div>
        <textarea autoFocus className="input" style={{ fontFamily: "var(--font)", fontSize: 13 }} rows={2} placeholder={tx("What should change")} value={text} onChange={(e) => setText(e.target.value)} onKeyDown={(e) => e.key === "Enter" && (e.ctrlKey || e.metaKey) && text && submit()} />
        <div className="rowx">
          <span className="grow muted" style={{ fontSize: 12 }}>{tx("Draw on the frame or click ·")} <span className="kbd">Ctrl</span>+<span className="kbd">Enter</span></span>
          <button className="btn sm primary" disabled={!text} onClick={submit}>{tx("Save note")}</button>
        </div>
      </div>, host)}
    </>
  );
}

function useNoteHost() {
  const [host, setHost] = useState<HTMLElement | null>(null);
  useEffect(() => setHost(document.getElementById("note-host")), []);
  return host;
}

function CheckLayer({ notes, seek, update, done }: Json) {
  const [i] = useState(0);
  const [busy, setBusy] = useState(false);
  // every hook runs on every render, also on the one where the last note was checked
  const host = useNoteHost();
  const n = notes[Math.min(i, notes.length - 1)];
  useEffect(() => {
    if (n) seek(n.t);
    else done();
  }, [n?.id]);
  if (!n) return null;
  const before = n.images?.annotated || n.images?.frame;
  const mark = async (p: Json) => {
    setBusy(true);
    try { await update(n, p); } finally { setBusy(false); }
  };
  return (
    <div style={{ position: "absolute", inset: 0, pointerEvents: "none" }}>
      {before && <img src={fileUrl(before)} alt="" style={{ position: "absolute", right: "3%", top: "3%", width: "34%", border: "2px solid #fff", borderRadius: 8, boxShadow: "0 8px 24px rgba(0,0,0,.4)" }} />}
      {host && createPortal(<div className="card" style={{ padding: 16, width: "100%" }}>
        <div className="rowx" style={{ marginBottom: 8 }}>
          <span className="muted num">{i + 1}/{notes.length}</span>
          <b className="grow" style={{ fontWeight: 500 }}>{n.text}</b>
        </div>
        {n.reply && <div className="sec" style={{ marginBottom: 12 }}>{tx("Claude:")} {n.reply}</div>}
        <div className="rowx">
          <span className="grow muted" style={{ fontSize: 12 }}>{tx("Before, top right; now, behind")}</span>
          <button className="btn sm" disabled={busy} onClick={() => mark({ status: "still", reply: "" })}>{tx("Still there")}</button>
          <button className="btn sm primary" disabled={busy} onClick={() => mark({ status: "done" })}><Check />{tx("Done")}</button>
        </div>
      </div>, host)}
    </div>
  );
}

function NotesList({ notes, fps, seek, update }: Json) {
  const [show, setShow] = useState<"open" | "all">("open");
  const list: Json[] = notes.filter((n: Json) => show === "all" || ["open", "claude", "still", "fixed"].includes(n.status)).slice().reverse();
  return (
    <div className="stack">
      <div className="seg" style={{ alignSelf: "flex-start" }}>
        <button className={show === "open" ? "on" : ""} onClick={() => setShow("open")}>{tx("Pending")}</button>
        <button className={show === "all" ? "on" : ""} onClick={() => setShow("all")}>{tx("All")}</button>
      </div>
      {!list.length && <p className="sec">{tx("No notes. Pause playback, press")} <span className="kbd">N</span> {tx("to draw on the frame and write what should change.")}</p>}
      {list.map((n) => (
        <div key={n.id} className="card" style={{ padding: 8, display: "grid", gridTemplateColumns: "72px 1fr", gap: 10, cursor: "pointer" }} onClick={() => seek(n)}>
          <div style={{ width: 72, height: 72, borderRadius: 6, overflow: "hidden", background: "var(--sunken)" }}>
            {(n.images?.crop || n.images?.frame) && <img src={fileUrl(n.images.crop || n.images.frame)} alt="" style={{ width: "100%", height: "100%", objectFit: "cover" }} />}
          </div>
          <div style={{ minWidth: 0, display: "flex", flexDirection: "column", gap: 4 }}>
            <div className="rowx">
              <span className="num muted" style={{ fontSize: 11 }}>{tc(n.t, fps)}{n.t1 ? `–${tc(n.t1, fps)}` : ""}</span>
              <span className="grow" />
              <span className="tag" style={n.status === "fixed" ? { color: "var(--good)" } : n.status === "still" ? { color: "var(--bad)" } : undefined}>{STATUS[n.status] || n.status}</span>
            </div>
            <div style={{ fontSize: 12, lineHeight: "18px" }}>{n.text}</div>
            {n.reply && <div className="sec" style={{ fontSize: 12, lineHeight: "16px" }}>{n.reply}</div>}
            <div className="rowx" onClick={(e) => e.stopPropagation()}>
              {n.status !== "done" && n.status !== "fixed" && <button className="btn icon sm ghost" title={tx("Done")} onClick={() => update(n, { status: "done" })}><Check /></button>}
              {(n.status === "done" || n.status === "fixed") && <button className="btn icon sm ghost" title={tx("Reopen")} onClick={() => update(n, { status: "open" })}><RefreshCw /></button>}
              <button className="btn icon sm ghost" title={tx("Delete")} onClick={() => update(n, { delete: true })}><Trash2 /></button>
            </div>
          </div>
        </div>
      ))}
    </div>
  );
}

function RenderMenu({ disabled, latest, fixed, run, lang }: Json) {
  const [open, setOpen] = useState(false);
  const [enc, setEnc] = useState("auto");
  const [scale, setScale] = useState(1);
  const go = (mode: string) => {
    setOpen(false);
    const label = mode === "preview" ? tx("Preview render") : mode === "partial" ? tx("Changed chunks") : tx("Full render");
    run(lang ? `${label} (${lang})` : label, "hub_render", { mode, encoder: enc, ...(mode === "full" && scale !== 1 ? { scale } : {}), ...(lang ? { lang } : {}) });
  };
  return (
    <div style={{ position: "relative" }}>
      <button className="btn primary" disabled={disabled} onClick={() => setOpen(!open)}>{tx("Render")}</button>
      {open && (
        <div className="card" style={{ position: "absolute", right: 0, top: 40, width: 300, padding: 8, zIndex: 20, boxShadow: "0 12px 32px rgba(0,0,0,.35)" }}>
          <button className="opt" onClick={() => go("full")}><span><b>{tx("Whole film")}</b><span>{tx("Every chunk is drawn again, the sound added, and saved as a new version")}</span></span></button>
          <button className="opt" disabled={!latest} style={!latest ? { opacity: 0.45 } : undefined} onClick={() => latest && go("partial")}><span><b>{tx("Only the fixes")}</b><span>{latest ? tx("The 2-second chunks touched by {0} fixed notes", fixed) : tx("Needs a full render first")}</span></span></button>
          <button className="opt" onClick={() => go("preview")}><span><b>{tx("Quick preview")}</b><span>{tx("Half size, for a look on the phone")}</span></span></button>
          {lang && <div className="sec" style={{ fontSize: 12, padding: "4px 8px" }}>{tx("Renders the language on screen: {0}", lang)}</div>}
          <div className="grid2" style={{ padding: 8 }}>
            <div className="field"><label>{tx("Encoder")}</label><select className="select" value={enc} onChange={(e) => setEnc(e.target.value)}><option value="auto">{tx("Automatic")}</option><option value="nvenc">NVIDIA (NVENC)</option><option value="amf">AMD (AMF)</option><option value="qsv">Intel (QSV)</option><option value="x264">{tx("CPU (x264)")}</option></select></div>
            <div className="field"><label>{tx("Size (whole film)")}</label><select className="select" value={scale} onChange={(e) => setScale(+e.target.value)}><option value={1}>%100</option><option value={0.75}>%75</option><option value={0.5}>%50</option></select></div>
          </div>
        </div>
      )}
    </div>
  );
}

function Versions({ versions, watch, setWatch }: Json) {
  const list: Json[] = versions.slice().reverse();
  return (
    <div className="stack" style={{ gap: 8 }}>
      <h2>{tx("Versions")}</h2>
      {!list.length && <p className="sec" style={{ fontSize: 12 }}>{tx("No renders yet. The film plays live; press Render when you are ready.")}</p>}
      <div className="list-rows">
        {list.map((v) => (
          <div key={v.v} className="rowx" style={{ padding: "8px 0" }}>
            <b className="num" style={{ fontWeight: 500, width: 36 }}>v{v.v}</b>
            <span className="grow sec" style={{ fontSize: 12 }}>{v.lang && <span className="kbd" style={{ marginRight: 6 }}>{v.lang}</span>}{v.kind === "preview" ? tx("Preview") : v.mode === "partial" ? tx("{0} chunks redrawn", (v.chunks || []).length) : v.mode === "outside" ? tx("External") : tx("Full")}</span>
            <button className={`btn icon sm ${watch?.v === v.v ? "" : "ghost"}`} title={tx("Watch")} onClick={() => setWatch(watch?.v === v.v ? null : v)}><Play /></button>
            {v.mode === "partial" && <button className="btn sm ghost" title={tx("Watch only the changes")} onClick={() => setWatch({ ...v, only: true })}>{tx("Changes")}</button>}
            <button className="btn icon sm ghost" title={tx("Show in folder")} onClick={() => tool("reveal", { path: v.file })}><FolderOpen /></button>
          </div>
        ))}
      </div>
    </div>
  );
}

function VersionPlayer({ v, onTime, close, note }: Json) {
  const ref = useRef<HTMLVideoElement>(null);
  const ranges: [number, number][] = v.only ? v.changedRanges || [] : [];
  const idx = useRef(0);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    if (ranges.length) el.currentTime = ranges[0][0];
    el.play().catch(() => {});
    const tick = () => {
      onTime(el.currentTime);
      if (ranges.length) {
        const r = ranges[idx.current];
        if (r && el.currentTime >= r[1]) {
          idx.current += 1;
          const n = ranges[idx.current];
          if (n) el.currentTime = n[0];
          else el.pause();
        }
      }
    };
    el.addEventListener("timeupdate", tick);
    return () => el.removeEventListener("timeupdate", tick);
  }, [v]);
  return (
    <>
      <video ref={ref} src={fileUrl(v.file)} controls style={{ width: "100%", height: "100%", background: "#000" }} />
      <button className="btn sm" style={{ position: "absolute", left: 24, top: 24, transform: "scale(2)", transformOrigin: "0 0" }} onClick={close}><MousePointer2 />{tx("Back to live")}</button>
      <button className="btn sm" style={{ position: "absolute", right: 24, top: 24, transform: "scale(2)", transformOrigin: "100% 0" }} onClick={() => { const el = ref.current; if (!el) return; el.pause(); const text = window.prompt(tx("What should change in this frame")); if (text) note(el.currentTime, text); }}><Pen />{tx("Note on this frame")}</button>
    </>
  );
}

function FilmTimeline({ dur: D, fps, t, seek, scenes, notes, range, setRange, changed, chunkSec, lanes }: Json) {
  const box = useRef<HTMLDivElement>(null);
  const [w, setW] = useState(800);
  useEffect(() => {
    const ro = new ResizeObserver((e) => setW(e[0].contentRect.width));
    if (box.current) ro.observe(box.current);
    return () => ro.disconnect();
  }, []);
  const pad = 16;
  const sx = (w - pad * 2) / Math.max(D, 0.1);
  const X = (x: number) => pad + x * sx;
  const at = (e: { clientX: number }) => {
    const r = box.current!.getBoundingClientRect();
    return Math.max(0, Math.min(D, (e.clientX - r.left - pad) / sx));
  };
  const down = (e: React.MouseEvent) => {
    const x0 = at(e);
    if (e.shiftKey) setRange([x0, x0]);
    else {
      setRange(null);
      seek(x0);
    }
    const move = (ev: MouseEvent) => (e.shiftKey ? setRange([Math.min(x0, at(ev)), Math.max(x0, at(ev))]) : seek(at(ev)));
    const up = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  };
  const steps = [0.5, 1, 2, 5, 10, 15, 30, 60, 120, 300];
  const step = steps.find((s) => s * sx >= 64) || 300;
  const ticks = useMemo(() => Array.from({ length: Math.floor(D / step) + 1 }, (_, i) => i * step), [D, step]);
  return (
    <div className="tl" ref={box} onMouseDown={down} style={{ height: 100 + (lanes?.length || 0) * LANE_H + 4 }}>
      {(lanes || []).map((ln: Lane, k: number) => <TrackLane key={ln.key} lane={ln} top={100 + k * LANE_H} sx={sx} pad={pad} total={D} />)}
      {ticks.map((x) => (
        <span key={x} className="num" style={{ position: "absolute", left: X(x), top: 0, fontSize: 10, color: "var(--text-3)", borderLeft: "1px solid var(--line-2)", paddingLeft: 4, height: 14, lineHeight: "12px" }}>{Math.floor(x / 60)}:{String(Math.floor(x % 60)).padStart(2, "0")}</span>
      ))}
      {Array.from({ length: Math.ceil(D / chunkSec) }, (_, i) => (
        <span key={`c${i}`} style={{ position: "absolute", left: X(i * chunkSec), top: 20, height: 4, width: Math.max(1, chunkSec * sx - 1), background: changed.some((r: number[]) => r[0] <= i * chunkSec + 1e-3 && r[1] >= (i + 1) * chunkSec - 1e-3 - 1 / fps) ? "var(--accent)" : "var(--line)" }} title={tx("Chunk {0}", i)} />
      ))}
      {scenes.map((s: Json, i: number) => (
        <span key={i} style={{ position: "absolute", left: X(s.t0), width: Math.max(2, (s.t1 - s.t0) * sx - 2), top: 30, height: 32, borderRadius: 6, background: "var(--take)", border: "1px solid var(--line)", fontSize: 11, lineHeight: "30px", padding: "0 8px", overflow: "hidden", whiteSpace: "nowrap", textOverflow: "ellipsis", color: "var(--text-2)" }}>{s.name}</span>
      ))}
      {range && <span style={{ position: "absolute", left: X(range[0]), width: Math.max(2, (range[1] - range[0]) * sx), top: 26, bottom: 4, background: "color-mix(in srgb, var(--accent) 18%, transparent)", border: "1px solid var(--accent)", borderRadius: 4, pointerEvents: "none" }} />}
      {notes.filter((n: Json) => n.status !== "done" && n.status !== "wontfix").map((n: Json) => (
        <span key={n.id} className="marker note" style={{ left: X(n.t), top: 72, borderColor: n.status === "fixed" ? "var(--good)" : undefined }} title={n.text} onMouseDown={(e) => { e.stopPropagation(); seek(n.t); }}>{n.text}</span>
      ))}
      <div className="playhead" style={{ left: X(t) }} />
      <span className="muted" style={{ position: "absolute", right: 16, bottom: 6, fontSize: 11 }}>{tx("Shift + drag: select a range")}</span>
    </div>
  );
}

function Cleanup({ film, say }: { film: string; say: (s: string) => void }) {
  const r = useLoad(() => tool("hub_cleanup", { film }), [film]);
  const items: Json[] = r.data?.items || [];
  if (!items.length) return null;
  return (
    <div className="stack" style={{ gap: 8 }}>
      <h2>{tx("Cleanup")}</h2>
      {items.map((x) => (
        <div key={x.path} className="rowx" style={{ fontSize: 12 }}>
          <span className="grow sec" title={x.path}>{x.why}</span>
          <span className="num muted">{(x.bytes / 1e6).toFixed(0)} MB</span>
          <button className="btn sm ghost" onClick={() => tool("trash", { path: x.path }).then(() => { say(tx("Moved to the Recycle Bin")); r.reload(); }).catch((e) => say(e.message))}>{tx("To Recycle Bin")}</button>
        </div>
      ))}
    </div>
  );
}
