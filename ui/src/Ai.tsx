// AI tools with local and API choices, publishing, and the key list.
import { tx } from "./i18n";
import { useState } from "react";
import { Send } from "lucide-react";
import { Json, runJob, tool, useLoad } from "./api";

const LANGS = ["en", "tr", "de", "es", "fr", "it", "pt", "ru", "ar", "nl", "ja", "ko", "zh", "hi"];

export function AiPanel({ proj, rid, reel, run, busy, patch, say }: Json) {
  const [lang, setLang] = useState("en");
  const [dubBy, setDubBy] = useState("elevenlabs-dubbing");
  const [hey, setHey] = useState("English (United States)");
  const [src, setSrc] = useState("pexels");
  const has = !!proj.transcript;
  const s = useLoad(() => tool("settings"), []);
  // translation and the voice-per-sentence dubs run on the assistant's model
  const noModel = !!s.data && (s.data.llm?.provider || "none") === "none";
  return (
    <div className="stack" style={{ gap: 8 }}>
      <h2>{tx("AI")}</h2>
      {!has && <p className="muted" style={{ fontSize: 12 }}>{tx("Transcribe first for translation, dubbing and automatic B-roll.")}</p>}
      {noModel && <p className="muted" style={{ fontSize: 12 }}>{tx("Choose a model in Settings > Assistant for translation and sentence-by-sentence dubbing. B-roll also works without one (by keywords).")}</p>}
      <div className="rowx">
        <select className="select" style={{ width: 72 }} value={lang} onChange={(e) => setLang(e.target.value)}>{LANGS.map((l) => <option key={l}>{l}</option>)}</select>
        <button className="btn sm grow" disabled={busy || !has || noModel} onClick={() => run(tx("Translating"), "translate", { reel: rid, lang })}>{tx("Translate captions")}</button>
        {reel.captions?.lang && <button className="btn sm ghost" onClick={() => patch({ captions: { lang: null } })}>{tx("Original language")}</button>}
      </div>
      <div className="rowx">
        <select className="select grow" value={dubBy} onChange={(e) => setDubBy(e.target.value)}>
          <option value="elevenlabs-dubbing">{tx("ElevenLabs dubbing (keeps the voice, API)")}</option>
          <option value="elevenlabs">{tx("Translation + ElevenLabs voice (API)")}</option>
          <option value="openai">{tx("Translation + OpenAI voice (API)")}</option>
          <option value="piper">{tx("Translation + Piper voice (local)")}</option>
        </select>
        <button className="btn sm" disabled={busy || !has || (noModel && dubBy !== "elevenlabs-dubbing")} onClick={() => run(tx("Dub"), "dub", { reel: rid, lang, provider: dubBy })}>{tx("Dub")}</button>
      </div>
      {reel.audio?.dub && <div className="rowx" style={{ fontSize: 12 }}><span className="grow sec">{tx("Audio:")} {reel.audio.dub.split("/").pop()}</span><button className="btn sm ghost" onClick={() => patch({ audio: { dub: null } })}>{tx("Original audio")}</button></div>}
      <div className="rowx">
        <input className="input grow" value={hey} onChange={(e) => setHey(e.target.value)} title={tx("HeyGen language name")} />
        <button className="btn sm" disabled={busy} onClick={() => run(tx("HeyGen translation"), "heygen_translate", { reel: rid, language: hey })}>HeyGen</button>
      </div>
      <div className="rowx">
        <select className="select grow" value={src} onChange={(e) => setSrc(e.target.value)}>
          <option value="pexels">{tx("Pexels (stock video)")}</option>
          <option value="pixabay">{tx("Pixabay (stock video)")}</option>
          <option value="openai">{tx("OpenAI image")}</option>
          <option value="gemini">{tx("Gemini image")}</option>
          <option value="local">{tx("Local Stable Diffusion")}</option>
        </select>
        <button className="btn sm" disabled={busy || !has} onClick={() => run("B-roll", "broll_auto", { reel: rid, source: src })}>{tx("Automatic B-roll")}</button>
      </div>
      <div className="rowx">
        <button className="btn sm grow" disabled={busy || !has} onClick={() => run(tx("Adding the hook"), "hook_add", { reel: rid })}>{tx("Add hook")}</button>
        <button className="btn sm ghost" disabled={busy} onClick={() => run(tx("Removing the hook"), "hook_add", { reel: rid, remove: true })}>{tx("Remove hook")}</button>
        <button className="btn sm ghost" disabled={busy} title={tx("Podcasts and interviews: the frame follows whoever speaks, and splits when both react")} onClick={() => run(tx("Finding the speakers"), "speakers").then((r: Json) => r && patch({ crop: { mode: "speaker" } }))}>{tx("Speakers")}</button>
      </div>
      <button className="btn sm" disabled={busy} onClick={() => { const u = window.prompt(tx("Public URL of the video for Sieve (leave empty if a local command is set)")) ?? null; if (u !== null) run(tx("Eye contact"), "eye_contact", u ? { provider: "sieve", video_url: u } : { provider: "command" }); }}>{tx("Fix eye contact")}</button>
    </div>
  );
}

export function PublishButton({ file, title, say }: { file: string; title: string; say: (s: string) => void }) {
  const [open, setOpen] = useState(false);
  const [svc, setSvc] = useState("youtube");
  const [t, setT] = useState(title);
  const [privacy, setPrivacy] = useState("private");
  const [busy, setBusy] = useState(false);
  const go = async () => {
    if (!window.confirm(tx("Post to your {0} account?", svc === "youtube" ? "YouTube" : svc === "tiktok" ? "TikTok" : "Instagram"))) return;
    setBusy(true);
    try {
      const r = await runJob(`publish_${svc}`, svc === "youtube" ? { file, title: t, description: `${t}\n#Shorts`, privacy } : svc === "tiktok" ? { file, title: t, mode: privacy === "public" ? "direct" : "draft", privacy: privacy === "public" ? "PUBLIC_TO_EVERYONE" : "SELF_ONLY" } : { file, caption: t });
      say(r.url ? tx("Uploaded: {0}", r.url) : r.where || r.note || tx("Sent"));
      setOpen(false);
    } catch (e: any) {
      say(e.message);
    }
    setBusy(false);
  };
  if (!open) return <button className="btn icon sm ghost" title={tx("Publish")} onClick={() => setOpen(true)}><Send /></button>;
  return (
    <div className="card" style={{ position: "absolute", right: 16, width: 280, padding: 12, zIndex: 30, display: "flex", flexDirection: "column", gap: 8, boxShadow: "0 12px 32px rgba(0,0,0,.35)" }}>
      <div className="seg">{[["youtube", "YouTube"], ["tiktok", "TikTok"], ["instagram", "Instagram"]].map(([k, l]) => <button key={k} className={svc === k ? "on" : ""} onClick={() => setSvc(k)}>{l}</button>)}</div>
      <input className="input" value={t} onChange={(e) => setT(e.target.value)} placeholder={tx("Title / description")} />
      {svc !== "instagram" && <select className="select" value={privacy} onChange={(e) => setPrivacy(e.target.value)}><option value="private">{svc === "tiktok" ? tx("Draft (finish in the app)") : tx("Private")}</option>{svc === "youtube" && <option value="unlisted">{tx("Unlisted")}</option>}<option value="public">{tx("Public")}</option></select>}
      <div className="rowx"><button className="btn sm ghost grow" onClick={() => setOpen(false)}>{tx("Cancel")}</button><button className="btn sm primary grow" disabled={busy} onClick={go}>{busy ? tx("Sending") : tx("Send")}</button></div>
    </div>
  );
}

const KEY_LABEL: Record<string, string> = {
  openai: "OpenAI", groq: "Groq", elevenlabs: "ElevenLabs", anthropic: "Anthropic (Claude)", pexels: "Pexels", pixabay: "Pixabay", heygen: "HeyGen", gemini: "Google Gemini", sieve: "Sieve",
  tiktok: tx("TikTok access token"), instagram: tx("Instagram access token"), youtube_client_id: tx("YouTube OAuth client ID"), youtube_client_secret: tx("YouTube OAuth client secret"), youtube_refresh: tx("YouTube session"),
};

export function KeysEditor({ s, set, say }: Json) {
  const [vals, setVals] = useState<Record<string, string>>({});
  const [ytUrl, setYtUrl] = useState("");
  return (
    <div className="card box list-rows">
      {Object.keys(s.keys || {}).filter((k) => k !== "youtube_refresh").map((k) => (
        <div className="line" key={k}>
          <div>{KEY_LABEL[k] || k}<small>{s.keys[k] ? tx("Saved") : tx("None")}</small></div>
          <input className="input" type="password" placeholder={tx("New value")} value={vals[k] || ""} onChange={(e) => setVals({ ...vals, [k]: e.target.value })} />
          <button className="btn sm" disabled={!vals[k]} onClick={() => set({ keys: { [k]: vals[k] } }).then(() => setVals({ ...vals, [k]: "" }))}>{tx("Save")}</button>
        </div>
      ))}
      <div className="line">
        <div>YouTube<small>{s.keys.youtube_refresh ? tx("Connected") : tx("Not connected")}</small></div>
        <span className="sec" style={{ fontSize: 12, wordBreak: "break-all" }}>{ytUrl ? <a href={ytUrl} target="_blank" rel="noreferrer">{tx("Open this link in your browser")}</a> : tx("Save the client ID and secret first")}</span>
        <button className="btn sm" onClick={() => tool("publish_login").then((r) => { setYtUrl(r.url); navigator.clipboard.writeText(r.url).catch(() => {}); say(tx("Sign-in link copied; open it in your browser")); }).catch((e) => say(e.message))}>{tx("Connect")}</button>
      </div>
      {[["instagram.user_id", tx("Instagram account ID")], ["matte.model", tx("Matte model (rvm, rvm-hq, birefnet-lite, birefnet-portrait)")], ["images.local_url", tx("Local image server (Automatic1111/Forge)")], ["eye_contact.command", tx("Eye contact command ({in} {out})")], ["voices.elevenlabs", tx("ElevenLabs voice ID")]].map(([k, l]) => {
        const [a, b] = k.split(".");
        return (
          <div className="line" key={k}>
            <div>{l}</div>
            <input className="input" defaultValue={s[a]?.[b] || ""} onBlur={(e) => e.target.value !== (s[a]?.[b] || "") && set({ [a]: { [b]: e.target.value } })} />
            <span />
          </div>
        );
      })}
    </div>
  );
}
