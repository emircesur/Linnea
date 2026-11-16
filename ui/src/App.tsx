import { BRAND, lang, setLang, tx } from "./i18n";
import { useEffect, useMemo, useRef, useState } from "react";
import { Clapperboard, Film as FilmIcon, ImageIcon, FolderOpen, Library as LibIcon, Link2, ListVideo, Play, Settings as Cog, Shapes, Square, Trash2, Upload } from "lucide-react";
import { Activity, dur, fileUrl, Json, runJob, tool, useActivity, useLoad } from "./api";
import { Editor } from "./Editor";
import { FilmReview, Films } from "./Film";
import { ThumbStudio } from "./Studio";
import { PuppetsPage } from "./PuppetsPage";
import { KeysEditor } from "./Ai";
import { arrange, CardActions, deleteItem, ListTools, renameItem, useStars } from "./Manage";
import { Pencil } from "lucide-react";

type Route = { page: "films" | "film" | "thumbs" | "projects" | "renders" | "library" | "settings" | "editor" | "puppets"; id?: string };

export function Toast({ text, onDone }: { text: string; onDone: () => void }) {
  useEffect(() => {
    const t = setTimeout(onDone, 6000);
    return () => clearTimeout(t);
  }, [text]);
  return <div className="toast">{text}</div>;
}

export function App() {
  const [route, setRoute] = useState<Route>({ page: "films" });
  const [toast, setToast] = useState("");
  const act = useActivity();
  const projects = useLoad(() => tool("projects"), [act.events.length]);
  const doctor = useLoad(() => tool("doctor"), []);
  const settings = useLoad(() => tool("settings"), []);
  const theme = settings.data?.ui?.theme || "grafit";
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);
  // An agent's `open` call brings its project to the front.
  const seenFocus = useRef(0);
  useEffect(() => {
    const f = act.focus;
    if (f && f.ts > seenFocus.current && Date.now() - f.ts < 15000) {
      seenFocus.current = f.ts;
      setRoute(f.kind === "film" ? { page: "film", id: f.project } : { page: "editor", id: f.project });
    }
  }, [act.focus]);
  const list: Json[] = projects.data?.projects || [];
  const running = act.jobs.filter((j) => j.status === "running" || j.status === "queued");
  const missing = (doctor.data?.tools || []).filter((t: Json) => !t.ok && t.tool !== "yt-dlp").length + (doctor.data && !doctor.data.whisper_model.ok && doctor.data.transcribe_backend === "whisper.cpp" ? 1 : 0);
  // the language chosen in the installer or in Settings wins over what this window remembered
  useEffect(() => {
    const l = settings.data?.ui?.lang;
    if (l === "en" || l === "tr") setLang(l);
  }, [settings.data?.ui?.lang]);
  useEffect(() => {
    document.title = BRAND;
  }, []);
  // [page, short label for the rail, full name, icon, count]
  const nav: [Route["page"], string, string, any, string?][] = [
    ["films", tx("Films"), tx("Films"), FilmIcon],
    ["projects", tx("Reels"), tx("Reels from footage"), Clapperboard, list.length ? String(list.length) : ""],
    ["puppets", tx("Puppets"), tx("Puppets"), Shapes],
    ["thumbs", tx("Covers"), tx("Thumbnails"), ImageIcon],
    ["library", tx("Library"), tx("Library"), LibIcon],
    ["renders", tx("Queue"), tx("Render queue"), ListVideo, running.length ? String(running.length) : ""],
  ];
  const at = (p: Route["page"]) => route.page === p || (p === "films" && route.page === "film") || (p === "projects" && route.page === "editor");
  const health = doctor.data ? (missing ? tx("{0} tools missing", missing) : tx("Tools ready")) : tx("Checking");
  return (
    <div className="shell">
      <aside className="rail">
        <div className="mark" title={BRAND}>
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d="M12 21V10.5M12 10.5C10.5 5.5 6.5 5 6.5 9.5M12 10.5c1.5-5 5.5-5.5 5.5-1" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
            <path d="M4 13.5a2.5 2.5 0 0 1 5 0zM15 13.5a2.5 2.5 0 0 1 5 0z" style={{ fill: "var(--accent)" }} />
          </svg>
        </div>
        <nav className="rail-nav">
          {nav.map(([p, short, full, Icon, count]) => (
            <button key={p} className={at(p) ? "on" : ""} title={full} onClick={() => setRoute({ page: p })}>
              <span className="ico"><Icon />{count ? <i className="count num">{count}</i> : null}</span>
              <span className="lbl">{short}</span>
            </button>
          ))}
        </nav>
        <span className="grow" />
        <button className={`rail-foot ${route.page === "settings" ? "on" : ""}`} title={`${tx("Settings")} · ${health} · ${doctor.data?.version || ""}`} onClick={() => setRoute({ page: "settings" })}>
          <span className="ico"><Cog /><i className={`dot ${doctor.data ? (missing ? "bad" : "ok") : ""}`} /></span>
          <span className="lbl">{tx("Settings")}</span>
        </button>
      </aside>
      <main className="main">
        {route.page === "films" && <Films act={act} say={setToast} open={(id) => setRoute({ page: "film", id })} />}
        {route.page === "film" && route.id && <FilmReview key={route.id} id={route.id} act={act} say={setToast} back={() => setRoute({ page: "films" })} />}
        {route.page === "thumbs" && <ThumbStudio say={setToast} />}
        {route.page === "projects" && <Projects list={list} act={act} loading={projects.loading && !projects.data} open={(id) => setRoute({ page: "editor", id })} reload={projects.reload} say={setToast} />}
        {route.page === "editor" && route.id && <Editor key={route.id} id={route.id} act={act} say={setToast} back={() => setRoute({ page: "projects" })} />}
        {route.page === "renders" && <Renders act={act} list={list} open={(id) => setRoute({ page: "editor", id })} />}
        {route.page === "library" && <Library />}
        {route.page === "puppets" && <PuppetsPage say={setToast} />}
        {route.page === "settings" && <Settings doctor={doctor} settings={settings} say={setToast} />}
      </main>
      {toast && <Toast text={toast} onDone={() => setToast("")} />}
    </div>
  );
}

function Projects({ list: all, act, loading, open, reload, say }: { list: Json[]; act: Activity; loading: boolean; open: (id: string) => void; reload: () => void; say: (s: string) => void }) {
  const stars = useStars("projects");
  const [q, setQ] = useState("");
  const [sort, setSort] = useState("recent");
  const list = arrange(all, q, sort, stars.has, (p) => p.created || 0);
  const [url, setUrl] = useState("");
  const [busy, setBusy] = useState<{ p: number; msg: string } | null>(null);
  const start = async (source: string) => {
    if (!source) return;
    setBusy({ p: 0, msg: tx("Starting") });
    try {
      const r = await runJob("import", { source }, (p, msg) => setBusy({ p, msg }));
      setUrl("");
      reload();
      open(r.project);
    } catch (e: any) {
      say(e.message);
    }
    setBusy(null);
  };
  const pick = async () => {
    const r = await tool("pick_file");
    if (r.path) start(r.path);
  };
  const label: Record<string, string> = { downloading: tx("Downloading"), preparing: tx("Preparing"), ready: tx("Ready"), failed: tx("Failed"), new: tx("New") };
  const progress = busy && (
    <div style={{ maxWidth: 440 }}>
      <div className="rowx sec" style={{ fontSize: 12, marginBottom: 8 }}>
        <span className="grow">{tr(busy.msg)}</span>
        <span className="num">{Math.round(busy.p * 100)}%</span>
      </div>
      <div className="meter"><i style={{ width: `${busy.p * 100}%` }} /></div>
    </div>
  );
  if (!loading && !all.length)
    return (
      <div className="page">
        <div className="wrap start">
          <div>
            <h1>{tx("From raw footage to vertical reels, all on this computer")}</h1>
            <p>{tx("Give it a talk recording or a long video. Linnea transcribes it, cuts the pauses and retakes, follows the face into a 9:16 frame and places captions word by word.")}</p>
            <div className="actions">
              <button className="btn primary" onClick={pick} disabled={!!busy}><Upload />{tx("Choose a video file")}</button>
              <div className="linkrow">
                <input className="input" placeholder={tx("YouTube or any other link")} value={url} onChange={(e) => setUrl(e.target.value)} onKeyDown={(e) => e.key === "Enter" && start(url)} />
                <button className="btn" onClick={() => start(url)} disabled={!url || !!busy}><Link2 />{tx("Fetch")}</button>
              </div>
              {progress}
            </div>
          </div>
          <div className="card steps">
            {[
              ["import", tx("The file or link is imported; a preview and an audio track are prepared")],
              ["transcribe", tx("The speech is transcribed with word timings (locally, whisper.cpp)")],
              ["clips_find", tx("Moments that stand on their own in a long video are scored")],
              ["autocut", tx("Pauses are cut; retakes keeps the best take of each line")],
              ["style_apply", tx("A style, caption template and beats are chosen")],
              ["render", tx("A preview, then a checked 1080x1920 master")],
            ].map(([c, t]) => (
              <div className="step" key={c}><code>{c}</code><span>{t}</span></div>
            ))}
            <div className="muted" style={{ fontSize: 12, paddingTop: 12, borderTop: "1px solid var(--line)" }}>{tx("The same commands work in the terminal and for agents over MCP")}</div>
          </div>
        </div>
      </div>
    );
  return (
    <div className="page">
      <div className="wrap">
        <div className="page-head">
          <h1>{tx("Projects")}</h1>
          <span className="grow" />
          <input className="input" style={{ width: 280 }} placeholder={tx("Paste a link")} value={url} onChange={(e) => setUrl(e.target.value)} onKeyDown={(e) => e.key === "Enter" && start(url)} />
          <button className="btn primary" onClick={pick} disabled={!!busy}><Upload />{tx("Add video")}</button>
        </div>
        {busy && <div style={{ marginBottom: 24 }}>{progress}</div>}
        <div className="rowx" style={{ marginBottom: 16 }}><ListTools q={q} setQ={setQ} sort={sort} setSort={setSort} placeholder={tx("Search projects")} /></div>
        {!list.length && <div className="empty">{tx("Nothing matches")}</div>}
        <div className="proj-grid">
          {list.map((p) => {
            const job = act.jobs.find((j) => j.project === p.id && (j.status === "running" || j.status === "queued"));
            return (
              <button key={p.id} className="card proj" onClick={() => open(p.id)}>
                <CardActions starred={stars.has(p.id)} onStar={() => stars.toggle(p.id)} actions={[
                  { icon: Pencil, label: tx("Rename"), run: () => renameItem("project", p.id, p.title, {}, say, reload) },
                  ...(p.dir ? [{ icon: FolderOpen, label: tx("Show in folder"), run: () => { tool("reveal", { path: p.dir }).catch((e) => say(e.message)); } }] : []),
                  { icon: Trash2, label: tx("Delete"), danger: true, run: () => deleteItem("project", p.id, p.title, {}, say, reload) },
                ]} />
                <div className="thumb">
                  {p.thumb && <img src={fileUrl(p.thumb)} alt="" />}
                  {p.duration ? <span className="tc">{dur(p.duration)}</span> : null}
                </div>
                <div className="meta">
                  <b>{p.title}</b>
                  <span>
                    {job ? `${tr(job.message) || tr(job.kind)} ${Math.round((job.progress || 0) * 100)}%` : [p.status !== "ready" ? label[p.status] || p.status : "", p.reels > 1 ? tx("{0} clips", p.reels - 1) : "", p.renders ? tx("{0} renders", p.renders) : "", p.open_notes ? tx("{0} notes", p.open_notes) : "", !p.has_words && p.status === "ready" ? tx("Not transcribed") : ""].filter(Boolean).join(" · ") || tx("Ready")}
                  </span>
                  {job && <div className="meter"><i style={{ width: `${(job.progress || 0) * 100}%` }} /></div>}
                </div>
              </button>
            );
          })}
        </div>
      </div>
    </div>
  );
}

const TR: Record<string, string> = {
  downloading: tx("Downloading"), "reading the link": tx("Reading the link"), thumbnail: tx("Thumbnail"), "voice track": tx("Audio track"), "preview proxy": tx("Preparing the preview"), transcribing: tx("Transcribing"), transcribed: tx("Transcribed"),
  rendering: tx("Rendering"), "checking the render": tx("Checking the render"), done: tx("Done"), "tracking the face": tx("Tracking the face"), import: tx("Import"), transcribe: tx("Transcription"), render: tx("Render"), analyze: tx("Analysis"), clips_find: tx("Finding clips"),
  clip_make: tx("Making the clip"), approval: tx("Approval sheet"), ask: tx("Assistant"), hub_ask: tx("Film assistant"), hub_render: tx("Film render"), puppet_ask: tx("Puppet maker"), queued: tx("Queued"), running: tx("Running"), failed: tx("Failed"), cancelled: tx("Cancelled"), lost: tx("Interrupted"),
};
export const tr = (s?: string) => (s ? TR[s] || s : "");

function Renders({ act, list, open }: { act: Activity; list: Json[]; open: (id: string) => void }) {
  const title = (id: string) => list.find((p) => p.id === id)?.title || id;
  return (
    <div className="page">
      <div className="wrap" style={{ maxWidth: 960 }}>
        <div className="page-head"><h1>{tx("Render queue")}</h1></div>
        {!act.jobs.length && <div className="empty">{tx("No jobs in the last 24 hours. Every job started from the app, the terminal or an agent shows up here.")}</div>}
        <div className="card list-rows">
          {act.jobs.map((j) => {
            const live = j.status === "running" || j.status === "queued";
            return (
              <div key={j.id} style={{ padding: "12px 16px" }}>
                <div className="rowx">
                  <span className={`dot ${live ? "live" : j.status === "done" ? "ok" : "bad"}`} />
                  <b style={{ fontWeight: 500 }}>{tr(j.kind)}</b>
                  <button className="btn ghost sm" onClick={() => j.project && open(j.project)}>{title(j.project)}</button>
                  <span className="grow" />
                  <span className="sec" style={{ fontSize: 12 }}>{live ? tr(j.message) || tr(j.status) : j.status === "done" ? j.result?.qa?.verdict || tx("Done") : tr(j.status)}</span>
                  {live && <span className="num sec">{Math.round((j.progress || 0) * 100)}%{j.progress > 0.03 ? tx(" · {0} min left", Math.max(1, Math.round(((Date.now() - j.created) / 1000) * (1 - j.progress) / j.progress / 60))) : ""}</span>}
                  {live && <button className="btn icon sm ghost" title={tx("Stop")} onClick={() => tool("job", { id: j.id, cancel: true })}><Square /></button>}
                  {j.result?.file && <button className="btn icon sm ghost" title={tx("Show in folder")} onClick={() => tool("reveal", { path: j.result.file })}><FolderOpen /></button>}
                </div>
                {live && <div className="meter" style={{ marginTop: 8 }}><i style={{ width: `${(j.progress || 0) * 100}%` }} /></div>}
                {j.error && <div className="err" style={{ fontSize: 12, marginTop: 8, whiteSpace: "pre-wrap" }}>{j.error}</div>}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

function Library() {
  const styles = useLoad(() => tool("styles"), []);
  const lib = useLoad(() => tool("library"), []);
  const [tab, setTab] = useState<"styles" | "captions" | "sounds">("styles");
  const audio = useRef<HTMLAudioElement | null>(null);
  const play = (path: string) => {
    audio.current?.pause();
    audio.current = new Audio(fileUrl(path));
    audio.current.play();
  };
  const all: Json[] = styles.data?.styles || [];
  return (
    <div className="page">
      <div className="wrap">
        <div className="page-head">
          <h1>{tx("Library")}</h1>
          <span className="grow" />
          <div className="seg">
            {([["styles", tx("Styles")], ["captions", tx("Caption templates")], ["sounds", tx("Sounds")]] as const).map(([k, l]) => (
              <button key={k} className={tab === k ? "on" : ""} onClick={() => setTab(k)}>{l}</button>
            ))}
          </div>
        </div>
        {tab === "styles" &&
          (["preset", "recipe", "saved"] as const).map((kind) => {
            const rows = all.filter((s) => s.kind === kind);
            if (!rows.length) return null;
            return (
              <section key={kind} style={{ marginBottom: 32 }}>
                <h2 style={{ marginBottom: 12 }}>{kind === "preset" ? tx("Presets") : kind === "recipe" ? tx("Recipe packs") : tx("Saved")}</h2>
                <div className="proj-grid" style={{ gridTemplateColumns: "repeat(auto-fill, minmax(320px, 1fr))" }}>
                  {rows.map((s) => (
                    <div key={s.id} className="card" style={{ padding: 16 }}>
                      <div className="rowx" style={{ marginBottom: 8 }}>
                        <span className="opt" style={{ padding: 0, width: "auto" }}><span className="sw" style={{ background: s.accent, color: s.ink, borderColor: "transparent" }}>Aa</span></span>
                        <b style={{ fontWeight: 500 }} className="grow">{s.name}</b>
                        <span className="tag">{s.font}</span>
                        <span className="tag">{s.captions}</span>
                      </div>
                      <div className="sec" style={{ fontSize: 12, lineHeight: "18px" }}>{s.about}</div>
                      {s.grammar && <div className="muted" style={{ fontSize: 12, lineHeight: "18px", marginTop: 8 }}>{s.grammar}</div>}
                    </div>
                  ))}
                </div>
              </section>
            );
          })}
        {tab === "captions" && (
          <div className="card list-rows" style={{ maxWidth: 760 }}>
            {(styles.data?.captions || []).map((c: Json) => (
              <div key={c.id} className="rowx" style={{ padding: "12px 16px" }}>
                <b style={{ fontWeight: 500, width: 140 }}>{c.label}</b>
                <span className="sec grow">{c.about}</span>
                <span className="num muted" style={{ fontSize: 12 }}>{c.id}</span>
              </div>
            ))}
          </div>
        )}
        {tab === "sounds" && (
          <div style={{ maxWidth: 760 }}>
            <p className="sec" style={{ marginBottom: 16 }}>{tx("Levels are never set by hand: each sound sits below the speech peak by its role (UI −14, whoosh −12, impact −8, reaction −3 dB).")}</p>
            <div className="card list-rows">
              {(lib.data?.sounds || []).map((s: Json) => (
                <div key={s.id} className="rowx" style={{ padding: "8px 16px" }}>
                  <button className="btn icon sm ghost" disabled={!s.path} onClick={() => play(s.path)} title={tx("Listen")}><Play /></button>
                  <span className="num" style={{ width: 200 }}>{s.id}</span>
                  <span className="tag">{s.role}</span>
                  <span className="grow muted" style={{ fontSize: 12 }}>{(s.tags || []).join(", ")}</span>
                  <span className="muted" style={{ fontSize: 12 }}>{s.source === "synthesized" ? tx("Generated") : "CC0"}</span>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function ToolsDownload({ done, say }: { done: () => void; say: (s: string) => void }) {
  const [p, setP] = useState<{ v: number; m: string } | null>(null);
  const go = async () => {
    setP({ v: 0, m: "" });
    try {
      const r = await runJob("setup_tools", {}, (v, m) => setP({ v, m }));
      say(r.installed?.length ? tx("Downloaded: {0}", r.installed.join(", ")) : tx("Nothing is missing"));
      done();
    } catch (e: any) {
      say(e.message);
    }
    setP(null);
  };
  if (p) return <div style={{ width: 160 }}><div className="muted" style={{ fontSize: 11 }}>{p.m || tx("Starting")}</div><div className="meter"><i style={{ width: `${p.v * 100}%` }} /></div></div>;
  return <button className="btn sm primary" onClick={go} title={tx("ffmpeg, yt-dlp, whisper.cpp, the model and the face model; only what is missing")}>{tx("Download missing")}</button>;
}

function ToolsDownloadOne({ what, label, say }: { what: string[]; label: string; say: (s: string) => void }) {
  const [p, setP] = useState<number | null>(null);
  return (
    <button className="btn sm" disabled={p !== null} onClick={async () => { setP(0); try { await runJob("setup_tools", { what }, (v) => setP(v)); say(tx("Downloaded: {0}", label)); } catch (e: any) { say(e.message); } setP(null); }}>
      {p === null ? label : `${Math.round(p * 100)}%`}
    </button>
  );
}

function Settings({ doctor, settings, say }: { doctor: ReturnType<typeof useLoad<Json>>; settings: ReturnType<typeof useLoad<Json>>; say: (s: string) => void }) {
  const d = doctor.data;
  const s = settings.data;
  const info = useLoad(() => fetch("/api/tools").then((r) => r.json()), []);
  const set = async (patch: Json) => {
    try {
      await tool("settings", { set: patch });
      settings.reload();
      doctor.reload();
    } catch (e: any) {
      say(e.message);
    }
  };
  const [keys, setKeys] = useState<Record<string, string>>({});
  const exe = (info.data?.exe || "").replace(/\//g, "\\") + "\\kesit.exe";
  const names: Record<string, string> = { ffmpeg: "ffmpeg", ffprobe: "ffprobe", "yt-dlp": "yt-dlp", whisper: "whisper.cpp" };
  const themes: [string, string, string, string][] = [["grafit", tx("Graphite"), "#18181a", "#0e0e10"], ["gece", tx("Night"), "#141820", "#0b0e13"], ["kagit", tx("Paper"), "#ffffff", "#f2f1ee"]];
  if (!d || !s) return <div className="page"><div className="muted">{tx("Loading")}</div></div>;
  return (
    <div className="page">
      <div className="sets">
        <div className="page-head" style={{ marginBottom: 0 }}><h1>{tx("Settings")}</h1></div>
        <section>
          <h2>{tx("Tools")}</h2>
          <div className="card box list-rows">
            {d.tools.map((t: Json) => (
              <div className="line" key={t.tool}>
                <div className="rowx"><span className={`dot ${t.ok ? "ok" : "bad"}`} />{names[t.tool]}</div>
                <small style={{ color: "var(--text-3)", fontSize: 12, wordBreak: "break-all" }}>{t.ok ? t.path : t.fix}</small>
                <span />
              </div>
            ))}
            <div className="line">
              <div className="rowx"><span className={`dot ${d.whisper_model.ok ? "ok" : "bad"}`} />{tx("Whisper model")}</div>
              <small style={{ color: "var(--text-3)", fontSize: 12, wordBreak: "break-all" }}>{d.whisper_model.ok ? d.whisper_model.path : d.whisper_model.fix}</small>
              <span />
            </div>
            <div className="line">
              <div className="rowx"><span className={`dot ${d.face_model.ok ? "ok" : "bad"}`} />{tx("Face model")}</div>
              <small style={{ color: "var(--text-3)", fontSize: 12 }}>{d.face_model.ok ? tx("The 9:16 frame follows the speaker's face") : d.face_model.fix}</small>
              <span />
            </div>
            <div className="line">
              <div>{tx("Folders")}</div>
              <small style={{ color: "var(--text-3)", fontSize: 12, wordBreak: "break-all" }}>{tx("Tools:")} {d.tools_folder}<br />{tx("Models:")} {d.models_folder}<br />{tx("Fonts:")} {d.fonts_folder}</small>
              <div className="stack" style={{ gap: 8 }}><button className="btn sm" onClick={() => doctor.reload()}>{tx("Check again")}</button><ToolsDownload done={() => doctor.reload()} say={say} /></div>
            </div>
          </div>
        </section>
        <section>
          <h2>{tx("Transcription")}</h2>
          <div className="card box list-rows">
            <div className="line">
              <div>{tx("Engine")}</div>
              <div className="seg">
                {[["whisper.cpp", tx("Local")], ["openai", "OpenAI"], ["groq", "Groq"], ["elevenlabs", "ElevenLabs"]].map(([k, l]) => (
                  <button key={k} className={s.transcribe.backend === k ? "on" : ""} onClick={() => set({ transcribe: { backend: k } })}>{l}</button>
                ))}
              </div>
              <span />
            </div>
            <div className="line">
              <div>{tx("Language")}<small>auto, tr, en…</small></div>
              <input className="input" style={{ maxWidth: 160 }} defaultValue={s.transcribe.lang} onBlur={(e) => e.target.value !== s.transcribe.lang && set({ transcribe: { lang: e.target.value } })} />
              <span />
            </div>
            {s.transcribe.backend !== "whisper.cpp" && (
              <div className="line">
                <div>{tx("API key")}<small>{s.keys[s.transcribe.backend] ? tx("Saved") : tx("None")}</small></div>
                <input className="input" type="password" placeholder={tx("New key")} value={keys[s.transcribe.backend] || ""} onChange={(e) => setKeys({ ...keys, [s.transcribe.backend]: e.target.value })} />
                <button className="btn sm" disabled={!keys[s.transcribe.backend]} onClick={() => set({ keys: { [s.transcribe.backend]: keys[s.transcribe.backend] } }).then(() => setKeys({}))}>{tx("Save")}</button>
              </div>
            )}
            {s.transcribe.backend !== "whisper.cpp" && <div className="line" style={{ gridTemplateColumns: "1fr" }}><small style={{ color: "var(--text-3)", fontSize: 12 }}>{tx("With this option the audio is sent to the service you chose. The local engine sends nothing.")}</small></div>}
          </div>
        </section>
        <section>
          <h2>{tx("Assistant")}</h2>
          <div className="card box list-rows">
            <div className="line">
              <div>{tx("Model provider")}</div>
              <div className="seg">
                {[["none", tx("Off")], ["anthropic", "Claude"], ["openai", "OpenAI"], ["local", tx("Local server")]].map(([k, l]) => (
                  <button key={k} className={(s.llm.provider || "none") === k ? "on" : ""} onClick={() => set({ llm: { provider: k } })}>{l}</button>
                ))}
              </div>
              <span />
            </div>
            {s.llm.provider === "local" && (
              <div className="line">
                <div>{tx("Address")}<small>Ollama, LM Studio</small></div>
                <input className="input" defaultValue={s.llm.base_url} onBlur={(e) => set({ llm: { base_url: e.target.value } })} />
                <span />
              </div>
            )}
            {s.llm.provider && s.llm.provider !== "none" && (
              <div className="line">
                <div>Model<small>{tx("Default if empty")}</small></div>
                <input className="input" defaultValue={s.llm.model} placeholder={s.llm.provider === "anthropic" ? "claude-opus-5-5" : s.llm.provider === "local" ? "llama3.1" : ""} onBlur={(e) => set({ llm: { model: e.target.value } })} />
                <span />
              </div>
            )}
            {(s.llm.provider === "anthropic" || s.llm.provider === "openai") && (
              <div className="line">
                <div>{tx("API key")}<small>{s.keys[s.llm.provider] ? tx("Saved") : tx("None")}</small></div>
                <input className="input" type="password" placeholder={tx("New key")} value={keys[s.llm.provider] || ""} onChange={(e) => setKeys({ ...keys, [s.llm.provider]: e.target.value })} />
                <button className="btn sm" disabled={!keys[s.llm.provider]} onClick={() => set({ keys: { [s.llm.provider]: keys[s.llm.provider] } }).then(() => setKeys({}))}>{tx("Save")}</button>
              </div>
            )}
          </div>
        </section>
        <section>
          <h2>{tx("Keys and services")}</h2>
          <p className="sec" style={{ marginBottom: 12 }}>{tx("Enter keys only for the services you use. Local options (whisper.cpp, Piper, person matte, a local model server) need no key and send nothing.")}</p>
          <KeysEditor s={s} set={set} say={say} />
          <div className="rowx" style={{ marginTop: 8 }}>
            <ToolsDownloadOne what={["matte"]} label={tx("Person matte model (RVM, 15 MB)")} say={say} />
            <ToolsDownloadOne what={["matte:birefnet-portrait"]} label="BiRefNet portrait (973 MB)" say={say} />
            <ToolsDownloadOne what={["piper", "voice:tr", "voice:en"]} label={tx("Piper + TR/EN voices (local dubbing)")} say={say} />
          </div>
        </section>
        <section>
          <h2>{tx("Agent connection")}</h2>
          <p className="sec" style={{ marginBottom: 12 }}>{tx("Claude Code and other MCP clients use Linnea with the same tools. Every change an agent makes shows up in the open project at once.")}</p>
          <div className="code">{`claude mcp add kesit -- "${exe}" mcp`}</div>
          <div className="rowx" style={{ marginTop: 8 }}>
            <button className="btn sm" onClick={() => navigator.clipboard.writeText(`claude mcp add kesit -- "${exe}" mcp`).then(() => say(tx("Command copied")))}>{tx("Copy command")}</button>
            <span className="muted" style={{ fontSize: 12 }}>{tx("The same tools in a terminal: kesit tools")}</span>
          </div>
        </section>
        <section>
          <h2>{tx("Language")}</h2>
          <div className="seg">
            {([["en", "English"], ["tr", "Türkçe"]] as const).map(([k, l]) => (
              <button key={k} className={lang === k ? "on" : ""} onClick={() => tool("settings", { set: { ui: { lang: k } } }).then(() => setLang(k)).catch((e) => say(e.message))}>{l}</button>
            ))}
          </div>
        </section>
        <section>
          <h2>{tx("Appearance")}</h2>
          <div className="grid2" style={{ gridTemplateColumns: "repeat(3, 1fr)" }}>
            {themes.map(([id, label, a, b]) => (
              <button key={id} className={`opt ${s.ui.theme === id ? "on" : ""}`} style={{ flexDirection: "column", gap: 8 }} onClick={() => set({ ui: { theme: id } })}>
                <span className="theme-sw"><span style={{ background: a }} /><span style={{ background: b, display: "grid", placeItems: "center" }}><span style={{ width: 8, height: 8, background: "var(--accent)", borderRadius: 4 }} /></span></span>
                <b>{label}</b>
              </button>
            ))}
          </div>
        </section>
        <section>
          <h2>{tx("Watched film folders")}</h2>
          <p className="sec" style={{ marginBottom: 12 }}>{tx("List film projects from other places too, besides the films folder in the workspace. Container: every subfolder is a film.")}</p>
          <div className="card box list-rows">
            {(s.films?.roots || []).map((r: Json, i: number) => (
              <div className="line" key={i}>
                <div>{(r.type || "container") === "project" ? tx("Single film") : tx("Container")}</div>
                <span className="num sec" style={{ fontSize: 12, wordBreak: "break-all" }}>{r.path || r}</span>
                <button className="btn sm ghost" onClick={() => set({ films: { roots: s.films.roots.filter((_: Json, k: number) => k !== i) } })}>{tx("Remove")}</button>
              </div>
            ))}
            <div className="line">
              <div>{tx("Add")}</div>
              <span />
              <div className="rowx">
                <button className="btn sm" onClick={async () => { const r = await tool("pick_file", { folder: true }); if (r.path) set({ films: { roots: [...(s.films?.roots || []), { path: r.path, type: "container" }] } }); }}>{tx("Container")}</button>
                <button className="btn sm" onClick={async () => { const r = await tool("pick_file", { folder: true }); if (r.path) set({ films: { roots: [...(s.films?.roots || []), { path: r.path, type: "project" }] } }); }}>{tx("Single film")}</button>
              </div>
            </div>
          </div>
        </section>
        <section>
          <h2>{tx("Workspace")}</h2>
          <div className="rowx">
            <input className="input" readOnly value={s.workspace} />
            <button className="btn" onClick={async () => { const r = await tool("pick_file", { folder: true }); if (r.path) set({ workspace: r.path }); }}><FolderOpen />{tx("Change")}</button>
          </div>
        </section>
      </div>
    </div>
  );
}
