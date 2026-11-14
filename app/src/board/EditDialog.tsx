import { Flame, Lock, Pencil } from "lucide-react";
import { motion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { PRIORITIES, type Task, type TaskEdit } from "../lib/api";
import { ProjectPicker } from "../lib/ProjectPicker";

/** Diálogo para editar título, nota, prioridad y proyecto de una tarea. */
export function EditDialog(props: {
  task: Task;
  projects: string[];
  onClose: () => void;
  onSave: (edit: TaskEdit) => Promise<void>;
}) {
  const t = props.task;
  // Mientras Claude la trabaja solo se cambia la prioridad (la regla vive en rm-core).
  const locked = t.status === "in_progress";
  const [title, setTitle] = useState(t.title);
  const [note, setNote] = useState(t.note);
  const [priority, setPriority] = useState(t.priority);
  const [project, setProject] = useState(t.project);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const titleRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!locked) titleRef.current?.select();
  }, [locked]);

  const edit: TaskEdit = {};
  if (title.trim() !== t.title) edit.title = title;
  if (note.trim() !== t.note) edit.note = note;
  if (priority !== t.priority) edit.priority = priority;
  if (project.trim() !== t.project) edit.project = project;
  const dirty = Object.keys(edit).length > 0;
  const valid = title.trim() !== "" && project.trim() !== "";

  const save = async () => {
    if (!dirty) return props.onClose();
    if (!valid || saving) return;
    setSaving(true);
    try {
      await props.onSave(edit);
      props.onClose();
    } catch (e) {
      setError(String(e));
      setSaving(false);
    }
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // Con el selector de proyecto abierto, Esc solo cierra el selector.
      if (e.key === "Escape" && (e.target as Element | null)?.closest?.("[data-picker-open]")) return;
      if (e.key === "Escape") {
        e.stopPropagation();
        props.onClose();
      } else if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        save();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  });

  const field =
    "w-full rounded-xl border border-line-strong bg-base px-3 py-2 text-[13.5px] outline-none placeholder:text-ink-3 focus:border-accent/50 disabled:opacity-50 select-text";

  return (
    <motion.div
      className="fixed inset-0 z-[55] grid place-items-center bg-black/55 p-6 backdrop-blur-[2px]"
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      onMouseDown={(e) => e.target === e.currentTarget && props.onClose()}
    >
      <motion.form
        initial={{ opacity: 0, y: 10, scale: 0.98 }}
        animate={{ opacity: 1, y: 0, scale: 1 }}
        exit={{ opacity: 0, y: 10, scale: 0.98 }}
        transition={{ type: "spring", stiffness: 420, damping: 34 }}
        onSubmit={(e) => {
          e.preventDefault();
          save();
        }}
        className="w-[540px] max-w-full rounded-2xl border border-line-strong bg-surface shadow-2xl shadow-black/60"
      >
        <header className="flex items-center gap-2 border-b border-line px-5 py-3.5">
          <Pencil size={15} className="text-accent" />
          <h2 className="text-[14px] font-semibold">Editar tarea</h2>
          <span className="font-mono text-[12px] text-ink-3">#{t.id}</span>
        </header>

        <div className="space-y-4 px-5 py-4">
          {locked && (
            <div className="flex gap-2 rounded-xl border border-accent/25 bg-accent-soft/60 px-3 py-2 text-[12px] text-ink-2">
              <Lock size={13} className="mt-0.5 shrink-0 text-accent" />
              Claude está trabajando en esta tarea: solo puedes cambiar la prioridad hasta que la entregue.
            </div>
          )}

          <label className="block">
            <span className="eyebrow mb-1.5 block">Título</span>
            <input ref={titleRef} value={title} disabled={locked} onChange={(e) => setTitle(e.target.value)} className={field} />
          </label>

          <label className="block">
            <span className="eyebrow mb-1.5 block">Nota</span>
            <textarea
              value={note}
              disabled={locked}
              onChange={(e) => setNote(e.target.value)}
              rows={5}
              placeholder="Qué hay que corregir y dónde"
              className={`${field} resize-none leading-relaxed`}
            />
          </label>

          <div className="grid grid-cols-[auto_minmax(0,1fr)] gap-4">
            <div>
              <span className="eyebrow mb-1.5 block">Prioridad</span>
              <div className="flex rounded-xl border border-line-strong bg-base p-0.5">
                {PRIORITIES.map((p) => (
                  <button
                    key={p.value}
                    type="button"
                    onClick={() => setPriority(p.value)}
                    className={`flex items-center gap-1 rounded-[10px] px-3 py-1.5 text-[12.5px] font-medium transition ${
                      priority === p.value ? "bg-white/[0.09] text-ink" : "text-ink-3 hover:text-ink-2"
                    }`}
                  >
                    {p.value > 0 && (
                      <span className={`flex ${p.value > 1 ? "text-red-400" : "text-amber-400"}`}>
                        <Flame size={12} />
                        {p.value > 1 && <Flame size={12} className="-ml-1" />}
                      </span>
                    )}
                    {p.label}
                  </button>
                ))}
              </div>
            </div>
            <div>
              <span className="eyebrow mb-1.5 block">Proyecto</span>
              <ProjectPicker variant="field" value={project} projects={props.projects} disabled={locked} onChange={setProject} />
            </div>
          </div>

          {error && <p className="rounded-lg bg-red-400/10 px-3 py-2 text-[12.5px] text-red-300">{error}</p>}
        </div>

        <footer className="flex items-center gap-2 border-t border-line px-5 py-3.5">
          <span className="text-[11.5px] text-ink-3">
            <span className="kbd">Ctrl Enter</span> guarda · <span className="kbd">Esc</span> cancela
          </span>
          <div className="flex-1" />
          <button
            type="button"
            onClick={props.onClose}
            className="rounded-xl border border-line-strong bg-white/[0.03] px-3.5 py-2 text-[13px] font-medium transition hover:bg-white/[0.07]"
          >
            Cancelar
          </button>
          <button
            type="submit"
            disabled={!valid || saving}
            className="rounded-xl bg-accent px-3.5 py-2 text-[13px] font-medium text-white transition hover:brightness-110 disabled:opacity-40"
          >
            {saving ? "Guardando…" : "Guardar"}
          </button>
        </footer>
      </motion.form>
    </motion.div>
  );
}
