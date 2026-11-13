import { useDraggable, useDroppable } from "@dnd-kit/core";
import { ListChecks, X } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import type { STATUSES, TaskCard } from "../lib/api";
import { CardView } from "./TaskCardView";

type StatusInfo = (typeof STATUSES)[number];

export function Column(props: {
  status: StatusInfo;
  tasks: TaskCard[];
  showProject: boolean;
  draggingId: number | null;
  /** Columna que no acepta tarjetas soltadas a mano. */
  locked?: boolean;
  onOpen: (id: number) => void;
  onApprove: (id: number) => void;
  onToggleQueue: (task: TaskCard) => void;
  onQueue: (ids: number[], queued: boolean) => void;
  empty: boolean;
  shortcut: string[];
}) {
  const { status, tasks } = props;
  const { setNodeRef, isOver } = useDroppable({ id: status.id, disabled: props.locked });
  const working = status.id === "in_progress" && tasks.some((t) => t.worker);
  const showLock = props.locked && props.draggingId !== null;
  const isInbox = status.id === "inbox";
  const queued = isInbox ? tasks.filter((t) => t.queued) : [];

  return (
    <section
      ref={setNodeRef}
      className={`relative flex min-h-0 min-w-[250px] flex-col rounded-2xl border transition-colors ${
        isOver ? "border-accent/50 bg-accent-soft" : "border-line bg-surface/50"
      }`}
    >
      {showLock && (
        <div className="pointer-events-none absolute inset-0 z-10 grid place-items-center rounded-2xl bg-base/80 p-6 text-center backdrop-blur-[2px]">
          <div>
            <div className="text-[13px] font-medium">Solo Claude mueve tareas aquí</div>
            <div className="mt-1 text-[12px] text-ink-3">
              Las toma cuando escribes <span className="kbd">/pendientes</span> en su sesión
            </div>
          </div>
        </div>
      )}
      <header className="flex items-center gap-2.5 px-4 pt-3.5 pb-3 whitespace-nowrap" title={status.hint}>
        <span
          className={`size-2 shrink-0 rounded-full ${working ? "animate-pulse-dot" : ""}`}
          style={{ background: status.color, boxShadow: `0 0 10px ${status.color}` }}
        />
        <h2 className="truncate text-[13px] font-semibold">{status.label}</h2>
        <span className="rounded-md bg-white/5 px-1.5 font-mono text-[11px] text-ink-2">{tasks.length}</span>
        {isInbox && tasks.length > 1 && queued.length < tasks.length && (
          <button
            onClick={() => props.onQueue(tasks.filter((t) => !t.queued).map((t) => t.id), true)}
            title="Marcar todas para la próxima pasada de /pendientes"
            className="ml-auto flex items-center gap-1 rounded-md px-1.5 py-0.5 text-[11px] text-ink-3 transition hover:bg-white/5 hover:text-ink"
          >
            <ListChecks size={13} /> Todas
          </button>
        )}
      </header>

      <AnimatePresence initial={false}>
        {queued.length > 0 && (
          <motion.div
            initial={{ opacity: 0, height: 0 }}
            animate={{ opacity: 1, height: "auto" }}
            exit={{ opacity: 0, height: 0 }}
            className="mx-3 mb-2.5 overflow-hidden"
          >
            <div className="flex items-center gap-2 rounded-xl border border-accent/30 bg-accent-soft px-3 py-2 text-[11.5px]">
              <span className="grid size-5 shrink-0 place-items-center rounded-full bg-accent font-mono text-[10.5px] font-bold text-white">
                {queued.length}
              </span>
              <span className="min-w-0 leading-snug text-ink-2">
                <span className="kbd border-accent/30 text-ink">/pendientes</span> trabajará{" "}
                {queued.length === 1 ? "solo esta" : `solo estas ${queued.length}`}
              </span>
              <button
                onClick={() => props.onQueue(queued.map((t) => t.id), false)}
                title="Vaciar la cola"
                className="ml-auto shrink-0 rounded-md p-1 text-ink-3 transition hover:bg-white/10 hover:text-ink"
              >
                <X size={13} />
              </button>
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      <div className="flex min-h-0 flex-1 flex-col gap-2.5 overflow-y-auto px-3 pb-3">
        <AnimatePresence mode="popLayout" initial={false}>
          {tasks.map((t) => (
            <DraggableCard
              key={t.id}
              task={t}
              showProject={props.showProject}
              hidden={props.draggingId === t.id}
              onOpen={props.onOpen}
              onApprove={props.onApprove}
              onToggleQueue={props.onToggleQueue}
            />
          ))}
        </AnimatePresence>
        {tasks.length === 0 && <EmptyHint status={status.id} firstRun={props.empty} shortcut={props.shortcut} />}
      </div>
    </section>
  );
}

function DraggableCard(props: {
  task: TaskCard;
  showProject: boolean;
  hidden: boolean;
  onOpen: (id: number) => void;
  onApprove: (id: number) => void;
  onToggleQueue: (task: TaskCard) => void;
}) {
  // Mientras un agente la trabaja no se puede arrastrar (se libera desde el detalle).
  const { attributes, listeners, setNodeRef } = useDraggable({
    id: props.task.id,
    disabled: props.task.status === "in_progress",
  });
  return (
    <motion.div
      ref={setNodeRef}
      layoutId={`card-${props.task.id}`}
      layout
      initial={{ opacity: 0, y: 8, scale: 0.98 }}
      animate={{ opacity: props.hidden ? 0.25 : 1, y: 0, scale: 1 }}
      exit={{ opacity: 0, scale: 0.96 }}
      transition={{ type: "spring", stiffness: 420, damping: 34 }}
      {...attributes}
      {...listeners}
      className="outline-none"
    >
      <CardView
        task={props.task}
        showProject={props.showProject}
        onOpen={() => props.onOpen(props.task.id)}
        onApprove={() => props.onApprove(props.task.id)}
        onToggleQueue={() => props.onToggleQueue(props.task)}
      />
    </motion.div>
  );
}

function EmptyHint({ status, firstRun, shortcut }: { status: string; firstRun: boolean; shortcut: string[] }) {
  if (status === "inbox" && firstRun) {
    return (
      <div className="mt-2 rounded-xl border border-dashed border-line-strong p-4 text-[12.5px] leading-relaxed text-ink-2">
        <p className="mb-2 font-medium text-ink">Captura tu primer pendiente</p>
        Pulsa{" "}
        {shortcut.map((k) => (
          <span key={k} className="kbd mr-1">
            {k}
          </span>
        ))}
        desde cualquier app, escribe qué hay que corregir y pega una captura con{" "}
        <span className="kbd">Ctrl V</span>. Luego dile a Claude Code <span className="kbd">/pendientes</span>.
      </div>
    );
  }
  const text: Record<string, string> = {
    inbox: "Nada pendiente.",
    in_progress: "Claude no está trabajando en nada ahora.",
    review: "Nada esperando tu revisión.",
    done: "Aún no hay tareas aprobadas.",
  };
  return <p className="px-2 py-6 text-center text-[12px] text-ink-3">{text[status]}</p>;
}
