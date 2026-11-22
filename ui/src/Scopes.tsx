// Scopes of the frame on screen: waveform, RGB parade, vectorscope and histogram. They read the preview's own
// canvas (so they follow playback and every drag at once); a page that draws with WebGL and keeps no picture
// to read falls back to an exact frame from hub_frame.
import { tx } from "./i18n";
import { useEffect, useRef, useState } from "react";
import { RefreshCw, X } from "lucide-react";
import { fileUrl, tool } from "./api";

export type ScopeMode = "wave" | "parade" | "vector" | "hist";
const SW = 320, SH = 180; // the picture is read at the scope's own size (stretched: a scope keeps no shape)

/** The pixels of a picture at scope size, or null when nothing can be read from it. */
function pixels(src: CanvasImageSource, w: number, h: number, buf: HTMLCanvasElement): Uint8ClampedArray | null {
  if (!w || !h) return null;
  // every column of the scope gets a column of the picture, whatever the picture's shape
  buf.width = SW;
  buf.height = SH;
  const g = buf.getContext("2d", { willReadFrequently: true });
  if (!g) return null;
  try {
    g.clearRect(0, 0, buf.width, buf.height);
    g.drawImage(src, 0, 0, buf.width, buf.height);
    const d = g.getImageData(0, 0, buf.width, buf.height).data;
    for (let i = 3; i < d.length; i += 64) if (d[i] > 0) return d;
    return null; // all transparent: a WebGL page without a kept picture
  } catch {
    return null;
  }
}

/** Draw one scope of `d` (w × h RGBA) into the canvas `out`. */
export function drawScope(out: HTMLCanvasElement, mode: ScopeMode, d: Uint8ClampedArray, w: number, h: number) {
  const W = out.width, H = out.height;
  const g = out.getContext("2d")!;
  const img = g.createImageData(W, H);
  const o = img.data;
  for (let i = 0; i < o.length; i += 4) { o[i] = 11; o[i + 1] = 11; o[i + 2] = 13; o[i + 3] = 255; }
  const add = (x: number, y: number, r: number, gg: number, b: number) => {
    if (x < 0 || y < 0 || x >= W || y >= H) return;
    const i = (y * W + x) * 4;
    o[i] = Math.min(255, o[i] + r); o[i + 1] = Math.min(255, o[i + 1] + gg); o[i + 2] = Math.min(255, o[i + 2] + b); o[i + 3] = 255;
  };
  const n = w * h;
  // brightness of one sample: a waveform column holds h samples over H rows, a vectorscope all n over its disc
  const k = mode === "vector" ? Math.max(24, Math.round((W * H * 40) / n)) : mode === "parade" ? 60 : 36;
  if (mode === "wave" || mode === "parade") {
    const cols = mode === "parade" ? 3 : 1, cw = W / cols;
    for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) {
      const i = (y * w + x) * 4;
      const r = d[i], gg = d[i + 1], b = d[i + 2];
      if (mode === "wave") {
        const l = 0.2126 * r + 0.7152 * gg + 0.0722 * b;
        add(Math.floor((x / w) * W), Math.round((1 - l / 255) * (H - 1)), k * 0.55, k * 0.9, k * 0.6);
      } else {
        const px = (x / w) * (cw - 4);
        add(Math.floor(px + 2), Math.round((1 - r / 255) * (H - 1)), k, k * 0.15, k * 0.15);
        add(Math.floor(px + cw + 2), Math.round((1 - gg / 255) * (H - 1)), k * 0.15, k, k * 0.15);
        add(Math.floor(px + 2 * cw + 2), Math.round((1 - b / 255) * (H - 1)), k * 0.25, k * 0.35, k);
      }
    }
  } else if (mode === "vector") {
    const R = Math.min(W, H) / 2 - 6, cx = W / 2, cy = H / 2;
    for (let i = 0; i < d.length; i += 4) {
      const r = d[i] / 255, gg = d[i + 1] / 255, b = d[i + 2] / 255;
      const cb = -0.1146 * r - 0.3854 * gg + 0.5 * b, cr = 0.5 * r - 0.4542 * gg - 0.0458 * b;
      add(Math.round(cx + cb * 2 * R), Math.round(cy - cr * 2 * R), k * 0.7, k * 0.9, k * 0.7);
    }
  } else {
    const bins = [new Array(256).fill(0), new Array(256).fill(0), new Array(256).fill(0)];
    for (let i = 0; i < d.length; i += 4) { bins[0][d[i]]++; bins[1][d[i + 1]]++; bins[2][d[i + 2]]++; }
    // the tallest bin but the clipped ends sets the height, so a black border does not flatten the rest
    let top = 1;
    for (const c of bins) for (let v = 2; v < 254; v++) top = Math.max(top, c[v]);
    const col = [[255, 70, 70], [70, 230, 90], [80, 130, 255]];
    bins.forEach((c, ci) => {
      for (let v = 0; v < 256; v++) {
        const hh = Math.min(H - 1, Math.round((c[v] / top) * (H - 4)));
        const x0 = Math.floor((v / 256) * W), x1 = Math.floor(((v + 1) / 256) * W);
        for (let x = x0; x < Math.max(x0 + 1, x1); x++) for (let y = H - 1; y >= H - 1 - hh; y--) add(x, y, col[ci][0] * 0.45, col[ci][1] * 0.45, col[ci][2] * 0.45);
      }
    });
  }
  g.putImageData(img, 0, 0);
  // the graticule
  g.strokeStyle = "rgba(255,255,255,0.13)";
  g.fillStyle = "rgba(255,255,255,0.4)";
  g.lineWidth = 1;
  g.font = "9px sans-serif";
  if (mode === "wave" || mode === "parade") {
    for (const v of [0, 25, 50, 75, 100]) {
      const y = Math.round((1 - v / 100) * (H - 1)) + 0.5;
      g.beginPath(); g.moveTo(0, y); g.lineTo(W, y); g.stroke();
      g.fillText(String(v), 2, Math.min(H - 2, Math.max(9, y - 2)));
    }
    if (mode === "parade") for (const x of [W / 3, (2 * W) / 3]) { g.beginPath(); g.moveTo(Math.round(x) + 0.5, 0); g.lineTo(Math.round(x) + 0.5, H); g.stroke(); }
  } else if (mode === "vector") {
    const R = Math.min(W, H) / 2 - 6, cx = W / 2, cy = H / 2;
    g.beginPath(); g.arc(cx, cy, R, 0, Math.PI * 2); g.stroke();
    g.beginPath(); g.moveTo(cx - R, cy); g.lineTo(cx + R, cy); g.moveTo(cx, cy - R); g.lineTo(cx, cy + R); g.stroke();
    // the 75 % colour targets and the skin-tone line
    const tgt: [string, number, number, number][] = [["R", 0.75, 0, 0], ["Y", 0.75, 0.75, 0], ["G", 0, 0.75, 0], ["C", 0, 0.75, 0.75], ["B", 0, 0, 0.75], ["M", 0.75, 0, 0.75]];
    for (const [lbl, r, gg, b] of tgt) {
      const cb = -0.1146 * r - 0.3854 * gg + 0.5 * b, cr = 0.5 * r - 0.4542 * gg - 0.0458 * b;
      const x = cx + cb * 2 * R, y = cy - cr * 2 * R;
      g.strokeRect(x - 4, y - 4, 8, 8);
      g.fillText(lbl, x + 6, y + 3);
    }
    const a = (123 * Math.PI) / 180; // skin tones lie along this line
    g.strokeStyle = "rgba(255,200,150,0.35)";
    g.beginPath(); g.moveTo(cx, cy); g.lineTo(cx + Math.cos(a) * R, cy - Math.sin(a) * R); g.stroke();
  }
}

export function Scopes({ film, t, playing, source, close }: { film: string; t: number; playing: boolean; source: () => HTMLCanvasElement | null; close: () => void }) {
  const [mode, setMode] = useState<ScopeMode>(() => { try { return (localStorage.getItem("linnea.scope") as ScopeMode) || "wave"; } catch { return "wave"; } });
  const [note, setNote] = useState("");
  const out = useRef<HTMLCanvasElement>(null);
  const buf = useRef<HTMLCanvasElement>(document.createElement("canvas"));
  const exact = useRef<{ t: number; img: HTMLImageElement } | null>(null);
  const pick = (m: ScopeMode) => { setMode(m); try { localStorage.setItem("linnea.scope", m); } catch { /* private window */ } };
  const paint = () => {
    const c = out.current;
    if (!c) return;
    const cv = source();
    let d = cv ? pixels(cv, cv.width, cv.height, buf.current) : null;
    if (!d && exact.current && Math.abs(exact.current.t - t) < 0.02) {
      const im = exact.current.img;
      d = pixels(im, im.naturalWidth, im.naturalHeight, buf.current);
    }
    if (!d) { setNote(tx("The page keeps no picture to read: press ↻ for the exact frame.")); return; }
    setNote("");
    drawScope(c, mode, d, buf.current.width, buf.current.height);
  };
  // after each change of time or picture; while playing, a few times a second
  useEffect(() => {
    const id = setTimeout(paint, playing ? 0 : 60);
    return () => clearTimeout(id);
  });
  useEffect(() => {
    if (!playing) return;
    const iv = setInterval(paint, 160);
    return () => clearInterval(iv);
  }, [playing, mode]);
  const fetchExact = async () => {
    try {
      const r = await tool("hub_frame", { film, t, scale: 0.5 });
      const f = r.frames?.[0]?.file;
      if (!f) return;
      const img = new Image();
      img.onload = () => { exact.current = { t, img }; paint(); };
      img.src = fileUrl(f, Date.now());
    } catch (e: any) {
      setNote(e.message);
    }
  };
  const modes: [ScopeMode, string][] = [["wave", tx("Waveform")], ["parade", tx("Parade")], ["vector", tx("Vectorscope")], ["hist", tx("Histogram")]];
  return (
    <div className="scopes" onPointerDown={(e) => e.stopPropagation()}>
      <div className="scopes-h">
        <div className="seg">{modes.map(([m, l]) => <button key={m} className={mode === m ? "on" : ""} onClick={() => pick(m)}>{l}</button>)}</div>
        <span className="grow" />
        <button className="btn icon sm ghost" title={tx("The exact frame, drawn the way a render draws it")} onClick={fetchExact}><RefreshCw /></button>
        <button className="btn icon sm ghost" title={tx("Close (U)")} onClick={close}><X /></button>
      </div>
      <canvas ref={out} width={320} height={180} className="scopes-c" />
      {note && <div className="muted" style={{ fontSize: 11, padding: "2px 4px" }}>{note}</div>}
    </div>
  );
}
