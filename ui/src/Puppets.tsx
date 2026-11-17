// Puppets: reusable drawings from the library (.ppet) and animated ones with states (.rppet).
// Place one on the film as a layer, change its params and when each state starts, send a film's copy back
// to the library, or turn any picture into a puppet with ready-made motions.
import { tx } from "./i18n";
import { useEffect, useState } from "react";
import { Plus, RefreshCw, Upload, Trash2 } from "lucide-react";
import { Json, tool } from "./api";
import type { Layers } from "./Layers";

const r2 = (x: number) => Math.round(x * 100) / 100;
export type PuppetInfo = { label?: string; kind?: string; size?: number[]; params: Json; states: Json; initial?: string };

// a picture as a rich puppet: still, float, wiggle, pop, spin
export const IMAGE_DRAW = `function (g, p, t, s, u) {
  const W = u.W, H = u.H, k = s.k, st = s.t;
  g.save(); g.translate(W / 2, H / 2);
  if (s.name === 'float') g.translate(0, Math.sin(st * Math.PI * 2 / s.dur) * H * 0.04);
  if (s.name === 'wiggle') g.rotate(Math.sin(st * 18) * 0.06);
  if (s.name === 'pop') { const e = u.ease.back(k); g.scale(e, e); }
  if (s.name === 'spin') { g.rotate(u.ease.out(k) * Math.PI * 2); const e = u.ease.out(k); g.scale(e, e); }
  g.globalAlpha *= p.opacity;
  g.drawImage(u.img.img, -W / 2, -H / 2, W, H);
  g.restore();
}`;
export const IMAGE_STATES = { still: { dur: 1, loop: true, label: tx("Still") }, float: { dur: 2.4, loop: true, label: tx("Float") }, wiggle: { dur: 1, loop: true, label: tx("Wiggle") }, pop: { dur: 0.5, loop: false, label: tx("Pops in") }, spin: { dur: 0.8, loop: false, label: tx("Spin in") } };

const TR_ASCII: Record<string, string> = { ç: "c", ğ: "g", ı: "i", İ: "i", ö: "o", ş: "s", ü: "u", Ç: "c", Ğ: "g", Ö: "o", Ş: "s", Ü: "u" };
/** A file name for a puppet: Turkish letters kept readable, never the name of one that exists. */
export function puppetName(label: string, taken: string[]): string {
  const base = label.replace(/[çğıİöşüÇĞÖŞÜ]/g, (c) => TR_ASCII[c] || c).toLowerCase().replace(/[^a-z0-9_-]+/g, "-").replace(/^-+|-+$/g, "") || "resim";
  let name = base;
  for (let n = 2; taken.includes(name); n++) name = `${base}-${n}`;
  return name;
}

/** Turn a picture into a puppet with ready-made motions; very large pictures are scaled down to 2048 px. */
export function puppetFromImage(f: File, taken: string[], cutout = false): Promise<string> {
  return new Promise((ok, no) => {
    const rd = new FileReader();
    rd.onerror = () => no(new Error(tx("The picture could not be read")));
    rd.onload = () => {
      const im = new Image();
      im.onerror = () => no(new Error(tx("This file could not be opened as a picture")));
      im.onload = async () => {
        const k = Math.min(1, 2048 / Math.max(im.naturalWidth, im.naturalHeight, 1));
        const w = Math.max(1, Math.round(im.naturalWidth * k)), h = Math.max(1, Math.round(im.naturalHeight * k));
        let data = String(rd.result);
        if (k < 1) {
          const c = document.createElement("canvas");
          c.width = w;
          c.height = h;
          c.getContext("2d")!.drawImage(im, 0, 0, w, h);
          data = c.toDataURL("image/png");
        }
        const label = f.name.replace(/\.[^.]+$/, "");
        const name = puppetName(label, taken);
        try {
          // the local matte model cuts the subject out of its background
          if (cutout) data = (await tool("puppet_cutout", { data })).data;
          await tool("puppet_save", { name, label, desc: "A picture with motions: still, float, wiggle, pop, spin.", tags: ["image", "picture"], size: [w, h], assets: { img: data }, params: { opacity: { type: "number", default: 1, min: 0, max: 1, label: tx("Opacity") } }, states: IMAGE_STATES, initial: "still", draw: IMAGE_DRAW });
          ok(name);
        } catch (e) {
          no(e);
        }
      };
      im.src = String(rd.result);
    };
    rd.readAsDataURL(f);
  });
}

const changedHere = (st?: string) => st === "changed in this film" || String(st || "").startsWith("both changed");

export function PuppetLibrary({ film, L, t, W, H, dur, say }: { film: string; L: Layers; t: number; W: number; H: number; dur?: number; say: (s: string) => void }) {
  const [list, setList] = useState<Json[]>([]);
  const [sync, setSync] = useState<Json>({});
  const [own, setOwn] = useState<string[]>([]);
  const [q, setQ] = useState("");
  const load = () => {
    tool("puppet_list", { film }).then((r) => {
      setList(r.puppets || []);
      // puppets the film made for itself: in its folder, never linked from the library
      const names = new Set((r.puppets || []).map((p: Json) => p.name));
      setOwn((r.local || []).map((f: string) => f.replace(/\.r?ppet$/, "")).filter((n: string) => !names.has(n) && !(r.linked || {})[n]));
    }).catch(() => {});
    tool("puppet_sync", { film, check: true }).then((r) => setSync(r.puppets || {})).catch(() => {});
  };
  useEffect(load, [film]);
  const add = async (p: Json) => {
    // a copy the film changed is what the film draws: linking again would refuse (or lose it)
    if (!changedHere(sync[p.name])) {
      try {
        await tool("puppet_link", { film, name: p.name });
      } catch (e: any) {
        return say(e.message);
      }
    }
    const [pw, ph] = p.size || [400, 400];
    const s = Math.min(1, (W * 0.45) / pw, (H * 0.45) / ph);
    const w = Math.round(pw * s), h = Math.round(ph * s);
    const t0 = r2(Math.max(0, Math.min(t, (dur ?? t + 4) - 0.5)));
    L.update([...L.items, { id: "p" + Date.now().toString(36), kind: "puppet", puppet: p.name, label: p.label, t0, t1: r2(Math.min(dur ?? t0 + 4, t0 + 4)), x: Math.round((W - w) / 2), y: Math.round((H - h) / 2), w, h, opacity: 1, fade: 0.2, params: {}, states: [] }]);
    say(tx("{0} added to the film", p.label || p.name));
    load();
  };
  const fromImage = (f: File) => puppetFromImage(f, list.map((p) => p.name)).then((n) => { say(tx("\"{0}\" saved to the library", n)); load(); }).catch((e) => say(e.message));
  const act = (p: Promise<any>, msg: string) => p.then(() => { say(msg); load(); }).catch((e) => say(e.message));
  const shown = list.filter((p) => !q || JSON.stringify(p).toLowerCase().includes(q.toLowerCase()));
  return (
    <div className="stack" style={{ gap: 6 }}>
      <div className="rowx">
        <h2 className="grow">{tx("Puppet library")}</h2>
        <label className="btn icon sm ghost" title={tx("Make a puppet from a picture")}><Upload /><input type="file" accept="image/*" hidden onChange={(e) => { const f = e.target.files?.[0]; e.target.value = ""; if (f) fromImage(f); }} /></label>
        <button className="btn icon sm ghost" title={tx("Refresh")} onClick={load}><RefreshCw /></button>
      </div>
      {own.length > 0 && (
        <div className="card" style={{ padding: 8, fontSize: 12 }}>
          <div style={{ marginBottom: 6 }}>{tx("This film has {0} puppets of its own that the library does not have: {1}", own.length, own.join(", "))}</div>
          <button className="btn sm" onClick={() => act(tool("puppet_harvest", { film }), tx("Saved to the library"))}>{tx("Save them to the library")}</button>
        </div>
      )}
      <input className="input" placeholder={tx("Search: pencil, bubble, arrow…")} value={q} onChange={(e) => setQ(e.target.value)} />
      {shown.map((p) => {
        const st = sync[p.name];
        return (
          <div key={p.name} className="rowx" style={{ fontSize: 12, alignItems: "flex-start" }}>
            <div className="grow" style={{ minWidth: 0 }}>
              <div style={{ fontWeight: 600 }}>{p.label || p.name} <span className="muted num">.{p.kind} v{p.version ?? 1}</span></div>
              <div className="muted" style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{p.states?.length ? tx("States: {0}", p.states.join(", ")) : p.desc}</div>
              {st && st !== "current" && (
                <div className="rowx" style={{ gap: 4 }}>
                  <span style={{ color: "var(--accent)" }}>{st === "library has a newer version" ? tx("Newer version in the library") : st === "changed in this film" ? tx("Changed in this film") : st === "missing in this film" ? tx("Missing in this film") : String(st).startsWith("both changed") ? tx("Both changed") : String(st).startsWith("gone") ? tx("Deleted from the library") : st}</span>
                  {(st === "library has a newer version" || st === "missing in this film") && <button className="btn sm ghost" onClick={() => act(tool("puppet_sync", { film }), tx("Puppet updated"))}>{tx("Get")}</button>}
                  {changedHere(st) && <button className="btn sm ghost" onClick={() => act(tool("puppet_save", { film, name: p.name }), tx("The film's version saved to the library"))}>{String(st).startsWith("both") ? tx("Send the film's version") : tx("Send to library")}</button>}
                  {String(st).startsWith("both changed") && <button className="btn sm ghost" onClick={() => window.confirm(tx("The film's changes will be replaced by the library version. Continue?")) && act(tool("puppet_link", { film, name: p.name, force: true }), tx("Library version taken"))}>{tx("Take the library version")}</button>}
                </div>
              )}
            </div>
            <button className="btn icon sm" title={tx("Add at this second")} onClick={() => add(p)}><Plus /></button>
          </div>
        );
      })}
      {!shown.length && <span className="muted" style={{ fontSize: 12 }}>{tx("The library is empty. Ask Claude to \"save this as a puppet\", or upload a picture.")}</span>}
    </div>
  );
}

/** Params and state times of one puppet (a layer, or a code element drawn with KE.puppet). */
export function PuppetFields({ info, params, states, localT, onParams, onStates, onLipsync, paramsOnly }: { info: PuppetInfo | null; params: Json; states: Json[]; localT: number; onParams: (p: Json) => void; onStates: (s: Json[]) => void; onLipsync?: () => void; paramsOnly?: boolean }) {
  const [pick, setPick] = useState("");
  if (!info) return <span className="muted" style={{ fontSize: 12 }}>{tx("Loading the puppet file…")}</span>;
  const names = Object.keys(info.states || {});
  const field = (k: string, d: Json) => {
    const v = params[k] ?? d?.default;
    const set = (x: any) => onParams({ ...params, [k]: x });
    const label = d?.label || k;
    if (d?.type === "color") return <div className="field" key={k}><label>{label}</label><input className="input" type="color" style={{ padding: 2, width: 40 }} value={v || "#ffffff"} onChange={(e) => set(e.target.value)} /></div>;
    if (d?.type === "bool") return <div className="field" key={k}><label>{label}</label><input type="checkbox" checked={!!v} onChange={(e) => set(e.target.checked)} /></div>;
    if (d?.type === "select") return <div className="field" key={k}><label>{label}</label><select className="select" value={v ?? ""} onChange={(e) => set(e.target.value)}>{(d.options || []).map((o: any) => <option key={String(o)} value={o}>{String(o)}</option>)}</select></div>;
    if (d?.type === "number") return <div className="field" key={k}><label>{label}</label><input className="input num" type="number" step={d.step ?? 0.05} min={d.min} max={d.max} key={`${k}${v}`} defaultValue={v} onBlur={(e) => { let x = +e.target.value; if (!isFinite(x)) return; if (d.min != null) x = Math.max(d.min, x); if (d.max != null) x = Math.min(d.max, x); if (x !== v) set(x); }} /></div>;
    return <div className="field" key={k} style={{ gridColumn: "1 / -1" }}><label>{label}</label><input className="input" key={`${k}${v}`} defaultValue={v ?? ""} onBlur={(e) => e.target.value !== v && set(e.target.value)} /></div>;
  };
  return (
    <div className="stack" style={{ gap: 8 }}>
      {Object.keys(info.params || {}).length > 0 && <div className="grid2">{Object.entries(info.params).map(([k, d]) => field(k, d))}</div>}
      {info.params?.mouth && (
        <div className="stack" style={{ gap: 4 }}>
          <span className="muted" style={{ fontSize: 12, lineHeight: "18px" }}>{tx("Talking puppet: leave the mouth and eye fields alone and the mouth follows the film's sound while the eyes blink on their own.")}{params.mouth !== undefined ? tx(" The mouth is set by hand now.") : ""}</span>
          <div className="rowx">
            {onLipsync && <button className="btn sm" onClick={onLipsync}>{tx("Make the mouth from the film's sound")}</button>}
            {(params.mouth !== undefined || params.blink !== undefined) && <button className="btn sm ghost" onClick={() => { const n = { ...params }; delete n.mouth; delete n.blink; onParams(n); }}>{tx("Leave it to the sound")}</button>}
          </div>
        </div>
      )}
      {names.length > 0 && !paramsOnly && (
        <div className="stack" style={{ gap: 6 }}>
          <h2>{tx("States")}</h2>
          {[...states].sort((a, b) => a.t0 - b.t0).map((s, i) => (
            <div key={i} className="rowx" style={{ fontSize: 12 }}>
              <span className="num" style={{ width: 60 }}>{r2(s.t0)} {tx("s")}</span>
              <span className="grow">{info.states[s.state]?.label || s.state}</span>
              <input className="input num" title={tx("Transition, s: a soft blend from the previous state (0: a cut)")} style={{ width: 56, height: 26 }} type="number" step={0.1} min={0} key={`b${i}${s.blend}`} defaultValue={s.blend ?? 0} onBlur={(e) => { const b = Math.max(0, +e.target.value || 0); if (b !== (s.blend ?? 0)) onStates(states.map((x) => (x === s ? { ...x, blend: b } : x))); }} />
              <button className="btn icon sm ghost" onClick={() => onStates(states.filter((x) => x !== s))}><Trash2 /></button>
            </div>
          ))}
          {!states.length && <span className="muted" style={{ fontSize: 12 }}>{tx("Plays \"")}{info.states[info.initial || names[0]]?.label || info.initial || names[0]}{tx("\" from start to end.")}</span>}
          <div className="rowx">
            <select className="select grow" value={pick || names[0]} onChange={(e) => setPick(e.target.value)}>
              {names.map((n) => <option key={n} value={n}>{info.states[n]?.label || n}</option>)}
            </select>
            <button className="btn sm" onClick={() => onStates([...states.filter((x) => Math.abs(x.t0 - localT) > 0.02), { state: pick || names[0], t0: r2(Math.max(0, localT)), blend: 0.3 }])}>{tx("Start at this second")}</button>
          </div>
        </div>
      )}
    </div>
  );
}
