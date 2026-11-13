import { Brain, Check, CircleDashed, Flame, Images, Lock, MessageCircleQuestion, Plus, RotateCcw } from "lucide-react";
import { actorName, assetUrl, timeAgo, type TaskCard } from "../lib/api";
import { WorkingLabel } from "./Menu";

export function CardView(props: {
  task: TaskCard;
  showProject: boolean;
  overlay?: boolean;
  onOpen?: () => void;
  onApprove?: () => void;
  onToggleQueue?: () => void;
}) {
  const t = props.task;
  const canQueue = t.status === "inbox" && !!props.onToggleQueue;
  const img = t.attachments[0];
  // Solo cuenta como «trabajando» si un agente la tomó de verdad (claim_task).
  const working = t.status === "in_progress" && t.worker !== null;
  const orphan = t.status === "in_progress" && t.worker === null;

  return (
    <article
      onClick={props.onOpen}
      className={`group relative cursor-pointer overflow-hidden rounded-xl border bg-elevated transition ${
        working
          ? "working border-transparent"
          : t.queued
            ? "border-accent/45 shadow-[0_0_0_1px_rgb(234_24_137/0.25),0_0_22px_rgb(234_24_137/0.12)]"
            : "border-line hover:border-line-strong"
      } ${props.overlay ? "rotate-[1.5deg] shadow-2xl shadow-black/60 ring-1 ring-accent/40" : "hover:bg-hover"}`}
    >
      {canQueue && (
        <button
          onClick={(e) => {
            e.stopPropagation();
            props.onToggleQueue?.();
          }}
          onPointerDown={(e) => e.stopPropagation()}
          title={t.queued ? "Quitar de la cola de Claude" : "Marcar para que Claude la trabaje con /pendientes"}
          className={`absolute top-2 right-2 z-10 flex items-center gap-1 rounded-full border px-2 py-0.5 text-[10.5px] font-medium backdrop-blur transition ${
            t.queued
              ? "border-accent/60 bg-accent text-white shadow-[0_0_14px_var(--color-accent-glow)]"
              : "border-white/15 bg-black/60 text-ink-2 opacity-0 group-hover:opacity-100 hover:border-accent/50 hover:text-ink"
          }`}
        >
          {t.queued ? <Check size={11} strokeWidth={3} /> : <Plus size={11} strokeWidth={2.5} />}
          {t.queued ? "En cola" : "Para Claude"}
        </button>
      )}
      {img && (
        <div className="relative aspect-[16/8] overflow-hidden border-b border-line bg-black/40">
          <img
            src={assetUrl(img.path)}
            alt=""
            draggable={false}
            className="size-full object-cover object-top opacity-90 transition group-hover:opacity-100"
          />
          {t.attachments.length > 1 && (
            <span className="absolute right-2 bottom-2 flex items-center gap-1 rounded-md bg-black/70 px-1.5 py-0.5 text-[10.5px] text-ink-2 backdrop-blur">
              <Images size={11} /> {t.attachments.length}
            </span>
          )}
        </div>
      )}

      <div className="p-3">
        <div className="mb-1.5 flex items-center gap-1.5 text-[11px] text-ink-3">
          <span className="font-mono">#{t.id}</span>
          {props.showProject && (
            <span className="truncate rounded-md bg-white/5 px-1.5 py-px text-ink-2">{t.project}</span>
          )}
          {t.priority > 0 && (
            <span className={`flex items-center ${t.priority > 1 ? "text-red-400" : "text-amber-400"}`}>
              <Flame size={12} />
              {t.priority > 1 && <Flame size={12} className="-ml-1" />}
            </span>
          )}
          {working && <Lock size={11} className="ml-auto text-accent/70" aria-label="Bloqueada mientras Claude trabaja" />}
          {/* Sin miniatura, el botón de cola cae sobre la fecha: la ocultamos mientras se ve el botón. */}
          <span
            className={`${working ? "" : "ml-auto"} shrink-0 ${
              canQueue && !img ? (t.queued ? "invisible" : "group-hover:invisible") : ""
            }`}
          >
            {timeAgo(t.updated_at)}
          </span>
        </div>

        <h3 className="line-clamp-2 text-[13.5px] leading-snug font-medium text-ink">{t.title}</h3>
        {!img && t.note && t.note !== t.title && (
          <p className="mt-1 line-clamp-2 text-[12px] text-ink-2">{t.note.replace(t.title, "").trim()}</p>
        )}

        {working && !t.blocked && (
          <div className="mt-2.5 text-[11.5px]">
            <div className="flex items-center justify-between">
              <WorkingLabel text={`${actorName(t.worker!.actor)} está en esto`} />
              <span className="text-[10.5px] text-ink-3">desde {timeAgo(t.worker!.since).replace("hace ", "")}</span>
            </div>
            {t.worker!.last_message && <p className="mt-1 line-clamp-2 text-ink-2">{t.worker!.last_message}</p>}
          </div>
        )}
        {orphan && (
          <div className="mt-2.5 flex items-center gap-1.5 rounded-lg border border-dashed border-line-strong px-2 py-1.5 text-[11.5px] text-ink-2">
            <CircleDashed size={13} className="shrink-0" /> Nadie la ha tomado · ábrela para liberarla
          </div>
        )}
        {t.blocked && (
          <div className="mt-2.5 flex gap-1.5 rounded-lg border border-amber-400/25 bg-amber-400/10 px-2 py-1.5 text-[11.5px] text-amber-200">
            <MessageCircleQuestion size={13} className="mt-0.5 shrink-0" />
            <span className="line-clamp-2">{t.blocked}</span>
          </div>
        )}
        {t.status === "review" && t.resolution && (
          <div className="mt-2 rounded-lg bg-white/[0.03] px-2 py-1.5 text-[11.5px] leading-relaxed text-ink-2">
            <p className="line-clamp-3">{t.resolution}</p>
          </div>
        )}

        {(t.memory_count > 0 || t.returns > 0 || t.status === "review") && (
          <div className="mt-2.5 flex items-center gap-2 text-[11px] text-ink-3">
            {t.memory_count > 0 && (
              <span
                className={`flex items-center gap-1 ${t.memories_created ? "text-violet-300" : ""}`}
                title={t.memories_created ? `${t.memories_created} memoria(s) nueva(s) en Engram` : "Memorias de Engram consultadas"}
              >
                <Brain size={12} /> {t.memory_count}
              </span>
            )}
            {t.returns > 0 && (
              <span className="flex items-center gap-1" title="Veces que se devolvió">
                <RotateCcw size={12} /> {t.returns}
              </span>
            )}
            {t.status === "review" && props.onApprove && (
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  props.onApprove?.();
                }}
                onPointerDown={(e) => e.stopPropagation()}
                className="ml-auto flex items-center gap-1 rounded-lg border border-emerald-400/30 bg-emerald-400/10 px-2 py-1 font-medium text-emerald-300 opacity-0 transition group-hover:opacity-100 hover:bg-emerald-400/20"
              >
                <Check size={12} strokeWidth={3} /> Aprobar
              </button>
            )}
          </div>
        )}
      </div>
    </article>
  );
}
