// The interface talks to the same tools as the terminal and the agent: POST /api/tool/<name>.
import { tx } from "./i18n";
import { useCallback, useEffect, useRef, useState } from "react";

export type Json = any;

export async function tool(name: string, args: Json = {}): Promise<Json> {
  const r = await fetch(`/api/tool/${name}`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(args) });
  const j = await r.json();
  if (!j.ok) throw new Error(j.error || tx("Unknown error"));
  return j.result;
}

/** Run a long tool as a job and wait for it, reporting progress. */
export async function runJob(name: string, args: Json, onProgress?: (p: number, msg: string) => void): Promise<Json> {
  const res = await tool(name, args);
  if (!res || !res.job) return res;
  for (;;) {
    await new Promise((r) => setTimeout(r, 450));
    const j = await tool("job", { id: res.job });
    onProgress?.(j.progress ?? 0, j.message ?? "");
    if (j.status === "done") return j.result;
    if (j.status === "failed" || j.status === "lost") throw new Error(j.error || tx("The job failed"));
    if (j.status === "cancelled") throw new Error(tx("Cancelled"));
  }
}

export const fileUrl = (path?: string | null, v?: number | string) => (path ? `/file?path=${encodeURIComponent(path)}${v ? `&v=${v}` : ""}` : "");

export async function fetchJson(path?: string | null): Promise<Json | null> {
  if (!path) return null;
  try {
    const r = await fetch(fileUrl(path, Date.now()));
    return r.ok ? await r.json() : null;
  } catch {
    return null;
  }
}

export function tc(t: number, fps = 30): string {
  if (!isFinite(t) || t < 0) t = 0;
  const f = Math.floor((t % 1) * fps + 1e-6);
  const s = Math.floor(t) % 60;
  const m = Math.floor(t / 60) % 60;
  const h = Math.floor(t / 3600);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${h ? p(h) + ":" : ""}${p(m)}:${p(s)}:${p(f)}`;
}
export function dur(t?: number): string {
  if (!t && t !== 0) return "";
  const m = Math.floor(t / 60);
  const s = Math.round(t % 60);
  return m >= 60 ? `${Math.floor(m / 60)}:${String(m % 60).padStart(2, "0")}:${String(s).padStart(2, "0")}` : `${m}:${String(s).padStart(2, "0")}`;
}

/** Load something, with a reload handle. */
export function useLoad<T>(fn: () => Promise<T>, deps: any[]): { data: T | null; error: string; reload: () => void; loading: boolean } {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);
  const [n, setN] = useState(0);
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    setLoading(true);
    fn()
      .then((d) => alive.current && (setData(d), setError("")))
      .catch((e) => alive.current && setError(String(e.message || e)))
      .finally(() => alive.current && setLoading(false));
    return () => {
      alive.current = false;
    };
    // eslint-disable-next-line
  }, [...deps, n]);
  return { data, error, loading, reload: useCallback(() => setN((x) => x + 1), []) };
}

export type Activity = { events: Json[]; jobs: Json[]; focus: Json | null };

/** One poll for the whole app: what tools did (from anywhere), running jobs, and where an agent pointed the window. */
export function useActivity(): Activity {
  const [state, setState] = useState<Activity>({ events: [], jobs: [], focus: null });
  const since = useRef(0);
  useEffect(() => {
    let stop = false;
    const tick = async () => {
      try {
        const r = await fetch(`/api/activity?since=${since.current}`);
        const j = await r.json();
        if (stop) return;
        if (j.events.length) since.current = j.events[j.events.length - 1].ts;
        setState((s) => ({ events: [...s.events, ...j.events].slice(-300), jobs: j.jobs, focus: j.focus }));
      } catch {
        /* the server is restarting */
      }
      if (!stop) setTimeout(tick, 1200);
    };
    // Start from "now" so old history does not replay as fresh events.
    fetch("/api/activity?since=0")
      .then((r) => r.json())
      .then((j) => {
        since.current = j.now;
        setState({ events: j.events.slice(-60), jobs: j.jobs, focus: null });
      })
      .catch(() => {})
      .finally(() => setTimeout(tick, 1200));
    return () => {
      stop = true;
    };
  }, []);
  return state;
}

/** `tool key=value key2={"json":1}` -> [name, args], the same grammar as the terminal. */
export function parseCommand(line: string): [string, Json] {
  const m = line.trim().match(/^(\S+)\s*(.*)$/s);
  if (!m) return ["", {}];
  const rest = m[2].trim();
  if (rest.startsWith("{")) return [m[1], JSON.parse(rest)];
  const args: Json = {};
  const re = /([\w.]+)=("(?:[^"\\]|\\.)*"|\{.*?\}(?=\s+[\w.]+=|$)|\[.*?\](?=\s+[\w.]+=|$)|\S+)/gs;
  let x: RegExpExecArray | null;
  while ((x = re.exec(rest))) {
    let v: Json = x[2];
    try {
      v = JSON.parse(x[2]);
    } catch {
      /* plain text */
    }
    const parts = x[1].split(".");
    let cur = args;
    for (const p of parts.slice(0, -1)) cur = cur[p] ??= {};
    cur[parts[parts.length - 1]] = v;
  }
  return [m[1], args];
}
