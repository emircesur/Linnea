// The film at a glance: small frames across its length, drawn on a hidden copy of the page so the preview
// never moves. Click one to go there; the one on screen is marked. Kept until the film's code changes.
import { lang, tx } from "./i18n";
import { useEffect, useRef, useState } from "react";
import { Json } from "./api";

const SHOTS = new Map<string, { t: number; src: string }[]>();

export function Storyboard({ url, rev, lang, W, H, dur, scenes, t, seek }: { url: string; rev: number; lang: string; W: number; H: number; dur: number; scenes: Json[]; t: number; seek: (t: number) => void }) {
  const key = `${url}@${rev}#${lang}`;
  const [shots, setShots] = useState(SHOTS.get(key) || []);
  const [working, setWorking] = useState(!SHOTS.has(key));
  const ref = useRef<HTMLIFrameElement>(null);
  useEffect(() => { const k = SHOTS.get(key); setShots(k || []); setWorking(!k); }, [key]);
  // a frame at each scene's start and between them, about 16 in all
  const times = () => {
    const n = 16, out = new Set<number>();
    for (const s of scenes) if (typeof s.t0 === "number" && s.t0 < dur) out.add(Math.round((s.t0 + 0.4) * 10) / 10);
    for (let i = 0; i < n && out.size < 24; i++) out.add(Math.round(((i + 0.5) * dur / n) * 10) / 10);
    return [...out].sort((a, b) => a - b);
  };
  const onLoad = () => {
    const w = ref.current?.contentWindow as any;
    const k = key;
    const alive = () => ref.current?.contentWindow === w && k === key;
    const start = Date.now();
    const wait = async () => {
      if (!alive()) return;
      if (w?.READY !== true) { if (Date.now() - start < 30000) setTimeout(wait, 100); else setWorking(false); return; }
      const canvases = [...w.document.querySelectorAll("canvas")] as HTMLCanvasElement[];
      const c = canvases.sort((a, b) => b.width * b.height - a.width * a.height)[0];
      if (!c) { setWorking(false); return; }
      const out: { t: number; src: string }[] = [];
      const tw = 176, th = Math.round(tw * H / W);
      const small = document.createElement("canvas");
      small.width = tw; small.height = th;
      const g = small.getContext("2d")!;
      for (const x of times()) {
        if (!alive()) return;
        try { const r = w.renderOut(x); if (r && r.then) await r; } catch { continue; }
        g.drawImage(c, 0, 0, tw, th);
        out.push({ t: x, src: small.toDataURL("image/jpeg", 0.7) });
        setShots([...out]);
        await new Promise((r) => setTimeout(r, 0));
      }
      SHOTS.set(k, out);
      if (SHOTS.size > 12) SHOTS.delete(SHOTS.keys().next().value!);
      setWorking(false);
    };
    wait();
  };
  const now = shots.reduce((best, s, i) => (s.t <= t + 0.05 ? i : best), 0);
  return (
    <div className="storyboard" style={{ display: "flex", gap: 6, overflowX: "auto", padding: "4px 0", width: "100%", maxWidth: 680 }}>
      {shots.map((s, i) => (
        <button key={s.t} onClick={() => seek(s.t)} title={`${s.t.toFixed(1)} s`} style={{ flex: "0 0 auto", padding: 0, border: `2px solid ${i === now ? "var(--accent)" : "transparent"}`, borderRadius: 6, background: "#000", cursor: "pointer", position: "relative" }}>
          <img src={s.src} alt="" style={{ display: "block", height: 72, borderRadius: 4 }} />
          <span className="num" style={{ position: "absolute", left: 4, bottom: 2, fontSize: 10, color: "#fff", textShadow: "0 1px 2px #000" }}>{s.t.toFixed(1)}</span>
        </button>
      ))}
      {working && <span className="muted" style={{ fontSize: 12, alignSelf: "center", whiteSpace: "nowrap" }}>{tx("Drawing the storyboard…")}</span>}
      {working && <iframe ref={ref} key={key} src={`${url}?r=${rev}&board=1${lang ? `&lang=${encodeURIComponent(lang)}` : ""}`} onLoad={onLoad} title="storyboard" aria-hidden tabIndex={-1} style={{ position: "fixed", left: -30000, top: 0, width: W, height: H, border: 0, visibility: "hidden", pointerEvents: "none" }} />}
    </div>
  );
}

const KEYS: [string, [string, string][]][] = [
  ["Play", [
    ["Space / K", "Play / pause"], [", .", "One frame"], ["← →", "One frame (nothing selected)"], ["Shift + ← →", "One second"], ["J / L", "One second back / on"],
    ["Home / End", "Start / end"], ["PageUp / PageDown", "Previous / next scene"], ["I / O", "Range in / out"], ["Shift + drag", "Select a range on the timeline"],
    ["N", "Note on the frame"], ["M / Shift + M", "Add a marker / go to the next one"], ["B", "Storyboard"], ["U", "Scopes"], ["G", "Safe areas / thirds"], ["1 – 5", "Properties, sound, languages, assistant, versions"], ["A", "Add panel"],
  ]],
  ["Edit", [
    ["Ctrl + K", "Every command, by name"], ["Ctrl + Z / Ctrl + Y", "Undo / redo"], ["Shift + click", "Select several"], ["Ctrl + A", "Select every layer on screen"],
    ["← → ↑ ↓", "Nudge 1 px (Shift: 10 px)"], ["Ctrl + D", "Duplicate"], ["Ctrl + C / X / V", "Copy / cut / paste (also between films)"], ["Delete", "Delete"],
    ["S", "Split at the playhead"], ["Alt + [ / ]", "Start / end at the playhead"], ["Ctrl + [ / ]", "Send backward / bring forward (Shift: to the back / front)"],
    ["H", "Hide / show"], ["Ctrl + L", "Lock / unlock"], ["Shift (drag)", "Keep the shape, one axis, 15° steps"], ["Alt (drag)", "From the centre; no snapping"], ["Ctrl + wheel", "Zoom the timeline"], ["Ctrl + \\", "Hide / show both side panels"], ["Ctrl + Shift + \\", "Hide / show the timeline"], ["Esc", "Deselect, back to the live view"],
  ]],
];

export function Shortcuts({ close }: { close: () => void }) {
  useEffect(() => {
    const k = (e: KeyboardEvent) => { if (e.key === "Escape" || e.key === "?") { e.preventDefault(); close(); } };
    window.addEventListener("keydown", k, true);
    return () => window.removeEventListener("keydown", k, true);
  }, [close]);
  return (
    <div onClick={close} style={{ position: "fixed", inset: 0, background: "rgba(0,0,0,.5)", zIndex: 50, display: "grid", placeItems: "center" }}>
      <div className="card" onClick={(e) => e.stopPropagation()} style={{ padding: 20, width: 860, maxWidth: "calc(100vw - 32px)", maxHeight: "86vh", overflow: "auto" }}>
        <h2 style={{ marginBottom: 12 }}>{tx("Keyboard shortcuts")}</h2>
        <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(340px, 1fr))", gap: 24 }}>
          {KEYS.map(([g, list]) => (
            <div key={g}>
              <div className="sub-h" style={{ marginBottom: 8 }}>{g === "Edit" && lang === "tr" ? "Düzen" : tx(g)}</div>
              <div className="kv" style={{ fontSize: 12, gridTemplateColumns: "auto 1fr", alignItems: "center" }}>
                {list.map(([k, d]) => [<span key={k} className="kbd" style={{ justifySelf: "start", whiteSpace: "nowrap" }}>{k}</span>, <span key={k + "d"} style={{ color: "var(--text-2)" }}>{tx(d)}</span>])}
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
