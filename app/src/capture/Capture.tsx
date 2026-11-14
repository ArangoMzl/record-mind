import { Check, Flame, ImagePlus, LayoutGrid, PenLine, Scissors, X } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useCallback, useEffect, useRef, useState } from "react";
import { api, onCaptureOpen, type CaptureContext } from "../lib/api";
import iconUrl from "../../src-tauri/icons/128x128.png";
import { ProjectPicker } from "../lib/ProjectPicker";
import { Annotator } from "./Annotator";
import type { Shape } from "./annotate";

interface Shot {
  id: number;
  /** Lo que se ve y se guarda: la imagen original o, si se anotó, el PNG con las anotaciones. */
  url: string;
  data: string;
  ext: string;
  /** La imagen tal como llegó, para poder volver a anotar sin pintar encima. */
  original: { url: string; data: string; ext: string };
  shapes: Shape[];
}

const PRIORITIES = [
  { v: 0, label: "Normal" },
  { v: 1, label: "Alta" },
  { v: 2, label: "Urgente" },
];

const EXT: Record<string, string> = { "image/png": "png", "image/jpeg": "jpg", "image/webp": "webp", "image/gif": "gif" };

export function Capture() {
  const [ctx, setCtx] = useState<CaptureContext | null>(null);
  const [project, setProject] = useState("");
  const [note, setNote] = useState("");
  const [shots, setShots] = useState<Shot[]>([]);
  const [priority, setPriority] = useState(0);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [dragOver, setDragOver] = useState(false);
  /** Captura que se está anotando (su id), o null. */
  const [editing, setEditing] = useState<number | null>(null);
  const noteRef = useRef<HTMLTextAreaElement>(null);

  const load = useCallback(async () => {
    const [c, snips] = await Promise.all([api.captureContext(), api.takeSnips()]);
    setCtx(c);
    // Proyecto por defecto: la sesión activa de Claude Code, si no hay uno elegido.
    setProject((p) => p || c.sessions[0]?.project || c.projects[0] || "");
    // Los recortes de pantalla llegan como PNG en base64 y entran como cualquier captura pegada.
    if (snips.length) {
      const fresh = snips.map(({ data, ext }): Shot => {
        const original = { url: `data:image/${ext};base64,${data}`, data, ext };
        return { id: Math.random(), ...original, original, shapes: [] };
      });
      setShots((s) => [...s, ...fresh].slice(0, 6));
    }
    setTimeout(() => noteRef.current?.focus(), 30);
  }, []);

  useEffect(() => {
    load();
    const un = onCaptureOpen(() => {
      setSaved(null);
      setError(null);
      load();
    });
    return () => {
      un.then((f) => f());
    };
  }, [load]);

  const addFiles = useCallback(async (files: File[]) => {
    const images = files.filter((f) => EXT[f.type]);
    const read = await Promise.all(
      images.map(
        (f) =>
          new Promise<Shot>((resolve, reject) => {
            const r = new FileReader();
            r.onload = () => {
              const url = String(r.result);
              const original = { url, data: url.slice(url.indexOf(",") + 1), ext: EXT[f.type] };
              resolve({ id: Math.random(), ...original, original, shapes: [] });
            };
            r.onerror = reject;
            r.readAsDataURL(f);
          }),
      ),
    );
    setShots((s) => [...s, ...read].slice(0, 6));
  }, []);

  useEffect(() => {
    const onPaste = (e: ClipboardEvent) => {
      if (editing !== null) return;
      const files = Array.from(e.clipboardData?.files ?? []);
      if (files.some((f) => EXT[f.type])) {
        e.preventDefault();
        addFiles(files);
      }
    };
    window.addEventListener("paste", onPaste);
    return () => window.removeEventListener("paste", onPaste);
  }, [addFiles, editing]);

  const close = () => api.hideCapture();

  // El editor necesita espacio: la ventana crece mientras está abierto y vuelve a su tamaño al cerrarlo.
  const openEditor = (id: number) => {
    setEditing(id);
    api.resizeCapture(true);
  };

  const closeEditor = () => {
    setEditing(null);
    api.resizeCapture(false);
    setTimeout(() => noteRef.current?.focus(), 30);
  };

  const finishEditing = (id: number, shapes: Shape[], out: { url: string; data: string } | null) => {
    setShots((xs) =>
      xs.map((s) => (s.id !== id ? s : out ? { ...s, url: out.url, data: out.data, ext: "png", shapes } : { ...s, ...s.original, shapes: [] })),
    );
    closeEditor();
  };

  const editingShot = shots.find((s) => s.id === editing);
  if (editingShot) {
    return (
      <div className="flex h-full p-3">
        <Annotator
          key={editingShot.id}
          src={editingShot.original.url}
          initial={editingShot.shapes}
          onCancel={closeEditor}
          onDone={(shapes, out) => finishEditing(editingShot.id, shapes, out)}
        />
      </div>
    );
  }

  const reset = () => {
    setNote("");
    setShots([]);
    setPriority(0);
  };

  const save = async () => {
    if (saving || !note.trim() || !project.trim()) return;
    setSaving(true);
    setError(null);
    try {
      const t = await api.create({
        project: project.trim(),
        note,
        priority,
        images: shots.map(({ data, ext }) => ({ data, ext })),
      });
      setSaved(`#${t.id} capturada en ${t.project}`);
      reset();
      setTimeout(() => {
        close();
        setSaved(null);
      }, 950);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      close();
    } else if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
      e.preventDefault();
      save();
    }
  };

  return (
    <div className="flex h-full items-center justify-center p-3" onKeyDown={onKey}>
      <motion.div
        initial={{ opacity: 0, scale: 0.97, y: 6 }}
        animate={{ opacity: 1, scale: 1, y: 0 }}
        transition={{ type: "spring", stiffness: 500, damping: 34 }}
        onDragOver={(e) => {
          e.preventDefault();
          setDragOver(true);
        }}
        onDragLeave={() => setDragOver(false)}
        onDrop={(e) => {
          e.preventDefault();
          setDragOver(false);
          addFiles(Array.from(e.dataTransfer.files));
        }}
        className="relative flex h-full w-full flex-col overflow-hidden rounded-[20px] border border-white/10 bg-[#0e0e11]/[0.97] shadow-[0_4px_10px_-2px_rgba(0,0,0,.5),0_0_0_1px_rgba(234,24,137,.14)]"
      >
        <div className="pointer-events-none absolute inset-x-0 top-0 h-28 bg-[radial-gradient(420px_120px_at_50%_-20%,rgba(234,24,137,.22),transparent)]" />

        <header data-tauri-drag-region className="relative flex items-center gap-2.5 px-4 pt-3.5 pb-2">
          <img src={iconUrl} alt="" className="pointer-events-none size-6" />
          <span data-tauri-drag-region className="text-[13px] font-semibold">
            Nuevo pendiente
          </span>

          <div className="ml-2 flex items-center">
            <ProjectPicker
              value={project}
              projects={ctx?.projects ?? []}
              active={ctx?.sessions.map((s) => s.project)}
              onChange={(p) => {
                setProject(p);
                setTimeout(() => noteRef.current?.focus(), 0);
              }}
            />
          </div>

          <div className="flex-1" data-tauri-drag-region />
          <button onClick={() => api.openBoard()} className="rounded-lg p-1.5 text-ink-3 hover:bg-white/5 hover:text-ink" title="Abrir tablero">
            <LayoutGrid size={15} />
          </button>
          <button onClick={close} className="rounded-lg p-1.5 text-ink-3 hover:bg-white/5 hover:text-ink" title="Cerrar (Esc)">
            <X size={16} />
          </button>
        </header>

        <div className="relative flex min-h-0 flex-1 flex-col px-4">
          <textarea
            ref={noteRef}
            value={note}
            onChange={(e) => setNote(e.target.value)}
            placeholder="¿Qué hay que corregir?  Ej: el título se ve muy pequeño en móvil"
            className="min-h-0 flex-1 resize-none bg-transparent py-2 text-[15.5px] leading-relaxed text-ink outline-none placeholder:text-ink-3"
          />

          <div className="flex gap-2 pb-3">
            <AnimatePresence initial={false}>
              {shots.map((s) => (
                <motion.div
                  key={s.id}
                  initial={{ opacity: 0, scale: 0.8 }}
                  animate={{ opacity: 1, scale: 1 }}
                  exit={{ opacity: 0, scale: 0.8 }}
                  className="group relative h-[72px] w-[112px] overflow-hidden rounded-xl border border-white/10 bg-black/40"
                >
                  <button onClick={() => openEditor(s.id)} title="Anotar con flechas y recuadros" className="block size-full">
                    <img src={s.url} alt="" className="size-full object-cover object-top" />
                    <span className="absolute inset-0 grid place-items-center bg-black/55 opacity-0 transition group-hover:opacity-100">
                      <span className="flex items-center gap-1 text-[11.5px] font-medium text-white">
                        <PenLine size={13} /> Anotar
                      </span>
                    </span>
                  </button>
                  {s.shapes.length > 0 && (
                    <span className="pointer-events-none absolute bottom-1 left-1 grid size-5 place-items-center rounded-md bg-accent text-white shadow" title="Anotada">
                      <PenLine size={11} />
                    </span>
                  )}
                  <button
                    onClick={() => setShots((xs) => xs.filter((x) => x.id !== s.id))}
                    className="absolute top-1 right-1 rounded-md bg-black/70 p-0.5 text-ink-2 opacity-0 transition group-hover:opacity-100 hover:text-white"
                  >
                    <X size={12} />
                  </button>
                </motion.div>
              ))}
            </AnimatePresence>
            {shots.length === 0 ? (
              <div className="flex h-[72px] flex-1 items-center gap-3 rounded-xl border border-dashed border-white/10 pr-2.5 pl-4 text-[12px] text-ink-3">
                <ImagePlus size={18} />
                <span className="flex-1">
                  Pega una captura con <span className="kbd">Ctrl V</span> o arrástrala aquí
                </span>
                <button
                  onClick={() => api.startSnip()}
                  className="flex items-center gap-1.5 rounded-lg border border-white/10 bg-white/[0.04] px-2.5 py-1.5 text-[12px] text-ink-2 transition hover:border-accent/50 hover:text-ink"
                >
                  <Scissors size={14} className="text-accent" /> Recortar pantalla
                </button>
              </div>
            ) : (
              shots.length < 6 && (
                <button
                  onClick={() => api.startSnip()}
                  title="Recortar otra parte de la pantalla"
                  className="flex h-[72px] w-[72px] flex-col items-center justify-center gap-1 rounded-xl border border-dashed border-white/10 text-[11px] text-ink-3 transition hover:border-accent/50 hover:text-ink-2"
                >
                  <Scissors size={16} />
                  Recortar
                </button>
              )
            )}
          </div>
        </div>

        <footer className="relative flex items-center gap-2 border-t border-white/[0.06] bg-black/20 px-4 py-2.5">
          <div className="flex rounded-lg border border-white/10 p-0.5">
            {PRIORITIES.map((p) => (
              <button
                key={p.v}
                onClick={() => setPriority(p.v)}
                className={`flex items-center gap-1 rounded-md px-2 py-0.5 text-[11.5px] transition ${
                  priority === p.v
                    ? p.v === 0
                      ? "bg-white/10 text-ink"
                      : p.v === 1
                        ? "bg-amber-400/15 text-amber-300"
                        : "bg-red-400/15 text-red-300"
                    : "text-ink-3 hover:text-ink-2"
                }`}
              >
                {p.v > 0 && <Flame size={11} />}
                {p.label}
              </button>
            ))}
          </div>
          <span className="ml-1 text-[11px] text-ink-3">
            <span className="kbd">⇧ Enter</span> salto · <span className="kbd">Esc</span> cerrar
          </span>
          <div className="flex-1" />
          {error && <span className="max-w-48 truncate text-[11.5px] text-red-300" title={error}>{error}</span>}
          <button
            onClick={save}
            disabled={!note.trim() || !project.trim() || saving}
            className="flex items-center gap-2 rounded-xl bg-accent px-3.5 py-1.5 text-[13px] font-medium text-white shadow-[0_0_20px_var(--color-accent-glow)] transition hover:brightness-110 disabled:opacity-40 disabled:shadow-none"
          >
            Guardar <span className="kbd border-white/25 bg-white/10 text-white/80">Enter</span>
          </button>
        </footer>

        <AnimatePresence>
          {dragOver && (
            <motion.div
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              className="pointer-events-none absolute inset-2 grid place-items-center rounded-2xl border-2 border-dashed border-accent/60 bg-accent/10 text-[14px] font-medium text-accent"
            >
              Suelta la imagen
            </motion.div>
          )}
          {saved && (
            <motion.div
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              className="absolute inset-0 grid place-items-center bg-[#0e0e11]/95"
            >
              <motion.div initial={{ scale: 0.8 }} animate={{ scale: 1 }} transition={{ type: "spring", stiffness: 500, damping: 22 }} className="flex flex-col items-center gap-3">
                <span className="grid size-14 place-items-center rounded-full bg-accent shadow-[0_0_40px_var(--color-accent-glow)]">
                  <Check size={28} strokeWidth={3} className="text-white" />
                </span>
                <span className="text-[14px] font-medium">{saved}</span>
              </motion.div>
            </motion.div>
          )}
        </AnimatePresence>
      </motion.div>
    </div>
  );
}
