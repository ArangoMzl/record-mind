import { useEffect, useRef, useState } from "react";
import { api, assetUrl, type SnipSource } from "../lib/api";

type Pt = { x: number; y: number };

/** Ventana de recorte: la pantalla congelada a tamaño completo; arrastras y suelta = listo. */
export function Snip() {
  const [source, setSource] = useState<SnipSource | null>(null);
  const [start, setStart] = useState<Pt | null>(null);
  const [end, setEnd] = useState<Pt | null>(null);
  const [cursor, setCursor] = useState<Pt | null>(null);
  const done = useRef(false);

  useEffect(() => {
    api.snipSource().then(setSource);
  }, []);

  const finish = (rect: { x: number; y: number; width: number; height: number } | null) => {
    if (done.current) return;
    done.current = true;
    api.finishSnip(rect);
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") finish(null);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const sel =
    start && end
      ? { x: Math.min(start.x, end.x), y: Math.min(start.y, end.y), w: Math.abs(end.x - start.x), h: Math.abs(end.y - start.y) }
      : null;

  // La ventana mide lo mismo que el monitor en píxeles lógicos; la captura va en físicos.
  const sx = source ? source.width / window.innerWidth : 1;
  const sy = source ? source.height / window.innerHeight : 1;

  const onDown = (e: React.PointerEvent) => {
    if (e.button === 2) return finish(null);
    if (e.button !== 0) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    setStart({ x: e.clientX, y: e.clientY });
    setEnd({ x: e.clientX, y: e.clientY });
  };

  const onMove = (e: React.PointerEvent) => {
    setCursor({ x: e.clientX, y: e.clientY });
    if (start) setEnd({ x: e.clientX, y: e.clientY });
  };

  const onUp = () => {
    if (sel && sel.w >= 4 && sel.h >= 4) {
      finish({
        x: Math.round(sel.x * sx),
        y: Math.round(sel.y * sy),
        width: Math.round(sel.w * sx),
        height: Math.round(sel.h * sy),
      });
    } else {
      // Un clic sin arrastrar no recorta nada: se sigue esperando.
      setStart(null);
      setEnd(null);
    }
  };

  if (!source) return null;

  return (
    <div
      className="fixed inset-0 cursor-crosshair select-none"
      onPointerDown={onDown}
      onPointerMove={onMove}
      onPointerUp={onUp}
      onContextMenu={(e) => e.preventDefault()}
    >
      <img
        src={assetUrl(source.path)}
        alt=""
        draggable={false}
        onLoad={() => api.snipReady()}
        className="pointer-events-none absolute inset-0 size-full"
      />

      {sel ? (
        <div
          className="pointer-events-none absolute border-[1.5px] border-accent"
          style={{ left: sel.x, top: sel.y, width: sel.w, height: sel.h, boxShadow: "0 0 0 100vmax rgb(0 0 0 / 0.5)" }}
        >
          <span
            className={`absolute right-0 rounded-md bg-black/80 px-1.5 py-0.5 font-mono text-[11px] text-white ${
              sel.y + sel.h + 26 > window.innerHeight ? "bottom-full mb-1.5" : "top-full mt-1.5"
            }`}
          >
            {Math.round(sel.w * sx)} × {Math.round(sel.h * sy)}
          </span>
        </div>
      ) : (
        <>
          <div className="pointer-events-none absolute inset-0 bg-black/40" />
          {cursor && (
            <>
              <div className="pointer-events-none absolute inset-x-0 h-px bg-white/35" style={{ top: cursor.y }} />
              <div className="pointer-events-none absolute inset-y-0 w-px bg-white/35" style={{ left: cursor.x }} />
            </>
          )}
          <div className="pointer-events-none absolute top-6 left-1/2 flex -translate-x-1/2 items-center gap-3 rounded-xl border border-white/10 bg-[#0e0e11]/90 px-4 py-2 text-[13px] text-ink-2 shadow-[0_8px_30px_rgba(0,0,0,.5)]">
            <span className="size-1.5 rounded-full bg-accent" />
            Arrastra para recortar
            <span className="text-ink-3">·</span>
            <span>
              <span className="kbd">Esc</span> cancelar
            </span>
          </div>
        </>
      )}
    </div>
  );
}
