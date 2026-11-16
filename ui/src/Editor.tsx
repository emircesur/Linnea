import { tx } from "./i18n";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ArrowLeft, Check, ChevronDown, ChevronUp, Copy, FolderOpen, Pause, Pencil, Play, Plus, Scissors, SendHorizontal, Trash2, Undo2 } from "lucide-react";
import { CardActions, deleteItem, renameItem } from "./Manage";
import { Activity, dur, fetchJson, fileUrl, Json, parseCommand, runJob, tc, tool } from "./api";
import { tr } from "./App";
import { AiPanel, PublishButton } from "./Ai";
import { LayerOverlay, LayersPanel, ScaledStage, useLayers } from "./Layers";
import { Lane, LANE_H, TItem, TrackLane } from "./Tracks";
import { useLoad } from "./api";

type Seg = { a: number; b: number; edit: number; hold: number };
type Word = { w: string; s: number; e: number };
type Take = { a: number; b: number; hold?: number; note?: string; speed?: number };
type Busy = { label: string; p: number } | null;

const r2 = (x: number) => Math.round(x * 100) / 100;

function removeRange(takes: Take[], x: number, y: number): Take[] {
  const out: Take[] = [];
  for (const t of takes) {
    if (y <= t.a || x >= t.b) out.push(t);
    else {
      if (x - t.a > 0.12) out.push({ ...t, b: r2(x), hold: undefined });
      if (t.b - y > 0.12) out.push({ ...t, a: r2(y) });
    }
  }
  return out;
}
function addRange(takes: Take[], x: number, y: number): Take[] {
  const all = [...takes, { a: r2(x), b: r2(y) }].sort((p, q) => p.a - q.a);
  const out: Take[] = [];
  for (const t of all) {
    const l = out[out.length - 1];
    if (l && t.a <= l.b + 0.05) l.b = Math.max(l.b, t.b);
    else out.push({ ...t });
  }
  return out;
}

export function Editor({ id, act, say, back }: { id: string; act: Activity; say: (s: string) => void; back: () => void }) {
  const [proj, setProj] = useState<Json | null>(null);
  const [info, setInfo] = useState<Json | null>(null);
  const [words, setWords] = useState<Word[]>([]);
  const [peaks, setPeaks] = useState<Json | null>(null);
  const [face, setFace] = useState<Json | null>(null);
  const [report, setReport] = useState<Json | null>(null);
  const [ver, setVer] = useState(0);
  const [tab, setTab] = useState<"clips" | "script" | "style" | "beats" | "layers" | "notes">("script");
  const [sel2d, setSel2d] = useState("");
  const [selTrack, setSelTrack] = useState("");
  const lib = useLoad(() => tool("library"), []);
  const ovl = useRef<HTMLIFrameElement>(null);
  const tNow = useRef(0);
  const [t, setT] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [selTake, setSelTake] = useState(-1);
  // auditioning: a stretch of the source plays as it is (a selection, a clip candidate), outside the edit
  const [aud, setAud] = useState<[number, number] | null>(null);
  const audition = (a: number, b: number) => {
    const v = video.current;
    if (!v) return;
    setPlaying(false);
    setAud([a, b]);
    v.currentTime = Math.max(0, a);
    v.play().catch(() => setAud(null));
  };
  useEffect(() => {
    if (!aud) return;
    const v = video.current;
    if (!v) return;
    const tick = () => { if (v.currentTime >= aud[1] || v.ended) { v.pause(); setAud(null); } };
    const stop = () => setAud(null);
    v.addEventListener("timeupdate", tick);
    v.addEventListener("pause", stop);
    return () => { v.removeEventListener("timeupdate", tick); v.removeEventListener("pause", stop); };
  }, [aud]);
  const [busy, setBusy] = useState<Busy>(null);
  const [err, setErr] = useState("");
  const video = useRef<HTMLVideoElement>(null);
  const cur = useRef(0);

  const load = useCallback(async () => {
    try {
      const p = await tool("project", { project: id });
      setProj(p);
      const r = await tool("reel", { project: id });
      setInfo(r);
      setVer((v) => v + 1);
      if (p.files.words) fetchJson(p.files.words).then((w) => setWords(w?.words || []));
      else setWords([]);
      if (p.files.peaks) fetchJson(p.files.peaks).then(setPeaks);
      if (p.files.face) fetchJson(p.files.face).then(setFace);
      tool("build", { project: id }).then(setReport).catch((e) => setReport({ warnings: [String(e.message)] }));
      setErr("");
    } catch (e: any) {
      setErr(e.message);
    }
  }, [id]);
  useEffect(() => {
    load();
  }, [load]);
  // Changes made from the terminal or by an agent show up at once.
  const lastEv = useRef(0);
  useEffect(() => {
    const ev = act.events.filter((e) => e.project === id && e.origin !== "app" && e.ts > lastEv.current);
    if (ev.length) {
      lastEv.current = ev[ev.length - 1].ts;
      load();
    }
  }, [act.events, id, load]);
  useEffect(() => {
    if (proj && !proj.transcript && tab === "script") setTab("clips");
  }, [!!proj]);

  const reel: Json = info?.reel || {};
  const rid: string = info?.id || "main";
  const segs: Seg[] = info?.timeline || [];
  tNow.current = t;
  const L = useLayers("reel_layers", { project: id, reel: rid }, (items) => {
    const w = ovl.current?.contentWindow as any;
    if (w?.VH2D) w.VH2D.set(items).then(() => w.renderOut?.(tNow.current));
  });
  const total: number = info?.seconds || 0;
  const fps = reel.fps || 30;
  const media = proj?.media || {};

  const segIndex = useCallback((x: number) => {
    for (let i = 0; i < segs.length; i++) if (x < segs[i].edit + (segs[i].b - segs[i].a) + segs[i].hold) return i;
    return Math.max(0, segs.length - 1);
  }, [segs]);
  const toSrc = useCallback((x: number) => {
    const s = segs[segIndex(x)];
    return s ? Math.min(s.b - 0.02, s.a + Math.max(0, x - s.edit)) : 0;
  }, [segs, segIndex]);
  const seek = useCallback((x: number) => {
    x = Math.max(0, Math.min(total, x));
    cur.current = segIndex(x);
    if (video.current) video.current.currentTime = toSrc(x);
    setT(x);
  }, [total, segIndex, toSrc]);
  useEffect(() => {
    if (!playing) return;
    const v = video.current;
    if (!v || !segs.length) return;
    cur.current = segIndex(t >= total - 0.05 ? 0 : t);
    v.currentTime = toSrc(t >= total - 0.05 ? 0 : t);
    v.play().catch(() => setPlaying(false));
    let raf = 0;
    const loop = () => {
      const s = segs[cur.current];
      if (!s) return setPlaying(false);
      if (v.currentTime >= s.b - 0.015 || v.ended) {
        const n = segs[cur.current + 1];
        if (!n) {
          setT(total);
          return setPlaying(false);
        }
        cur.current += 1;
        if (Math.abs(n.a - v.currentTime) > 0.06) v.currentTime = n.a;
      } else setT(s.edit + Math.max(0, v.currentTime - s.a));
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => {
      cancelAnimationFrame(raf);
      v.pause();
    };
    // eslint-disable-next-line
  }, [playing, ver]);
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      const el = e.target as HTMLElement;
      if (/INPUT|TEXTAREA|SELECT/.test(el.tagName)) return;
      if ((e.ctrlKey || e.metaKey) && (e.key === "z" || e.key === "Z" || e.key === "y" || e.key === "Y")) {
        e.preventDefault();
        const redo = e.key === "y" || e.key === "Y" || e.shiftKey;
        tool("reel_undo", { project: id, reel: rid, ...(redo ? { redo: true } : {}) }).then(load).catch((er) => say(er.message));
        return;
      }
      if (e.code === "Space") {
        e.preventDefault();
        if (aud) { video.current?.pause(); setAud(null); return; }
        setPlaying((p) => !p);
      } else if (e.code === "ArrowRight") seek(t + (e.shiftKey ? 1 : 1 / fps));
      else if (e.code === "ArrowLeft") seek(t - (e.shiftKey ? 1 : 1 / fps));
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [seek, t, fps, rid, aud, load]);

  const run = async (label: string, name: string, args: Json = {}) => {
    setBusy({ label, p: 0 });
    try {
      const r = await runJob(name, { project: id, ...args }, (p) => setBusy({ label, p }));
      await load();
      setBusy(null);
      return r;
    } catch (e: any) {
      say(e.message);
      setBusy(null);
      await load();
      return null;
    }
  };
  const patch = (p: Json) => run(tx("Saving"), "reel_set", { patch: p, check: false });
  // Instant edits: a cut shows at once in the text, the timeline and the player, and is saved behind it.
  const saving = useRef(0);
  const setTakes = (next: Take[]) => {
    const n = ++saving.current;
    setInfo((prev: Json) => {
      if (!prev) return prev;
      let edit = 0;
      const timeline = next.map((k) => { const sp = k.speed || 1; const s = { a: k.a, b: k.b, edit, hold: k.hold || 0, speed: sp }; edit += (k.b - k.a) / sp + (k.hold || 0); return s; });
      return { ...prev, reel: { ...prev.reel, takes: next }, timeline, seconds: Math.round((edit + (prev.reel?.outro || 0)) * 100) / 100 };
    });
    tool("reel_set", { project: id, reel: rid, patch: { takes: next }, check: false })
      .then(() => { if (n === saving.current) tool("build", { project: id }).then(setReport).catch(() => {}); })
      .catch((e) => { say(e.message); load(); });
  };
  const takes: Take[] = useMemo(() => (reel.takes?.length ? reel.takes : reel.clip ? [{ a: reel.clip.a, b: reel.clip.b }] : [{ a: 0, b: media.duration || 0 }]), [reel, media.duration]);
  const origNow = toSrc(t);

  if (err) return <div className="page"><button className="btn ghost" onClick={back}><ArrowLeft />{tx("Projects")}</button><p className="err" style={{ marginTop: 16 }}>{err}</p></div>;
  if (!proj || !info) return <div className="page"><span className="muted">{tx("Loading")}</span></div>;
  const job = act.jobs.find((j) => j.project === id && (j.status === "running" || j.status === "queued"));
  const openNotes = (proj.notes || []).filter((n: Json) => n.status === "open" && n.reel === rid);
  const r2x = (x: number) => Math.round(x * 100) / 100;
  const toEdit = (x: number) => {
    const sg: any = segs.find((g) => x >= g.a - 1e-3 && x <= g.b + 1e-3);
    return sg ? sg.edit + (x - sg.a) / (sg.speed || 1) : null;
  };
  const beatLabel = (b: Json) => b?.text || b?.title || b?.word || b?.emoji || b?.key || b?.kind || "";
  const lanes: Lane[] = [
    {
      key: "beats", label: tx("Text and cards"), color: "var(--accent)", sel: selTrack,
      items: (report?.beats || []).map((b: Json) => ({ id: `b${b.beat}`, s: b.at, e: b.to, label: `${beatLabel(reel.beats?.[b.beat]) || b.type}` })),
      onSelect: (id) => { setSelTrack(id); setTab("beats"); },
      onChange: (id, s, e) => { const i = +id.slice(1); const bs = [...(reel.beats || [])]; bs[i] = { ...bs[i], at: r2x(toSrc(s)), to: r2x(toSrc(e)) }; patch({ beats: bs }); },
      onDelete: (id) => { const i = +id.slice(1); patch({ beats: (reel.beats || []).filter((_: Json, j: number) => j !== i) }); setSelTrack(""); },
    },
    {
      key: "sfx", label: tx("Sounds"), color: "#7fa7d9", sel: selTrack,
      items: [
        ...((reel.sfx || []).map((x: Json, i: number) => { const s = typeof x.at === "number" ? toEdit(x.at) : null; return s === null ? null : { id: `s${i}`, s, e: s + 0.6, label: x.id }; }).filter(Boolean) as TItem[]),
        ...(report?.sfx || []).filter((x: Json) => x.why !== "placed by hand").map((x: Json, i: number) => ({ id: `d${i}`, s: x.t, e: x.t, label: `${x.id} (${x.why})`, tick: true, locked: true })),
      ],
      onSelect: setSelTrack,
      onChange: (id, s) => { const i = +id.slice(1); const sf = [...(reel.sfx || [])]; sf[i] = { ...sf[i], at: r2x(toSrc(s)) }; patch({ sfx: sf }); },
      onDelete: (id) => { const i = +id.slice(1); patch({ sfx: (reel.sfx || []).filter((_: Json, j: number) => j !== i) }); setSelTrack(""); },
    },
    {
      key: "layers", label: tx("Layers"), color: "#86b991", sel: selTrack,
      items: L.items.map((it) => ({ id: `l:${it.id}`, s: it.t0, e: it.t1, label: it.text || it.title || it.kind })),
      onSelect: (id) => { setSelTrack(id); setSel2d(id.slice(2)); setTab("layers"); },
      onChange: (id, s, e) => L.update(L.items.map((it) => (`l:${it.id}` === id ? { ...it, t0: s, t1: e } : it))),
      onDelete: (id) => { L.update(L.items.filter((it) => `l:${it.id}` !== id)); setSelTrack(""); },
    },
  ];
  const addTitle = () => patch({ beats: [...(reel.beats || []), { type: "title", text: tx("NEW TEXT"), at: r2x(origNow), to: r2x(origNow + 2.5), size: 120, y: 300, anim: "pop" }] }).then(() => setTab("beats"));
  const addSound = (sid: string) => sid && patch({ sfx: [...(reel.sfx || []), { at: r2x(origNow), id: sid }] });

  return (
    <div className="ed">
      <div className="ed-top">
        <button className="btn icon ghost" onClick={back} title={tx("Projects")}><ArrowLeft /></button>
        <span className="title">{proj.title}</span>
        <select className="select" style={{ width: 220 }} value={rid} onChange={(e) => tool("project", { project: id, set: { active_reel: e.target.value } }).then(load)}>
          {proj.reels.map((r: Json) => (
            <option key={r.id} value={r.id}>{r.id === "main" ? tx("Main edit") : r.title || r.id} · {dur(r.seconds)}</option>
          ))}
        </select>
        <span className="grow" />
        {(busy || job) && (
          <div className="rowx" style={{ width: 240 }}>
            <span className="sec" style={{ fontSize: 12, whiteSpace: "nowrap" }}>{busy ? busy.label : tr(job.message) || tr(job.kind)}</span>
            <div className="meter grow"><i style={{ width: `${(busy ? busy.p : job.progress || 0) * 100}%` }} /></div>
          </div>
        )}
        {!proj.transcript && proj.media?.has_audio && <button className="btn primary" disabled={!!busy} onClick={() => run(tx("Transcribing"), "transcribe")}>{tx("Transcribe")}</button>}
      </div>

      <div className="ed-left">
        <div className="tabs">
          {([["clips", tx("Clips")], ["script", tx("Transcript")], ["style", tx("Style")], ["beats", tx("Beats")], ["layers", tx("Layers")], ["notes", tx("Notes{0}", openNotes.length ? ` ${openNotes.length}` : "")]] as const).map(([k, l]) => (
            <button key={k} className={tab === k ? "on" : ""} onClick={() => setTab(k)}>{l}</button>
          ))}
        </div>
        <div className="tab-body">
          {tab === "clips" && <ClipsPanel proj={proj} rid={rid} run={run} busy={!!busy} reload={load} say={say} audition={audition} />}
          {tab === "script" && <ScriptPanel words={words} reel={reel} takes={takes} full={reel.clip ? [{ a: reel.clip.a, b: reel.clip.b }] : [{ a: 0, b: media.duration || 0 }]} segs={segs} t={t} seek={seek} setTakes={setTakes} run={run} hasWords={!!proj.transcript} busy={!!busy} audition={audition} playing={playing} say={say} />}
          {tab === "style" && <StylePanel reel={reel} report={report} run={run} patch={patch} />}
          {tab === "beats" && <BeatsPanel reel={reel} report={report} origNow={origNow} t={t} seek={seek} run={run} patch={patch} sel={selTrack} setSel={setSelTrack} />}
          {tab === "layers" && <LayersPanel L={L} t={t} W={reel.width || 1080} H={reel.height || 1920} dur={total} sel={sel2d} setSel={setSel2d} />}
          {tab === "notes" && <NotesPanel proj={proj} rid={rid} t={t} seek={seek} reload={load} say={say} />}
        </div>
      </div>

      <Stage proj={proj} reel={reel} rid={rid} ver={ver} t={t} total={total} fps={fps} playing={playing || !!aud} setPlaying={(x: Json) => { if (aud) { video.current?.pause(); setAud(null); } setPlaying(x); }} video={video} segs={segs} words={words} face={face} origNow={origNow} report={report} L={L} ovl={ovl} layersMode={tab === "layers"} sel2d={sel2d} setSel2d={setSel2d} />

      <div className="ed-right">
        <Inspector proj={proj} reel={reel} rid={rid} total={total} takes={takes} selTake={selTake} setSelTake={setSelTake} report={report} patch={patch} run={run} busy={!!busy} say={say} reload={load} />
      </div>

      <div className="dock">
        <div style={{ position: "relative" }}>
          <div className="rowx" style={{ position: "absolute", right: 16, top: 2, zIndex: 3 }}>
            <button className="btn sm" onClick={addTitle}>{tx("+ Text")}</button>
            <select className="select" style={{ height: 28, width: 150 }} value="" onChange={(e) => addSound(e.target.value)}>
              <option value="">{tx("+ Sound")}</option>
              {(lib.data?.sounds || []).map((x: Json) => <option key={x.id} value={x.id}>{x.id}</option>)}
            </select>
          </div>
          <Timeline segs={segs} total={total} t={t} seek={seek} peaks={peaks} sel={selTake} setSel={setSelTake} beats={report?.beats || []} notes={openNotes} reel={reel} setPlaying={setPlaying} lanes={lanes} />
        </div>
        <Console id={id} act={act} reload={load} />
      </div>
    </div>
  );
}

function Stage({ proj, reel, rid, ver, t, total, fps, playing, setPlaying, video, segs, words, face, origNow, report, L, ovl, layersMode, sel2d, setSel2d }: Json) {
  useEffect(() => {
    const w = ovl.current?.contentWindow as any;
    try { w?.renderOut?.(t); } catch { /* the page is loading */ }
  }, [t, L.url]);
  const [exact, setExact] = useState<{ t: number; url: string; ver: number } | null>(null);
  const [exactErr, setExactErr] = useState("");
  const w = reel.width || 1080;
  const h = reel.height || 1920;
  // The exact frame, a moment after the playhead rests.
  useEffect(() => {
    if (playing || !proj.files.proxy) return;
    const h = setTimeout(async () => {
      try {
        const r = await tool("snapshot", { project: proj.id, reel: rid, t });
        setExact({ t, url: fileUrl(r.file, Date.now()), ver });
        setExactErr("");
      } catch (e: any) {
        setExactErr(String(e.message).split("\n")[0]);
      }
    }, 320);
    return () => clearTimeout(h);
  }, [t, playing, ver, rid]);
  const showExact = !playing && !layersMode && exact && exact.ver === ver && Math.abs(exact.t - t) < 0.001;
  // Approximate framing while playing: follow the face like the render does.
  const fx = useMemo(() => {
    if (reel.crop?.mode === "center") return 0.5;
    if (typeof reel.crop?.x === "number") return reel.crop.x;
    const xs = (face?.samples || []).filter((s: number[]) => Math.abs(s[0] - origNow) < 4).map((s: number[]) => s[1]).sort();
    return xs.length ? xs[Math.floor(xs.length / 2)] : 0.5;
  }, [face, Math.round(origNow / 2), reel.crop]);
  const cap = useMemo(() => {
    if (!playing || reel.captions?.style === "none") return null;
    const g = reel.captions?.group || 3;
    const s = segs.find((x: Seg) => origNow >= x.a && origNow <= x.b);
    if (!s) return null;
    const ws = words.filter((x: Word) => (x.s + x.e) / 2 >= s.a && (x.s + x.e) / 2 <= s.b);
    const i = ws.findIndex((x: Word) => origNow < x.e + 0.25);
    if (i < 0 || ws[i].s - origNow > 0.4) return null;
    const b = Math.floor(i / g) * g;
    return ws.slice(b, b + g).map((x: Word, k: number) => ({ text: x.w.replace(/[.,;:]$/, ""), on: b + k === i }));
  }, [playing, Math.round(origNow * 12), words, segs]);
  const capY = report?.captions?.[0]?.match?.(/./) ? 62 : 62;
  return (
    <div className="stage">
      <div style={{ flex: 1, minHeight: 0, display: "flex", alignItems: "center", justifyContent: "center", width: "100%" }}>
        <div className="monitor" style={{ aspectRatio: `${w} / ${h}`, containerType: "size" } as any}>
          {proj.files.proxy ? (
            <video ref={video} src={fileUrl(proj.files.proxy)} playsInline preload="auto" style={{ objectFit: reel.crop?.mode === "fit" ? "contain" : "cover", objectPosition: `${fx * 100}% 40%` }} />
          ) : (
            <div className="muted" style={{ position: "absolute", inset: 0, display: "grid", placeItems: "center", color: "#888" }}>{tx("Preparing the preview")}</div>
          )}
          {showExact && <img className="exact" src={exact!.url} alt="" />}
          {L.url && (
            <ScaledStage W={w} H={h}>
              <iframe ref={ovl} src={`${L.url}?v=${ver}`} title="layers" onLoad={() => { const x = ovl.current?.contentWindow as any; const wait = () => (x?.READY ? x.renderOut?.(t) : setTimeout(wait, 80)); wait(); }}
                style={{ position: "absolute", inset: 0, width: w, height: h, border: 0, background: "transparent", pointerEvents: "none", visibility: showExact ? "hidden" : "visible" }} />
              {layersMode && !playing && <LayerOverlay L={L} t={t} sel={sel2d} setSel={setSel2d} W={w} H={h} />}
            </ScaledStage>
          )}
          {cap && (
            <div className="cap" style={{ top: `${capY}%`, fontSize: "3.3cqh", ["--cap-accent" as any]: reel.brand?.accent || "#FFD23F" }}>
              {cap.map((c: Json, i: number) => (c.on ? <b key={i}>{c.text} </b> : <span key={i}>{c.text} </span>))}
            </div>
          )}
          {playing && <span className="badge">{tx("Approximate preview")}</span>}
          {!playing && exactErr && <span className="badge" style={{ right: 8, whiteSpace: "normal" }}>{exactErr}</span>}
        </div>
      </div>
      <div className="transport">
        <button className="btn icon" onClick={() => setPlaying(!playing)} title={tx("Play / pause")}>{playing ? <Pause /> : <Play />}</button>
        <span className="tc"><b>{tc(t, fps)}</b> <span>/ {tc(total, fps)}</span></span>
        <span className="muted" style={{ fontSize: 12 }}>{tx("Source")} {dur(origNow)}</span>
        <span className="kbd">{tx("Gap")}</span>
      </div>
    </div>
  );
}

function Timeline({ segs, total, t, seek, peaks, sel, setSel, beats, notes, reel, setPlaying, lanes }: Json) {
  const box = useRef<HTMLDivElement>(null);
  const cv = useRef<HTMLCanvasElement>(null);
  const [width, setWidth] = useState(800);
  useEffect(() => {
    const ro = new ResizeObserver((e) => setWidth(e[0].contentRect.width));
    if (box.current) ro.observe(box.current);
    return () => ro.disconnect();
  }, []);
  const pad = 16;
  const sx = (width - pad * 2) / Math.max(total, 0.1);
  useEffect(() => {
    const c = cv.current;
    if (!c) return;
    const dpr = window.devicePixelRatio || 1;
    c.width = width * dpr;
    c.height = 132 * dpr;
    const g = c.getContext("2d")!;
    g.scale(dpr, dpr);
    const css = getComputedStyle(document.documentElement);
    const v = (n: string) => css.getPropertyValue(n).trim();
    g.clearRect(0, 0, width, 132);
    // ruler
    const steps = [0.5, 1, 2, 5, 10, 15, 30, 60, 120, 300, 600];
    const step = steps.find((s) => s * sx >= 72) || 600;
    g.font = `10px ${v("--mono")}`;
    g.fillStyle = v("--text-3");
    g.strokeStyle = v("--line-2");
    for (let x = 0; x <= total; x += step) {
      const px = pad + x * sx;
      g.beginPath();
      g.moveTo(px + 0.5, 14);
      g.lineTo(px + 0.5, 20);
      g.stroke();
      const m = Math.floor(x / 60), s = x % 60;
      g.fillText(`${m}:${String(Math.floor(s)).padStart(2, "0")}${step < 1 && s % 1 ? ".5" : ""}`, px + 4, 12);
    }
    // takes with the voice inside them
    const top = 28, hgt = 60;
    segs.forEach((s: Seg, i: number) => {
      const x0 = pad + s.edit * sx, w = (s.b - s.a) * sx;
      g.fillStyle = v("--take");
      g.beginPath();
      g.roundRect(x0, top, Math.max(1, w - 1), hgt, 5);
      g.fill();
      if (peaks?.peaks) {
        g.fillStyle = i === sel ? v("--text-2") : v("--wave");
        for (let px = 1; px < w - 1; px += 2) {
          const pk = peaks.peaks[Math.floor((s.a + px / sx) * peaks.rate)] || 0;
          const bh = Math.max(1, (pk / 255) * (hgt - 12));
          g.fillRect(x0 + px, top + hgt / 2 - bh / 2, 1, bh);
        }
      }
      if (s.hold > 0) {
        g.fillStyle = v("--line");
        g.fillRect(x0 + w, top + 8, s.hold * sx, hgt - 16);
      }
      if (i === sel) {
        g.strokeStyle = v("--accent");
        g.lineWidth = 1.5;
        g.beginPath();
        g.roundRect(x0 + 0.75, top + 0.75, Math.max(1, w - 2.5), hgt - 1.5, 5);
        g.stroke();
        g.lineWidth = 1;
      }
      // the cut mark: a grease-pencil tick where two takes meet
      if (i > 0) {
        g.strokeStyle = v("--accent");
        g.lineWidth = 2;
        g.lineCap = "round";
        g.beginPath();
        g.moveTo(x0 + 2.5, top - 4);
        g.lineTo(x0 - 3.5, top + 10);
        g.stroke();
        g.lineWidth = 1;
      }
    });
  }, [segs, total, width, peaks, sel, sx]);
  const at = (e: React.MouseEvent | MouseEvent) => {
    const r = box.current!.getBoundingClientRect();
    return Math.max(0, Math.min(total, (e.clientX - r.left - pad) / sx));
  };
  const down = (e: React.MouseEvent) => {
    setPlaying(false);
    const x = at(e);
    seek(x);
    const i = segs.findIndex((s: Seg) => x >= s.edit && x < s.edit + (s.b - s.a) + s.hold);
    setSel(i === sel ? -1 : i);
    const move = (ev: MouseEvent) => seek(at(ev));
    const up = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  };
  return (
    <div className="tl" ref={box} onMouseDown={down} style={{ height: 98 + (lanes?.length || 0) * LANE_H + 6 }}>
      <canvas ref={cv} style={{ height: 132 }} />
      {(lanes || []).map((ln: Lane, k: number) => <TrackLane key={ln.key} lane={ln} top={98 + k * LANE_H} sx={sx} pad={pad} total={total} />)}
      {notes.map((n: Json) => (
        <span key={n.id} className="marker note" style={{ left: pad + n.t * sx, top: 72 }} title={n.text}>{n.text}</span>
      ))}
      <div className="playhead" style={{ left: pad + t * sx }} />
    </div>
  );
}

function Console({ id, act, reload }: { id: string; act: Activity; reload: () => void }) {
  const [open, setOpen] = useState(false);
  const [line, setLine] = useState("");
  const [local, setLocal] = useState<Json[]>([]);
  const end = useRef<HTMLDivElement>(null);
  const rows = useMemo(() => [...act.events.filter((e) => !e.project || e.project === id), ...local].sort((a, b) => a.ts - b.ts).slice(-120), [act.events, local, id]);
  useEffect(() => end.current?.scrollIntoView(), [rows.length, open]);
  const who: Record<string, string> = { app: tx("app"), terminal: tx("terminal"), agent: tx("agent"), assistant: tx("assistant"), konsol: tx("console") };
  const submit = async () => {
    if (!line.trim()) return;
    const cmd = line;
    setLine("");
    try {
      const [name, args] = parseCommand(cmd);
      const out = await runJob(name, { project: id, _origin: "terminal", ...args });
      setLocal((l) => [...l, { ts: Date.now(), origin: "konsol", summary: cmd, ok: true, out: JSON.stringify(out, null, 1).slice(0, 6000) }]);
      reload();
    } catch (e: any) {
      setLocal((l) => [...l, { ts: Date.now(), origin: "konsol", summary: cmd, ok: false, out: e.message }]);
    }
  };
  const last = rows[rows.length - 1];
  const time = (ts: number) => new Date(ts).toLocaleTimeString("tr-TR", { hour12: false });
  return (
    <div className="console">
      <div className="console-bar" onClick={() => setOpen(!open)}>
        {open ? <ChevronDown size={14} /> : <ChevronUp size={14} />}
        <span>{tx("Console")}</span>
        <span className="grow">{!open && last ? `${who[last.origin] || last.origin}  ${last.summary}` : ""}</span>
        <span className="muted">{tx("App, terminal and agent use the same commands")}</span>
      </div>
      {open && (
        <>
          <div className="console-log">
            {rows.map((e, i) => (
              <div key={i}>
                <div className={`ln ${e.origin} ${e.ok ? "" : "bad"}`}>
                  <span className="muted">{time(e.ts)}</span>
                  <span className="who">{who[e.origin] || e.origin}</span>
                  <span className="what">{e.summary}</span>
                </div>
                {e.out && <pre>{e.out}</pre>}
              </div>
            ))}
            <div ref={end} />
          </div>
          <div className="console-in">
            <span>›</span>
            <input autoFocus placeholder={tx("autocut min_pause=0.4 · style_apply captions=hype · beat_add beat={…} · for tools: styles")} value={line} onChange={(e) => setLine(e.target.value)} onKeyDown={(e) => e.key === "Enter" && submit()} />
          </div>
        </>
      )}
    </div>
  );
}

function ClipsPanel({ proj, rid, run, busy, reload, say, audition }: Json) {
  const [count, setCount] = useState(6);
  const [min, setMin] = useState(20);
  const [max, setMax] = useState(60);
  const [ai, setAi] = useState(false);
  const [hook, setHook] = useState(true);
  const clips: Json[] = proj.clips || [];
  return (
    <div className="stack lg">
      <div className="stack">
        <h2>{tx("Edits")}</h2>
        <div>
          {proj.reels.map((r: Json) => (
            <button key={r.id} className={`opt ${r.id === rid ? "on" : ""}`} onClick={() => tool("project", { project: proj.id, set: { active_reel: r.id } }).then(reload)}>
              <span className="grow"><b>{r.id === "main" ? tx("Main edit") : r.title || r.id}</b><span>{r.takes || 1} {tx("cuts ·")} {r.style}</span></span>
              <span className="num sec">{dur(r.seconds)}</span>
              {r.id !== "main" && <span className="row-acts"><CardActions actions={[{ icon: Pencil, label: tx("Rename"), run: () => renameItem("reel", r.id, r.title || r.id, { project: proj.id }, say, reload) }, { icon: Trash2, label: tx("Delete"), danger: true, run: () => deleteItem("reel", r.id, r.title || r.id, { project: proj.id }, say, reload) }]} /></span>}
            </button>
          ))}
        </div>
      </div>
      <div className="stack">
        <h2>{tx("Highlights")}</h2>
        {!proj.transcript ? (
          <p className="sec">{tx("Transcribe the speech first to find clips.")}</p>
        ) : (
          <>
            <div className="grid2" style={{ gridTemplateColumns: "1fr 1fr 1fr" }}>
              <div className="field"><label>{tx("Count")}</label><input className="input num" type="number" value={count} onChange={(e) => setCount(+e.target.value)} /></div>
              <div className="field"><label>{tx("Shortest, s")}</label><input className="input num" type="number" value={min} onChange={(e) => setMin(+e.target.value)} /></div>
              <div className="field"><label>{tx("Longest, s")}</label><input className="input num" type="number" value={max} onChange={(e) => setMax(+e.target.value)} /></div>
            </div>
            <label className="rowx sec" style={{ fontSize: 12 }}><input type="checkbox" checked={ai} onChange={(e) => setAi(e.target.checked)} />{tx("Let the assistant choose")}</label>
            <label className="rowx sec" style={{ fontSize: 12 }}><input type="checkbox" checked={hook} onChange={(e) => setHook(e.target.checked)} />{tx("Put the strongest moment at the start as a hook when making a reel")}</label>
            <div className="rowx">
              <button className="btn sm" disabled={busy} title={tx("Audio energy, pitch change and motion")} onClick={() => run(tx("Reading the cues"), "signals")}>{tx("Audio and motion cues")}</button>
              <button className="btn sm" disabled={busy} title={tx("A vision model scores one frame per second")} onClick={() => run(tx("Scoring frames"), "vision_score")}>{tx("Visual score")}</button>
            </div>
            <button className="btn" disabled={busy} onClick={() => run(tx("Finding clips"), "clips_find", { count, min, max, ai })}>{tx("Find clips")}</button>
          </>
        )}
        {clips.map((c) => (
          <div key={c.id} className="card clip">
            <div className="rowx">
              <span className="score">{c.score}</span>
              <b className="grow" style={{ fontWeight: 500 }}>{c.title}</b>
            </div>
            <div className="rowx muted num" style={{ fontSize: 12 }}>{dur(c.a)} – {dur(c.b)} · {Math.round(c.dur)} {tx("s")}</div>
            {c.breakdown && (
              <div style={{ display: "grid", gridTemplateColumns: "repeat(5, 1fr)", gap: 6 }}>
                {[["hook", tx("Hook")], ["flow", tx("Flow")], ["value", tx("Value")], ["emotion", tx("Emotion")], ["trend", "Trend"]].map(([k, l]) => (
                  <div key={k} title={c.breakdown[k] == null ? tx("Trend is scored only by the assistant") : ""}>
                    <div className="muted" style={{ fontSize: 10 }}>{l} <span className="num">{c.breakdown[k] ?? "–"}</span></div>
                    <div className="meter"><i style={{ width: `${c.breakdown[k] ?? 0}%` }} /></div>
                  </div>
                ))}
              </div>
            )}
            <div className="sec" style={{ fontSize: 12, lineHeight: "18px", display: "-webkit-box", WebkitLineClamp: 3, WebkitBoxOrient: "vertical", overflow: "hidden" }}>{c.text}</div>
            <div className="rowx">
              <span className="grow muted" style={{ fontSize: 12 }}>{(c.why || []).slice(0, 2).join(" · ")}</span>
              <button className="btn sm ghost" title={tx("Listen to this moment in the source")} onClick={() => audition(c.a, c.b)}><Play />{tx("Play")}</button>
              <button className="btn sm" disabled={busy} onClick={() => run(tx("Making the clip"), "clip_make", { clip: c.id, prepend_hook: hook })}><Scissors />{tx("Make reel")}</button>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

// Hesitations that carry no meaning, in English and Turkish.
const FILLERS = new Set(["um", "umm", "uh", "uhh", "uhm", "erm", "er", "hmm", "mm", "mhm", "ııı", "ıı", "ı", "eee", "ee", "eh", "ah", "ıh"]);
const norm = (s: string) => s.toLocaleLowerCase().replace(/[.,!?;:"'“”‘’()…-]+/g, "");

/** Edit through the text: drag across words to select, Delete cuts (or restores), P plays the selection.
 *  Every change shows at once; it is saved behind the scenes. */
function ScriptPanel({ words, reel, takes, full, segs, t, seek, setTakes, run, hasWords, busy, audition, playing, say }: Json) {
  const [sel, setSel] = useState<[number, number] | null>(null);
  const [q, setQ] = useState("");
  const [maxPause, setMaxPause] = useState(0.3);
  const drag = useRef<number | null>(null);
  const scope: [number, number] = reel.clip ? [reel.clip.a - 1.5, reel.clip.b + 1.5] : [0, Infinity];
  const list: [Word, number][] = useMemo(() => words.map((w: Word, i: number) => [w, i] as [Word, number]).filter(([w]: [Word, number]) => w.e >= scope[0] && w.s <= scope[1]).slice(0, 20000), [words, scope[0], scope[1]]);
  // a word the recogniser gave no length lasts until the next one starts
  const ends: number[] = useMemo(() => words.map((w: Word, i: number) => (w.e - w.s < 0.03 ? Math.max(w.e, Math.min(words[i + 1]?.s ?? w.e + 0.25, w.s + 0.6)) : w.e)), [words]);
  const inTakes = (x: number) => takes.some((k: Take) => x >= k.a && x <= k.b);
  const kept = (i: number) => inTakes((words[i].s + ends[i]) / 2);
  const editOf = (x: number) => {
    const s = segs.find((g: Seg) => x >= g.a - 0.001 && x <= g.b + 0.001);
    return s ? s.edit + (x - s.a) : null;
  };
  const src = segs.find((g: Seg) => t >= g.edit && t <= g.edit + (g.b - g.a));
  const now = src ? src.a + (t - src.edit) : -1;
  const rangeOf = (s: [number, number]): [number, number] => [words[s[0]].s - 0.04, ends[s[1]] + 0.06];
  const range = sel ? rangeOf(sel) : null;
  const selAllCut = sel ? Array.from({ length: sel[1] - sel[0] + 1 }, (_, k) => sel[0] + k).every((i) => !kept(i)) : false;
  const cutOrRestore = () => {
    if (!range) return;
    setTakes(selAllCut ? addRange(takes, range[0], range[1]) : removeRange(takes, range[0], range[1]));
    setSel(null);
  };
  // search: a word or a phrase, matched word by word
  const hits: [number, number][] = useMemo(() => {
    const toks = q.trim().split(/\s+/).map(norm).filter(Boolean);
    if (!toks.length) return [];
    const out: [number, number][] = [];
    for (let k = 0; k + toks.length <= list.length; k++) {
      if (toks.every((tk, j) => (j === toks.length - 1 ? norm(list[k + j][0].w).startsWith(tk) : norm(list[k + j][0].w) === tk))) out.push([list[k][1], list[k + toks.length - 1][1]]);
    }
    return out;
  }, [q, list]);
  const hitSet = useMemo(() => { const s = new Set<number>(); hits.forEach(([a, b]) => { for (let i = a; i <= b; i++) s.add(i); }); return s; }, [hits]);
  const fillers = useMemo(() => list.filter(([w, i]) => FILLERS.has(norm(w.w)) && kept(i)).map(([, i]) => i), [list, takes]);
  const cutMany = (ranges: [number, number][]) => {
    let next = takes;
    for (const r of ranges) { const [a, b] = rangeOf(r); next = removeRange(next, a, b); }
    setTakes(next);
  };
  // pauses longer than this get a chip that cuts (or brings back) the silence
  const gapAfter = (k: number): [number, number] | null => {
    const cur = list[k], nx = list[k + 1];
    if (!nx) return null;
    const a = ends[cur[1]] + 0.05, b = nx[0].s - 0.05;
    return b - a >= 0.5 ? [a, b] : null;
  };
  // every pause longer than maxPause is shortened to it (cut from its middle, so breaths stay natural)
  const tighten = () => {
    let next = takes;
    for (let k = 0; k < list.length - 1; k++) {
      const a = ends[list[k][1]], b = list[k + 1][0].s;
      if (b - a > maxPause + 0.05 && kept(list[k][1]) && kept(list[k + 1][1])) next = removeRange(next, a + maxPause / 2, b - maxPause / 2);
    }
    setTakes(next);
  };
  const copyText = () => {
    const text = list.filter(([, i]) => kept(i)).map(([w]) => w.w).join(" ");
    navigator.clipboard.writeText(text).then(() => say(tx("The kept text was copied"))).catch((e: any) => say(e.message));
  };
  const keptSec = takes.reduce((s: number, k: Take) => s + (k.b - k.a) / (k.speed || 1), 0);
  const fullSec = full.reduce((s: number, k: Take) => s + (k.b - k.a), 0);

  useEffect(() => {
    const up = () => { drag.current = null; };
    window.addEventListener("mouseup", up);
    return () => window.removeEventListener("mouseup", up);
  }, []);
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (/INPUT|TEXTAREA|SELECT/.test((e.target as HTMLElement).tagName) || !sel) return;
      if (e.key === "Delete" || e.key === "Backspace") { e.preventDefault(); cutOrRestore(); }
      else if (e.key === "Escape") setSel(null);
      else if ((e.key === "p" || e.key === "P") && range) { e.preventDefault(); audition(range[0], range[1]); }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  });
  const down = (i: number, e: React.MouseEvent) => {
    if (e.button !== 0) return;
    e.preventDefault();
    if (e.shiftKey) { setSel((s) => (s ? [Math.min(s[0], i), Math.max(s[1], i)] : [i, i])); return; }
    drag.current = i;
    setSel([i, i]);
    const x = editOf(words[i].s);
    if (x !== null && !playing) seek(x);
  };
  const enter = (i: number) => { if (drag.current !== null) setSel([Math.min(drag.current, i), Math.max(drag.current, i)]); };

  if (!hasWords) return <p className="sec">{tx("The text appears here once the speech is transcribed. Click words to edit through the text.")}</p>;
  const paras: number[][] = [];
  let p: number[] = [];
  list.forEach((x, k) => {
    const prev = list[k - 1];
    if (p.length && prev && (x[0].s - ends[prev[1]] > 1.1 || (p.length > 44 && /[.?!]$/.test(prev[0].w)))) { paras.push(p); p = []; }
    p.push(k);
  });
  if (p.length) paras.push(p);
  return (
    <div className="stack">
      <div className="rowx" style={{ flexWrap: "wrap" }}>
        <button className="btn sm" disabled={busy} onClick={() => run(tx("Cutting pauses"), "autocut")}>{tx("Cut pauses")}</button>
        <button className="btn sm" disabled={busy} onClick={() => run(tx("Removing retakes"), "retakes", reel.clip ? { from: reel.clip.a, to: reel.clip.b } : {})}>{tx("Remove retakes")}</button>
        <button className="btn sm" disabled={!fillers.length} title={tx("um, uh, eee…")} onClick={() => cutMany(fillers.map((i) => [i, i] as [number, number]))}>{tx("Cut fillers")} <span className="num muted">{fillers.length}</span></button>
        <button className="btn sm ghost" onClick={() => { setTakes(full); setSel(null); }}><Undo2 />{tx("Reset")}</button>
      </div>
      <div className="rowx" style={{ flexWrap: "wrap" }}>
        <span className="sec" style={{ fontSize: 12 }}>{tx("Shorten pauses to")}</span>
        <input className="input num" style={{ width: 64, height: 28 }} type="number" min={0.05} max={2} step={0.05} value={maxPause} onChange={(e) => setMaxPause(Math.max(0.05, +e.target.value || 0.3))} />
        <span className="sec" style={{ fontSize: 12 }}>{tx("s")}</span>
        <button className="btn sm" onClick={tighten}>{tx("Tighten")}</button>
        <span className="grow" />
        <button className="btn sm ghost" title={tx("Copy the text that stays in the edit")} onClick={copyText}><Copy />{tx("Copy text")}</button>
      </div>
      <div className="rowx">
        <input className="input" placeholder={tx("Find words in the text")} value={q} onChange={(e) => setQ(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter" && hits[0]) { setSel(hits[0]); const x = editOf(words[hits[0][0]].s); if (x !== null) seek(x); } }} />
        {q.trim() && <span className="num muted" style={{ fontSize: 12, whiteSpace: "nowrap" }}>{hits.length}</span>}
        {hits.length > 0 && <button className="btn sm" onClick={() => cutMany(hits)}><Scissors />{tx("Cut all")}</button>}
      </div>
      <div className="selbar">
        {sel && range ? (
          <>
            <span className="grow sec num" style={{ fontSize: 12 }}>{tx("{0} words · {1} s", sel[1] - sel[0] + 1, (range[1] - range[0]).toFixed(1))}</span>
            <button className="btn sm ghost" title={tx("Play the selection (P)")} onClick={() => audition(range[0], range[1])}><Play /></button>
            <button className="btn sm" onClick={cutOrRestore}>{selAllCut ? <><Undo2 />{tx("Restore")}</> : <><Scissors />{tx("Cut")}</>} <span className="kbd">Del</span></button>
            <button className="btn sm ghost" onClick={() => { setTakes([{ a: r2(range[0] - 0.08), b: r2(range[1] + 0.2) }]); setSel(null); }}>{tx("Keep only this")}</button>
            <button className="btn icon sm ghost" title={tx("Clear (Esc)")} onClick={() => setSel(null)}>×</button>
          </>
        ) : (
          <span className="grow muted" style={{ fontSize: 12 }}>{tx("{0} of {1} kept · drag across words to select", dur(keptSec), dur(fullSec))}</span>
        )}
      </div>
      <div className="script" onMouseLeave={() => { drag.current = null; }}>
        {paras.map((pp, n) => (
          <p key={n}>
            <span className="ts">{dur(list[pp[0]][0].s)}</span>
            {pp.map((k, m) => {
              const [w, i] = list[k];
              const kp = kept(i);
              const prevKept = m > 0 ? kept(list[pp[m - 1]][1]) : kp;
              const gap = m < pp.length - 1 ? gapAfter(k) : null;
              const gapKept = gap ? inTakes((gap[0] + gap[1]) / 2) : false;
              return (
                <span key={i}>
                  {kp !== prevKept && <span className="cutmark" />}
                  <span
                    className={`w ${kp ? "" : "cut"} ${sel && i >= sel[0] && i <= sel[1] ? "sel" : ""} ${hitSet.has(i) ? "hit" : ""} ${now >= w.s && now < ends[i] ? "now" : ""}`}
                    onMouseDown={(e) => down(i, e)}
                    onMouseOver={() => enter(i)}
                    onDoubleClick={() => { const v = window.prompt(tx("How it should read in captions"), w.w); if (v && v !== w.w) run(tx("Saving"), "reel_set", { patch: { captions: { fixes: { ...(reel.captions?.fixes || {}), [w.w.replace(/[.,!?;:]+$/, "")]: v } } }, check: false }); }}
                    title={tx("Double-click: fix how it reads in captions")}
                  >{w.w}</span>{" "}
                  {gap && <span className={`gap ${gapKept ? "" : "cut"}`} title={gapKept ? tx("Cut this pause") : tx("Bring the pause back")} onMouseDown={(e) => e.stopPropagation()} onClick={() => setTakes(gapKept ? removeRange(takes, gap[0], gap[1]) : addRange(takes, gap[0], gap[1]))}>{(gap[1] - gap[0] + 0.1).toFixed(1)}s</span>}
                  {gap && " "}
                </span>
              );
            })}
          </p>
        ))}
      </div>
      <div className="muted" style={{ fontSize: 12, lineHeight: "18px" }}>{tx("Drag or Shift-click to select · Del cuts or restores · P plays the selection · Ctrl+Z undoes · the grey chips are pauses: click to cut them")}</div>
    </div>
  );
}

function StylePanel({ reel, report, run, patch }: Json) {
  const [data, setData] = useState<Json | null>(null);
  useEffect(() => {
    tool("styles").then(setData);
  }, []);
  const size = `${reel.width || 1080}x${reel.height || 1920}`;
  const capNow = reel.captions?.style || report?.caption_style || "clean";
  const group = (kind: string, title: string) => {
    const rows = (data?.styles || []).filter((s: Json) => s.kind === kind);
    if (!rows.length) return null;
    return (
      <div className="stack" style={{ gap: 4 }}>
        <h2 style={{ marginBottom: 4 }}>{title}</h2>
        {rows.map((s: Json) => (
          <button key={s.id} className={`opt ${reel.style === s.id ? "on" : ""}`} onClick={() => run(tx("Applying the style"), "style_apply", { style: s.id })} title={s.grammar}>
            <span className="sw" style={{ background: s.accent, color: s.ink, borderColor: "transparent" }}>Aa</span>
            <span><b>{s.name}</b><span>{s.about}</span></span>
          </button>
        ))}
      </div>
    );
  };
  return (
    <div className="stack lg">
      <div className="stack">
        <div className="field">
          <label>{tx("Format")}</label>
          <div className="seg">
            {[["1080x1920", "9:16"], ["1080x1350", "4:5"], ["1080x1080", "1:1"], ["1920x1080", "16:9"]].map(([v, l]) => (
              <button key={v} className={size === v ? "on" : ""} onClick={() => patch({ width: +v.split("x")[0], height: +v.split("x")[1] })}>{l}</button>
            ))}
          </div>
        </div>
        <div className="field">
          <label>{tx("Framing")}</label>
          <div className="seg">
            {[["auto", tx("Follow the face")], ["speaker", tx("Speaker")], ["center", tx("Centre")], ["fit", tx("Fit")]].map(([v, l]) => (
              <button key={v} className={(reel.crop?.mode || "auto") === v ? "on" : ""} onClick={() => patch({ crop: { mode: v, x: null } })}>{l}</button>
            ))}
          </div>
        </div>
        <div className="field">
          <label>{tx("Tracking")}</label>
          <div className="seg">
            {[["face", tx("Face")], ["person", tx("Person")], ["off", tx("Still")]].map(([v, l]) => (
              <button key={v} className={(reel.crop?.track || "face") === v ? "on" : ""} onClick={() => patch({ crop: { track: v } })}>{l}</button>
            ))}
          </div>
        </div>
        <div className="field">
          <label>{tx("Safe area")}</label>
          <div className="seg">
            {[["instagram", "Instagram"], ["tiktok", "TikTok"], ["shorts", "Shorts"], ["all", tx("All")]].map(([v, l]) => (
              <button key={v} className={(reel.platform || "instagram") === v ? "on" : ""} onClick={() => patch({ platform: v })}>{l}</button>
            ))}
          </div>
        </div>
        <div className="grid2">
          <div className="field">
            <label>{tx("Caption template")}</label>
            <select className="select" value={capNow} onChange={(e) => run(tx("Changing captions"), "style_apply", { captions: e.target.value })}>
              {(data?.captions || []).map((c: Json) => <option key={c.id} value={c.id}>{c.label}</option>)}
            </select>
          </div>
          <div className="field">
            <label>{tx("Accent colour")}</label>
            <input className="input" type="color" style={{ padding: 2 }} value={reel.brand?.accent || (data?.styles || []).find((s: Json) => s.id === reel.style)?.accent || "#FFD23F"} onChange={(e) => (e.target.dataset.v = e.target.value)} onBlur={(e) => e.target.dataset.v && patch({ brand: { accent: e.target.dataset.v } })} />
          </div>
        </div>
        <div className="grid2">
          <div className="field">
            <label>{tx("Word group")}</label>
            <input className="input num" type="number" min={1} max={8} defaultValue={reel.captions?.group || ""} placeholder={tx("template")} onBlur={(e) => patch({ captions: { group: e.target.value ? +e.target.value : null } })} />
          </div>
          <div className="field">
            <label>{tx("Caption height")}</label>
            <input className="input num" type="number" step={20} defaultValue={reel.captions?.y || ""} placeholder={tx("template")} onBlur={(e) => patch({ captions: { y: e.target.value ? +e.target.value : null } })} />
          </div>
        </div>
      </div>
      {group("preset", tx("Presets"))}
      {group("recipe", tx("Recipe packs"))}
      {group("saved", tx("Saved"))}
    </div>
  );
}

const BEAT_KINDS: [string, string, string][] = [
  ["big", tx("Big card"), tx("A number or a claim")],
  ["chips", tx("Tags"), tx("Separate with commas")],
  ["strike", tx("Strike"), tx("A rejected claim")],
  ["quote", tx("Quote"), tx("One per line")],
  ["list", tx("List"), tx("One per line")],
  ["title", tx("Huge title"), tx("Above the head")],
  ["hook", tx("Hook"), tx("key | word")],
  ["headline", tx("Headline"), tx("*marked* words")],
  ["word", tx("Full-screen word"), tx("1–3 words")],
  ["card", tx("Full-screen card"), tx("One per line")],
  ["kinetic", tx("Kinetic text"), tx("*key* _filler_")],
  ["emoji", "Emoji", tx("Single emoji")],
  ["behind", tx("Word behind the speaker"), tx("1–2 big words (person matte)")],
  ["cutout", tx("Cutout effect"), tx("bg-blur, selective, spotlight or bg-swap")],
  ["endcard", tx("End card"), tx("title | line | address")],
];

function BeatsPanel({ reel, report, origNow, t, seek, run, patch, sel, setSel }: Json) {
  const selIdx = typeof sel === "string" && sel.startsWith("b") ? +sel.slice(1) : -1;
  const selBeat = selIdx >= 0 ? (reel.beats || [])[selIdx] : null;
  const [kind, setKind] = useState("big");
  const [text, setText] = useState("");
  const [secs, setSecs] = useState(2.5);
  const [raw, setRaw] = useState("");
  const beats: Json[] = reel.beats || [];
  const make = (): Json => {
    const at = r2(origNow);
    const to = r2(origNow + secs);
    const lines = text.split("\n").map((s) => s.trim()).filter(Boolean);
    const parts = text.split("|").map((s) => s.trim());
    switch (kind) {
      case "chips": return { type: "chips", at, to, items: text.split(",").map((s) => ({ text: s.trim() })).filter((x) => x.text) };
      case "quote": return { type: "quote", at, to, lines: lines.map((l) => ({ text: l })) };
      case "list": return { type: "list", at, to, lines: lines.map((l) => ({ text: l })) };
      case "hook": return { type: "textfx", kind: "hook", at, to, key: parts[0] || "", word: parts[1] || "" };
      case "headline": return { type: "textfx", kind: "headline", at, to, lines: lines.length ? lines : [text] };
      case "word": return { type: "scene", kind: "word", at, to, text };
      case "card": return { type: "scene", kind: "card", at, to, lines: lines.map((l) => ({ text: l })) };
      case "kinetic": return { type: "scene", kind: "kinetic", at, to, text };
      case "emoji": return { type: "emoji", at, to, emoji: text };
      case "behind": return { type: "behind", at, to, text: text.toUpperCase() };
      case "cutout": return { type: "cutout", at, to, fx: (text || "bg-blur").trim() };
      case "endcard": return { type: "endcard", at: "outro+0", title: parts[0] || "", line: parts[1] || "", url: parts[2] || "" };
      default: return { type: kind, at, to, text: parts[0], ...(parts[1] ? { sub: parts[1] } : {}) };
    }
  };
  const add = async () => {
    let beat: Json;
    try {
      beat = raw ? JSON.parse(raw) : make();
    } catch {
      return;
    }
    if (beat.type === "endcard" && !(reel.outro > 0)) await patch({ outro: 2.5 });
    await run(tx("Adding the beat"), "beat_add", { beat });
    setText("");
    setRaw("");
  };
  const hint = BEAT_KINDS.find((k) => k[0] === kind)?.[2];
  return (
    <div className="stack lg">
      <div className="stack">
        <h2>{tx("Add beat")}</h2>
        <div className="grid2">
          <div className="field">
            <label>{tx("Type")}</label>
            <select className="select" value={kind} onChange={(e) => setKind(e.target.value)}>{BEAT_KINDS.map(([k, l]) => <option key={k} value={k}>{l}</option>)}</select>
          </div>
          <div className="field">
            <label>{tx("Length, s")}</label>
            <input className="input num" type="number" step={0.1} value={secs} onChange={(e) => setSecs(+e.target.value)} />
          </div>
        </div>
        <div className="field">
          <label>{hint}</label>
          <textarea className="input" style={{ fontFamily: "var(--font)", fontSize: 13 }} rows={kind === "quote" || kind === "list" || kind === "card" ? 3 : 2} value={text} onChange={(e) => setText(e.target.value)} />
        </div>
        <div className="rowx">
          <span className="grow muted" style={{ fontSize: 12 }}>{tx("Lands on the word under the playhead:")} <span className="num">{dur(origNow)}</span></span>
          <button className="btn sm ghost" onClick={() => setRaw(raw ? "" : JSON.stringify(make(), null, 1))}>{raw ? "Form" : "JSON"}</button>
          <button className="btn sm primary" onClick={add} disabled={!text && !raw}><Plus />{tx("Add")}</button>
        </div>
        {raw && <textarea className="input" rows={8} value={raw} onChange={(e) => setRaw(e.target.value)} />}
      </div>
      {selBeat && (
        <div className="stack" style={{ gap: 8 }} key={selIdx + JSON.stringify(selBeat)}>
          <h2>{tx("Selected beat")}</h2>
          {"text" in selBeat && <textarea className="input" style={{ fontFamily: "var(--font)", fontSize: 13 }} rows={2} defaultValue={selBeat.text} onBlur={(e) => e.target.value !== selBeat.text && patch({ beats: reel.beats.map((b: Json, j: number) => (j === selIdx ? { ...b, text: e.target.value } : b)) })} />}
          <textarea className="input" rows={8} defaultValue={JSON.stringify(selBeat, null, 1)} onBlur={(e) => { try { const v = JSON.parse(e.target.value); if (JSON.stringify(v) !== JSON.stringify(selBeat)) patch({ beats: reel.beats.map((b: Json, j: number) => (j === selIdx ? v : b)) }); } catch { /* keep editing */ } }} />
          <span className="muted" style={{ fontSize: 12 }}>{tx("Change the text or any field; drag the bar on the timeline to move it.")}</span>
        </div>
      )}
      <div className="stack">
        <h2>{tx("Beats in this edit")}</h2>
        {!beats.length && <p className="sec">{tx("No beats yet. Leave some sentences bare: silence makes the next card land harder.")}</p>}
        <div className="list-rows">
          {beats.map((b, i) => {
            const at = (report?.beats || []).find((x: Json) => x.beat === i)?.at;
            return (
              <div key={i} className="rowx" style={{ padding: "8px 0", cursor: "pointer", color: i === selIdx ? "var(--text)" : undefined }} onClick={() => setSel(`b${i}`)}>
                <button className="btn sm ghost num" style={{ width: 56 }} onClick={() => at !== undefined && seek(at)}>{at !== undefined ? dur(at) : "–"}</button>
                <span className="tag">{b.kind || b.type}</span>
                <span className="grow" style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{b.text || b.title || b.word || b.emoji || (b.lines || b.items || []).map((l: Json) => l.text || l).join(", ")}</span>
                <button className="btn icon sm ghost" title={tx("Delete")} onClick={() => run(tx("Deleting"), "beat_remove", { index: i })}><Trash2 /></button>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

function NotesPanel({ proj, rid, t, seek, reload, say }: Json) {
  const [text, setText] = useState("");
  const notes: Json[] = (proj.notes || []).filter((n: Json) => n.reel === rid);
  const add = async () => {
    await tool("note_add", { project: proj.id, reel: rid, t: r2(t), text });
    setText("");
    reload();
  };
  const open = notes.filter((n) => n.status === "open").length;
  return (
    <div className="stack lg">
      <div className="stack">
        <h2>{tx("Note on this frame")}</h2>
        <textarea className="input" style={{ fontFamily: "var(--font)", fontSize: 13 }} rows={2} placeholder={tx("What should change")} value={text} onChange={(e) => setText(e.target.value)} />
        <div className="rowx">
          <span className="grow muted num" style={{ fontSize: 12 }}>{dur(t)}</span>
          <button className="btn sm" disabled={!text} onClick={add}><Plus />{tx("Leave a note")}</button>
        </div>
      </div>
      <div className="stack">
        <div className="rowx">
          <h2 className="grow">{tx("Notes")}</h2>
          {open > 0 && (
            <button className="btn sm ghost" onClick={() => navigator.clipboard.writeText(tx("I left {3} notes on \"{0}\" ({1}, reel {2}) in Linnea. Read them with the notes tool, fix them, set each one to fixed with note_update and write a short reply.", proj.title, proj.id, rid, open)).then(() => say(tx("Message for the agent copied")))}><Copy />{tx("Send to agent")}</button>
          )}
        </div>
        {!notes.length && <p className="sec">{tx("No notes. The agent reads your notes with the notes tool and marks them when fixed.")}</p>}
        <div className="list-rows">
          {notes.map((n) => (
            <div key={n.id} style={{ padding: "8px 0" }}>
              <div className="rowx">
                <button className="btn sm ghost num" style={{ width: 56 }} onClick={() => seek(n.t)}>{dur(n.t)}</button>
                <span className="grow" style={n.status !== "open" ? { color: "var(--text-3)" } : undefined}>{n.text}</span>
                {n.status === "open" ? (
                  <button className="btn icon sm ghost" title={tx("Fixed")} onClick={() => tool("note_update", { project: proj.id, id: n.id, status: "fixed" }).then(reload)}><Check /></button>
                ) : (
                  <span className="tag">{n.status === "fixed" ? tx("Fixed") : n.status}</span>
                )}
                <button className="btn icon sm ghost" title={tx("Delete")} onClick={() => tool("note_update", { project: proj.id, id: n.id, delete: true }).then(reload)}><Trash2 /></button>
              </div>
              {n.reply && <div className="sec" style={{ fontSize: 12, padding: "0 0 0 64px" }}>{n.reply}</div>}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

function BatchRender({ proj, run, busy }: Json) {
  const [f, setF] = useState<string[]>(["9:16"]);
  const clips = proj.reels.filter((r: Json) => r.id !== "main").length;
  return (
    <div className="stack" style={{ gap: 6 }}>
      <div className="rowx">
        <div className="seg">
          {["9:16", "1:1", "4:5", "16:9"].map((x) => <button key={x} className={f.includes(x) ? "on" : ""} onClick={() => setF(f.includes(x) ? f.filter((y) => y !== x) : [...f, x])}>{x}</button>)}
        </div>
      </div>
      <button className="btn sm" disabled={busy || !f.length} onClick={() => run(tx("Batch render"), "render_batch", { formats: f })}>{clips ? tx("All {0} clips in", clips) : tx("This edit in")} {f.join(", ")} {tx("— render")}</button>
    </div>
  );
}

function Inspector({ proj, reel, rid, total, takes, selTake, setSelTake, report, patch, run, busy, say, reload }: Json) {
  const [prompt, setPrompt] = useState("");
  const [answer, setAnswer] = useState("");
  const tk: Take | undefined = reel.takes?.[selTake];
  const setTake = (p: Partial<Take>) => patch({ takes: reel.takes.map((x: Take, i: number) => (i === selTake ? { ...x, ...p } : x)) });
  const warnings: string[] = report?.warnings || [];
  const renders: Json[] = (proj.renders || []).slice(0, 6);
  const ask = async () => {
    setAnswer("");
    const r = await run(tx("Assistant working"), "ask", { reel: rid, prompt });
    if (r) {
      setAnswer(r.answer);
      setPrompt("");
    }
  };
  return (
    <>
      <div className="stack">
        <h2>{tx("Edit")}</h2>
        <input className="input" key={rid + reel.title} defaultValue={reel.title} onBlur={(e) => e.target.value !== reel.title && patch({ title: e.target.value })} />
        <div className="kv">
          <span>{tx("Length")}</span><b className="num" style={{ fontWeight: 400 }}>{dur(total)}</b>
          <span>{tx("Cuts")}</span><b className="num" style={{ fontWeight: 400 }}>{reel.takes?.length || 1}</b>
          <span>{tx("Format")}</span><b className="num" style={{ fontWeight: 400 }}>{reel.width || 1080}×{reel.height || 1920}</b>
          <span>{tx("End padding, s")}</span>
          <input className="input num" style={{ width: 72, height: 28 }} type="number" step={0.5} min={0} key={rid + reel.outro} defaultValue={reel.outro || 0} onBlur={(e) => +e.target.value !== (reel.outro || 0) && patch({ outro: +e.target.value })} />
        </div>
      </div>
      <div className="stack" style={{ gap: 8 }}>
        <div className="rowx">
          <button className="btn sm" disabled={busy} title={tx("Undo")} onClick={() => tool("reel_undo", { project: proj.id, reel: rid }).then(reload).catch((e) => say(e.message))}>{tx("Undo")}</button>
          <button className="btn sm ghost" disabled={busy} title={tx("Redo")} onClick={() => tool("reel_undo", { project: proj.id, reel: rid, redo: true }).then(reload).catch((e) => say(e.message))}>{tx("Redo")}</button>
          <span className="grow" />
          <button className="btn sm ghost" onClick={async () => { const name = window.prompt(tx("Style name (brand kit)")); if (name) { try { const r = await tool("style_save", { project: proj.id, reel: rid, name }); say(tx("Style saved: {0}", r.style)); } catch (e: any) { say(e.message); } } }}>{tx("Save style")}</button>
        </div>
        <label className="rowx sec" style={{ fontSize: 12 }}><input type="checkbox" checked={!!reel.voice?.clean} onChange={(e) => patch({ voice: { clean: e.target.checked } })} />{tx("Clean the voice (noise reduction, compression)")}</label>
      </div>
      {tk && (
        <div className="stack">
          <div className="rowx"><h2 className="grow">{tx("Selected cut")}</h2><span className="num muted">{selTake + 1}/{reel.takes.length}</span></div>
          <div className="grid2">
            <div className="field"><label>{tx("Start")}</label><input className="input num" type="number" step={0.05} key={`a${selTake}${tk.a}`} defaultValue={tk.a} onBlur={(e) => +e.target.value !== tk.a && setTake({ a: +e.target.value })} /></div>
            <div className="field"><label>{tx("End")}</label><input className="input num" type="number" step={0.05} key={`b${selTake}${tk.b}`} defaultValue={tk.b} onBlur={(e) => +e.target.value !== tk.b && setTake({ b: +e.target.value })} /></div>
          </div>
          <div className="rowx">
            <div className="field" style={{ width: 80 }}><label>{tx("Speed")}</label><input className="input num" type="number" step={0.1} min={0.25} max={4} key={`s${selTake}${tk.speed}`} defaultValue={(tk as any).speed || 1} onBlur={(e) => +e.target.value !== ((tk as any).speed || 1) && setTake({ speed: +e.target.value } as any)} /></div>
            <div className="field grow"><label>{tx("Hold at end, s")}</label><input className="input num" type="number" step={0.1} min={0} key={`h${selTake}${tk.hold}`} defaultValue={tk.hold || 0} onBlur={(e) => setTake({ hold: +e.target.value || undefined })} /></div>
            <button className="btn icon" style={{ alignSelf: "flex-end" }} title={tx("Delete cut")} onClick={() => { patch({ takes: reel.takes.filter((_: Take, i: number) => i !== selTake) }); setSelTake(-1); }}><Trash2 /></button>
          </div>
        </div>
      )}
      <div className="stack" style={{ gap: 4 }}>
        <h2>{tx("Checks")}</h2>
        {!warnings.length && <p className="sec" style={{ fontSize: 12 }}>{tx("No warnings")}</p>}
        {warnings.map((w, i) => <div key={i} className="warn">{w}</div>)}
      </div>
      <div className="stack">
        <h2>{tx("Output")}</h2>
        <div className="rowx">
          <button className="btn grow" disabled={busy} onClick={() => run(tx("Rendering preview"), "render", { reel: rid, mode: "preview" })}>{tx("Preview")}</button>
          <button className="btn primary grow" disabled={busy} onClick={() => run(tx("Rendering"), "render", { reel: rid, mode: "final" })}>{tx("Final render")}</button>
        </div>
        <BatchRender proj={proj} run={run} busy={busy} />
        <div className="rowx">
          <button className="btn sm ghost" onClick={() => tool("export_subs", { project: proj.id, reel: rid }).then((r) => say(`SRT: ${r.file}`)).catch((e) => say(e.message))}>SRT</button>
          <button className="btn sm ghost" onClick={() => tool("export_subs", { project: proj.id, reel: rid, format: "vtt" }).then((r) => say(`VTT: ${r.file}`)).catch((e) => say(e.message))}>VTT</button>
        </div>
        <div className="list-rows">
          {renders.map((r) => (
            <div key={r.file} className="rowx" style={{ padding: "8px 0" }}>
              <span className={`dot ${r.qa ? (String(r.qa).startsWith("PASS") ? "ok" : "bad") : ""}`} title={r.qa || ""} />
              <span className="grow" style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", fontSize: 12 }}>{r.file}</span>
              <span className="num muted" style={{ fontSize: 12 }}>{(r.size / 1e6).toFixed(1)} MB</span>
              <PublishButton file={r.path} title={reel.title || proj.title} say={say} />
              <button className="btn icon sm ghost" title={tx("Show in folder")} onClick={() => tool("reveal", { path: r.path })}><FolderOpen /></button>
            </div>
          ))}
        </div>
      </div>
      <AiPanel proj={proj} rid={rid} reel={reel} run={run} busy={busy} patch={patch} say={say} />
      <div className="stack">
        <h2>{tx("Assistant")}</h2>
        <textarea className="input" style={{ fontFamily: "var(--font)", fontSize: 13 }} rows={3} placeholder={tx("Cut the pauses, use hype captions, show the number at 12 s as a big card")} value={prompt} onChange={(e) => setPrompt(e.target.value)} />
        <div className="rowx">
          <span className="grow muted" style={{ fontSize: 12 }}>{tx("Works when a model is chosen in Settings")}</span>
          <button className="btn sm" disabled={!prompt || busy} onClick={ask}><SendHorizontal />{tx("Send")}</button>
        </div>
        {answer && <div className="sec" style={{ fontSize: 12, lineHeight: "18px", whiteSpace: "pre-wrap" }}>{answer}</div>}
      </div>
    </>
  );
}
