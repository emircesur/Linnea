// Linnea's own updates from GitHub: a quiet note when a newer version is out, and the Settings section that
// checks and installs it.
import { tx } from "./i18n";
import { useEffect, useState } from "react";
import { Download, RefreshCw, X } from "lucide-react";
import { Json, runJob, tool } from "./api";

export function useUpdate(enabled: boolean) {
  const [info, setInfo] = useState<Json | null>(null);
  const [err, setErr] = useState("");
  const check = (force = false) => tool("app_update", { force }).then((r) => { setInfo(r); setErr(""); return r; }).catch((e) => { setErr(e.message); return null; });
  useEffect(() => { if (enabled) check(false); }, [enabled]);
  return { info, err, check };
}
export type Update = ReturnType<typeof useUpdate>;

function useInstall(say: (s: string) => void) {
  const [p, setP] = useState<number | null>(null);
  const go = async () => {
    setP(0);
    try {
      const r = await runJob("app_update_install", {}, (x) => setP(x));
      say(r.restarting ? tx("Installing; Linnea starts again in a moment") : r.note || tx("Update downloaded"));
    } catch (e: any) {
      say(e.message);
    }
    setP(null);
  };
  return { p, go };
}

/** The note in the corner: once per version, until it is installed or put off. */
export function UpdateNote({ u, say }: { u: Update; say: (s: string) => void }) {
  const v = u.info?.latest;
  const [hidden, setHidden] = useState(() => { try { return localStorage.getItem("linnea.update.later") === v; } catch { return false; } });
  useEffect(() => { try { setHidden(localStorage.getItem("linnea.update.later") === v); } catch { /* private window */ } }, [v]);
  const ins = useInstall(say);
  if (!u.info?.available || hidden) return null;
  return (
    <div className="card" style={{ position: "fixed", right: 16, bottom: 16, zIndex: 40, padding: 12, width: 300, boxShadow: "0 12px 32px rgba(0,0,0,.35)" }}>
      <div className="rowx" style={{ marginBottom: 6 }}>
        <b className="grow">{tx("Linnea {0} is out", v)}</b>
        <button className="btn icon sm ghost" title={tx("Later")} onClick={() => { try { localStorage.setItem("linnea.update.later", v); } catch { /* private */ } setHidden(true); }}><X /></button>
      </div>
      <div className="sec" style={{ fontSize: 12, marginBottom: 8 }}>{tx("You have {0}. Your films and settings stay as they are.", u.info.current)}</div>
      {ins.p !== null ? <div className="meter"><i style={{ width: `${ins.p * 100}%` }} /></div> : (
        <div className="rowx">
          <button className="btn sm primary" onClick={ins.go} disabled={!u.info.asset}><Download />{tx("Update now")}</button>
          {u.info.page && <a className="btn sm ghost" href={u.info.page} target="_blank" rel="noreferrer">{tx("What's new")}</a>}
        </div>
      )}
    </div>
  );
}

/** Settings: the version, a check, the install, and whether to look by itself. */
export function UpdateSettings({ u, auto, setAuto, say }: { u: Update; auto: boolean; setAuto: (b: boolean) => void; say: (s: string) => void }) {
  const ins = useInstall(say);
  const [busy, setBusy] = useState(false);
  const i = u.info;
  return (
    <section>
      <h2>{tx("Updates")}</h2>
      <div className="card box stack" style={{ gap: 10 }}>
        <div className="rowx">
          <span className="grow">{i ? (i.available ? tx("Linnea {0} is out (you have {1})", i.latest, i.current) : tx("Linnea {0} is the newest version", i.current)) : u.err || tx("Not checked yet")}</span>
          <button className="btn sm ghost" disabled={busy} onClick={async () => { setBusy(true); await u.check(true); setBusy(false); }}><RefreshCw />{tx("Check now")}</button>
          {i?.available && (ins.p !== null ? <div className="meter" style={{ width: 120 }}><i style={{ width: `${ins.p * 100}%` }} /></div> : <button className="btn sm primary" disabled={!i.asset} onClick={ins.go}><Download />{tx("Update now")}</button>)}
        </div>
        {i?.available && i.notes && <pre className="sec" style={{ fontSize: 12, whiteSpace: "pre-wrap", maxHeight: 180, overflow: "auto", margin: 0 }}>{i.notes}</pre>}
        <label className="rowx" style={{ fontSize: 13 }}><input type="checkbox" checked={auto} onChange={(e) => setAuto(e.target.checked)} />{tx("Look for a new version when Linnea starts")}</label>
      </div>
    </section>
  );
}
