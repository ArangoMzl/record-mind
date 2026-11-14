import { Check, FileText, Lightbulb, X } from "lucide-react";
import { motion } from "motion/react";
import { useEffect, useState } from "react";
import { api, timeAgo, type RuleDecision, type RuleSuggestion, type TaskCard } from "../lib/api";

/**
 * Reglas que Claude sugirió a partir de errores repetidos. Nada llega a CLAUDE.md sin pasar por
 * aquí: «Añadir a CLAUDE.md» la escribe en el archivo del proyecto; «Descartar» solo la archiva.
 */
export function RulesDialog(props: {
  rules: RuleSuggestion[];
  tasks: TaskCard[];
  onClose: () => void;
  onOpenTask: (id: number) => void;
  onDecided: (d: RuleDecision, accepted: boolean) => void;
}) {
  // Texto editable de cada regla antes de aceptarla.
  const [drafts, setDrafts] = useState<Record<number, string>>({});
  const [busy, setBusy] = useState<number | null>(null);
  const [errors, setErrors] = useState<Record<number, string>>({});

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        props.onClose();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [props]);

  const decide = async (r: RuleSuggestion, accept: boolean) => {
    setBusy(r.id);
    setErrors((e) => ({ ...e, [r.id]: "" }));
    try {
      const d = await api.decideRule(r.id, accept, accept ? (drafts[r.id] ?? r.rule) : undefined);
      props.onDecided(d, accept);
    } catch (err) {
      setErrors((e) => ({ ...e, [r.id]: String(err) }));
    } finally {
      setBusy(null);
    }
  };

  const title = (id: number) => props.tasks.find((t) => t.id === id)?.title;

  return (
    <motion.div
      className="fixed inset-0 z-[55] grid place-items-center bg-black/55 p-6 backdrop-blur-[2px]"
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      onMouseDown={(e) => e.target === e.currentTarget && props.onClose()}
    >
      <motion.div
        initial={{ opacity: 0, y: 10, scale: 0.98 }}
        animate={{ opacity: 1, y: 0, scale: 1 }}
        exit={{ opacity: 0, y: 10, scale: 0.98 }}
        transition={{ type: "spring", stiffness: 420, damping: 34 }}
        className="flex max-h-[85vh] w-[640px] max-w-full flex-col rounded-2xl border border-line-strong bg-surface shadow-2xl shadow-black/60"
      >
        <header className="flex items-center gap-2 border-b border-line px-5 py-3.5">
          <Lightbulb size={15} className="text-accent" />
          <h2 className="text-[14px] font-semibold">Reglas sugeridas</h2>
          <span className="text-[12px] text-ink-3">para el CLAUDE.md de cada proyecto</span>
          <div className="flex-1" />
          <button onClick={props.onClose} className="rounded-lg p-1.5 text-ink-3 hover:bg-white/5 hover:text-ink" title="Cerrar (Esc)">
            <X size={15} />
          </button>
        </header>

        <div className="min-h-0 flex-1 space-y-3 overflow-y-auto px-5 py-4">
          {props.rules.length === 0 && (
            <div className="rounded-xl border border-dashed border-line-strong px-4 py-6 text-center text-[12.5px] text-ink-3">
              No hay reglas pendientes. Claude las sugiere al terminar <code className="text-ink-2">/pendientes</code> cuando
              ve un error que se repite (o pídeselo con <code className="text-ink-2">/pendientes reglas</code>).
            </div>
          )}
          {props.rules.map((r) => (
            <div key={r.id} className="rounded-xl border border-line-strong bg-base p-3.5">
              <div className="mb-2 flex items-center gap-2 text-[11.5px] text-ink-3">
                <span className="rounded-md bg-white/[0.06] px-1.5 py-0.5 font-medium text-ink-2">{r.project}</span>
                <span>sugerida {timeAgo(r.created_at)}</span>
              </div>
              <textarea
                value={drafts[r.id] ?? r.rule}
                onChange={(e) => setDrafts((d) => ({ ...d, [r.id]: e.target.value }))}
                rows={2}
                className="w-full resize-none rounded-lg border border-line-strong bg-surface px-3 py-2 text-[13.5px] leading-relaxed text-ink outline-none focus:border-accent/50"
              />
              {r.reason && <p className="mt-2 text-[12.5px] leading-relaxed text-ink-2">{r.reason}</p>}
              {r.task_ids.length > 0 && (
                <div className="mt-2 flex flex-wrap gap-1.5">
                  {r.task_ids.map((id) => (
                    <button
                      key={id}
                      onClick={() => props.onOpenTask(id)}
                      title={title(id) ?? "Abrir la tarea"}
                      className="max-w-56 truncate rounded-md border border-line-strong px-1.5 py-0.5 text-[11.5px] text-ink-2 hover:border-white/25 hover:text-ink"
                    >
                      #{id}
                      {title(id) ? ` ${title(id)}` : ""}
                    </button>
                  ))}
                </div>
              )}
              {errors[r.id] && <p className="mt-2 text-[12px] text-red-300">{errors[r.id]}</p>}
              <div className="mt-3 flex items-center gap-2">
                <button
                  onClick={() => decide(r, true)}
                  disabled={busy !== null || !(drafts[r.id] ?? r.rule).trim()}
                  className="flex items-center gap-1.5 rounded-xl bg-accent px-3 py-1.5 text-[12.5px] font-medium text-white shadow-[0_0_18px_var(--color-accent-glow)] transition hover:brightness-110 disabled:opacity-40"
                >
                  <FileText size={13} /> Añadir a CLAUDE.md
                </button>
                <button
                  onClick={() => decide(r, false)}
                  disabled={busy !== null}
                  className="rounded-xl px-3 py-1.5 text-[12.5px] text-ink-2 hover:bg-white/5 hover:text-ink disabled:opacity-40"
                >
                  Descartar
                </button>
                {busy === r.id && <Check size={13} className="animate-pulse text-ink-3" />}
              </div>
            </div>
          ))}
        </div>

        <footer className="border-t border-line px-5 py-3 text-[11.5px] text-ink-3">
          Se añade como viñeta en la sección «Reglas aprendidas (record-mind)» del CLAUDE.md del proyecto; el resto del
          archivo no se toca.
        </footer>
      </motion.div>
    </motion.div>
  );
}
