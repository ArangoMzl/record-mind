import { ArrowUpRight, Check, Hash, Pencil, Redo2, Square, Trash2, Undo2, X } from "lucide-react";
import { motion } from "motion/react";
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import {
  arrowGeom,
  COLORS,
  constrain,
  isMeaningful,
  nextNumber,
  numInk,
  numRadius,
  rectGeom,
  renderAnnotated,
  strokeFor,
  type Point,
  type Shape,
  type Tool,
} from "./annotate";

const TOOLS: { id: Tool; label: string; key: string; Icon: typeof Square }[] = [
  { id: "arrow", label: "Flecha", key: "A", Icon: ArrowUpRight },
  { id: "rect", label: "Recuadro", key: "R", Icon: Square },
  { id: "pen", label: "Lápiz", key: "L", Icon: Pencil },
  { id: "num", label: "Número", key: "N", Icon: Hash },
];

interface Props {
  /** La imagen sin anotar (data URL). */
  src: string;
  /** Anotaciones previas: volver a abrir el editor no pinta encima del PNG ya anotado. */
  initial: Shape[];
  onCancel: () => void;
  onDone: (shapes: Shape[], out: { url: string; data: string } | null) => void;
}

export function Annotator({ src, initial, onCancel, onDone }: Props) {
  const [tool, setTool] = useState<Tool>("arrow");
  const [color, setColor] = useState<string>(COLORS[0].value);
  const [shapes, setShapes] = useState<Shape[]>(initial);
  const [past, setPast] = useState<Shape[][]>([]);
  const [future, setFuture] = useState<Shape[][]>([]);
  const [draft, setDraft] = useState<Shape | null>(null);
  const [natural, setNatural] = useState<{ w: number; h: number } | null>(null);
  const [box, setBox] = useState<{ w: number; h: number } | null>(null);
  const [saving, setSaving] = useState(false);
  const stageRef = useRef<HTMLDivElement>(null);
  const svgRef = useRef<SVGSVGElement>(null);
  const start = useRef<Point | null>(null);

  // Tamaño real de la imagen.
  useEffect(() => {
    const img = new Image();
    img.onload = () => setNatural({ w: img.naturalWidth, h: img.naturalHeight });
    img.src = src;
  }, [src]);

  // Encaja la imagen en el escenario sin agrandarla nunca más allá de su tamaño real.
  useLayoutEffect(() => {
    const el = stageRef.current;
    if (!el || !natural) return;
    const fit = () => {
      const s = Math.min(el.clientWidth / natural.w, el.clientHeight / natural.h, 1);
      setBox({ w: Math.floor(natural.w * s), h: Math.floor(natural.h * s) });
    };
    fit();
    const ro = new ResizeObserver(fit);
    ro.observe(el);
    return () => ro.disconnect();
  }, [natural]);

  const width = natural ? strokeFor(natural.w, natural.h) : 4;
  const scale = natural && box ? box.w / natural.w : 1;

  const commit = useCallback(
    (next: Shape[]) => {
      setPast((p) => [...p, shapes]);
      setFuture([]);
      setShapes(next);
    },
    [shapes],
  );

  const undo = useCallback(() => {
    if (!past.length) return;
    setFuture((f) => [shapes, ...f]);
    setShapes(past[past.length - 1]);
    setPast((p) => p.slice(0, -1));
  }, [past, shapes]);

  const redo = useCallback(() => {
    if (!future.length) return;
    setPast((p) => [...p, shapes]);
    setShapes(future[0]);
    setFuture((f) => f.slice(1));
  }, [future, shapes]);

  const done = useCallback(async () => {
    if (saving) return;
    if (!shapes.length) return onDone([], null);
    setSaving(true);
    try {
      onDone(shapes, await renderAnnotated(src, shapes));
    } finally {
      setSaving(false);
    }
  }, [onDone, saving, shapes, src]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const k = e.key.toLowerCase();
      if (e.key === "Escape") {
        e.preventDefault();
        onCancel();
      } else if (e.key === "Enter") {
        e.preventDefault();
        done();
      } else if (e.ctrlKey && (k === "y" || (k === "z" && e.shiftKey))) {
        e.preventDefault();
        redo();
      } else if (e.ctrlKey && k === "z") {
        e.preventDefault();
        undo();
      } else if (!e.ctrlKey && !e.altKey) {
        const t = TOOLS.find((t) => t.key.toLowerCase() === k);
        if (t) setTool(t.id);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [done, onCancel, redo, undo]);

  const toImage = (e: React.PointerEvent): Point => {
    const r = svgRef.current!.getBoundingClientRect();
    return [((e.clientX - r.left) / r.width) * natural!.w, ((e.clientY - r.top) / r.height) * natural!.h];
  };

  const onDown = (e: React.PointerEvent<SVGSVGElement>) => {
    if (e.button !== 0 || !natural) return;
    const p = toImage(e);
    if (tool === "num") {
      commit([...shapes, { tool, color, width, pts: [p], n: nextNumber(shapes) }]);
      return;
    }
    e.currentTarget.setPointerCapture(e.pointerId);
    start.current = p;
    setDraft({ tool, color, width, pts: tool === "pen" ? [p] : [p, p] });
  };

  const onMove = (e: React.PointerEvent<SVGSVGElement>) => {
    if (!draft || !start.current) return;
    const p = toImage(e);
    if (draft.tool === "pen") {
      const last = draft.pts[draft.pts.length - 1];
      // Saltarse los puntos casi repetidos deja el trazo más limpio y más ligero.
      if (Math.hypot(p[0] - last[0], p[1] - last[1]) * scale < 1.5) return;
      setDraft({ ...draft, pts: [...draft.pts, p] });
    } else {
      setDraft({ ...draft, pts: [start.current, e.shiftKey ? constrain(draft.tool, start.current, p) : p] });
    }
  };

  const onUp = () => {
    if (draft && isMeaningful(draft, 6 / scale)) commit([...shapes, draft]);
    setDraft(null);
    start.current = null;
  };

  const visible = draft ? [...shapes, draft] : shapes;

  return (
    <motion.div
      initial={{ opacity: 0, scale: 0.985 }}
      animate={{ opacity: 1, scale: 1 }}
      transition={{ type: "spring", stiffness: 500, damping: 36 }}
      className="relative flex h-full w-full flex-col overflow-hidden rounded-[20px] border border-white/10 bg-[#0e0e11]/[0.98] shadow-[0_4px_10px_-2px_rgba(0,0,0,.5),0_0_0_1px_rgba(234,24,137,.14)]"
    >
      <header data-tauri-drag-region className="flex items-center gap-2 border-b border-white/[0.06] px-3 py-2">
        <div className="flex rounded-xl border border-white/10 bg-white/[0.03] p-0.5">
          {TOOLS.map(({ id, label, key, Icon }) => (
            <button
              key={id}
              onClick={() => setTool(id)}
              title={`${label} (${key})`}
              aria-pressed={tool === id}
              className={`flex items-center gap-1.5 rounded-lg px-2.5 py-1 text-[12px] transition ${
                tool === id ? "bg-accent-soft text-ink" : "text-ink-3 hover:text-ink-2"
              }`}
            >
              <Icon size={14} className={tool === id ? "text-accent" : ""} />
              {label}
            </button>
          ))}
        </div>

        <div className="ml-1 flex items-center gap-1.5 rounded-xl border border-white/10 bg-white/[0.03] px-2 py-1.5">
          {COLORS.map((c) => (
            <button
              key={c.value}
              onClick={() => setColor(c.value)}
              title={c.label}
              aria-label={c.label}
              aria-pressed={color === c.value}
              className={`size-4 rounded-full transition ${color === c.value ? "ring-2 ring-white/80 ring-offset-2 ring-offset-[#0e0e11]" : "opacity-70 hover:opacity-100"}`}
              style={{ background: c.value }}
            />
          ))}
        </div>

        <div className="ml-1 flex items-center">
          <button onClick={undo} disabled={!past.length} title="Deshacer (Ctrl Z)" className="rounded-lg p-1.5 text-ink-2 hover:bg-white/5 hover:text-ink disabled:opacity-30 disabled:hover:bg-transparent">
            <Undo2 size={15} />
          </button>
          <button onClick={redo} disabled={!future.length} title="Rehacer (Ctrl Y)" className="rounded-lg p-1.5 text-ink-2 hover:bg-white/5 hover:text-ink disabled:opacity-30 disabled:hover:bg-transparent">
            <Redo2 size={15} />
          </button>
          <button onClick={() => commit([])} disabled={!shapes.length} title="Borrar todo" className="rounded-lg p-1.5 text-ink-2 hover:bg-white/5 hover:text-ink disabled:opacity-30 disabled:hover:bg-transparent">
            <Trash2 size={15} />
          </button>
        </div>

        <div className="flex-1 self-stretch" data-tauri-drag-region />

        <button onClick={onCancel} className="flex items-center gap-1.5 rounded-xl px-3 py-1.5 text-[12.5px] text-ink-2 hover:bg-white/5 hover:text-ink">
          <X size={14} /> Cancelar
        </button>
        <button
          onClick={done}
          disabled={saving}
          className="flex items-center gap-1.5 rounded-xl bg-accent px-3.5 py-1.5 text-[12.5px] font-medium text-white shadow-[0_0_20px_var(--color-accent-glow)] transition hover:brightness-110 disabled:opacity-50"
        >
          <Check size={14} /> Listo
        </button>
      </header>

      <div
        ref={stageRef}
        className="relative m-4 grid min-h-0 flex-1 place-items-center bg-[radial-gradient(rgba(255,255,255,0.05)_1px,transparent_1px)] bg-[size:16px_16px]"
      >
        {natural && box && (
          <div className="relative overflow-hidden rounded-lg shadow-[0_8px_30px_rgba(0,0,0,.5)] ring-1 ring-white/10" style={{ width: box.w, height: box.h }}>
            <img src={src} alt="" draggable={false} className="block size-full" />
            <svg
              ref={svgRef}
              viewBox={`0 0 ${natural.w} ${natural.h}`}
              className="absolute inset-0 size-full touch-none"
              style={{ cursor: "crosshair" }}
              onPointerDown={onDown}
              onPointerMove={onMove}
              onPointerUp={onUp}
              onPointerCancel={onUp}
            >
              <g style={{ filter: `drop-shadow(0 ${width * 0.25}px ${width * 0.45}px rgba(0,0,0,.45))` }}>
                {visible.map((s, i) => (
                  <ShapeView key={i} s={s} />
                ))}
              </g>
            </svg>
          </div>
        )}
      </div>

      <footer className="flex items-center gap-3 border-t border-white/[0.06] bg-black/20 px-4 py-2 text-[11px] text-ink-3">
        <span>
          <span className="kbd">A</span> <span className="kbd">R</span> <span className="kbd">L</span> <span className="kbd">N</span> herramientas
        </span>
        <span>
          <span className="kbd">⇧</span> recto / cuadrado
        </span>
        <span>
          <span className="kbd">Ctrl Z</span> deshacer
        </span>
        <div className="flex-1" />
        <span>
          <span className="kbd">Enter</span> listo · <span className="kbd">Esc</span> cancelar
        </span>
      </footer>
    </motion.div>
  );
}

function ShapeView({ s }: { s: Shape }) {
  const common = { stroke: s.color, strokeWidth: s.width, strokeLinecap: "round" as const, strokeLinejoin: "round" as const, fill: "none" };
  if (s.tool === "rect") {
    const r = rectGeom(s.pts);
    return <rect x={r.x} y={r.y} width={r.w} height={r.h} rx={s.width * 1.2} {...common} />;
  }
  if (s.tool === "arrow") {
    const g = arrowGeom(s.pts, s.width);
    return (
      <g>
        <line x1={g.shaft[0][0]} y1={g.shaft[0][1]} x2={g.shaft[1][0]} y2={g.shaft[1][1]} {...common} />
        <polygon points={g.head.map((p) => p.join(",")).join(" ")} fill={s.color} />
      </g>
    );
  }
  if (s.tool === "pen") {
    return <polyline points={s.pts.map((p) => p.join(",")).join(" ")} {...common} />;
  }
  const [cx, cy] = s.pts[0];
  const r = numRadius(s.width);
  return (
    <g>
      <circle cx={cx} cy={cy} r={r} fill={s.color} />
      <text
        x={cx}
        y={cy + r * 0.06}
        textAnchor="middle"
        dominantBaseline="central"
        fill={numInk(s.color)}
        style={{ font: `700 ${Math.round(r * 1.15)}px "Geist Variable", system-ui, sans-serif`, filter: "none" }}
      >
        {s.n ?? 1}
      </text>
    </g>
  );
}
