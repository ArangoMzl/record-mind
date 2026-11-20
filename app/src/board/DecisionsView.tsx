import { Check, ChevronDown, FileText, MessageSquareText, RotateCcw, Sparkles, X } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useMemo, useState } from "react";
import { api, parseDate, timeAgo, type Decision, type DecisionFile } from "../lib/api";
import type { Toast } from "./Toasts";

export type DecisionFilter = "open" | "all";

/** Días que lleva abierta una decisión; a partir de aquí se resalta para que no se quede olvidada. */
const STALE_DAYS = 3;

/**
 * Decisiones que quedaron pendientes en conversaciones de Claude Code. Cada tarjeta es una
 * conversación (un .md en ~/.record-mind/decisiones); marcar una decisión cambia su casilla en el
 * archivo. Claude las registra con /decisiones y cierra las que se resuelven en otra conversación.
 */
export function DecisionsView(props: {
  files: DecisionFile[];
  filter: DecisionFilter;
  showProject: boolean;
  /** Sin ninguna conversación registrada (ni con otros filtros). */
  empty: boolean;
  onToast: (t: Omit<Toast, "id">) => void;
  onChanged: () => void;
}) {
  const visible = useMemo(
    () =>
      props.files
        .map((f) => ({ ...f, decisions: f.decisions.filter((d) => props.filter === "all" || !d.resolved) }))
        .filter((f) => f.decisions.length > 0),
    [props.files, props.filter],
  );

  if (visible.length === 0) {
    return (
      <div className="grid flex-1 place-items-center p-8">
        <div className="max-w-md rounded-2xl border border-dashed border-line-strong px-6 py-8 text-center">
          <MessageSquareText size={22} className="mx-auto mb-3 text-ink-3" />
          <p className="text-[14px] font-medium text-ink-2">
            {props.empty ? "Todavía no hay decisiones registradas" : "No hay decisiones abiertas aquí"}
          </p>
          <p className="mt-2 text-[12.5px] leading-relaxed text-ink-3">
            Cuando una conversación con Claude deje cosas por decidir, escribe <code className="text-ink-2">/decisiones</code>.
            Claude las deja aquí con un resumen, cierra las que ya se resolvieron y crea capturas para la deuda técnica.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="min-h-0 flex-1 overflow-y-auto px-5 py-5">
      <div className="mx-auto flex max-w-3xl flex-col gap-4">
        <AnimatePresence initial={false}>
          {visible.map((f) => (
            <Conversation key={f.key} file={f} showProject={props.showProject} onToast={props.onToast} onChanged={props.onChanged} />
          ))}
        </AnimatePresence>
      </div>
    </div>
  );
}

function Conversation(props: {
  file: DecisionFile;
  showProject: boolean;
  onToast: (t: Omit<Toast, "id">) => void;
  onChanged: () => void;
}) {
  const f = props.file;
  const [summaryOpen, setSummaryOpen] = useState(false);
  const open = f.decisions.filter((d) => !d.resolved).length;

  const openFile = () =>
    api.openDecisionFile(f.key).catch((err) => props.onToast({ tone: "error", title: "No se pudo abrir el archivo", body: String(err) }));

  return (
    <motion.section
      layout
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -6 }}
      transition={{ type: "spring", stiffness: 420, damping: 36 }}
      className="rounded-2xl border border-line-strong bg-surface/80"
    >
      <header className="flex items-start gap-3 border-b border-line px-4 pt-3.5 pb-3">
        <div className="min-w-0 flex-1">
          <div className="mb-1 flex flex-wrap items-center gap-2 text-[11.5px] text-ink-3">
            {props.showProject && <span className="rounded-md bg-white/[0.06] px-1.5 py-0.5 font-medium text-ink-2">{f.project}</span>}
            <span title={parseDate(f.updated_at).toLocaleString("es")}>actualizado {timeAgo(f.updated_at)}</span>
            {open > 0 && (
              <span className="rounded-full bg-st-review/15 px-1.5 font-medium text-st-review">
                {open} {open === 1 ? "abierta" : "abiertas"}
              </span>
            )}
          </div>
          <h2 className="text-[15px] leading-snug font-semibold tracking-tight">{f.title}</h2>
          {f.summary && (
            <button
              onClick={() => setSummaryOpen((v) => !v)}
              className="group mt-1 flex w-full items-start gap-1 text-left text-[12.5px] leading-relaxed text-ink-2"
              title={summaryOpen ? "Ocultar el resumen" : "Ver el resumen completo"}
            >
              <span className={summaryOpen ? "whitespace-pre-line" : "line-clamp-2"}>{f.summary}</span>
              <ChevronDown size={13} className={`mt-1 shrink-0 text-ink-3 transition group-hover:text-ink-2 ${summaryOpen ? "rotate-180" : ""}`} />
            </button>
          )}
        </div>
        <button
          onClick={openFile}
          title={f.path}
          className="flex shrink-0 items-center gap-1.5 rounded-lg border border-line-strong px-2 py-1 text-[11.5px] text-ink-2 transition hover:border-white/25 hover:text-ink"
        >
          <FileText size={12} /> Abrir .md
        </button>
      </header>

      <ul className="divide-y divide-line">
        <AnimatePresence initial={false}>
          {f.decisions.map((d) => (
            <DecisionRow key={d.id} decision={d} onToast={props.onToast} onChanged={props.onChanged} />
          ))}
        </AnimatePresence>
      </ul>
    </motion.section>
  );
}

function DecisionRow(props: { decision: Decision; onToast: (t: Omit<Toast, "id">) => void; onChanged: () => void }) {
  const d = props.decision;
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);

  const days = d.created_at ? (Date.now() - parseDate(d.created_at).getTime()) / 86_400_000 : 0;
  const stale = !d.resolved && days >= STALE_DAYS;

  const run = async (action: () => Promise<unknown>, done: Omit<Toast, "id">) => {
    setBusy(true);
    try {
      await action();
      props.onToast(done);
      setEditing(false);
      props.onChanged();
    } catch (err) {
      props.onToast({ tone: "error", title: "No se pudo guardar", body: String(err) });
    } finally {
      setBusy(false);
    }
  };

  const resolve = (text: string | null) =>
    run(() => api.resolveDecision(d.id, text?.trim() || null), { tone: "review", title: "Decisión tomada", body: text?.trim() || d.question });
  const discard = () => run(() => api.resolveDecision(d.id, null, true), { tone: "inbox", title: "Decisión descartada", body: d.question });
  const reopen = () => run(() => api.reopenDecision(d.id), { tone: "inbox", title: "Decisión abierta de nuevo", body: d.question });

  return (
    <motion.li
      layout
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0, height: 0 }}
      className={`group flex gap-3 px-4 py-3 ${d.resolved ? "opacity-70" : ""}`}
    >
      <button
        onClick={() => (d.resolved ? reopen() : resolve(null))}
        disabled={busy}
        title={d.resolved ? "Volver a abrirla" : "Marcar como tomada"}
        aria-label={d.resolved ? "Volver a abrir la decisión" : "Marcar la decisión como tomada"}
        className={`mt-0.5 grid size-[18px] shrink-0 place-items-center rounded-full border transition disabled:opacity-40 ${
          d.resolved
            ? "border-st-done bg-st-done text-[var(--color-base)]"
            : "border-line-strong text-transparent hover:border-st-done hover:text-st-done"
        }`}
      >
        <Check size={11} strokeWidth={3} />
      </button>

      <div className="min-w-0 flex-1">
        <p className={`text-[13.5px] leading-snug font-medium ${d.resolved ? "text-ink-2 line-through decoration-ink-3/60" : "text-ink"}`}>
          {d.question}
        </p>
        {d.context && !d.resolved && <p className="mt-1 text-[12.5px] leading-relaxed text-ink-2">{d.context}</p>}
        {d.recommendation && !d.resolved && (
          <p className="mt-2 flex items-start gap-1.5 rounded-lg bg-accent-soft px-2.5 py-1.5 text-[12px] leading-relaxed text-ink-2">
            <Sparkles size={12} className="mt-[3px] shrink-0 text-accent" />
            <span>
              <span className="font-medium text-accent">Claude recomienda:</span> {d.recommendation}
            </span>
          </p>
        )}
        {d.resolved && (
          <p className="mt-1 text-[12.5px] leading-relaxed text-ink-2">
            {d.resolution && <span className="text-ink">→ {d.resolution}</span>}
            <span className="text-ink-3">
              {d.resolution ? " · " : ""}
              {d.resolved_by === "tablero" ? "desde el tablero" : d.resolved_by ? `por ${d.resolved_by}` : "tomada"}
              {d.resolved_at ? ` ${timeAgo(d.resolved_at)}` : ""}
            </span>
          </p>
        )}

        <AnimatePresence initial={false}>
          {editing && (
            <motion.div
              initial={{ opacity: 0, height: 0 }}
              animate={{ opacity: 1, height: "auto" }}
              exit={{ opacity: 0, height: 0 }}
              className="overflow-hidden"
            >
              <textarea
                autoFocus
                value={draft}
                onChange={(e) => setDraft(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && !e.shiftKey) {
                    e.preventDefault();
                    if (draft.trim()) resolve(draft);
                  } else if (e.key === "Escape") {
                    e.stopPropagation();
                    setEditing(false);
                  }
                }}
                rows={2}
                placeholder="Qué decidiste (Enter para guardar)"
                className="mt-2.5 w-full resize-none rounded-lg border border-line-strong bg-base px-3 py-2 text-[13px] leading-relaxed text-ink outline-none select-text placeholder:text-ink-3 focus:border-accent/50"
              />
              <div className="mt-2 flex flex-wrap items-center gap-2">
                <button
                  onClick={() => resolve(draft)}
                  disabled={busy || !draft.trim()}
                  className="flex items-center gap-1.5 rounded-xl bg-accent px-3 py-1.5 text-[12.5px] font-medium text-white shadow-[0_0_18px_var(--color-accent-glow)] transition hover:brightness-110 disabled:opacity-40"
                >
                  <Check size={13} /> Guardar decisión
                </button>
                {d.recommendation && (
                  <button
                    onClick={() => setDraft(d.recommendation ?? "")}
                    className="rounded-xl border border-accent/30 px-3 py-1.5 text-[12.5px] text-accent transition hover:border-accent/60"
                  >
                    Usar la recomendación
                  </button>
                )}
                <button onClick={() => setEditing(false)} className="rounded-xl px-3 py-1.5 text-[12.5px] text-ink-2 hover:bg-white/5 hover:text-ink">
                  Cancelar
                </button>
              </div>
            </motion.div>
          )}
        </AnimatePresence>

        {!editing && (
          <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-[11.5px]">
            {d.created_at && !d.resolved && (
              <span
                className={stale ? "font-medium text-st-review" : "text-ink-3"}
                title={stale ? `Lleva más de ${STALE_DAYS} días abierta` : parseDate(d.created_at).toLocaleString("es")}
              >
                abierta {timeAgo(d.created_at)}
              </span>
            )}
            {!d.resolved ? (
              <>
                <button
                  onClick={() => {
                    setDraft("");
                    setEditing(true);
                  }}
                  disabled={busy}
                  className="font-medium text-ink-2 transition hover:text-ink disabled:opacity-40"
                >
                  Decidir…
                </button>
                <button onClick={discard} disabled={busy} className="flex items-center gap-1 text-ink-3 transition hover:text-ink-2 disabled:opacity-40">
                  <X size={11} /> Descartar
                </button>
              </>
            ) : (
              <button onClick={reopen} disabled={busy} className="flex items-center gap-1 text-ink-3 transition hover:text-ink-2 disabled:opacity-40">
                <RotateCcw size={11} /> Reabrir
              </button>
            )}
          </div>
        )}
      </div>
    </motion.li>
  );
}
