// Films written as code: live preview from the page itself, notes drawn on frames, the check tour, manual renders.
import { lang, tx } from "./i18n";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Redo2, SendHorizontal } from "lucide-react";
import { SkipBack, SkipForward } from "lucide-react";
import { ArrowLeft, Check, ChevronLeft, ChevronRight, Circle, Copy, FolderOpen, Image as ImageIcon, MousePointer2, Pause, Pen, Play, Plus, RefreshCw, Repeat, Square, Trash2, Undo2, X } from "lucide-react";
import { Activity, dur, fetchJson, fileUrl, Json, runJob, tc, tool, useLoad } from "./api";
import { SoundPanel } from "./Studio";
import { evalItem, LayerOverlay, useLayerKeys, useLayers } from "./Layers";
import { Scopes } from "./Scopes";
import { AddPanel, Command, LayerStack, Menu, MenuButton, MenuItem, Palette, PuppetGrid } from "./LayerPanels";
import { SceneGrid } from "./Scenes";
import { LayerInspector, MultiInspector } from "./Inspector";
import { copyItems, copyStyle, duplicate, group, hasClip, hideThenDeleteEls, hideThenRemove, keyTimes, layerName, moveKeyTime, pasteItems, pasteStyle, r2 as rr2, remove, reorder, replace, split, trim, ungroup, withInside } from "./LayerOps";
import { DROP } from "./Layers";
import { Command as CmdIcon, Grid3x3, Magnet, Scissors, ZoomIn, ZoomOut, MousePointerClick } from "lucide-react";
import { Eye, EyeOff, Bot, ChevronDown, ChevronUp, History, Languages as LanguagesIcon, Layers as LayersIcon, PanelLeftClose, PanelLeftOpen, PanelRightClose, PanelRightOpen, SlidersHorizontal, StickyNote, Volume2 } from "lucide-react";
import { createPortal } from "react-dom";
import { Lane, LANE_H, laneRows, TrackLane } from "./Tracks";
import { PuppetFields, puppetFromImage } from "./Puppets";
import { Box, ElementOverlay, ElementPanel, Reg, scanRegistry, useElements } from "./Elements";
import { arrange, CardActions, deleteItem, ListTools, renameItem, useStars } from "./Manage";
import { Pencil } from "lucide-react";
import { Keyboard, LayoutGrid } from "lucide-react";
import { LangsPanel, ViewLang } from "./Langs";
import { Shortcuts, Storyboard } from "./Storyboard";

/** The elements found in each film's code (film id @ revision), so opening Layers again needs no new scan. */
const REGISTRY = new Map<string, Reg[]>();
/** What each film's code paints outside its elements (film id @ revision), found with the same scan. */
const LOOSE = new Map<string, Json[]>();

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
          <button className="btn ghost" onClick={() => make({ title: "Hello, Linnea", example: "hello-film" })}>{tx("Example film")}</button>
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
  // markers: the person's own places on the timeline (M), kept in tweaks.json for the agent to read too
  const [markers, setMarkersS] = useState<Json[]>([]);
  const markersRef = useRef<Json[]>([]);
  markersRef.current = markers;
  useEffect(() => { tool("hub_2d", { film: id }).then((r) => setMarkersS(r.markers || [])).catch(() => {}); }, [id]);
  const setMarkers = (m: Json[]) => { const s2 = [...m].sort((a, b) => a.t - b.t); setMarkersS(s2); tool("hub_2d", { film: id, markers: s2 }).catch(() => {}); };
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
  const [side, setSide] = useState<"props" | "sound" | "langs" | "ai" | "versions">("props");
  const showSide = (k: "props" | "sound" | "langs" | "ai" | "versions") => { setSide(k); setPane((p) => (p.r ? p : { ...p, r: true })); };
  const [left, setLeft] = useState<"layers" | "add" | "notes">(() => { try { return (localStorage.getItem("linnea.left") as any) || "layers"; } catch { return "layers"; } });
  useEffect(() => { try { localStorage.setItem("linnea.left", left); } catch { /* private window */ } }, [left]);
  // the side panels and the timeline open and close; a closed side panel leaves a strip of its tabs' icons
  const [pane, setPane] = useState<{ l: boolean; r: boolean; tl: boolean }>(() => { try { return { l: true, r: true, tl: true, ...JSON.parse(localStorage.getItem("linnea.panes") || "{}") }; } catch { return { l: true, r: true, tl: true }; } });
  useEffect(() => { try { localStorage.setItem("linnea.panes", JSON.stringify(pane)); } catch { /* private window */ } }, [pane]);
  const flip = (k: "l" | "r" | "tl") => setPane((p) => ({ ...p, [k]: !p[k] }));
  const showLeft = (k: "layers" | "add" | "notes") => { setLeft(k); setPane((p) => (p.l ? p : { ...p, l: true })); };
  const [multi, setMulti] = useState<string[]>([]);
  const [zoom, setZoom] = useState(0);
  const [viewScale, setViewScale] = useState(0.3);
  const [menu, setMenu] = useState<{ x: number; y: number; id: string } | null>(null);
  const [palette, setPalette] = useState(false);
  const [guides, setGuides] = useState<"" | "safe" | "thirds">(() => { try { return (localStorage.getItem("linnea.guides") as any) || ""; } catch { return ""; } });
  useEffect(() => { try { localStorage.setItem("linnea.guides", guides); } catch { /* private window */ } }, [guides]);
  const [snapOn, setSnapOn] = useState(() => { try { return localStorage.getItem("linnea.snap") !== "0"; } catch { return true; } });
  useEffect(() => { try { localStorage.setItem("linnea.snap", snapOn ? "1" : "0"); } catch { /* private window */ } }, [snapOn]);
  // the language shown on the frame (null: the film's own texts), and the extra views
  const [viewLang, setViewLang] = useState<ViewLang>(null);
  const [scopes, setScopes] = useState(() => { try { return localStorage.getItem("linnea.scopes") === "1"; } catch { return false; } });
  useEffect(() => { try { localStorage.setItem("linnea.scopes", scopes ? "1" : "0"); } catch { /* private window */ } }, [scopes]);
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
  const kb = useRef<{ selEl: string; E: typeof E }>({ selEl: "", E });
  const vsRef = useRef(0.3);
  vsRef.current = viewScale;
  kb.current = { selEl, E };
  // one way to choose layers everywhere: the frame, the stack, the timeline
  const select = (ids: string[], primary?: string) => {
    setMulti(ids);
    setSel2d(primary ?? ids[ids.length - 1] ?? "");
    if (ids.length) setSelEl("");
    setPathFor("");
    if (ids.length) setSide("props");
  };
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
  const editOn = mode === "live" && !playing && !watch;
  useEffect(() => {
    if (editOn) refreshBoxes();
  }, [t, editOn, ready]);
  useLayerKeys({ L, t, sel: sel2d, setSel: setSel2d, multi, setMulti, active: editOn && !palette && !menu && mode === "live", say });
  // The list of all elements comes from drawing the whole film once. That runs on a hidden copy of the page,
  // so the preview on screen never jumps through the film, and it is kept for the film's current code.
  const [scanning, setScanning] = useState(false);
  // what the film paints outside every element, at nine seconds (from the same hidden page as the scan)
  const [loose, setLoose] = useState<{ t: number; loose: number[][]; share: number }[]>([]);
  const scanFrame = useRef<HTMLIFrameElement>(null);
  const scanKey = `${id}@${rev}`;
  useEffect(() => {
    if ((left !== "layers" && side !== "langs" && side !== "props") || !ready || !info) return;
    const w = frame.current?.contentWindow as any;
    if (!w?.KE) return;
    const known = REGISTRY.get(scanKey);
    if (known) { setReg(known); setLoose(LOOSE.get(scanKey) || []); }
    else setScanning(true);
  }, [side, left, ready, rev]);
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
        const d = info?.dur || 10;
        const times = Array.from({ length: 9 }, (_, k) => Math.round(((d * (k + 0.5)) / 9) * 100) / 100);
        Promise.resolve(w.KE_AUDIT ? w.KE_AUDIT(times) : null).catch(() => null).then((a: Json) => {
          const found = ((a?.frames || []) as Json[]).filter((f) => f.loose?.length);
          LOOSE.set(key, found);
          if (alive()) setLoose(found);
          setScanning(false);
        });
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
    // video layers play along while the preview plays, and seek to the exact frame when it stops
    const pw = win();
    if (pw) pw.VH2D_PLAYING = true;
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
      if (pw) { pw.VH2D_PLAYING = false; draw(tRef.current); }
    };
    // eslint-disable-next-line
  }, [playing, loop, range, info?.audio, DUR]);
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (/INPUT|TEXTAREA|SELECT/.test((e.target as HTMLElement).tagName)) return;
      if ((e.ctrlKey || e.metaKey) && (e.key === "k" || e.key === "K" || (e.shiftKey && (e.key === "p" || e.key === "P")))) {
        e.preventDefault();
        setPalette((p) => !p);
        return;
      }
      const ke = kb.current;
      // a code element selected on the frame: arrows move it, Delete hides it, Esc lets go
      if (ke.selEl && !e.ctrlKey && !e.metaKey && !e.altKey) {
        const ed = ke.E.edits[ke.selEl] || {};
        if (e.key.startsWith("Arrow")) {
          e.preventDefault();
          const d = e.shiftKey ? 10 : 1;
          ke.E.set(ke.selEl, { dx: (ed.dx || 0) + (e.key === "ArrowLeft" ? -d : e.key === "ArrowRight" ? d : 0), dy: (ed.dy || 0) + (e.key === "ArrowUp" ? -d : e.key === "ArrowDown" ? d : 0) });
          return;
        }
        if (e.key === "Delete" || e.key === "Backspace") {
          // the first press hides it, the second deletes it (out of the picture and the lists, the code untouched)
          e.preventDefault();
          const r = hideThenDeleteEls(ke.E.edits, [ke.selEl]);
          ke.E.update(r.edits);
          if (r.deleted) setSelEl(""); else say(tx("Hidden: press Delete again to delete"));
          return;
        }
        if (e.key === "Escape") { e.preventDefault(); setSelEl(""); return; }
      }
      if ((e.ctrlKey || e.metaKey) && (e.key === "\\" || e.code === "Backslash")) {
        // Ctrl+\ : both side panels away (or back); with Shift the timeline
        e.preventDefault();
        if (e.shiftKey) flip("tl"); else setPane((p) => { const open = !(p.l || p.r); return { ...p, l: open, r: open }; });
        return;
      }
      if ((e.ctrlKey || e.metaKey) && ["0", "1", "2", "=", "+", "-"].includes(e.key)) {
        e.preventDefault();
        setZoom((z) => (e.key === "0" ? 0 : e.key === "1" ? 1 : e.key === "2" ? 2 : Math.max(0.05, Math.min(8, (z || vsRef.current) * (e.key === "-" ? 1 / 1.25 : 1.25)))));
        return;
      }
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
      else if (e.key === "u" || e.key === "U") setScopes((v) => !v);
      else if (e.key === "M" || (e.key === "m" && e.shiftKey)) {
        // the next marker (back to the first after the last)
        const ms = markersRef.current;
        const nx = ms.find((m) => m.t > tRef.current + 0.01) || ms[0];
        if (nx) seek(nx.t);
      } else if (e.key === "m") {
        const tt = Math.round(tRef.current * 1000) / 1000;
        if (!markersRef.current.some((m) => Math.abs(m.t - tt) < 0.01)) setMarkers([...markersRef.current, { id: "m" + Date.now().toString(36), t: tt, color: "blue" }]);
      }
      else if (/^[1-5]$/.test(e.key)) showSide((["props", "sound", "langs", "ai", "versions"] as const)[+e.key - 1]);
      else if (e.key === "a" || e.key === "A") { showLeft("add"); setPlaying(false); }
      else if (e.key === "g" || e.key === "G") setGuides((g) => (g === "" ? "safe" : g === "safe" ? "thirds" : ""));
      else if (e.key === "k" || e.key === "K") setPlaying((p) => !p);
      else if (e.key === "j" || e.key === "J") seek(tRef.current - 1);
      else if (e.key === "l" || e.key === "L") seek(tRef.current + 1);
      else if (e.key === "i" || e.key === "I") setRange((r) => [tRef.current, Math.max(tRef.current + 1 / FPS, r ? r[1] : DUR)]);
      else if (e.key === "o" || e.key === "O") setRange((r) => [Math.min(r ? r[0] : 0, tRef.current - 1 / FPS), tRef.current]);
      else if (e.key === "n" || e.key === "N") {
        setPlaying(false);
        setMode("note");
      } else if (e.key === "Escape") { setMode("live"); setRange(null); }
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
  // ---- a video layer's sound: a clip in the mix that follows the layer (place, trim, speed, hidden) ----
  const linkTimer = useRef<any>(0);
  useEffect(() => {
    if (!L.loaded) return;
    clearTimeout(linkTimer.current);
    linkTimer.current = setTimeout(() => {
      const cur = clips.filter((c) => c.link);
      const want = L.items.filter((v) => v.kind === "video" && v.sound !== false && v.src).map((v) => {
        const old = cur.find((c) => c.link === v.id);
        return { id: old?.id || "v" + v.id, link: v.id, file: v.src, name: layerName(v), at: v.t0, from: v.in || 0, len: rr2((v.t1 - v.t0) * (v.speed || 1)), speed: v.speed && v.speed !== 1 ? v.speed : undefined, gain: v.gain ?? old?.gain ?? 0, mute: v.hidden ? true : old?.mute || undefined };
      });
      const norm = (c: Json) => JSON.stringify([c.link, c.file, c.at, c.from || 0, c.len, c.speed || 1, c.gain || 0, !!c.mute, c.name]);
      if (JSON.stringify(cur.map(norm).sort()) === JSON.stringify(want.map(norm).sort())) return;
      saveClips([...clips.filter((c) => !c.link), ...want]);
    }, 900);
  }, [L.items, clips, L.loaded]);
  const addSound = (path: string, at: number) => tool("hub_sound_clip", { film: id, file: path, at: rr2(at), apply: true }).then(() => { load(); setAudioV(Date.now()); say(tx("Sound added at {0} s", rr2(at))); }).catch((e) => say(e.message));
  useEffect(() => { DROP.sound = (src: string, at: number) => addSound(src, at); return () => { DROP.sound = null; }; });

  // ---- what can be done, in one place: menus, the right-click menu, the command palette ----
  const items2d = L.items;
  const chosen = multi.length ? multi : sel2d ? [sel2d] : [];
  const one = items2d.find((x) => x.id === sel2d);
  const layerOps = (ids: string[]): MenuItem[] => {
    const picked = items2d.filter((x) => ids.includes(x.id));
    const n = picked.length;
    if (!n) return [{ label: tx("Paste"), keys: "Ctrl+V", disabled: !hasClip(), run: () => { const got = pasteItems(tRef.current); if (got.length) { L.update([...items2d, ...got]); select(got.map((x) => x.id)); } } }];
    const solo = n === 1 ? picked[0] : null;
    return [
      { label: tx("Duplicate"), keys: "Ctrl+D", run: () => { const r = duplicate(items2d, ids); L.update(r.items); select(r.ids); } },
      { label: tx("Copy"), keys: "Ctrl+C", run: () => { copyItems(picked, items2d); say(tx("{0} layers copied", n)); } },
      { label: tx("Paste"), keys: "Ctrl+V", disabled: !hasClip(), run: () => { const got = pasteItems(tRef.current); if (got.length) { L.update([...items2d, ...got]); select(got.map((x) => x.id)); } } },
      "-",
      { label: tx("Split at the playhead"), keys: "S", disabled: !solo || tRef.current <= solo.t0 || tRef.current >= solo.t1, run: () => { const r = split(items2d, solo!.id, tRef.current); if (r.id) { L.update(r.items); select([r.id]); } } },
      { label: tx("Start at the playhead"), keys: "Alt+[", run: () => L.update(trim(items2d, ids, tRef.current, "start")) },
      { label: tx("End at the playhead"), keys: "Alt+]", run: () => L.update(trim(items2d, ids, tRef.current, "end")) },
      { label: tx("Move to the playhead"), run: () => L.update(items2d.map((x) => { if (!ids.includes(x.id)) return x; const d = tRef.current - x.t0; return { ...x, t0: rr2(x.t0 + d), t1: rr2(Math.min(DUR, x.t1 + d)), keys: x.keys?.map((k) => ({ ...k, t: rr2(k.t + d) })), pkeys: x.pkeys?.map((k: Json) => ({ ...k, t: rr2(k.t + d) })) }; })) },
      "-",
      { label: tx("Group"), keys: "Ctrl+G", run: () => { const r = group(items2d, ids, tRef.current); if (r.id) { L.update(r.items); select([r.id]); } } },
      ...(picked.some((x) => x.kind === "group") ? [{ label: tx("Ungroup"), keys: "Ctrl+Shift+G", run: () => { let cur = items2d; const out: string[] = []; for (const g of picked.filter((x) => x.kind === "group")) { const r = ungroup(cur, g.id); cur = r.items; out.push(...r.ids); } L.update(cur); select(out); } }] as MenuItem[] : []),
      "-",
      { label: tx("Bring to front"), keys: "Ctrl+Shift+]", run: () => L.update(reorder(items2d, ids, "top")) },
      { label: tx("Bring forward"), keys: "Ctrl+]", run: () => L.update(reorder(items2d, ids, "up")) },
      { label: tx("Send backward"), keys: "Ctrl+[", run: () => L.update(reorder(items2d, ids, "down")) },
      { label: tx("Send to back"), keys: "Ctrl+Shift+[", run: () => L.update(reorder(items2d, ids, "bottom")) },
      "-",
      { label: picked.every((x) => x.hidden) ? tx("Show") : tx("Hide"), keys: "H", run: () => L.update(replace(items2d, ids, (x) => ({ ...x, hidden: !picked.every((y) => y.hidden) || undefined }))) },
      { label: picked.every((x) => x.locked) ? tx("Unlock") : tx("Lock"), keys: "Ctrl+L", run: () => L.update(replace(items2d, ids, (x) => ({ ...x, locked: !picked.every((y) => y.locked) || undefined }))) },
      ...(solo ? [{ label: tx("Copy the look"), run: () => { copyStyle(solo); say(tx("Look copied")); } }, { label: tx("Paste the look"), run: () => L.update(replace(items2d, ids, (x) => pasteStyle(x))) }] as MenuItem[] : []),
      "-",
      { label: n > 1 ? tx("Delete {0} layers", n) : tx("Delete"), keys: "Del", danger: true, run: () => { L.update(remove(items2d, ids)); select([]); } },
    ];
  };
  const go = (k: typeof side) => showSide(k);
  const commands: Command[] = [
    { id: "play", group: tx("Play"), label: tx("Play / pause"), keys: "Space", run: () => setPlaying((p) => !p) },
    { id: "start", group: tx("Play"), label: tx("Go to the start"), keys: "Home", run: () => seek(0) },
    { id: "end", group: tx("Play"), label: tx("Go to the end"), keys: "End", run: () => seek(DUR) },
    { id: "loop", group: tx("Play"), label: loop ? tx("Stop looping") : tx("Loop"), run: () => setLoop(!loop) },
    { id: "in", group: tx("Play"), label: tx("Range starts here"), keys: "I", run: () => setRange((r) => [tRef.current, Math.max(tRef.current + 1 / FPS, r ? r[1] : DUR)]) },
    { id: "out", group: tx("Play"), label: tx("Range ends here"), keys: "O", run: () => setRange((r) => [Math.min(r ? r[0] : 0, tRef.current - 1 / FPS), tRef.current]) },
    { id: "undo", group: lang === "tr" ? "Düzen" : "Edit", label: tx("Undo"), keys: "Ctrl+Z", run: undo, when: () => hist.current.past.length > 0 },
    { id: "redo", group: lang === "tr" ? "Düzen" : "Edit", label: tx("Redo"), keys: "Ctrl+Y", run: redo, when: () => hist.current.future.length > 0 },
    { id: "selall", group: lang === "tr" ? "Düzen" : "Edit", label: tx("Select every layer on screen"), keys: "Ctrl+A", run: () => select(items2d.filter((x) => tRef.current >= x.t0 && tRef.current < x.t1 && !x.locked).map((x) => x.id)) },
    ...layerOps(chosen).filter((m): m is Exclude<MenuItem, "-"> => m !== "-" && !m.disabled).map((m, i) => ({ id: "op" + i, group: tx("Layer"), label: m.label, keys: m.keys, run: m.run, when: () => chosen.length > 0 || m.label === tx("Paste") })),
    ...["text", "subtitle", "caption", "typing", "lower", "counter", "marker", "rect", "ellipse", "arrow", "progress", "gradient", "scrim", "image", "phone"].map((k) => ({ id: "add" + k, group: tx("Add"), label: tx("Add: {0}", ({ text: tx("Title"), subtitle: tx("Subtitle"), caption: tx("Caption box"), typing: tx("Typewriter"), lower: tx("Lower third"), counter: tx("Counter"), marker: tx("Handwriting"), rect: tx("Box"), ellipse: tx("Circle"), arrow: tx("Arrow"), progress: tx("Progress bar"), gradient: tx("Gradient background"), scrim: tx("Shade for text"), image: tx("Picture…"), phone: tx("Phone") } as Json)[k]), run: () => { showLeft("add"); setTimeout(() => (document.querySelector(`[data-add="${k}"]`) as HTMLElement | null)?.click(), 30); } })),
    { id: "note", group: tx("Review"), label: tx("Leave a note on this frame"), keys: "N", run: () => { setPlaying(false); setMode("note"); } },
    { id: "frame", group: tx("Review"), label: range ? tx("Range to Claude") : tx("Frame to Claude"), run: sendFrame },
    { id: "guides", group: tx("View"), label: tx("Safe areas / thirds / none"), keys: "G", run: () => setGuides((g) => (g === "" ? "safe" : g === "safe" ? "thirds" : "")) },
    { id: "board", group: tx("View"), label: tx("Storyboard"), keys: "B", run: () => setBoard(!board) },
    { id: "scopes", group: tx("View"), label: tx("Scopes"), keys: "U", run: () => setScopes(!scopes) },
    { id: "snap", group: tx("View"), label: snapOn ? tx("Turn snapping off") : tx("Turn snapping on"), run: () => setSnapOn(!snapOn) },
    { id: "keys", group: tx("View"), label: tx("Keyboard shortcuts"), keys: "?", run: () => setKeysOpen(true) },
    { id: "tl", group: tx("View"), label: tx("Panel: layers"), run: () => showLeft("layers") },
    { id: "ta", group: tx("View"), label: tx("Panel: add"), keys: "A", run: () => showLeft("add") },
    { id: "tn", group: tx("View"), label: tx("Panel: notes"), run: () => showLeft("notes") },
    { id: "tp", group: tx("View"), label: tx("Panel: properties"), keys: "1", run: () => go("props") },
    { id: "ts", group: tx("View"), label: tx("Panel: sound"), keys: "2", run: () => go("sound") },
    { id: "tg", group: tx("View"), label: tx("Panel: languages"), keys: "3", run: () => go("langs") },
    { id: "tai", group: tx("View"), label: tx("Panel: assistant"), keys: "4", run: () => go("ai") },
    { id: "tv", group: tx("View"), label: tx("Panel: versions"), keys: "5", run: () => go("versions") },
    { id: "pl", group: tx("View"), label: pane.l ? tx("Hide the left panel") : tx("Show the left panel"), run: () => flip("l") },
    { id: "pr", group: tx("View"), label: pane.r ? tx("Hide the right panel") : tx("Show the right panel"), run: () => flip("r") },
    { id: "pb", group: tx("View"), label: tx("Hide / show both side panels"), keys: "Ctrl+\\", run: () => setPane((p) => { const open = !(p.l || p.r); return { ...p, l: open, r: open }; }) },
    { id: "pt", group: tx("View"), label: pane.tl ? tx("Hide the timeline") : tx("Show the timeline"), keys: "Ctrl+Shift+\\", run: () => flip("tl") },
    { id: "folder", group: tx("Film"), label: tx("Open the film folder"), run: () => tool("reveal", { path: info?.dir }) },
    { id: "lip", group: tx("Film"), label: tx("Make the mouth from the film's sound"), run: lip },
    { id: "claude", group: tx("Film"), label: tx("Introduce the film to Claude"), run: () => copy(tx("claude: open the film \"{0}\" ({1}) in Linnea: look at its state with hub_project and hub_live.", info?.title, id), tx("Copied")) },
  ];
  const editMenu = (): MenuItem[] => [
    { label: tx("Undo"), keys: "Ctrl+Z", disabled: !hist.current.past.length, run: undo },
    { label: tx("Redo"), keys: "Ctrl+Y", disabled: !hist.current.future.length, run: redo },
    "-",
    { label: tx("Select every layer on screen"), keys: "Ctrl+A", run: () => select(items2d.filter((x) => tRef.current >= x.t0 && tRef.current < x.t1 && !x.locked).map((x) => x.id)) },
    { label: tx("Deselect"), keys: "Esc", disabled: !chosen.length && !selEl, run: () => { select([]); setSelEl(""); } },
    "-",
    ...layerOps(chosen),
  ];
  const viewMenu = (): MenuItem[] => [
    { label: (guides === "safe" ? "✓ " : "") + tx("Safe areas"), keys: "G", run: () => setGuides(guides === "safe" ? "" : "safe") },
    { label: (guides === "thirds" ? "✓ " : "") + tx("Rule of thirds"), run: () => setGuides(guides === "thirds" ? "" : "thirds") },
    { label: (snapOn ? "✓ " : "") + tx("Snapping"), run: () => setSnapOn(!snapOn) },
    { label: (board ? "✓ " : "") + tx("Storyboard"), keys: "B", run: () => setBoard(!board) },
    { label: (scopes ? "✓ " : "") + tx("Scopes"), keys: "U", run: () => setScopes(!scopes) },
    "-",
    { label: tx("Add a marker"), keys: "M", run: () => { const tt = Math.round(tRef.current * 1000) / 1000; if (!markers.some((m) => Math.abs(m.t - tt) < 0.01)) setMarkers([...markers, { id: "m" + Date.now().toString(36), t: tt, color: "blue" }]); } },
    { label: tx("Next marker"), keys: "Shift+M", disabled: !markers.length, run: () => { const nx = markers.find((m) => m.t > tRef.current + 0.01) || markers[0]; if (nx) seek(nx.t); } },
    { label: tx("Remove all markers"), disabled: !markers.length, run: () => { if (window.confirm(tx("Remove all {0} markers?", markers.length))) setMarkers([]); } },
    { label: (loop ? "✓ " : "") + tx("Loop"), run: () => setLoop(!loop) },
    "-",
    { label: (pane.l ? "✓ " : "") + tx("Left panel"), run: () => flip("l") },
    { label: (pane.r ? "✓ " : "") + tx("Right panel"), run: () => flip("r") },
    { label: (pane.tl ? "✓ " : "") + tx("Timeline"), keys: "Ctrl+Shift+\\", run: () => flip("tl") },
    "-",
    { label: tx("All commands…"), keys: "Ctrl+K", run: () => setPalette(true) },
    { label: tx("Keyboard shortcuts"), keys: "?", run: () => setKeysOpen(true) },
  ];

  // ---- the timeline: the film's code elements, one lane per layer (top first), sounds ----
  const laneOf = (it: Json): Lane => {
    const marks = [
      ...keyTimes(it).map((k) => ({ t: k, kind: "key" as const })),
      ...(it.kind === "puppet" ? (it.states || []).map((s: Json) => ({ t: rr2(it.t0 + s.t0), kind: "state" as const, label: s.state })) : []),
    ];
    return {
      key: `l:${it.id}`, group: "layers", groupLabel: tx("Layers"), label: layerName(it), color: it.kind === "puppet" ? "#b79cf2" : it.kind === "image" ? "#d9a66f" : "#86b991", ownKeys: false,
      sel: sel2d ? `l:${sel2d}` : "", selSet: chosen.map((x) => `l:${x}`),
      items: [{ id: `l:${it.id}`, s: it.t0, e: it.t1, label: (it.hidden ? tx("(hidden) ") : "") + layerName(it), locked: !!it.locked, dim: !!it.hidden, marks }],
      onSelect: (lid, e) => {
        const x = lid.slice(2);
        if (e && (e.shiftKey || e.ctrlKey || e.metaKey)) select(chosen.includes(x) ? chosen.filter((y) => y !== x) : [...chosen, x], x);
        else if (!chosen.includes(x) || chosen.length < 2) select([x], x);
        else setSel2d(x);
      },
      onChange: (lid, s, e) => {
        const x = lid.slice(2);
        const it0 = L.items.find((q) => q.id === x);
        if (!it0) return;
        // moving a bar takes its keys along; trimming keeps them where they are
        const d = Math.abs(it0.t1 - it0.t0 - (e - s)) < 1e-3 ? s - it0.t0 : 0;
        // a group moved in time takes what is in it along
        const inner = it0.kind === "group" && d ? withInside(L.items, [x]).filter((y) => y !== x) : [];
        L.update(L.items.map((q) => (inner.includes(q.id) ? { ...q, t0: rr2(q.t0 + d), t1: rr2(q.t1 + d), keys: q.keys?.map((k) => ({ ...k, t: rr2(k.t + d) })), pkeys: q.pkeys?.map((k: Json) => ({ ...k, t: rr2(k.t + d) })) } : q.id === x ? { ...q, t0: s, t1: e, keys: d ? q.keys?.map((k) => ({ ...k, t: rr2(k.t + d) })) : q.keys, pkeys: d ? q.pkeys?.map((k: Json) => ({ ...k, t: rr2(k.t + d) })) : q.pkeys } : q)));
      },
      onDelete: (lid) => { const r = hideThenRemove(L.items, [lid.slice(2)]); L.update(r.items); if (r.removed) select([]); },
      onMark: (lid, from, to) => {
        const x = lid.slice(2);
        L.update(L.items.map((q) => {
          if (q.id !== x) return q;
          const si = (q.states || []).findIndex((s: Json) => Math.abs(q.t0 + s.t0 - from) < 0.011);
          if (q.kind === "puppet" && si >= 0 && !keyTimes(q).some((k) => Math.abs(k - from) < 0.011)) return { ...q, states: q.states.map((s: Json, i: number) => (i === si ? { ...s, t0: rr2(Math.max(0, to - q.t0)) } : s)) };
          return moveKeyTime(q, from, to);
        }));
      },
      onSeek: (x) => { setPlaying(false); seek(x); },
      onMenu: (lid, x, y) => setMenu({ x, y, id: lid.slice(2) }),
    } as Lane;
  };
  const lanes: Lane[] = [
    {
      key: "elements", group: "elements", groupLabel: tx("From the film's code"), label: tx("Code elements"), color: "#7fb0ff", sel: selEl ? `e:${selEl}` : "", stack: true, ownKeys: false,
      items: reg.filter((r) => !E.edits[r.id]?.deleted).map((r) => ({ id: `e:${r.id}`, s: E.edits[r.id]?.t0 ?? r.t0, e: E.edits[r.id]?.t1 ?? r.t1, label: (E.edits[r.id]?.hidden ? tx("(hidden) ") : "") + (E.edits[r.id]?.text ?? r.label), dim: !!E.edits[r.id]?.hidden })),
      onSelect: (eid) => { setSelEl(eid.slice(2)); setSel2d(""); setMulti([]); setSide("props"); },
      onChange: (eid, s, e) => E.set(eid.slice(2), { t0: s, t1: e }),
      onDelete: (eid) => { const r = hideThenDeleteEls(E.edits, [eid.slice(2)]); E.update(r.edits); if (r.deleted) setSelEl(""); },
    } as Lane,
    ...[...L.items].reverse().map(laneOf),
    {
      key: "sound", group: "sound", groupLabel: tx("Sound clips"), label: tx("Sound clips"), color: "#7fa7d9", sel: selTrack, stack: true,
      // each sound at its real length; a muted one says so, and selecting one opens it in the Sound tab
      items: clips.map((c, i) => ({ id: `c:${c.id || i}`, s: c.at, e: c.at + Math.max(0.3, c.dur || 1), locked: !!c.link, label: (c.link ? "▶ " : "") + (c.mute ? tx("(muted) ") : "") + (c.name || c.file) })),
      onSelect: (cid) => { setSelTrack(cid); setSide("sound"); },
      onChange: (cid, s) => saveClips(clips.map((c, i) => (`c:${c.id || i}` === cid ? { ...c, at: Math.round(s * 100) / 100 } : c))),
      onDelete: (cid) => { saveClips(clips.filter((c, i) => `c:${c.id || i}` !== cid)); setSelTrack(""); },
    } as Lane,
  ].filter((ln) => ln.key !== "elements" || reg.length > 0);
  const puppetInfo = (name: string) => win()?.KE_PUPPET?.(name) || null;
  const SIDE_TABS = [["props", tx("Properties"), SlidersHorizontal], ["sound", tx("Sound"), Volume2], ["langs", tx("Languages"), LanguagesIcon], ["ai", tx("Assistant"), Bot], ["versions", tx("Versions"), History]] as const;

  if (err) return <div className="page"><button className="btn ghost" onClick={back}><ArrowLeft />{tx("Films")}</button><p className="err" style={{ marginTop: 16 }}>{err}</p></div>;
  if (!info) return <div className="page"><span className="muted">{tx("Loading")}</span></div>;
  const looseText = () => tx("In the film \"{0}\" ({1}) in Linnea, some drawings are made outside el(), so they show in the preview but not in the Layers panel (at {2} s). Move each into its own el(id, t, {label, x, y, t0, t1}, draw) with an English id and label, keep the existing ids and the user's edits, then hub_live until checks.not_editable is empty.", info.title, id, loose.map((f) => f.t).join(", "));
  const copyLoose = () => copy(looseText(), tx("Message for Claude copied"));
  const fixLoose = async () => {
    setSide("ai");
    const r = await run(tx("Assistant making the drawings editable"), "hub_ask", { prompt: looseText() });
    if (r) say(tx("Done: the drawings are being scanned again"));
  };
  const askEditable = () => navigator.clipboard.writeText(tx("Make the film \"{0}\" ({1}) in Linnea editable: draw every text and element on screen (title frames, caption boxes, characters, objects) with KE.text / KE.group so I can move and resize each as one piece in the app. Keep the current tweaks.json edits. Then hub_live.", info.title, id)).then(() => say(tx("Message for Claude copied")));
  return (
    <div className="ed film-ed" style={{ gridTemplateColumns: `${pane.l ? 288 : 44}px minmax(0, 1fr) ${pane.r ? 320 : 44}px` }}>
      <div className="ed-top">
        <button className="btn icon ghost" onClick={back} title={tx("Films")}><ArrowLeft /></button>
        <span className="title">{info.title}</span>
        <span className="muted num" style={{ fontSize: 12 }}>{W}×{H} · {FPS} {tx("fps ·")} {dur(DUR)}</span>
        <span className="top-menus">
          <MenuButton label={lang === "tr" ? "Düzen" : "Edit"} items={editMenu} />
          <MenuButton label={tx("View")} items={viewMenu} />
          <button className="btn sm ghost cmdk" onClick={() => setPalette(true)} title={tx("All commands (Ctrl+K)")}><CmdIcon />{tx("Commands")}<span className="kbd">Ctrl K</span></button>
        </span>
        <span className="grow" />
        {busy && (
          <div className="rowx" style={{ width: 260 }}>
            <span className="sec" style={{ fontSize: 12, whiteSpace: "nowrap" }}>{busy.label}</span>
            <div className="meter grow"><i style={{ width: `${busy.p * 100}%` }} /></div>
            <button className="btn icon sm ghost" title={tx("Stop")} onClick={() => act.jobs.filter((j) => j.project === id && j.status === "running").forEach((j) => tool("hub_job", { id: j.id, cancel: true }))}><Square /></button>
          </div>
        )}
        <button className="btn icon sm ghost" title={tx("Undo (Ctrl+Z)")} disabled={!hist.current.past.length} onClick={undo}><Undo2 /></button>
        <button className="btn icon sm ghost" title={tx("Redo (Ctrl+Y)")} disabled={!hist.current.future.length} onClick={redo}><Redo2 /></button>
        <button className="btn ghost icon" title={tx("Film folder")} onClick={() => tool("reveal", { path: info.dir })}><FolderOpen /></button>
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

      {!pane.l && (
        <div className="ed-left prail">
          <button className="prail-b" title={tx("Show the left panel")} onClick={() => flip("l")}><PanelLeftOpen /></button>
          <span className="prail-sep" />
          {([["layers", tx("Layers"), LayersIcon, L.items.length], ["add", tx("Add"), Plus, 0], ["notes", tx("Notes"), StickyNote, openNotes.length]] as const).map(([k, l, I, n]) => (
            <button key={k} className={`prail-b ${left === k ? "on" : ""}`} title={l} onClick={() => { showLeft(k); if (k === "add") setPlaying(false); }}><I />{n ? <span className="prail-n">{n}</span> : null}</button>
          ))}
        </div>
      )}
      <div className="ed-left" style={{ padding: 0, gap: 0, display: pane.l ? undefined : "none" }}>
        <div className="tabs">
          <button className={left === "layers" ? "on" : ""} onClick={() => setLeft("layers")}>{tx("Layers")}{L.items.length ? <span className="tab-n">{L.items.length}</span> : null}</button>
          <button className={left === "add" ? "on" : ""} onClick={() => { setLeft("add"); setPlaying(false); }}><Plus size={14} style={{ marginRight: 2, verticalAlign: -2 }} />{tx("Add")}</button>
          <button className={left === "notes" ? "on" : ""} onClick={() => setLeft("notes")}>{tx("Notes")}{openNotes.length ? <span className="tab-n">{openNotes.length}</span> : null}</button>
          <span className="tabs-fill" />
          <button className="tabs-x" title={tx("Hide the left panel (Ctrl+\\ hides both)")} onClick={() => flip("l")}><PanelLeftClose /></button>
        </div>
        <div className="tab-body">
          {left !== "notes" && !L.installed && (
            <div className="card" style={{ padding: 12, marginBottom: 12 }}>
              <p className="sec" style={{ fontSize: 12, marginBottom: 8, lineHeight: "18px" }}>{tx("Editing is not switched on for this film yet. It adds one script line to the page; the film's code stays as it is.")}</p>
              <button className="btn sm primary" onClick={() => L.install().then(() => say(tx("Editing hook added to the film")))}>{tx("Switch on editing")}</button>
            </div>
          )}
          {left === "layers" && <LayerStack L={L} t={t} sel={sel2d} multi={chosen} select={select} seek={(x) => { setPlaying(false); seek(x); }} reg={reg} E={E} selEl={selEl} scanning={scanning}
            pickEl={(x, t0, t1) => { setSelEl(x); setSel2d(""); setMulti([]); setSide("props"); if (tRef.current < t0 || tRef.current >= t1) { setPlaying(false); seek(t0 + 0.05); } }}
            onMenu={(x, y, lid) => setMenu({ x, y, id: lid })} onAdd={() => showLeft("add")} askClaude={askEditable}
            loose={loose} onLoose={(x) => { setPlaying(false); seek(x); }} fixLoose={fixLoose} copyLoose={copyLoose} busy={!!busy} />}
          {left === "add" && <AddPanel L={L} t={t} W={W} H={H} dur={DUR} say={say} select={(ids) => { select(ids); }} addSound={addSound}
            scenes={<SceneGrid film={id} L={L} t={t} W={W} H={H} dur={DUR} say={say} select={(ids) => select(ids)} />}
            puppets={(dropRef: any) => <PuppetGrid film={id} L={L} t={t} W={W} H={H} dur={DUR} say={say} select={(ids) => select(ids)} dropRef={dropRef} makeFromImage={puppetFromImage} />} />}
          {left === "notes" && <NotesList notes={notes} fps={FPS} seek={(n: Json) => { seek(n.t); if (n.t1) setRange([n.t, n.t1]); }} update={async (n: Json, p: Json) => { await tool("hub_note_update", { film: id, id: n.id, ...p }); load(); }} />}
        </div>
      </div>

      <div className="stage">
        <Viewer W={W} H={H} zoom={zoom} setZoom={setZoom} onScale={setViewScale}>
          {watch ? (
            <VersionPlayer v={watch} t={t} onTime={(x: number) => setT(x)} close={() => setWatch(null)} note={async (tt: number, text: string) => { await tool("hub_note_add", { film: id, t: tt, text, video: watch.file }); load(); say(tx("Note left on the version")); }} />
          ) : (
            <>
              <iframe ref={frame} key={rev} src={`${info.url}?r=${rev}`} onLoad={onLoad} style={{ width: W, height: H, border: 0, display: "block", background: "#000", pointerEvents: "none" }} title="film" />
              {guides && <FrameGuides W={W} H={H} kind={guides} />}
              {editOn && (() => { const f = loose.find((x) => Math.abs(x.t - t) < 0.02); return f ? <LooseBoxes W={W} H={H} boxes={f.loose} /> : null; })()}
              {mode === "note" && <NoteLayer W={W} H={H} t={t} range={range} fps={FPS} cancel={() => setMode("live")} save={async (p: Json) => { setBusy({ label: tx("Saving the note"), p: 0.5 }); try { await tool("hub_note_add", { film: id, ...p }); setMode("live"); setRange(null); load(); } catch (e: any) { say(e.message); } setBusy(null); }} />}
              {editOn && <ElementOverlay boxes={boxes.filter((b) => !E.edits[b.id]?.deleted)} E={E} sel={selEl} setSel={(x) => { setSelEl(x); if (x) { setSel2d(""); setMulti([]); setSide("props"); } else select([]); }} t={t} W={W} H={H} L={L} />}
              {editOn && <LayerOverlay L={L} t={t} sel={sel2d} setSel={(x) => { setSel2d(x); if (x) { setSelEl(""); setSide("props"); } }} multi={chosen} setMulti={setMulti} W={W} H={H} passThrough pathFor={pathFor} snap={snapOn} others={boxes} onMenu={(x, y, lid) => setMenu({ x, y, id: lid })} view={viewScale} />}
              {mode === "check" && <CheckLayer notes={toCheck} seek={seek} update={async (n: Json, p: Json) => { await tool("hub_note_update", { film: id, id: n.id, ...p }); load(); }} done={() => setMode("live")} />}
            </>
          )}
        </Viewer>
        {scopes && !watch && <Scopes film={id} t={t} playing={playing} close={() => setScopes(false)} source={() => {
          try { const d = frame.current?.contentDocument; return d ? [...d.querySelectorAll("canvas")].sort((a, b) => b.width * b.height - a.width * a.height)[0] || null : null; } catch { return null; }
        }} />}
        {scanning && info && <iframe ref={scanFrame} key={scanKey} src={`${info.url}?r=${rev}&scan=1`} onLoad={onScanLoad} title="scan" aria-hidden tabIndex={-1} style={{ position: "fixed", left: -20000, top: 0, width: W, height: H, border: 0, visibility: "hidden", pointerEvents: "none" }} />}
        <div id="note-host" style={{ width: "100%", maxWidth: 680 }} />
        {!ready && !watch && <div className="muted" style={{ fontSize: 12 }}>{pageErr ? <span className="err">{pageErr}</span> : tx("Loading the page")}</div>}
        {ready && pageErr && <div className="err" style={{ fontSize: 12 }}>{pageErr}</div>}
        <div className="transport">
          <div className="tp-g">
            <button className="btn icon sm ghost" onClick={() => seek(0)} title={tx("Start (Home)")}><SkipBack /></button>
            <button className="btn icon sm ghost" onClick={() => seek(t - 1 / FPS)} title={tx("Previous frame (,)")}><ChevronLeft /></button>
            <button className="btn icon tp-play" onClick={() => setPlaying(!playing)} disabled={mode !== "live"} title={tx("Play / pause (Space, K)")}>{playing ? <Pause /> : <Play />}</button>
            <button className="btn icon sm ghost" onClick={() => seek(t + 1 / FPS)} title={tx("Next frame (.)")}><ChevronRight /></button>
            <button className="btn icon sm ghost" onClick={() => seek(DUR)} title={tx("End (End)")}><SkipForward /></button>
          </div>
          <div className="tp-g">
            <span className="tc"><b>{tc(t, FPS)}</b> <span>/ {tc(DUR, FPS)}</span></span>
            <span className="muted num tp-frame" style={{ fontSize: 12 }}>{tx("frame")} {Math.round(t * FPS)}</span>
            <button className={`btn icon sm ${loop ? "" : "ghost"}`} onClick={() => setLoop(!loop)} title={tx("Loop")}><Repeat /></button>
          </div>
          <div className="tp-g">
            <button className={`btn sm ${mode === "note" ? "primary" : ""}`} onClick={() => { setPlaying(false); setMode(mode === "note" ? "live" : "note"); }} title={tx("Note (N)")}><Pen /><span className="lbl">{tx("Note")}</span> <span className="kbd lbl">N</span></button>
            <button className="btn sm" onClick={sendFrame} title={tx("Capture the frame (or the selected range) for Claude")}><ImageIcon /><span className="lbl">{range ? tx("Range to Claude") : tx("Frame to Claude")}</span></button>
            {range && <button className="btn icon sm ghost" title={tx("Clear range")} onClick={() => setRange(null)}><X /></button>}
          </div>
          <div className="tp-g">
          {viewLang && <button className="btn sm ghost" title={tx("Back to the original texts")} onClick={() => setViewLang(null)} style={{ color: "var(--accent)" }}>{viewLang.code} <X /></button>}
          <select className="select zoomsel" value={zoom ? String(zoom) : "0"} title={tx("Zoom the frame (Ctrl + wheel; Ctrl+0 fits)")} onChange={(e) => setZoom(+e.target.value)}>
            <option value="0">{tx("Fit")}{!zoom ? ` · ${Math.round(viewScale * 100)}%` : ""}</option>
            {[0.25, 0.5, 1, 2, 4].map((z) => <option key={z} value={String(z)}>{z * 100}%</option>)}
            {zoom && ![0.25, 0.5, 1, 2, 4].includes(zoom) ? <option value={String(zoom)}>{Math.round(zoom * 100)}%</option> : null}
          </select>
          <button className={`btn icon sm ${guides ? "" : "ghost"}`} onClick={() => setGuides(guides === "" ? "safe" : guides === "safe" ? "thirds" : "")} title={tx("Safe areas / thirds (G)")} style={guides ? { color: "var(--accent)" } : undefined}><Grid3x3 /></button>
          <button className={`btn icon sm ${board ? "" : "ghost"}`} onClick={() => setBoard(!board)} title={tx("Storyboard (B)")}><LayoutGrid /></button>
          <button className="btn icon sm ghost" onClick={() => setKeysOpen(true)} title={tx("Keyboard shortcuts (?)")}><Keyboard /></button>
          </div>
        </div>
        {board && !watch && info && <Storyboard url={info.url} rev={rev} lang={viewLang?.code || ""} W={W} H={H} dur={DUR} scenes={scenesRef.current} t={t} seek={(x) => { setPlaying(false); seek(x); }} />}
        {keysOpen && <Shortcuts close={() => setKeysOpen(false)} />}
        {info.audio && <audio ref={audio} src={fileUrl(info.audio, audioV)} preload="auto" />}
      </div>

      {!pane.r && (
        <div className="ed-right prail">
          <button className="prail-b" title={tx("Show the right panel")} onClick={() => flip("r")}><PanelRightOpen /></button>
          <span className="prail-sep" />
          {SIDE_TABS.map(([k, l, I]) => <button key={k} className={`prail-b ${side === k ? "on" : ""}`} title={l} onClick={() => showSide(k)}><I /></button>)}
        </div>
      )}
      <div className="ed-right" style={{ padding: 0, gap: 0, overflow: "hidden", display: pane.r ? undefined : "none" }}>
        <div className="tabs side-tabs">
          <button className="tabs-x" title={tx("Hide the right panel (Ctrl+\\ hides both)")} onClick={() => flip("r")}><PanelRightClose /></button>
          {SIDE_TABS.map(([k, l, I]) => (
            <button key={k} className={side === k ? "on" : ""} title={l} onClick={() => setSide(k)}><I className="tab-i" /><span className="tab-l">{l}</span></button>
          ))}
        </div>
        <div style={{ padding: 16, display: "flex", flexDirection: "column", gap: 24, overflow: "auto", flex: 1 }}>
        {side === "langs" && <LangsPanel film={id} reg={reg} scanning={scanning} t={t} seek={(x) => { setPlaying(false); seek(x); }} say={say} view={viewLang} setView={setViewLang} run={run} copy={copy} />}
        {side === "ai" && <FilmAssistant id={id} dir={info.dir} t={t} fps={FPS} sel={selEl} run={run} busy={!!busy} say={say} />}
        {side === "sound" && <SoundPanel film={id} t={t} range={range} say={say} rev={audioV} sel={selTrack.startsWith("c:") ? selTrack.slice(2) : ""} seek={(x) => { setPlaying(false); seek(x); }} onApplied={() => { setAudioV(Date.now()); load(); }} />}
        {side === "props" && chosen.length > 1 && <MultiInspector L={L} ids={chosen} t={t} W={W} H={H} dur={DUR} select={(ids) => select(ids)} />}
        {side === "props" && chosen.length <= 1 && one && <LayerInspector L={L} id={one.id} t={t} W={W} H={H} dur={DUR} seek={(x) => { setPlaying(false); seek(x); }} pathFor={pathFor} setPathFor={setPathFor} puppetInfo={puppetInfo} onLipsync={lip} say={say} select={(ids) => select(ids)} />}
        {side === "props" && !one && selEl && <ElementPanel id={selEl} box={boxes.find((b) => b.id === selEl)} reg={reg.find((r) => r.id === selEl)} E={E} t={t} close={() => setSelEl("")} />}
        {side === "props" && !one && selEl && (() => {
          const b = boxes.find((x) => x.id === selEl);
          if (b?.kind === "puppet") return <PuppetFields info={win()?.KE_PUPPET?.(b.puppet) || null} params={E.edits[selEl]?.params ?? b.params ?? {}} states={E.edits[selEl]?.states ?? b.states ?? []} localT={b.pt ?? 0} onParams={(params) => E.set(selEl, { params })} onStates={(states) => E.set(selEl, { states })} onLipsync={lip} />;
          return <button className="btn sm" onClick={() => navigator.clipboard.writeText(tx("In the film \"{0}\" in Linnea, turn the element \"{1}\" (id {2}) into a puppet: save its drawing code to the library with puppet_save (things that may change, like colour or text, become params; if it animates, make it a rich puppet with states), draw it in the film with KE.puppet, keep the current edits, then hub_live.", id, b?.label || selEl, selEl)).then(() => say(tx("Message for Claude copied")))}>{tx("Have Claude make it a puppet")}</button>;
        })()}
        {side === "props" && !one && !selEl && (
          <div className="stack empty-props">
            <MousePointerClick size={28} style={{ color: "var(--text-3)" }} />
            <p style={{ margin: 0 }}>{tx("Select something on the frame, in Layers or on the timeline to change it here.")}</p>
            <div className="kv tips">
              <span className="kbd">A</span><span>{tx("Add a title, shape, picture or puppet")}</span>
              <span className="kbd">Shift</span><span>{tx("Click to select several")}</span>
              <span className="kbd">S</span><span>{tx("Split the selected layer at the playhead")}</span>
              <span className="kbd">Ctrl K</span><span>{tx("Every command, by name")}</span>
              <span className="kbd">G</span><span>{tx("Safe areas and thirds")}</span>
            </div>
            <button className="btn sm" onClick={askEditable}>{tx("Have Claude make everything editable")}</button>
          </div>
        )}
        {side === "versions" && <>
        <Versions film={id} say={say} versions={versions} watch={watch} setWatch={setWatch} />
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
        <FilmTimeline markers={markers} setMarkers={setMarkers} dur={DUR} fps={FPS} t={t} seek={(x: number) => { setPlaying(false); seek(x); }} scenes={scenes.length ? scenes : info.scenes || []} notes={notes} range={range} setRange={setRange} changed={latest?.changedRanges || []} chunkSec={info.chunk_frames / FPS} lanes={lanes}
          playing={playing} snapOn={snapOn} setSnapOn={setSnapOn} open={pane.tl} setOpen={() => flip("tl")}
          groupActs={{
            elements: (() => { const ids = reg.filter((r) => !E.edits[r.id]?.deleted).map((r) => r.id); const hidden = ids.length > 0 && ids.every((x) => E.edits[x]?.hidden);
              return { hidden, eye: () => E.update({ ...E.edits, ...Object.fromEntries(ids.map((x) => [x, { ...(E.edits[x] || {}), hidden: !hidden }])) }),
                trash: () => { if (hidden && !window.confirm(tx("Delete all {0} elements? They leave the picture and the lists; the code is not touched, and Layers › Deleted brings them back.", ids.length))) return; E.update(hideThenDeleteEls(E.edits, ids).edits); } }; })(),
            layers: (() => { const ids = L.items.map((x) => x.id); const hidden = ids.length > 0 && L.items.every((x) => x.hidden);
              return { hidden, eye: () => L.update(L.items.map((x) => ({ ...x, hidden: !hidden || undefined }))),
                trash: () => { if (hidden && !window.confirm(tx("Delete all {0} layers? (Ctrl+Z brings them back)", ids.length))) return; const r = hideThenRemove(L.items, ids); L.update(r.items); if (r.removed) select([]); } }; })(),
          }}
          tools={<>
            <button className="btn sm ghost" title={tx("Split the selected layer at the playhead (S)")} disabled={!one || t <= one.t0 || t >= one.t1} onClick={() => { const r = split(L.items, one!.id, t); if (r.id) { L.update(r.items); select([r.id]); } }}><Scissors />{tx("Split")}</button>
            <button className="btn sm ghost" title="Alt+[" disabled={!chosen.length} onClick={() => L.update(trim(L.items, chosen, t, "start"))}>{tx("Start here")}</button>
            <button className="btn sm ghost" title="Alt+]" disabled={!chosen.length} onClick={() => L.update(trim(L.items, chosen, t, "end"))}>{tx("End here")}</button>
            <button className="btn sm ghost" title={tx("Range starts here (I)")} onClick={() => setRange((r) => [t, Math.max(t + 1 / FPS, r ? r[1] : DUR)])}>I</button>
            <button className="btn sm ghost" title={tx("Range ends here (O)")} onClick={() => setRange((r) => [Math.min(r ? r[0] : 0, t - 1 / FPS), t])}>O</button>
          </>} />
      </div>
      {menu && <Menu x={menu.x} y={menu.y} items={layerOps(chosen.includes(menu.id) ? chosen : [menu.id])} close={() => setMenu(null)} />}
      {palette && <Palette commands={commands} close={() => setPalette(false)} />}
    </div>
  );
}

/** Where the film paints outside every element, at an audited second: red, so it can be found. */
function LooseBoxes({ W, H, boxes }: { W: number; H: number; boxes: number[][] }) {
  const lw = Math.max(2, W / 400);
  return (
    <svg width={W} height={H} viewBox={`0 0 ${W} ${H}`} style={{ position: "absolute", inset: 0, pointerEvents: "none" }}>
      {boxes.map((b, i) => <rect key={i} x={b[0]} y={b[1]} width={b[2]} height={b[3]} fill="rgba(229,72,77,0.12)" stroke="#e5484d" strokeWidth={lw} strokeDasharray={`${lw * 4} ${lw * 3}`} />)}
    </svg>
  );
}

/** Safe areas (action 93 %, title 90 %, and the 9:16 social UI band) or the rule of thirds, over the frame. */
function FrameGuides({ W, H, kind }: { W: number; H: number; kind: "safe" | "thirds" }) {
  const lw = Math.max(1, W / 900);
  const st = { stroke: "rgba(255,255,255,.55)", strokeWidth: lw, fill: "none" } as const;
  return (
    <svg width={W} height={H} viewBox={`0 0 ${W} ${H}`} style={{ position: "absolute", inset: 0, pointerEvents: "none" }}>
      {kind === "thirds" ? (
        <>
          {[1, 2].map((i) => <line key={"v" + i} x1={(W * i) / 3} x2={(W * i) / 3} y1={0} y2={H} {...st} />)}
          {[1, 2].map((i) => <line key={"h" + i} y1={(H * i) / 3} y2={(H * i) / 3} x1={0} x2={W} {...st} />)}
        </>
      ) : (
        <>
          <rect x={W * 0.035} y={H * 0.035} width={W * 0.93} height={H * 0.93} {...st} strokeDasharray={`${lw * 8} ${lw * 6}`} />
          <rect x={W * 0.05} y={H * 0.05} width={W * 0.9} height={H * 0.9} {...st} />
          {H > W && <rect x={0} y={H * 0.8} width={W} height={H * 0.2} fill="rgba(255,80,120,.10)" stroke="none" />}
          {H > W && <rect x={W * 0.86} y={H * 0.45} width={W * 0.14} height={H * 0.35} fill="rgba(255,80,120,.10)" stroke="none" />}
          <line x1={W / 2 - lw * 12} x2={W / 2 + lw * 12} y1={H / 2} y2={H / 2} {...st} />
          <line y1={H / 2 - lw * 12} y2={H / 2 + lw * 12} x1={W / 2} x2={W / 2} {...st} />
        </>
      )}
    </svg>
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

/** The frame, fitted to the space (zoom 0) or at a set zoom (1 = one frame pixel per screen pixel), scrolling
 *  when it is bigger. Ctrl + wheel zooms around the pointer, the middle button drags it around. */
function Viewer({ W, H, children, zoom = 0, setZoom, onScale }: { W: number; H: number; children: any; zoom?: number; setZoom?: (z: number) => void; onScale?: (s: number) => void }) {
  const box = useRef<HTMLDivElement>(null);
  const [fit, setFit] = useState(0.3);
  useEffect(() => {
    const ro = new ResizeObserver((e) => {
      const r = e[0].contentRect;
      setFit(Math.max(0.05, Math.min(r.width / W, r.height / H)));
    });
    if (box.current) ro.observe(box.current);
    return () => ro.disconnect();
  }, [W, H]);
  const s = zoom > 0 ? zoom : fit;
  useEffect(() => onScale?.(s), [s]);
  const anchor = useRef<{ fx: number; fy: number; mx: number; my: number } | null>(null);
  // the frame point in the middle of the view, kept as the view scrolls: a zoom from the menu or the keys
  // keeps it there; a zoom with the wheel keeps the point under the pointer
  const mid = useRef({ fx: W / 2, fy: H / 2 });
  const onScroll = () => { const el = box.current; if (!el) return; const m = el.firstElementChild as HTMLElement; mid.current = { fx: (el.scrollLeft + el.clientWidth / 2 - m.offsetLeft) / s, fy: (el.scrollTop + el.clientHeight / 2 - m.offsetTop) / s }; };
  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const a = anchor.current || { ...mid.current, mx: el.clientWidth / 2, my: el.clientHeight / 2 };
    anchor.current = null;
    const m = el.firstElementChild as HTMLElement;
    el.scrollLeft = m.offsetLeft + a.fx * s - a.mx;
    el.scrollTop = m.offsetTop + a.fy * s - a.my;
  }, [s]);
  useEffect(() => {
    const el = box.current;
    if (!el || !setZoom) return;
    const wheel = (e: WheelEvent) => {
      if (!e.ctrlKey && !e.metaKey) return;
      e.preventDefault();
      const r = el.getBoundingClientRect(), m = el.firstElementChild as HTMLElement;
      const mx = e.clientX - r.left, my = e.clientY - r.top;
      anchor.current = { fx: (el.scrollLeft + mx - m.offsetLeft) / s, fy: (el.scrollTop + my - m.offsetTop) / s, mx, my };
      setZoom(Math.max(0.05, Math.min(8, s * Math.exp(-e.deltaY * 0.0025))));
    };
    el.addEventListener("wheel", wheel, { passive: false });
    return () => el.removeEventListener("wheel", wheel);
  });
  const pan = (e: React.PointerEvent) => {
    if (e.button !== 1 || !box.current) return;
    e.preventDefault();
    const el = box.current, x0 = e.clientX, y0 = e.clientY, l0 = el.scrollLeft, t0 = el.scrollTop;
    const mv = (ev: PointerEvent) => { el.scrollLeft = l0 - (ev.clientX - x0); el.scrollTop = t0 - (ev.clientY - y0); };
    const up = () => { window.removeEventListener("pointermove", mv); window.removeEventListener("pointerup", up); };
    window.addEventListener("pointermove", mv);
    window.addEventListener("pointerup", up);
  };
  return (
    <div ref={box} onPointerDown={pan} onScroll={onScroll} style={{ flex: 1, minHeight: 0, width: "100%", display: "flex", overflow: zoom > 0 ? "auto" : "hidden" }}>
      <div className="monitor" style={{ width: W * s, height: H * s, flex: "0 0 auto", maxHeight: "none", margin: "auto" }}>
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

function Versions({ versions, watch, setWatch, film, say }: Json) {
  const list: Json[] = versions.slice().reverse();
  return (
    <div className="stack" style={{ gap: 8 }}>
      <h2>{tx("Versions")}</h2>
      {!list.length && <p className="sec" style={{ fontSize: 12 }}>{tx("No renders yet. The film plays live; press Render when you are ready.")}</p>}
      {list.length > 0 && film && (
        <div className="rowx" style={{ gap: 4, flexWrap: "wrap" }}>
          <span className="muted grow" style={{ fontSize: 12 }}>{tx("Open in another editor")}</span>
          {([["fcpxml", "Final Cut", tx("Final Cut Pro and DaVinci Resolve (FCPXML)")], ["premiere", "Premiere", tx("Premiere Pro and DaVinci Resolve (XML), with the markers")]] as const).map(([f, l, title]) => (
            <button key={f} className="btn sm ghost" title={title} onClick={() => tool("export_timeline", { film, format: f }).then((r) => { say?.(tx("Timeline written: {0}", r.files.map((x: Json) => String(x.file).split("/").pop()).join(", "))); tool("reveal", { path: r.files[0].file }).catch(() => {}); }).catch((e) => say?.(e.message))}>{l}</button>
          ))}
        </div>
      )}
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

const HEAD = 148;
/** The film's timeline: ruler, render chunks, scenes, notes and one lane per layer, grouped and foldable.
 *  Ctrl + wheel zooms around the pointer, Shift + wheel (or a trackpad swipe) scrolls; bars snap. */
const MARKER_COLORS: Record<string, string> = { blue: "#5b9dff", green: "#5fcf7a", yellow: "#e9c84a", orange: "#f08c3a", red: "#ef5b5b", purple: "#b07cf2" };
/** A marker on the ruler: drag it along, double-click to name it, right-click for its colour or to remove it. */
function MarkerFlag({ m, X, at, fps, seek, change, remove }: Json) {
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const down = (e: React.MouseEvent) => {
    if (e.button !== 0) return;
    e.stopPropagation();
    if (e.detail === 2) {
      const name = window.prompt(tx("Marker name"), m.name || "");
      if (name !== null) change({ ...m, name: name.trim() || undefined });
      return;
    }
    let moved = false, tt = m.t;
    const mv = (ev: MouseEvent) => { moved = true; tt = Math.round(at(ev) * fps) / fps; (e.target as HTMLElement).style.left = X(tt) + "px"; };
    const up = () => { window.removeEventListener("mousemove", mv); window.removeEventListener("mouseup", up); if (moved) change({ ...m, t: Math.round(tt * 1000) / 1000 }); else seek(m.t); };
    window.addEventListener("mousemove", mv);
    window.addEventListener("mouseup", up);
  };
  return (
    <>
      <span className="tl-mark" style={{ left: X(m.t), ["--mc" as any]: MARKER_COLORS[m.color] || MARKER_COLORS.blue }} title={`${m.name || tx("Marker")} · ${tc(m.t, fps)}\n${tx("Drag to move, double-click to name, right-click for more")}`}
        onMouseDown={down} onContextMenu={(e) => { e.preventDefault(); e.stopPropagation(); setMenu({ x: e.clientX, y: e.clientY }); }}>{m.name ? <b>{m.name}</b> : null}</span>
      {menu && <Menu x={menu.x} y={menu.y} close={() => setMenu(null)} items={[
        { label: tx("Name…"), run: () => { const name = window.prompt(tx("Marker name"), m.name || ""); if (name !== null) change({ ...m, name: name.trim() || undefined }); } },
        "-",
        ...Object.keys(MARKER_COLORS).map((c) => ({ label: (m.color === c ? "✓ " : "") + tx(c[0].toUpperCase() + c.slice(1)), run: () => change({ ...m, color: c }) })),
        "-",
        { label: tx("Remove the marker"), run: remove },
      ]} />}
    </>
  );
}

function FilmTimeline({ markers = [], setMarkers, dur: D, fps, t, seek, scenes, notes, range, setRange, changed, chunkSec, lanes, playing, snapOn, setSnapOn, tools, open = true, setOpen, groupActs }: Json) {
  const box = useRef<HTMLDivElement>(null);
  const wrap = useRef<HTMLDivElement>(null);
  const [w, setW] = useState(800);
  const [zoom, setZoom] = useState(1);
  const [off, setOff] = useState(0);
  const [snapLine, setSnapLine] = useState<number | null>(null);
  const [hover, setHover] = useState<number | null>(null);
  // the timeline's height: drag its top edge
  const [tlH, setTlH] = useState(() => { try { return +(localStorage.getItem("linnea.tlh") || 0) || 260; } catch { return 260; } });
  useEffect(() => { try { localStorage.setItem("linnea.tlh", String(Math.round(tlH))); } catch { /* private window */ } }, [tlH]);
  const resize = (e: React.PointerEvent) => {
    const y0 = e.clientY, h0 = tlH;
    const mv = (ev: PointerEvent) => setTlH(Math.max(120, Math.min(window.innerHeight * 0.7, h0 - (ev.clientY - y0))));
    const up = () => { window.removeEventListener("pointermove", mv); window.removeEventListener("pointerup", up); document.body.style.cursor = ""; };
    document.body.style.cursor = "ns-resize";
    window.addEventListener("pointermove", mv);
    window.addEventListener("pointerup", up);
  };
  const [shut, setShut] = useState<Record<string, boolean>>(() => { try { return JSON.parse(localStorage.getItem("linnea.tlshut") || "{}"); } catch { return {}; } });
  useEffect(() => { try { localStorage.setItem("linnea.tlshut", JSON.stringify(shut)); } catch { /* private window */ } }, [shut]);
  useEffect(() => {
    const ro = new ResizeObserver((e) => setW(e[0].contentRect.width));
    if (box.current) ro.observe(box.current);
    return () => ro.disconnect();
  }, []);
  const padR = 16;
  const span = Math.max(1, w - HEAD - padR);
  const sx = (span / Math.max(D, 0.1)) * zoom;
  const vis = span / sx;
  const offC = Math.max(0, Math.min(Math.max(0, D - vis), off));
  const X = (x: number) => HEAD + (x - offC) * sx;
  const at = (e: { clientX: number }) => {
    const r = box.current!.getBoundingClientRect();
    return Math.max(0, Math.min(D, (e.clientX - r.left - HEAD) / sx + offC));
  };
  // keep the playhead in view while playing
  useEffect(() => { if (playing && (t > offC + vis * 0.95 || t < offC)) setOff(Math.max(0, t - vis * 0.1)); }, [t, playing]);
  const zoomAt = (z: number, anchor: number) => {
    const nz = Math.max(1, Math.min(200, z));
    const nsx = (span / Math.max(D, 0.1)) * nz;
    setZoom(nz);
    setOff(anchor - (X(anchor) - HEAD) / nsx);
  };
  useEffect(() => {
    const el = wrap.current;
    if (!el) return;
    const wheel = (e: WheelEvent) => {
      if (e.ctrlKey || e.metaKey) { e.preventDefault(); zoomAt(zoom * Math.exp(-e.deltaY * 0.0025), at(e)); }
      else if (e.shiftKey || Math.abs(e.deltaX) > Math.abs(e.deltaY)) { if (zoom > 1) { e.preventDefault(); setOff((o) => Math.max(0, Math.min(D - vis, o + ((e.deltaX || e.deltaY) / sx)))); } }
    };
    el.addEventListener("wheel", wheel, { passive: false });
    return () => el.removeEventListener("wheel", wheel);
  });
  const down = (e: React.MouseEvent) => {
    const r = box.current!.getBoundingClientRect();
    if (e.clientX - r.left < HEAD) return;
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
  const steps = [1 / fps, 0.1, 0.25, 0.5, 1, 2, 5, 10, 15, 30, 60, 120, 300];
  const step = steps.find((s) => s * sx >= 64) || 300;
  const ticks = useMemo(() => { const a = Math.floor(offC / step); return Array.from({ length: Math.ceil(vis / step) + 2 }, (_, i) => (a + i) * step).filter((x) => x <= D + 1e-6); }, [offC, vis, step, D]);
  const label = (x: number) => (step < 1 ? `${Math.floor(x / 60)}:${(x % 60).toFixed(step < 0.1 ? 2 : 1).padStart(step < 0.1 ? 5 : 4, "0")}` : `${Math.floor(x / 60)}:${String(Math.floor(x % 60)).padStart(2, "0")}`);
  // the lanes, in groups that fold
  const TOP = 98, GH = 20;
  let y = TOP;
  const rows: { kind: "group" | "lane"; key: string; label: string; y: number; h: number; lane?: Lane; n?: number; color?: string }[] = [];
  const groups: string[] = [];
  for (const ln of lanes as Lane[]) { const g = (ln as Json).group || ln.key; if (!groups.includes(g)) groups.push(g); }
  for (const g of groups) {
    const ls = (lanes as Lane[]).filter((ln) => ((ln as Json).group || ln.key) === g);
    const n = ls.reduce((a, ln) => a + ln.items.length, 0);
    rows.push({ kind: "group", key: g, label: (ls[0] as Json).groupLabel || ls[0].label, y, h: GH, n, color: ls[0].color });
    y += GH;
    if (shut[g]) continue;
    for (const ln of ls) { const h = laneRows(ln) * LANE_H; rows.push({ kind: "lane", key: ln.key, label: ln.label, y, h, lane: ln }); y += h; }
  }
  const height = y + 6;
  const snaps = useMemo(() => !snapOn ? undefined : [0, D, t, ...markers.map((m: Json) => m.t), ...scenes.flatMap((s: Json) => [s.t0, s.t1]), ...(range || []), ...(lanes as Lane[]).flatMap((ln) => ln.items.flatMap((i) => [i.s, i.e]))], [snapOn, D, t, markers, scenes, range, lanes]);
  return (
    <>
      {open && <div className="tl-resize" onPointerDown={resize} title={tx("Drag to make the timeline taller or shorter")} />}
      <div className="tl-bar">
        {setOpen && <button className="btn icon sm ghost" title={open ? tx("Hide the timeline (Ctrl+Shift+\\)") : tx("Show the timeline (Ctrl+Shift+\\)")} onClick={setOpen}>{open ? <ChevronDown /> : <ChevronUp />}</button>}
        {tools}
        <span className="tl-sep" />
        <button className={`btn icon sm ${snapOn ? "" : "ghost"}`} style={snapOn ? { color: "var(--accent)" } : undefined} title={tx("Snapping (bars stick to the playhead, cuts and each other; hold Alt to drag freely)")} onClick={() => setSnapOn(!snapOn)}><Magnet /></button>
        <span className="grow" />
        {hover !== null && <span className="num muted" style={{ fontSize: 11 }}>{tc(hover, fps)}</span>}
        <button className="btn icon sm ghost" title={tx("Zoom out (Ctrl + wheel)")} onClick={() => zoomAt(zoom / 1.5, t)}><ZoomOut /></button>
        <input type="range" className="tl-zoom" min={0} max={100} value={Math.round((Math.log(zoom) / Math.log(200)) * 100)} onChange={(e) => zoomAt(Math.exp((+e.target.value / 100) * Math.log(200)), t)} title={tx("Zoom")} />
        <button className="btn icon sm ghost" title={tx("Zoom in (Ctrl + wheel)")} onClick={() => zoomAt(zoom * 1.5, t)}><ZoomIn /></button>
        <button className="btn sm ghost" title={tx("Show the whole film")} onClick={() => { setZoom(1); setOff(0); }}>{tx("Fit")}</button>
      </div>
      <div className="tl-wrap" ref={wrap} style={{ height: tlH, maxHeight: "none", display: open ? undefined : "none" }}>
        <div className="tl" ref={box} onMouseDown={down} onMouseMove={(e) => { const r = box.current!.getBoundingClientRect(); setHover(e.clientX - r.left > HEAD ? at(e) : null); }} onMouseLeave={() => setHover(null)} style={{ height }}>
          {rows.filter((r) => r.kind === "lane").map((r) => <TrackLane key={r.key} lane={r.lane!} top={r.y} sx={sx} pad={HEAD - offC * sx} total={D} snaps={snaps} onSnap={setSnapLine} />)}
          {ticks.map((x) => (
            <span key={x} className="num" style={{ position: "absolute", left: X(x), top: 0, fontSize: 10, color: "var(--text-3)", borderLeft: "1px solid var(--line-2)", paddingLeft: 4, height: 14, lineHeight: "12px" }}>{label(x)}</span>
          ))}
          {Array.from({ length: Math.ceil(D / chunkSec) }, (_, i) => (
            <span key={`c${i}`} style={{ position: "absolute", left: X(i * chunkSec), top: 20, height: 4, width: Math.max(1, chunkSec * sx - 1), background: changed.some((r: number[]) => r[0] <= i * chunkSec + 1e-3 && r[1] >= (i + 1) * chunkSec - 1e-3 - 1 / fps) ? "var(--accent)" : "var(--line)" }} title={tx("Chunk {0}", i)} />
          ))}
          {scenes.map((s: Json, i: number) => (
            <span key={i} onMouseDown={(e) => { if (e.detail === 2) { e.stopPropagation(); setRange([s.t0, s.t1]); } }} title={tx("{0} · double-click: select this scene", s.name)} style={{ position: "absolute", left: X(s.t0), width: Math.max(2, (s.t1 - s.t0) * sx - 2), top: 30, height: 32, borderRadius: 6, background: "var(--take)", border: "1px solid var(--line)", fontSize: 11, lineHeight: "30px", padding: "0 8px", overflow: "hidden", whiteSpace: "nowrap", textOverflow: "ellipsis", color: "var(--text-2)" }}>{s.name}</span>
          ))}
          {range && <span style={{ position: "absolute", left: X(range[0]), width: Math.max(2, (range[1] - range[0]) * sx), top: 26, bottom: 4, background: "color-mix(in srgb, var(--accent) 12%, transparent)", border: "1px solid var(--accent)", borderRadius: 4, pointerEvents: "none" }} />}
          {notes.filter((n: Json) => n.status !== "done" && n.status !== "wontfix").map((n: Json) => (
            <span key={n.id} className="marker note" style={{ left: X(n.t), top: 70, borderColor: n.status === "fixed" ? "var(--good)" : undefined }} title={n.text} onMouseDown={(e) => { e.stopPropagation(); seek(n.t); }}>{n.text}</span>
          ))}
          {markers.map((m: Json) => <MarkerFlag key={m.id} m={m} X={X} at={at} fps={fps} seek={seek} change={(n: Json) => setMarkers?.(markers.map((x: Json) => (x.id === m.id ? n : x)))} remove={() => setMarkers?.(markers.filter((x: Json) => x.id !== m.id))} />)}
          {snapLine !== null && <div style={{ position: "absolute", left: X(snapLine), top: 0, bottom: 0, width: 1, background: "#ff4fa3", pointerEvents: "none", zIndex: 4 }} />}
          <div className="playhead" style={{ left: X(t), zIndex: 4 }} />
          <div className="tl-head" style={{ width: HEAD, height }}>
            <span className="muted" style={{ position: "absolute", left: 10, top: 34, fontSize: 11 }}>{tx("Scenes")}</span>
            <span className="muted" style={{ position: "absolute", left: 10, top: 72, fontSize: 11 }}>{tx("Notes")}</span>
            {rows.map((r) => r.kind === "group" ? (
              <button key={"g" + r.key} className="tl-g" style={{ top: r.y, height: r.h }} onMouseDown={(e) => e.stopPropagation()} onClick={() => setShut({ ...shut, [r.key]: !shut[r.key] })}>
                <ChevronRight size={12} style={{ transform: shut[r.key] ? undefined : "rotate(90deg)", transition: "transform .12s", flex: "none" }} /><i style={{ background: r.color }} /><span>{r.label}</span><span className="muted">{r.n}</span>
                {groupActs?.[r.key] && r.n ? (
                  <span className="tl-g-b" onClick={(e) => e.stopPropagation()}>
                    <span role="button" title={groupActs[r.key].hidden ? tx("Show them all") : tx("Hide them all")} onClick={groupActs[r.key].eye}>{groupActs[r.key].hidden ? <EyeOff size={12} /> : <Eye size={12} />}</span>
                    <span role="button" className={groupActs[r.key].hidden ? "armed" : ""} title={groupActs[r.key].hidden ? tx("Delete them all") : tx("Hide them all (press again to delete)")} onClick={groupActs[r.key].trash}><Trash2 size={12} /></span>
                  </span>
                ) : null}
              </button>
            ) : (
              <div key={"h" + r.key} className={`tl-l ${r.lane!.items.some((i) => i.id === r.lane!.sel || r.lane!.selSet?.includes(i.id)) ? "on" : ""}`} style={{ top: r.y, height: r.h }} onMouseDown={(e) => { e.stopPropagation(); const it = r.lane!.items[0]; if (it && r.lane!.items.length === 1) r.lane!.onSelect?.(it.id, e); }} title={r.label}>{r.label}</div>
            ))}
          </div>
        </div>
      </div>
    </>
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
