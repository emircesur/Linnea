// Build a puppet from bones and animate it: Build mode places joints (drag a joint) and sets each bone's
// direction and length (drag its tip); Pose mode turns bones (drag one) at the current second of a state and
// keys that pose. Each state is a short loop or a one-off; the saved puppet is an ordinary rich puppet.
import { tx } from "./i18n";
import { useEffect, useMemo, useRef, useState } from "react";
import { Bone as BoneIcon, Diamond, Pause, Play, Plus, Save, Trash2, X } from "lucide-react";
import { Json, tool } from "./api";
import { Bone, poseAt, paintRig, puppetFromRig, Rig, RIG_TEMPLATES, solve } from "./Rig";
import { puppetName } from "./Puppets";

const clone = <T,>(x: T): T => JSON.parse(JSON.stringify(x));
const r1 = (x: number) => Math.round(x * 10) / 10;

export function RigEditor({ start, taken, close, saved, say }: { start?: { name: string; label: string; rig: Rig; assets?: Json }; taken: string[]; close: () => void; saved: (name: string) => void; say: (s: string) => void }) {
  const [R, setR] = useState<Rig>(() => clone(start?.rig || RIG_TEMPLATES[0][2]()));
  const [label, setLabel] = useState(start?.label || tx("My puppet"));
  const [assets, setAssets] = useState<Json>(start?.assets || {});
  const [sel, setSel] = useState(R.bones[1]?.id || R.bones[0]?.id || "");
  const [mode, setMode] = useState<"pose" | "build">("pose");
  const [stName, setSt] = useState(R.initial in R.states ? R.initial : Object.keys(R.states)[0]);
  const [T, setT] = useState(0);
  const [playing, setPlaying] = useState(false);
  const cv = useRef<HTMLCanvasElement>(null);
  const st = R.states[stName];
  const pose = useMemo(() => (st ? poseAt(st, T) : {}), [R, stName, T]);
  const params = useMemo(() => Object.fromEntries(Object.entries(R.params || {}).map(([k, v]: [string, Json]) => [k, v?.default])), [R.params]);
  const imgs = useMemo(() => Object.fromEntries(Object.entries(assets).map(([k, src]) => { const im = new Image(); im.src = String(src); return [k, im]; })), [assets]);
  const bone = R.bones.find((b) => b.id === sel);
  const edit = (f: (r: Rig) => void) => setR((r) => { const n = clone(r); f(n); return n; });

  // draw
  useEffect(() => {
    const c = cv.current;
    if (!c) return;
    const g = c.getContext("2d")!;
    g.setTransform(1, 0, 0, 1, 0, 0);
    g.clearRect(0, 0, c.width, c.height);
    try { paintRig(g, params, T, { name: stName, t: T, k: st ? (T / st.dur) % 1 : 0, dur: st?.dur || 1 }, { img: imgs, W: R.size[0], H: R.size[1] }, R, pose, playing ? undefined : sel); } catch (e: any) { g.fillStyle = "#e5484d"; g.fillText(String(e?.message || e), 10, 20); }
  });
  useEffect(() => { Object.values(imgs).forEach((im: any) => (im.onload = () => setT((x) => x + 0))); }, [imgs]);
  // play the state
  useEffect(() => {
    if (!playing || !st) return;
    let raf = 0, t0 = performance.now() - T * 1000;
    const tick = () => {
      let x = (performance.now() - t0) / 1000;
      if (x > st.dur) { if (st.loop) { t0 = performance.now(); x = 0; } else { setPlaying(false); setT(st.dur); return; } }
      setT(x);
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [playing, stName]);

  // the pose at T is a key: change it (made from what shows now when there is none yet)
  const keyIdx = st ? st.keys.findIndex((k) => Math.abs(k.t - T) <= 0.02) : -1;
  const setPose = (p: Json) => edit((r) => {
    const s = r.states[stName];
    let i = s.keys.findIndex((k) => Math.abs(k.t - T) <= 0.02);
    if (i < 0) { s.keys.push({ t: r1(T * 10) / 10, pose: { ...poseAt(s, T) } }); s.keys.sort((a, b) => a.t - b.t); i = s.keys.findIndex((k) => Math.abs(k.t - T) <= 0.02); }
    Object.assign(s.keys[i].pose, p);
  });

  // pointer: pick a bone, then turn it (pose) or move its joint / tip (build)
  const at = (e: { clientX: number; clientY: number }) => {
    const r = cv.current!.getBoundingClientRect();
    return [((e.clientX - r.left) / r.width) * R.size[0], ((e.clientY - r.top) / r.height) * R.size[1]];
  };
  const down = (e: React.PointerEvent) => {
    if (playing) setPlaying(false);
    const [px, py] = at(e);
    const W = solve(R, pose);
    const k = R.size[0] / cv.current!.getBoundingClientRect().width;
    let best = "", bd = 18 * k, part: "joint" | "tip" | "bone" = "bone";
    for (const b of R.bones) {
      const w = W[b.id], A = (b.a * Math.PI) / 180 + w.d, tx2 = w.x + Math.sin(A) * b.len, ty2 = w.y + Math.cos(A) * b.len;
      const dj = Math.hypot(px - w.x, py - w.y), dt = Math.hypot(px - tx2, py - ty2);
      const L2 = b.len * b.len || 1, u = Math.max(0, Math.min(1, ((px - w.x) * (tx2 - w.x) + (py - w.y) * (ty2 - w.y)) / L2));
      const ds = Math.hypot(px - (w.x + (tx2 - w.x) * u), py - (w.y + (ty2 - w.y) * u));
      const d = Math.min(dj, dt, ds);
      if (d < bd) { bd = d; best = b.id; part = d === dj ? "joint" : d === dt && b.len > 0 ? "tip" : "bone"; }
    }
    if (!best) return;
    setSel(best);
    const b0 = R.bones.find((b) => b.id === best)!;
    const w0 = W[best];
    const base = clone(R);
    const kids = (id: string): string[] => base.bones.filter((x) => x.parent === id).flatMap((x) => [x.id, ...kids(x.id)]);
    const mv = (ev: PointerEvent) => {
      const [qx, qy] = at(ev);
      if (mode === "pose") {
        if (!b0.parent && (part === "joint" || b0.len === 0)) { setPose({ $x: Math.round(qx - b0.x), $y: Math.round(qy - b0.y) }); return; }
        const parentD = w0.d - ((pose[best] || 0) * Math.PI) / 180;
        const want = Math.atan2(qx - w0.x, qy - w0.y);
        let d = ((want - (b0.a * Math.PI) / 180 - parentD) * 180) / Math.PI;
        d = ((d + 540) % 360) - 180;
        setPose({ [best]: Math.round(ev.shiftKey ? Math.round(d / 15) * 15 : d) });
      } else if (part === "tip") {
        edit((r) => {
          const b = r.bones.find((x) => x.id === best)!;
          b.a = Math.round((((Math.atan2(qx - w0.x, qy - w0.y) - w0.d) * 180) / Math.PI) * 10) / 10;
          b.len = Math.max(0, Math.round(Math.hypot(qx - w0.x, qy - w0.y)));
        });
      } else {
        const dx = qx - w0.x, dy = qy - w0.y;
        edit((r) => { for (const b of r.bones) if (b.id === best || kids(best).includes(b.id)) { const o = base.bones.find((x) => x.id === b.id)!; b.x = Math.round(o.x + dx); b.y = Math.round(o.y + dy); } });
      }
    };
    const up = () => { window.removeEventListener("pointermove", mv); window.removeEventListener("pointerup", up); };
    window.addEventListener("pointermove", mv);
    window.addEventListener("pointerup", up);
  };

  const addBone = () => {
    const p = bone || R.bones[0];
    const id = "b" + Date.now().toString(36);
    const [x, y] = p ? [p.x + Math.sin((p.a * Math.PI) / 180) * p.len, p.y + Math.cos((p.a * Math.PI) / 180) * p.len] : [R.size[0] / 2, R.size[1] / 2];
    edit((r) => r.bones.push({ id, label: tx("Bone {0}", r.bones.length + 1), parent: p?.id, x: Math.round(x), y: Math.round(y), a: p?.a ?? 180, len: 80, shape: "capsule", w: 30, color: p?.color || "#e9b84a", z: (p?.z || 0) + 1 }));
    setSel(id);
    setMode("build");
  };
  const delBone = () => {
    if (!bone || R.bones.length < 2) return;
    edit((r) => { r.bones = r.bones.filter((b) => b.id !== bone.id).map((b) => (b.parent === bone.id ? { ...b, parent: bone.parent } : b)); for (const s of Object.values(r.states)) for (const k of s.keys) delete k.pose[bone.id]; });
    setSel(R.bones[0].id);
  };
  const setBone = (p: Partial<Bone>) => edit((r) => Object.assign(r.bones.find((b) => b.id === sel)!, p));
  const picture = (f: File) => {
    const rd = new FileReader();
    rd.onload = () => { const im = new Image(); im.onload = () => { const key = "i" + Date.now().toString(36); setAssets((a: Json) => ({ ...a, [key]: String(rd.result) })); const w = Math.min(im.naturalWidth, R.size[0] * 0.6); setBone({ shape: "image", img: key, w: Math.round(w), len: Math.round((w * im.naturalHeight) / im.naturalWidth) }); }; im.src = String(rd.result); };
    rd.readAsDataURL(f);
  };
  const addState = () => {
    const n = window.prompt(tx("Name of the new state (a word, like jump)"), "");
    if (!n) return;
    const key = n.trim().toLowerCase().replace(/[^a-z0-9_-]+/g, "-");
    if (!key || R.states[key]) return;
    edit((r) => (r.states[key] = { dur: 1, loop: true, label: n.trim(), keys: [{ t: 0, pose: {} }] }));
    setSt(key);
    setT(0);
  };
  const save = async () => {
    const name = start?.name || puppetName(label, taken);
    try {
      await tool("puppet_save", puppetFromRig(R, name, label, Object.keys(assets).length ? assets : undefined));
      say(tx("\"{0}\" saved to the puppet library", label));
      saved(name);
    } catch (e: any) { say(e.message); }
  };
  const scale = Math.min(460 / R.size[0], 520 / R.size[1]);
  const colorOpts = Object.entries(R.params || {}).filter(([, v]: [string, Json]) => v?.type === "color").map(([k]) => "$" + k);

  return (
    <div className="palette-bg" onPointerDown={close}>
      <div className="rig-ed card" onPointerDown={(e) => e.stopPropagation()}>
        <div className="rowx rig-top">
          <BoneIcon />
          <input className="insp-name" value={label} onChange={(e) => setLabel(e.target.value)} style={{ maxWidth: 260 }} />
          {!start && (
            <select className="select" style={{ width: 280, height: 30 }} onChange={(e) => { const tpl = RIG_TEMPLATES.find((x) => x[0] === e.target.value); if (tpl) { const r = tpl[2](); setR(r); setSt(r.initial); setSel(r.bones[0].id); setT(0); } }} defaultValue="presenter">
              {RIG_TEMPLATES.map(([k, l]) => <option key={k} value={k}>{l}</option>)}
            </select>
          )}
          <span className="grow" />
          <div className="seg">
            <button className={mode === "pose" ? "on" : ""} onClick={() => setMode("pose")} title={tx("Drag a bone to turn it; the pose is keyed at this second")}>{tx("Pose")}</button>
            <button className={mode === "build" ? "on" : ""} onClick={() => setMode("build")} title={tx("Drag a joint to move it, a bone's tip to point and stretch it")}>{tx("Build")}</button>
          </div>
          <button className="btn sm primary" onClick={save}><Save />{tx("Save to the library")}</button>
          <button className="btn icon sm ghost" onClick={close}><X /></button>
        </div>
        <div className="rig-body">
          <div className="rig-side">
            <span className="sub-h">{tx("Bones")}</span>
            <div className="stack-list" style={{ maxHeight: 220, overflow: "auto" }}>
              {R.bones.map((b) => {
                let d = 0;
                for (let p = b.parent; p && d < 8; p = R.bones.find((x) => x.id === p)?.parent) d++;
                return <div key={b.id} className={`lrow ${b.id === sel ? "on prim" : ""}`} onClick={() => setSel(b.id)}><span style={{ width: d * 12, flex: "none" }} /><span className="lrow-name">{b.label || b.id}</span><span className="lrow-t num">{pose[b.id] ? `${Math.round(pose[b.id])}°` : ""}</span></div>;
              })}
            </div>
            <div className="rowx" style={{ gap: 4 }}>
              <button className="btn sm grow" onClick={addBone}><Plus />{tx("Bone on this one")}</button>
              <button className="btn icon sm ghost" title={tx("Delete the bone")} onClick={delBone} disabled={R.bones.length < 2}><Trash2 /></button>
            </div>
            {bone && (
              <div className="stack" style={{ gap: 6 }}>
                <input className="input" value={bone.label || ""} placeholder={bone.id} onChange={(e) => setBone({ label: e.target.value })} />
                <div className="g2">
                  <select className="select" value={bone.shape} onChange={(e) => setBone({ shape: e.target.value as any })}>
                    {[["capsule", tx("Rounded")], ["ellipse", tx("Oval")], ["rect", tx("Box")], ["circle", tx("Circle")], ["line", tx("Line")], ["image", tx("Picture")], ["none", tx("Nothing (a joint)")]].map(([k, l]) => <option key={k} value={k}>{l}</option>)}
                  </select>
                  <select className="select" value={bone.parent || ""} onChange={(e) => setBone({ parent: e.target.value || undefined })}>
                    <option value="">{tx("No parent")}</option>
                    {R.bones.filter((b) => b.id !== bone.id).map((b) => <option key={b.id} value={b.id}>{b.label || b.id}</option>)}
                  </select>
                </div>
                <div className="g2">
                  <label className="nf"><span className="nf-l">{tx("Thick")}</span><input className="num" type="number" value={bone.w} onChange={(e) => setBone({ w: +e.target.value })} /></label>
                  <label className="nf"><span className="nf-l">{tx("Length")}</span><input className="num" type="number" value={bone.len} onChange={(e) => setBone({ len: +e.target.value })} /></label>
                  <label className="nf"><span className="nf-l">{tx("Angle")}</span><input className="num" type="number" value={bone.a} onChange={(e) => setBone({ a: +e.target.value })} /></label>
                  <label className="nf"><span className="nf-l">{tx("Layer")}</span><input className="num" type="number" value={bone.z || 0} onChange={(e) => setBone({ z: +e.target.value })} /></label>
                </div>
                <div className="nf wide">
                  <span className="nf-l">{tx("Colour")}</span>
                  <select value={colorOpts.includes(bone.color || "") ? bone.color : "custom"} onChange={(e) => setBone({ color: e.target.value === "custom" ? "#e9b84a" : e.target.value })}>
                    {colorOpts.map((c) => <option key={c} value={c}>{(R.params[c.slice(1)]?.label || c.slice(1))}</option>)}
                    <option value="custom">{tx("Own colour")}</option>
                  </select>
                  {!colorOpts.includes(bone.color || "") && <input type="color" value={/^#[0-9a-f]{6}$/i.test(bone.color || "") ? bone.color : "#e9b84a"} onChange={(e) => setBone({ color: e.target.value })} style={{ width: 28 }} />}
                </div>
                <div className="rowx" style={{ gap: 10 }}>
                  <label className="rowx" style={{ fontSize: 12, gap: 6 }}><input type="checkbox" checked={!!bone.face} onChange={(e) => setBone({ face: e.target.checked || undefined })} />{tx("A face (talks, blinks)")}</label>
                  <label className="btn sm ghost">{tx("Picture…")}<input type="file" accept="image/*" hidden onChange={(e) => { const f = e.target.files?.[0]; e.target.value = ""; if (f) picture(f); }} /></label>
                </div>
              </div>
            )}
          </div>
          <div className="rig-stage">
            <canvas ref={cv} width={R.size[0]} height={R.size[1]} onPointerDown={down} style={{ width: R.size[0] * scale, height: R.size[1] * scale, cursor: mode === "pose" ? "grab" : "crosshair" }} />
            <span className="muted" style={{ fontSize: 11 }}>{mode === "pose" ? tx("Drag a bone to turn it (Shift: 15° steps); drag the first joint to move the whole puppet. The pose is keyed at this second.") : tx("Drag a joint to move it with everything on it; drag a bone's tip to point and stretch it.")}</span>
          </div>
        </div>
        <div className="rig-states">
          <div className="rowx" style={{ gap: 4, flexWrap: "wrap" }}>
            {Object.entries(R.states).map(([k, s]) => <button key={k} className={`chip ${k === stName ? "on" : ""}`} onClick={() => { setSt(k); setT(0); setPlaying(false); }}>{s.label || k}</button>)}
            <button className="chip" onClick={addState}><Plus size={11} /> {tx("State")}</button>
            <span className="grow" />
            {st && (
              <>
                <label className="nf" style={{ width: 110 }}><span className="nf-l">{tx("Length s")}</span><input className="num" type="number" step={0.1} min={0.1} value={st.dur} onChange={(e) => edit((r) => (r.states[stName].dur = Math.max(0.1, +e.target.value || 1)))} /></label>
                <label className="rowx" style={{ fontSize: 12, gap: 6 }}><input type="checkbox" checked={st.loop} onChange={(e) => edit((r) => (r.states[stName].loop = e.target.checked))} />{tx("Loop")}</label>
                <label className="rowx" style={{ fontSize: 12, gap: 6 }}><input type="radio" checked={R.initial === stName} onChange={() => edit((r) => (r.initial = stName))} />{tx("First state")}</label>
                <button className="btn icon sm ghost" title={tx("Delete the state")} disabled={Object.keys(R.states).length < 2} onClick={() => { edit((r) => { delete r.states[stName]; if (r.initial === stName) r.initial = Object.keys(r.states)[0]; }); setSt(Object.keys(R.states).find((k) => k !== stName)!); }}><Trash2 /></button>
              </>
            )}
          </div>
          {st && (
            <div className="rowx" style={{ gap: 8 }}>
              <button className="btn icon sm" onClick={() => { if (!playing && T >= st.dur - 0.01) setT(0); setPlaying(!playing); }}>{playing ? <Pause /> : <Play />}</button>
              <div className="rig-tl" onPointerDown={(e) => { const r = (e.currentTarget as HTMLElement).getBoundingClientRect(); const go = (x: number) => setT(Math.max(0, Math.min(st.dur, ((x - r.left) / r.width) * st.dur))); setPlaying(false); go(e.clientX); const mv = (ev: PointerEvent) => go(ev.clientX); const up = () => { window.removeEventListener("pointermove", mv); window.removeEventListener("pointerup", up); }; window.addEventListener("pointermove", mv); window.addEventListener("pointerup", up); }}>
                {st.keys.map((k, i) => <span key={i} className="rig-key" style={{ left: `${(k.t / st.dur) * 100}%` }} title={`${k.t} s`} />)}
                <span className="rig-head" style={{ left: `${(T / st.dur) * 100}%` }} />
              </div>
              <span className="num muted" style={{ fontSize: 11, width: 56 }}>{T.toFixed(2)} s</span>
              <button className="btn sm" title={tx("Key the pose that shows now")} onClick={() => setPose({})}><Diamond />{tx("Key")}</button>
              {keyIdx >= 0 && (
                <>
                  <select className="select" style={{ width: 92, height: 28 }} value={st.keys[keyIdx].ease || "inOut"} onChange={(e) => edit((r) => (r.states[stName].keys[keyIdx].ease = e.target.value))}>
                    {["inOut", "out", "in", "linear", "hold"].map((x) => <option key={x}>{x}</option>)}
                  </select>
                  <button className="btn icon sm ghost" title={tx("Delete this key")} onClick={() => edit((r) => r.states[stName].keys.splice(keyIdx, 1))}><Trash2 /></button>
                </>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
