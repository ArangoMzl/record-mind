import { Brain, Check, CircleDashed, FileCode2, Lock, Pencil, Plus, RotateCcw, Sparkles, Trash2, Unlock, X } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import {
  actorName,
  api,
  assetUrl,
  EVENT_LABEL,
  STATUSES,
  timeAgo,
  type Memory,
  type Status,
  type TaskDetail,
} from "../lib/api";
import { Menu, WorkingLabel, type MenuItem } from "./Menu";
import type { Toast } from "./Toasts";

const MOVE_TARGETS: Record<Status, MenuItem[]> = {
  // Solo Para revisar ↔ Hecho (ver canMove). Pasar a Hecho desde revisión es «Aprobar».
  inbox: [],
  in_progress: [],
  review: [],
  done: [{ id: "review", label: "Para revisar", hint: "Vuelve a revisión, sin nota", color: "var(--color-st-review)" }],
};

export function DetailDrawer(props: {
  id: number;
  version?: string;
  onClose: () => void;
  onToast: (t: Omit<Toast, "id">) => void;
  onEdit: (task: TaskDetail) => void;
}) {
  const [d, setD] = useState<TaskDetail | null>(null);
  const [zoom, setZoom] = useState<string | null>(null);
  const [returning, setReturning] = useState(false);
  const [note, setNote] = useState("");
  const [confirmDelete, setConfirmDelete] = useState(false);
  const noteRef = useRef<HTMLTextAreaElement>(null);

  // Se recarga cuando la tarea cambia (p. ej. Claude la completa con el panel abierto).
  useEffect(() => {
    api.detail(props.id).then(setD).catch((e) => props.onToast({ tone: "error", title: "Error", body: String(e) }));
  }, [props.id, props.version]);

  useEffect(() => {
    if (returning) noteRef.current?.focus();
  }, [returning]);

  const act = async (fn: () => Promise<unknown>, ok?: string) => {
    try {
      await fn();
      if (ok) props.onToast({ tone: "inbox", title: ok });
    } catch (e) {
      props.onToast({ tone: "error", title: "No se pudo completar", body: String(e) });
    }
  };

  const status = STATUSES.find((s) => s.id === d?.status);

  return (
    <>
      <motion.div
        className="fixed inset-0 z-30 bg-black/50 backdrop-blur-[2px]"
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        exit={{ opacity: 0 }}
        onClick={props.onClose}
      />
      <motion.aside
        className="fixed top-0 right-0 bottom-0 z-40 flex w-[540px] max-w-[92vw] flex-col border-l border-line-strong bg-surface shadow-2xl shadow-black/60"
        initial={{ x: "100%" }}
        animate={{ x: 0 }}
        exit={{ x: "100%" }}
        transition={{ type: "spring", stiffness: 380, damping: 38 }}
      >
        {!d ? (
          <div className="p-6 text-ink-3">Cargando…</div>
        ) : (
          <>
            <header className="border-b border-line px-6 pt-5 pb-4">
              <div className="mb-2 flex items-center gap-2 text-[12px]">
                <span className="font-mono text-ink-3">#{d.id}</span>
                <span
                  className="flex items-center gap-1.5 rounded-full border px-2 py-0.5 font-medium"
                  style={{ color: status?.color, borderColor: `color-mix(in srgb, ${status?.color} 35%, transparent)` }}
                >
                  <span className="size-1.5 rounded-full" style={{ background: status?.color }} />
                  {status?.label}
                </span>
                <span className="rounded-md bg-white/5 px-1.5 py-0.5 text-ink-2">{d.project}</span>
                {d.status === "inbox" && (
                  <button
                    onClick={() =>
                      act(() => api.queue([d.id], !d.queued)).then(() => api.detail(d.id).then(setD))
                    }
                    title="La próxima vez que escribas /pendientes, Claude trabajará solo las tareas en cola"
                    className={`flex items-center gap-1 rounded-full border px-2 py-0.5 font-medium transition ${
                      d.queued
                        ? "border-accent/60 bg-accent text-white shadow-[0_0_14px_var(--color-accent-glow)]"
                        : "border-line-strong text-ink-2 hover:border-accent/50 hover:text-ink"
                    }`}
                  >
                    {d.queued ? <Check size={12} strokeWidth={3} /> : <Plus size={12} strokeWidth={2.5} />}
                    {d.queued ? "En cola para Claude" : "Poner en cola para Claude"}
                  </button>
                )}
                <button
                  onClick={() => props.onEdit(d)}
                  title={d.status === "in_progress" ? "Editar prioridad" : "Editar título, nota, prioridad o proyecto"}
                  className="ml-auto rounded-lg p-1.5 text-ink-3 hover:bg-white/5 hover:text-ink"
                >
                  <Pencil size={15} />
                </button>
                <button onClick={props.onClose} className="rounded-lg p-1 text-ink-3 hover:bg-white/5 hover:text-ink">
                  <X size={18} />
                </button>
              </div>
              <h2 className="text-[18px] leading-snug font-semibold tracking-tight select-text">{d.title}</h2>
              <p className="mt-1 text-[12px] text-ink-3">Capturada {timeAgo(d.created_at)}</p>
            </header>

            {d.status === "in_progress" &&
              (d.worker ? (
                <div className="flex items-start gap-3 border-b border-accent/20 bg-accent-soft/60 px-6 py-3.5">
                  <Lock size={15} className="mt-0.5 shrink-0 text-accent" />
                  <div className="min-w-0 flex-1 text-[12.5px]">
                    <WorkingLabel text={`${actorName(d.worker.actor)} está trabajando en esto`} />
                    <div className="mt-0.5 text-ink-2">
                      La tomó {timeAgo(d.worker.since)} · última actividad {timeAgo(d.worker.last_activity)}. Está
                      bloqueada hasta que la entregue; si la sesión se cortó, puedes liberarla.
                    </div>
                    {d.worker.last_message && (
                      <div className="mt-1.5 line-clamp-2 text-ink">«{d.worker.last_message}»</div>
                    )}
                  </div>
                </div>
              ) : (
                <div className="flex items-start gap-3 border-b border-line bg-white/[0.02] px-6 py-3.5 text-[12.5px]">
                  <CircleDashed size={15} className="mt-0.5 shrink-0 text-ink-2" />
                  <div className="text-ink-2">
                    <b className="font-medium text-ink">Nadie está trabajando en esta tarea.</b> Está en «Claude
                    trabajando» porque se movió a mano, pero ningún Claude la ha tomado. Devuélvela a Capturado y
                    escribe <span className="kbd">/pendientes</span> en la sesión del proyecto.
                  </div>
                </div>
              ))}

            <div className="flex-1 space-y-6 overflow-y-auto px-6 py-5">
              {d.attachments.length > 0 && (
                <div className={`grid gap-2 ${d.attachments.length > 1 ? "grid-cols-2" : ""}`}>
                  {d.attachments.map((a) => (
                    <button
                      key={a.id}
                      onClick={() => setZoom(assetUrl(a.path))}
                      className="overflow-hidden rounded-xl border border-line bg-black/40 transition hover:border-line-strong"
                    >
                      <img src={assetUrl(a.path)} alt="" className="max-h-72 w-full object-contain" />
                    </button>
                  ))}
                </div>
              )}

              {d.note && d.note !== d.title && (
                <Section title="Nota">
                  <p className="whitespace-pre-wrap text-[13.5px] leading-relaxed text-ink-2 select-text">{d.note}</p>
                </Section>
              )}

              {d.resolution && (
                <Section title="Lo que hizo Claude" icon={<Sparkles size={13} className="text-accent" />}>
                  <div className="rounded-xl border border-accent/20 bg-accent-soft/50 p-3.5">
                    <p className="text-[13px] leading-relaxed text-ink select-text">{d.resolution}</p>
                    {d.files_changed.length > 0 && (
                      <div className="mt-3 flex flex-wrap gap-1.5">
                        {d.files_changed.map((f) => (
                          <span key={f} className="flex items-center gap-1 rounded-md bg-black/30 px-2 py-0.5 font-mono text-[11px] text-ink-2">
                            <FileCode2 size={11} /> {f}
                          </span>
                        ))}
                      </div>
                    )}
                  </div>
                </Section>
              )}

              {(d.linked.length > 0 || d.related.length > 0) && (
                <Section title="Memoria de Engram" icon={<Brain size={13} className="text-violet-300" />}>
                  <div className="space-y-2">
                    {d.linked.map((l) => (
                      <MemoryRow key={l.reference + l.relation} memory={l.memory} fallback={l.reference} badge={l.relation === "created" ? "nueva" : "consultada"} />
                    ))}
                    {d.related.length > 0 && (
                      <>
                        {d.linked.length > 0 && <div className="eyebrow pt-2">Posiblemente relacionadas</div>}
                        {d.related.map((m) => (
                          <MemoryRow key={m.id} memory={m} faded />
                        ))}
                      </>
                    )}
                  </div>
                </Section>
              )}

              <Section title="Historial">
                <ol className="relative space-y-3 border-l border-line pl-4">
                  {d.events.map((e) => (
                    <li key={e.id} className="relative">
                      <span
                        className={`absolute top-1.5 -left-[21px] size-2.5 rounded-full border-2 border-surface ${
                          e.actor === "claude" ? "bg-accent" : "bg-ink-3"
                        }`}
                      />
                      <div className="text-[12.5px]">
                        <b className={`font-medium ${e.actor === "claude" ? "text-accent" : "text-ink"}`}>
                          {actorName(e.actor)}
                        </b>{" "}
                        <span className="text-ink-2">{EVENT_LABEL[e.kind] ?? e.kind}</span>
                        <span className="ml-2 text-[11px] text-ink-3">{timeAgo(e.created_at)}</span>
                      </div>
                      {e.message && e.kind !== "completed" && (
                        <p className="mt-0.5 text-[12.5px] text-ink-2 select-text">{e.message}</p>
                      )}
                    </li>
                  ))}
                </ol>
              </Section>
            </div>

            <footer className="border-t border-line px-6 py-4">
              <AnimatePresence mode="wait" initial={false}>
                {returning ? (
                  <motion.div key="ret" initial={{ opacity: 0, y: 6 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0 }}>
                    <textarea
                      ref={noteRef}
                      value={note}
                      onChange={(e) => setNote(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter" && !e.shiftKey && note.trim()) {
                          e.preventDefault();
                          act(() => api.reopen(d.id, note), `#${d.id} devuelta a Capturado`).then(props.onClose);
                        }
                      }}
                      rows={3}
                      placeholder="¿Qué quedó mal o qué falta? Claude lo leerá primero la próxima vez."
                      className="w-full resize-none rounded-xl border border-line-strong bg-base px-3 py-2.5 text-[13px] outline-none placeholder:text-ink-3 focus:border-accent/50"
                    />
                    <div className="mt-2 flex justify-end gap-2">
                      <Btn onClick={() => setReturning(false)}>Cancelar</Btn>
                      <Btn
                        tone="accent"
                        disabled={!note.trim()}
                        onClick={() => act(() => api.reopen(d.id, note), `#${d.id} devuelta a Capturado`).then(props.onClose)}
                      >
                        <RotateCcw size={14} /> Devolver
                      </Btn>
                    </div>
                  </motion.div>
                ) : (
                  <motion.div key="act" className="flex items-center gap-2" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }}>
                    {d.status === "review" && (
                      <>
                        <Btn tone="ok" onClick={() => act(() => api.approve(d.id), `#${d.id} aprobada`).then(props.onClose)}>
                          <Check size={14} strokeWidth={3} /> Aprobar
                        </Btn>
                        <Btn onClick={() => setReturning(true)}>
                          <RotateCcw size={14} /> Devolver con nota
                        </Btn>
                      </>
                    )}
                    {d.status === "done" && (
                      <Btn onClick={() => setReturning(true)}>
                        <RotateCcw size={14} /> Reabrir
                      </Btn>
                    )}
                    {(d.status === "inbox" || d.status === "in_progress") && (
                      <input
                        value={note}
                        onChange={(e) => setNote(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === "Enter" && note.trim()) {
                            const msg = note;
                            setNote("");
                            act(() => api.comment(d.id, msg)).then(() => api.detail(d.id).then(setD));
                          }
                        }}
                        placeholder={d.worker ? "Responder a Claude… (Enter)" : "Agregar un comentario… (Enter)"}
                        className="min-w-0 flex-1 rounded-xl border border-line-strong bg-base px-3 py-2 text-[13px] outline-none placeholder:text-ink-3 focus:border-accent/50"
                      />
                    )}
                    {d.status === "in_progress" && !d.worker && (
                      <Btn tone="accent" onClick={() => act(() => api.release(d.id, ""), `#${d.id} devuelta a Capturado`)}>
                        <RotateCcw size={14} /> A Capturado
                      </Btn>
                    )}
                    {d.status === "in_progress" && d.worker && (
                      <Menu
                        label={
                          <>
                            <Unlock size={14} /> Liberar
                          </>
                        }
                        items={[
                          {
                            id: "release",
                            label: "Liberar y devolver a Capturado",
                            hint: "Solo si la sesión de Claude se cortó; si sigue activa, podría seguir editando",
                            icon: <Unlock size={13} />,
                            danger: true,
                          },
                        ]}
                        onSelect={() =>
                          act(() => api.release(d.id, "liberada desde el tablero"), `#${d.id} liberada`)
                        }
                      />
                    )}
                    {MOVE_TARGETS[d.status].length > 0 && (
                      <Menu
                        label="Mover a…"
                        items={MOVE_TARGETS[d.status]}
                        onSelect={(to) => act(() => api.move(d.id, to as Status), `#${d.id} → ${STATUSES.find((s) => s.id === to)?.label}`)}
                      />
                    )}
                    {(d.status === "review" || d.status === "done") && <div className="flex-1" />}
                    {confirmDelete ? (
                      <Btn tone="danger" onClick={() => act(() => api.remove(d.id), `#${d.id} borrada`).then(props.onClose)}>
                        <Trash2 size={14} /> Confirmar borrado
                      </Btn>
                    ) : (
                      <button
                        onClick={() => setConfirmDelete(true)}
                        className="rounded-lg p-2 text-ink-3 transition hover:bg-red-400/10 hover:text-red-300"
                        title="Borrar tarea"
                      >
                        <Trash2 size={16} />
                      </button>
                    )}
                  </motion.div>
                )}
              </AnimatePresence>
            </footer>
          </>
        )}
      </motion.aside>

      <AnimatePresence>
        {zoom && (
          <motion.div
            className="fixed inset-0 z-50 grid place-items-center bg-black/85 p-10 backdrop-blur"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            onClick={() => setZoom(null)}
          >
            <motion.img
              src={zoom}
              initial={{ scale: 0.94 }}
              animate={{ scale: 1 }}
              className="max-h-full max-w-full rounded-xl border border-line-strong shadow-2xl"
            />
          </motion.div>
        )}
      </AnimatePresence>
    </>
  );
}

function Section({ title, icon, children }: { title: string; icon?: React.ReactNode; children: React.ReactNode }) {
  return (
    <section>
      <h3 className="eyebrow mb-2.5 flex items-center gap-1.5">
        {icon}
        {title}
      </h3>
      {children}
    </section>
  );
}

function MemoryRow({ memory, fallback, badge, faded }: { memory: Memory | null; fallback?: string; badge?: string; faded?: boolean }) {
  return (
    <div className={`rounded-xl border border-line bg-base/60 px-3 py-2.5 ${faded ? "opacity-75" : ""}`}>
      <div className="flex items-center gap-2 text-[11px]">
        {memory && <span className="font-mono text-ink-3">#{memory.id}</span>}
        {memory && <span className="rounded bg-violet-400/10 px-1.5 text-violet-300">{memory.kind}</span>}
        {badge && (
          <span className={`ml-auto rounded px-1.5 ${badge === "nueva" ? "bg-emerald-400/10 text-emerald-300" : "bg-white/5 text-ink-2"}`}>
            {badge}
          </span>
        )}
      </div>
      <div className="mt-1 text-[13px] font-medium">{memory?.title ?? fallback}</div>
      {memory && <p className="mt-0.5 line-clamp-2 text-[12px] text-ink-2">{memory.snippet}</p>}
    </div>
  );
}

function Btn(props: {
  onClick: () => void;
  children: React.ReactNode;
  tone?: "ok" | "accent" | "danger";
  disabled?: boolean;
}) {
  const tone = {
    ok: "bg-emerald-500 text-white hover:bg-emerald-400",
    accent: "bg-accent text-white hover:brightness-110",
    danger: "bg-red-500/90 text-white hover:bg-red-500",
    none: "border border-line-strong bg-white/[0.03] text-ink hover:bg-white/[0.07]",
  }[props.tone ?? "none"];
  return (
    <button
      onClick={props.onClick}
      disabled={props.disabled}
      className={`flex items-center gap-1.5 rounded-xl px-3.5 py-2 text-[13px] font-medium transition disabled:opacity-40 ${tone}`}
    >
      {props.children}
    </button>
  );
}
