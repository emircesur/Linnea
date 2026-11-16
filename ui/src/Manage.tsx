// Keeping the work tidy from its cards: star, rename, duplicate, show in folder, delete (to the Recycle Bin),
// plus search and order for the lists of films, projects and thumbnails.
import { useState } from "react";
import { Star } from "lucide-react";
import { Json, tool } from "./api";
import { tx } from "./i18n";

/** Starred items, remembered in this window. */
export function useStars(kind: string) {
  const key = `stars:${kind}`;
  const read = (): string[] => {
    try {
      const v = JSON.parse(localStorage.getItem(key) || "[]");
      return Array.isArray(v) ? v : [];
    } catch {
      return [];
    }
  };
  const [stars, setStars] = useState<string[]>(read);
  const toggle = (id: string) => {
    const next = stars.includes(id) ? stars.filter((x) => x !== id) : [...stars, id];
    setStars(next);
    try {
      localStorage.setItem(key, JSON.stringify(next));
    } catch {
      /* not remembered, still shown */
    }
  };
  return { toggle, has: (id: string) => stars.includes(id) };
}

/** Words filter the list; starred items come first, then newest or by name. */
export function arrange(list: Json[], q: string, sort: string, starred: (id: string) => boolean, time: (x: Json) => number): Json[] {
  const w = q.trim().toLocaleLowerCase();
  const title = (x: Json) => String(x.title || x.id || "");
  return list
    .filter((x) => !w || title(x).toLocaleLowerCase().includes(w) || String(x.id).toLocaleLowerCase().includes(w))
    .sort((a, b) => Number(starred(b.id)) - Number(starred(a.id)) || (sort === "name" ? title(a).localeCompare(title(b)) : time(b) - time(a)));
}

export function ListTools({ q, setQ, sort, setSort, placeholder }: { q: string; setQ: (s: string) => void; sort: string; setSort: (s: string) => void; placeholder: string }) {
  return (
    <div className="list-tools">
      <input className="input" style={{ width: 240 }} placeholder={placeholder} value={q} onChange={(e) => setQ(e.target.value)} />
      <div className="seg">
        {[["recent", tx("Recent")], ["name", tx("Name")]].map(([k, l]) => (
          <button key={k} className={sort === k ? "on" : ""} onClick={() => setSort(k)}>{l}</button>
        ))}
      </div>
    </div>
  );
}

export type CardAct = { icon: any; label: string; run: () => void; danger?: boolean };

/** Small actions on a card, shown on hover (a starred card keeps its star visible). */
export function CardActions({ actions, starred, onStar }: { actions: CardAct[]; starred?: boolean; onStar?: () => void }) {
  const stop = (e: React.SyntheticEvent) => e.stopPropagation();
  const key = (run: () => void) => (e: React.KeyboardEvent) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); e.stopPropagation(); run(); } };
  return (
    <div className="card-actions" onClick={stop} onPointerDown={stop} onKeyDown={stop}>
      {onStar && <span role="button" tabIndex={0} className={`ca ${starred ? "on" : ""}`} title={starred ? tx("Unstar") : tx("Star")} onClick={onStar} onKeyDown={key(onStar)}><Star /></span>}
      {actions.map((a) => (
        <span key={a.label} role="button" tabIndex={0} className={`ca ${a.danger ? "danger" : ""}`} title={a.label} onClick={a.run} onKeyDown={key(a.run)}><a.icon /></span>
      ))}
    </div>
  );
}

/** Rename and delete, worded the same everywhere. `extra` names the project or film an item belongs to. */
export async function renameItem(kind: string, id: string, current: string, extra: Json, say: (s: string) => void, done: () => void) {
  const title = window.prompt(tx("New name"), current);
  if (!title || !title.trim() || title.trim() === current) return;
  try {
    await tool("item_rename", { kind, id, title: title.trim(), ...extra });
    say(tx("Renamed"));
    done();
  } catch (e: any) {
    say(e.message);
  }
}

export async function deleteItem(kind: string, id: string, title: string, extra: Json, say: (s: string) => void, done: () => void) {
  if (!window.confirm(tx("Move \"{0}\" to the Recycle Bin? You can restore it from there.", title))) return;
  try {
    await tool("item_delete", { kind, id, ...extra });
    say(tx("\"{0}\" moved to the Recycle Bin", title));
    done();
  } catch (e: any) {
    say(e.message);
  }
}
