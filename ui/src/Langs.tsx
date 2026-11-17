// Films in more than one language: the texts the film draws, translated per language (tweaks.json langs),
// previewed live on the frame, checked for texts that no longer fit, and rendered as their own files.
import { tx } from "./i18n";
import { useEffect, useMemo, useRef, useState } from "react";
import { AlertTriangle, Check, Copy, Languages, Play, Plus, Sparkles, Trash2 } from "lucide-react";
import { Json, tool } from "./api";
import type { Reg } from "./Elements";

export type ViewLang = { code: string; texts: Json } | null;

/** Common languages, each named in its own language. */
export const COMMON: [string, string][] = [
  ["en", "English"], ["tr", "Türkçe"], ["de", "Deutsch"], ["fr", "Français"], ["es", "Español"], ["it", "Italiano"],
  ["pt-BR", "Português (Brasil)"], ["nl", "Nederlands"], ["pl", "Polski"], ["ru", "Русский"], ["uk", "Українська"],
  ["ar", "العربية"], ["hi", "हिन्दी"], ["ja", "日本語"], ["ko", "한국어"], ["zh-Hans", "中文 (简体)"], ["id", "Bahasa Indonesia"],
];

type Props = {
  film: string; reg: Reg[]; scanning: boolean; t: number; seek: (t: number) => void; say: (s: string) => void;
  view: ViewLang; setView: (v: ViewLang) => void; run: (label: string, name: string, args: Json) => Promise<Json | undefined>;
  copy: (text: string, msg: string) => void;
};

export function LangsPanel({ film, reg, scanning, t, seek, say, view, setView, run, copy }: Props) {
  const [langs, setLangs] = useState<Json[]>([]);
  const [pick, setPick] = useState("de");
  const [problems, setProblems] = useState<Json | null>(null);
  const [draft, setDraft] = useState<Json>({});
  const save = useRef<Record<string, number>>({});
  const load = () => tool("hub_lang", { film }).then((r) => setLangs(r.langs || [])).catch((e) => say(e.message));
  useEffect(() => { load(); }, [film]);
  const cur = langs.find((l) => l.lang === view?.code) || null;
  // only the texts: an element that draws no words has nothing to translate
  const texts = useMemo(() => reg.filter((r) => typeof r.text === "string" && r.text.trim()).sort((a, b) => a.t0 - b.t0), [reg]);
  useEffect(() => { setDraft({ ...(cur?.texts || {}) }); setProblems(null); }, [view?.code, langs]);
  const show = (code: string) => {
    const l = langs.find((x) => x.lang === code);
    setView(code && l ? { code, texts: l.texts || {} } : null);
  };
  const add = async () => {
    const name = COMMON.find((c) => c[0] === pick)?.[1] || pick;
    try {
      await tool("hub_lang", { film, action: "set", lang: pick, name, texts: {} });
      await load();
      setView({ code: pick, texts: {} });
    } catch (e: any) { say(e.message); }
  };
  // typing shows on the frame at once and is saved a moment later
  const type = (id: string, value: string) => {
    if (!view) return;
    const next = { ...draft, [id]: value };
    setDraft(next);
    setView({ code: view.code, texts: next });
    const code = view.code;
    clearTimeout(save.current[id]);
    save.current[id] = window.setTimeout(() => {
      tool("hub_lang", { film, action: "set", lang: code, texts: { [id]: value } }).then((r) => setLangs(r.langs || [])).catch((e) => say(e.message));
    }, 500);
  };
  const translate = async (redo = false) => {
    if (!view) return;
    const r = await run(tx("Translating into {0}", cur?.name || view.code), "hub_lang_scan", { action: "translate", lang: view.code, name: cur?.name, redo });
    if (r) say(tx("{0} texts translated", r.translated ?? 0));
    const l = await tool("hub_lang", { film });
    setLangs(l.langs || []);
    const now = (l.langs || []).find((x: Json) => x.lang === view.code);
    if (now) setView({ code: view.code, texts: now.texts || {} });
  };
  const check = async () => {
    if (!view) return;
    const r = await run(tx("Checking the layout in {0}", cur?.name || view.code), "hub_lang_scan", { action: "check", lang: view.code });
    if (r) setProblems(r);
  };
  const done = texts.filter((x) => (draft[x.id] || "").trim()).length;
  const forClaude = () => copy(tx("Translate the film \"{0}\" in Linnea into {1} ({2}): read its texts with hub_lang_scan action=texts, write them with hub_lang action=set lang={2} texts={id: text} (keep each about as long as the original), then check with hub_lang_scan action=check lang={2} and shorten anything it reports.", film, cur?.name || view?.code, view?.code), tx("Message for Claude copied"));

  return (
    <div className="stack" style={{ gap: 10 }}>
      <div className="rowx"><h2 className="grow"><Languages style={{ width: 15, height: 15, verticalAlign: -2 }} /> {tx("Languages")}</h2></div>
      <p className="sec" style={{ fontSize: 12, margin: 0 }}>{tx("One film, many languages: each language replaces the texts on screen, can have its own sound, and renders as its own file.")}</p>
      <div className="rowx" style={{ flexWrap: "wrap", gap: 4 }}>
        <button className={`btn sm ${!view ? "" : "ghost"}`} onClick={() => show("")}>{tx("Original")}</button>
        {langs.map((l) => (
          <button key={l.lang} className={`btn sm ${view?.code === l.lang ? "" : "ghost"}`} onClick={() => show(l.lang)} title={l.name}>
            {l.lang}{texts.length > 0 && <span className="muted num" style={{ marginLeft: 4 }}>{Math.min(l.count, texts.length)}/{texts.length}</span>}
          </button>
        ))}
      </div>
      <div className="rowx">
        <select className="select grow" value={pick} onChange={(e) => setPick(e.target.value)}>
          {COMMON.filter((c) => !langs.some((l) => l.lang === c[0])).map(([c, n]) => <option key={c} value={c}>{n} ({c})</option>)}
        </select>
        <button className="btn sm" onClick={add}><Plus />{tx("Add")}</button>
      </div>

      {view && cur && (
        <>
          <div className="rowx" style={{ flexWrap: "wrap", gap: 4 }}>
            <button className="btn sm" onClick={() => translate(false)} title={tx("The assistant set in Settings translates the texts that have none yet")}><Sparkles />{tx("Translate the rest")}</button>
            <button className="btn sm ghost" onClick={forClaude}><Copy />{tx("Ask Claude")}</button>
            <button className="btn sm ghost" onClick={check} title={tx("Finds texts the translation pushed off the frame or made much wider")}><AlertTriangle />{tx("Check layout")}</button>
            <button className="btn sm ghost" onClick={() => run(tx("Render in {0}", cur.name || cur.lang), "hub_render", { mode: "full", lang: cur.lang })}><Play />{tx("Render")}</button>
            <span className="grow" />
            <button className="btn icon sm ghost" title={tx("Remove this language")} onClick={async () => {
              if (!window.confirm(tx("Remove {0} and its translations?", cur.name || cur.lang))) return;
              await tool("hub_lang", { film, action: "remove", lang: cur.lang });
              setView(null);
              load();
            }}><Trash2 /></button>
          </div>
          <div className="field">
            <label>{tx("Sound for this language (optional)")}</label>
            <input className="input" placeholder={`audio/mix.${cur.lang}.wav`} defaultValue={cur.audio || ""} key={cur.lang}
              onBlur={(e) => { if ((e.target.value || "") !== (cur.audio || "")) tool("hub_lang", { film, action: "set", lang: cur.lang, audio: e.target.value }).then(() => { say(tx("Sound saved")); load(); }).catch((x) => say(x.message)); }} />
          </div>
          {problems && (
            <div className="card" style={{ padding: 8, fontSize: 12 }}>
              {problems.ok ? <span><Check style={{ width: 13, height: 13, verticalAlign: -2 }} /> {tx("Everything fits and is translated.")}</span> : (
                <div className="stack" style={{ gap: 4 }}>
                  {(problems.problems || []).map((p: Json) => (
                    <button key={p.id} className="opt" style={{ padding: 4 }} onClick={() => seek(p.t || 0)}><span><b>{p.text || p.id}</b><span>{p.problem} · {(p.t || 0).toFixed(1)} s</span></span></button>
                  ))}
                  {(problems.untranslated || []).length > 0 && <span className="muted">{tx("{0} texts are not translated yet", problems.untranslated.length)}</span>}
                </div>
              )}
            </div>
          )}
          <div className="rowx"><span className="sec grow" style={{ fontSize: 12 }}>{tx("{0} of {1} texts translated", done, texts.length)}</span>{scanning && <span className="muted" style={{ fontSize: 12 }}>{tx("Finding texts…")}</span>}</div>
          <div className="stack" style={{ gap: 8 }}>
            {texts.map((x) => {
              const on = t >= x.t0 && t < x.t1;
              return (
                <div key={x.id} className="stack" style={{ gap: 2 }}>
                  <button style={{ textAlign: "left", fontSize: 12, background: "none", border: 0, padding: 0, cursor: "pointer", color: on ? "var(--accent)" : "var(--text-2)" }} onClick={() => seek(x.t0)} title={tx("Go to {0} s", x.t0.toFixed(1))}>
                    <span className="muted num">{x.t0.toFixed(1)}s</span> {x.text}
                  </button>
                  <textarea className="input" rows={Math.min(4, String(x.text).split("\n").length)} value={draft[x.id] || ""} placeholder={x.text} onChange={(e) => type(x.id, e.target.value)} style={{ resize: "vertical", fontSize: 13, fontFamily: "inherit", lineHeight: "18px" }} />
                </div>
              );
            })}
            {!texts.length && !scanning && <span className="muted" style={{ fontSize: 12 }}>{tx("No texts found. Texts the film draws with KE.text, or with fillText, show up here.")}</span>}
          </div>
        </>
      )}
    </div>
  );
}
