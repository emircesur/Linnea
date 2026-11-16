// The puppet library page (app menu). Puppet code runs in a sandboxed frame (no access to the app),
// animating through its states.
import { tx } from "./i18n";
import { useEffect, useRef, useState } from "react";
import { Plus, Sparkles, Trash2, Upload } from "lucide-react";
import { fileUrl, Json, runJob, tool, useLoad } from "./api";
import { PuppetFields, puppetFromImage, puppetName } from "./Puppets";

const PREVIEW_DOC = [
  "<!doctype html><html><body style=\"margin:0;background:transparent;display:grid;place-items:center;height:100vh;overflow:hidden\">",
  "<canvas id=\"c\" style=\"max-width:100%;max-height:100vh\"></canvas><script>",
  "const EASE={linear:k=>k,out:k=>1-Math.pow(1-k,3),in:k=>k*k*k,inOut:k=>k<.5?4*k*k*k:1-Math.pow(-2*k+2,3)/2,back:k=>{const c=1.7;return 1+(c+1)*Math.pow(k-1,3)+c*Math.pow(k-1,2)}};",
  "const clamp=(x,a,b)=>Math.max(a,Math.min(b,x)),rand=n=>{n=Math.sin(n*127.1+311.7)*43758.5453;return n-Math.floor(n)};",
  "let def=null,fn=null,img={},only='',t0=performance.now(),params={};",
  "const c=document.getElementById('c'),g=c.getContext('2d');",
  "addEventListener('message',e=>{const m=e.data||{};if(m.def){def=m.def;img={};for(const[k,s]of Object.entries(def.assets||{})){const i=new Image();i.src=s;img[k]=i}",
  "try{fn=new Function('return ('+def.draw+')')()}catch(x){fn=null}[c.width,c.height]=def.size||[400,400];t0=performance.now()}",
  "if('state'in m&&m.state!==only){only=m.state||'';t0=performance.now()}if(m.params)params=m.params});",
  "const oops=m=>{g.setTransform(1,0,0,1,0,0);g.fillStyle='#e5484d';g.font='600 '+Math.max(16,c.width/24)+'px sans-serif';g.textBaseline='top';g.fillText(String(m).slice(0,48),12,12)};",
  "function frame(){requestAnimationFrame(frame);if(!def)return;if(!fn){g.setTransform(1,0,0,1,0,0);g.clearRect(0,0,c.width,c.height);oops('Çizim kodu derlenemedi');return}const t=(performance.now()-t0)/1000;const S=def.states||{},names=Object.keys(S);",
  "let s={name:'',t:t,k:0,dur:1};if(names.length){const seq=only&&S[only]?[only]:names;let total=0;const lens=seq.map(n=>{const d=S[n].dur||1;const l=S[n].loop?Math.max(d,2):d+.6;total+=l;return l});",
  "let x=seq.length===1&&S[seq[0]].loop?t:t%total,i=0;while(i<lens.length-1&&x>lens[i]){x-=lens[i];i++}const n=seq[i],d=S[n].dur||1;s={name:n,t:x,k:S[n].loop?(x/d)%1:clamp(x/d,0,1),dur:d}}",
  "const p={};for(const[k,v]of Object.entries(def.params||{}))p[k]=v&&typeof v==='object'&&'default'in v?v.default:v;Object.assign(p,params);",
  "g.setTransform(1,0,0,1,0,0);g.clearRect(0,0,c.width,c.height);g.save();try{fn(g,p,t,s,{ease:EASE,clamp,rand,img,W:c.width,H:c.height})}catch(x){g.restore();g.save();oops(x&&x.message||x)}g.restore()}",
  "frame();</scr" + "ipt></body></html>",
].join("\n");

export function PuppetPreview({ def, state, params, style }: { def: Json | null; state?: string; params?: Json; style?: React.CSSProperties }) {
  const ref = useRef<HTMLIFrameElement>(null);
  const send = () => ref.current?.contentWindow?.postMessage({ def, state: state || "", params: params || {} }, "*");
  useEffect(send, [def, state, JSON.stringify(params || {})]);
  return <iframe ref={ref} title="kukla" sandbox="allow-scripts" srcDoc={PREVIEW_DOC} onLoad={send} style={{ border: 0, width: "100%", aspectRatio: "1", display: "block", background: "repeating-conic-gradient(var(--hover) 0 25%, transparent 0 50%) 0 0 / 16px 16px", borderRadius: 8, ...style }} />;
}

export function PuppetsPage({ say }: { say: (s: string) => void }) {
  const [list, setList] = useState<Json[]>([]);
  const [defs, setDefs] = useState<Record<string, Json>>({});
  const [q, setQ] = useState("");
  const [open, setOpen] = useState("");
  const [state, setState] = useState("");
  const [films, setFilms] = useState<Json[]>([]);
  const [film, setFilm] = useState("");
  const [cut, setCut] = useState(false);
  const [tryParams, setTryParams] = useState<Json>({});
  useEffect(() => setTryParams({}), [open]);
  // the puppet maker: words in, a tried and saved puppet out
  const settings = useLoad(() => tool("settings"), []);
  const noModel = !!settings.data && (settings.data.llm?.provider || "none") === "none";
  const [maker, setMaker] = useState<{ open: boolean; name?: string }>({ open: false });
  const [mPrompt, setMPrompt] = useState("");
  const [mBusy, setMBusy] = useState<{ msg: string; img?: string } | null>(null);
  const [mAnswer, setMAnswer] = useState("");
  const stepLabel = (m: string) => (m.startsWith("try ") ? tx("Try {0} drawn", m.slice(4)) : m === "trying" ? tx("Drawing in the browser") : m === "puppet_save" ? tx("Saving") : m.startsWith("puppet_") ? tx("Looking at the library") : m || tx("Thinking"));
  const make = async () => {
    setMBusy({ msg: "" });
    setMAnswer("");
    try {
      const r = await runJob("puppet_ask", { prompt: mPrompt, ...(maker.name ? { name: maker.name } : {}) }, (_p, msg) => setMBusy((b) => { const [m, img] = String(msg || "").split("|"); return { msg: m, img: img || b?.img }; }));
      setMAnswer(r.answer || "");
      setMPrompt("");
      await load();
      if (r.saved) { setOpen(r.saved); setState(""); }
      say(r.saved ? tx("\"{0}\" puppet saved", r.saved) : tx("The assistant did not save a puppet; see its answer"));
    } catch (e: any) {
      say(e.message);
    }
    setMBusy(null);
  };
  const load = () => tool("puppet_list", {}).then(async (r) => {
    const ps: Json[] = r.puppets || [];
    setList(ps);
    const all: Record<string, Json> = {};
    await Promise.all(ps.map((p) => tool("puppet_get", { name: p.name }).then((x) => { all[p.name] = x.puppet; }).catch(() => {})));
    setDefs(all);
  }).catch((e) => say(e.message));
  useEffect(() => { load(); tool("hub_films", {}).then((r) => setFilms(r.films || [])).catch(() => {}); }, []);
  const fromImage = (f: File) => puppetFromImage(f, list.map((p) => p.name), cut).then((n) => { say(tx("\"{0}\" puppet saved", n)); load(); }).catch((e) => say(e.message));
  const copy = async (p: Json) => {
    const label = window.prompt(tx("Name of the copy"), `${p.label || p.name} 2`);
    if (!label) return;
    const name = puppetName(label, list.map((x) => x.name));
    try {
      await tool("puppet_save", { name, from: p.name, label });
      say(tx("Copy \"{0}\" saved", label));
      load();
      setOpen(name);
    } catch (e: any) {
      say(e.message);
    }
  };
  const toFilm = async (p: Json) => {
    if (!film) return say(tx("Choose a film first"));
    try {
      const st = (await tool("puppet_sync", { film, check: true })).puppets?.[p.name];
      if (!(st === "changed in this film" || String(st || "").startsWith("both changed"))) await tool("puppet_link", { film, name: p.name });
      const cur = await tool("hub_2d", { film });
      const fm = films.find((x) => x.id === film) || {};
      const [FW, FH, FD] = [fm.width || 1080, fm.height || 1920, fm.dur || 10];
      const [pw, ph] = p.size || [400, 400];
      const s = Math.min(1, (FW * 0.45) / pw, (FH * 0.45) / ph);
      const w = Math.round(pw * s), h = Math.round(ph * s);
      const t1 = Math.min(FD, 4);
      const items = [...(cur.items || []), { id: "p" + Date.now().toString(36), kind: "puppet", puppet: p.name, label: p.label, t0: 0, t1, x: Math.round((FW - w) / 2), y: Math.round((FH - h) / 2), w, h, opacity: 1, fade: 0.2, params: {}, states: [] }];
      await tool("hub_2d", { film, items });
      say(tx("{0} added to the film (0–{1} s, centred); move it from the Layers tab", p.label || p.name, t1));
    } catch (e: any) {
      say(e.message);
    }
  };
  const shown = list.filter((p) => !q || JSON.stringify(p).toLowerCase().includes(q.toLowerCase()));
  const sel = list.find((p) => p.name === open);
  const def = sel ? defs[sel.name] : null;
  return (
    <div className="page">
      <div className="wrap">
        <div className="page-head">
          <h1 className="grow">{tx("Puppets")}</h1>
          <input className="input" style={{ width: 240 }} placeholder={tx("Search: pencil, bubble, character…")} value={q} onChange={(e) => setQ(e.target.value)} />
          <label className="btn"><Upload />{tx("Puppet from picture")}<input type="file" accept="image/*" hidden onChange={(e) => { const f = e.target.files?.[0]; e.target.value = ""; if (f) fromImage(f); }} /></label>
          <label className="rowx sec" style={{ fontSize: 12, gap: 4 }} title={tx("The local person matte model removes the picture's background")}><input type="checkbox" checked={cut} onChange={(e) => setCut(e.target.checked)} />{tx("Remove background")}</label>
          <button className={`btn ${maker.open && !maker.name ? "primary" : ""}`} onClick={() => setMaker(maker.open && !maker.name ? { open: false } : { open: true })}><Sparkles />{tx("Puppet with AI")}</button>
        </div>
        <p className="sec" style={{ marginBottom: 16, maxWidth: 760 }}>{tx("Reusable drawings made by Claude and by you.")} <b>.ppet</b> {tx("is one drawing,")} <b>.rppet</b> {tx("is a rich puppet with named animations (a pencil: stands still, then writes). Linked into a film, it is copied into the film's")} <span className="num">puppets/</span> {tx("folder; a newer library version reaches the film with \"Get\".")}</p>
        {maker.open && (
          <div className="card" style={{ padding: 16, marginBottom: 16, display: "grid", gridTemplateColumns: "minmax(0, 1fr) 260px", gap: 16 }}>
            <div className="stack" style={{ gap: 8 }}>
              <h2>{maker.name ? tx("Change \"{0}\" with the assistant", maker.name) : tx("Puppet with AI")}</h2>
              {noModel && <div className="warn">{tx("No model chosen: pick Claude, OpenAI or a local server in Settings > Assistant.")}</div>}
              <textarea className="input" style={{ fontFamily: "var(--font)", fontSize: 13 }} rows={3} value={mPrompt} onChange={(e) => setMPrompt(e.target.value)} placeholder={maker.name ? tx("Add glasses, raise both hands when waving") : tx("A steaming coffee cup; states: still, sway. Its colour can be changed.")} onKeyDown={(e) => e.key === "Enter" && (e.ctrlKey || e.metaKey) && mPrompt.trim() && !mBusy && !noModel && make()} />
              <div className="rowx">
                <span className="grow muted" style={{ fontSize: 12, lineHeight: "18px" }}>{tx("The assistant writes the drawing code, tries it in the browser, looks at its own picture, fixes it and saves it to the library. Talking characters move their mouth with the film's sound.")}</span>
                <button className="btn sm ghost" onClick={() => setMaker({ open: false })}>{tx("Close")}</button>
                <button className="btn sm primary" disabled={!mPrompt.trim() || !!mBusy || noModel} onClick={make}><Sparkles />{mBusy ? tx("Running") : maker.name ? tx("Change") : tx("Make")}</button>
              </div>
              {mBusy && <div className="rowx sec" style={{ fontSize: 12 }}><span className="dot live" />{stepLabel(mBusy.msg)}</div>}
              {mAnswer && !mBusy && <div className="sec" style={{ fontSize: 12, lineHeight: "18px", whiteSpace: "pre-wrap" }}>{mAnswer}</div>}
            </div>
            <div>{mBusy?.img ? <img src={fileUrl(mBusy.img)} alt="" style={{ width: "100%", borderRadius: 8, border: "1px solid var(--line)" }} /> : <div className="muted" style={{ fontSize: 12, lineHeight: "18px" }}>{tx("Each try shows here: rows are the states, columns four moments of each.")}</div>}</div>
          </div>
        )}
        <div style={{ display: "grid", gridTemplateColumns: sel ? "minmax(0, 1fr) 360px" : "1fr", gap: 24, alignItems: "start" }}>
          <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(170px, 1fr))", gap: 12 }}>
            {shown.map((p) => (
              <button key={p.name} className="card" onClick={() => { setOpen(p.name); setState(""); }} style={{ padding: 10, textAlign: "left", cursor: "pointer", color: "inherit", font: "inherit", outline: open === p.name ? "2px solid var(--accent)" : undefined }}>
                <PuppetPreview def={defs[p.name] || null} style={{ pointerEvents: "none" }} />
                <div style={{ marginTop: 8, fontWeight: 600, fontSize: 13 }}>{p.label || p.name}</div>
                <div className="rowx" style={{ gap: 4, marginTop: 2 }}><span className="tag">.{p.kind}</span>{p.where === "sample" && <span className="tag">{tx("sample")}</span>}<span className="muted num" style={{ fontSize: 11 }}>v{p.version ?? 1}</span></div>
              </button>
            ))}
            {!shown.length && <span className="muted">{tx("No puppets. Ask Claude to \"save this as a puppet\", or upload a picture.")}</span>}
          </div>
          {sel && (
            <div className="card stack" style={{ padding: 16, gap: 12, position: "sticky", top: 0 }}>
              <div className="rowx"><h2 className="grow">{sel.label || sel.name}</h2><button className="btn sm ghost" onClick={() => setOpen("")}>{tx("Close")}</button></div>
              <PuppetPreview def={def} state={state} params={tryParams} />
              {sel.states?.length > 0 && (
                <div className="seg" style={{ flexWrap: "wrap" }}>
                  <button className={!state ? "on" : ""} onClick={() => setState("")}>{tx("All")}</button>
                  {sel.states.map((s: string) => <button key={s} className={state === s ? "on" : ""} onClick={() => setState(s)}>{def?.states?.[s]?.label || s}</button>)}
                </div>
              )}
              {sel.desc && <p className="sec" style={{ fontSize: 12, lineHeight: "18px" }}>{sel.desc}</p>}
              {def?.params && Object.keys(def.params).length > 0 && (
                <div className="stack" style={{ gap: 6 }}>
                  <div className="rowx"><b className="grow" style={{ fontWeight: 500 }}>{tx("Settings")}</b>{Object.keys(tryParams).length > 0 && <button className="btn sm ghost" onClick={() => setTryParams({})}>{tx("Reset")}</button>}</div>
                  <span className="muted" style={{ fontSize: 12 }}>{tx("Try values here; the preview follows. Films keep their own settings.")}</span>
                  <PuppetFields info={def} params={tryParams} states={[]} localT={0} onParams={setTryParams} onStates={() => {}} paramsOnly />
                </div>
              )}
              <div className="muted num" style={{ fontSize: 11, wordBreak: "break-all" }}>{sel.file}</div>
              <div className="rowx">
                <select className="select grow" value={film} onChange={(e) => setFilm(e.target.value)}>
                  <option value="">{tx("Choose a film")}</option>
                  {films.map((f) => <option key={f.id} value={f.id}>{f.title || f.id}</option>)}
                </select>
                <button className="btn sm primary" onClick={() => toFilm(sel)}><Plus />{tx("Add to film")}</button>
              </div>
              <div className="rowx" style={{ flexWrap: "wrap" }}>
                <button className="btn sm" onClick={() => { setMaker({ open: true, name: sel.name }); setMAnswer(""); window.scrollTo(0, 0); }}><Sparkles />{tx("Change with assistant")}</button>
                <button className="btn sm" onClick={() => copy(sel)}>{tx("Duplicate")}</button>
                <button className="btn sm" onClick={() => tool("puppet_get", { name: sel.name, as: "js" }).then((r) => navigator.clipboard.writeText(r.js)).then(() => say(tx("JS module copied"))).catch((e) => say(e.message))}>{tx("Copy as JS")}</button>
                <button className="btn sm" onClick={() => navigator.clipboard.writeText(tx("Use the \"{0}\" puppet from the Linnea puppet library (puppet_get name={1}, puppet_link).", sel.name, sel.name)).then(() => say(tx("Message for Claude copied"))).catch((e) => say(e.message))}>{tx("Give to Claude")}</button>
                <button className="btn sm ghost" onClick={() => tool("reveal", { path: sel.file }).catch((e) => say(e.message))}>{tx("Show in folder")}</button>
                {sel.where === "library" && <button className="btn sm ghost" onClick={() => window.confirm(tx("Move \"{0}\" to the Recycle Bin? Films linked to it keep their own copies.", sel.label || sel.name)) && tool("trash", { path: sel.file }).then(() => { setOpen(""); load(); say(tx("Moved to the Recycle Bin")); }).catch((e) => say(e.message))}><Trash2 />{tx("Delete")}</button>}
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
