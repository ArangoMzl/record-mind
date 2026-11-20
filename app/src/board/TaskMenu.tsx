import {
  ArrowRightLeft,
  Check,
  ChevronRight,
  Copy,
  CopyPlus,
  Flame,
  ListPlus,
  ListX,
  PanelRight,
  Pencil,
  Trash2,
} from "lucide-react";
import { motion } from "motion/react";
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { canMove, isMovable, PRIORITIES, STATUSES, type Status, type TaskCard } from "../lib/api";

export type TaskAction =
  | { kind: "open" }
  | { kind: "edit" }
  | { kind: "priority"; value: number }
  | { kind: "queue" }
  | { kind: "move"; to: Status }
  | { kind: "approve" }
  | { kind: "duplicate" }
  | { kind: "copy"; what: "text" | "id" }
  | { kind: "delete" };

interface Item {
  id: string;
  label: string;
  icon?: React.ReactNode;
  /** Tecla que la dispara con el menú abierto. */
  key?: string;
  danger?: boolean;
  /** Motivo por el que no está disponible. */
  disabled?: string;
  checked?: boolean;
  action?: TaskAction;
  children?: Item[];
}
type Entry = Item | "sep";

const WIDTH = 236;
const ROW = 32;

function Flames({ n }: { n: number }) {
  if (n === 0) return <span className="size-[13px]" />;
  return (
    <span className={`flex ${n > 1 ? "text-red-400" : "text-amber-400"}`}>
      <Flame size={13} />
      {n > 1 && <Flame size={13} className="-ml-1.5" />}
    </span>
  );
}

function buildEntries(t: TaskCard): Entry[] {
  const inProgress = t.status === "in_progress";
  const entries: (Entry | false)[] = [
    { id: "open", label: "Abrir detalle", icon: <PanelRight size={14} />, key: "Enter", action: { kind: "open" } },
    {
      id: "edit",
      label: inProgress ? "Editar prioridad…" : "Editar…",
      icon: <Pencil size={14} />,
      key: "E",
      action: { kind: "edit" },
    },
    {
      id: "priority",
      label: "Prioridad",
      icon: <Flame size={14} />,
      children: PRIORITIES.map((p) => ({
        id: `p${p.value}`,
        label: p.label,
        icon: <Flames n={p.value} />,
        key: String(p.value + 1),
        checked: t.priority === p.value,
        action: { kind: "priority", value: p.value },
      })),
    },
    t.status === "inbox" && {
      id: "queue",
      label: t.queued ? "Quitar de la cola" : "Poner en cola para Claude",
      icon: t.queued ? <ListX size={14} /> : <ListPlus size={14} />,
      key: "C",
      action: { kind: "queue" },
    },
    t.status === "review" && {
      id: "approve",
      label: "Aprobar",
      icon: <Check size={14} strokeWidth={3} className="text-emerald-300" />,
      key: "A",
      action: { kind: "approve" },
    },
    isMovable(t.status) && {
      id: "move",
      label: "Mover a",
      icon: <ArrowRightLeft size={14} />,
      children: STATUSES.filter((s) => canMove(t.status, s.id)).map((s) => ({
        id: `m-${s.id}`,
        label: s.id === "done" && t.status === "review" ? "Hecho (aprobar)" : s.label,
        icon: <span className="m-[3px] size-2 rounded-full" style={{ background: s.color, boxShadow: `0 0 8px ${s.color}` }} />,
        action: { kind: "move", to: s.id },
      })),
    },
    "sep",
    { id: "dup", label: "Duplicar", icon: <CopyPlus size={14} />, key: "D", action: { kind: "duplicate" } },
    {
      id: "copy",
      label: "Copiar",
      icon: <Copy size={14} />,
      children: [
        { id: "c-text", label: "Título y nota", action: { kind: "copy", what: "text" } },
        { id: "c-id", label: `Número (#${t.id})`, action: { kind: "copy", what: "id" } },
      ],
    },
    "sep",
    { id: "delete", label: "Eliminar", icon: <Trash2 size={14} />, key: "Supr", danger: true, action: { kind: "delete" } },
  ];
  return entries.filter((e): e is Entry => e !== false);
}

/** Menú contextual (clic derecho) de una tarjeta del tablero. */
export function TaskContextMenu(props: {
  task: TaskCard;
  x: number;
  y: number;
  onClose: () => void;
  onAction: (a: TaskAction) => void;
}) {
  const { task, onClose } = props;
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ left: props.x, top: props.y });
  const [sub, setSub] = useState<{ id: string; up: boolean } | null>(null);
  const [confirming, setConfirming] = useState(false);
  const entries = useMemo(() => buildEntries(task), [task]);

  // Que no se salga de la ventana.
  useLayoutEffect(() => {
    const r = ref.current?.getBoundingClientRect();
    if (!r) return;
    setPos({
      left: Math.max(8, Math.min(props.x, window.innerWidth - r.width - 8)),
      top: Math.max(8, Math.min(props.y, window.innerHeight - r.height - 8)),
    });
  }, [props.x, props.y, entries.length]);

  const run = (it: Item) => {
    if (it.disabled || !it.action) return;
    if (it.action.kind === "delete" && !confirming) {
      setConfirming(true);
      return;
    }
    onClose();
    props.onAction(it.action);
  };

  // Atajos de teclado: todas las acciones con `key`, incluidas las de los submenús.
  const byKey = useMemo(() => {
    const m = new Map<string, Item>();
    const walk = (xs: Entry[]) =>
      xs.forEach((e) => {
        if (e === "sep") return;
        if (e.key && !e.disabled) m.set(e.key.toLowerCase(), e);
        if (e.children) walk(e.children);
      });
    walk(entries);
    return m;
  }, [entries]);

  useEffect(() => {
    const outside = (e: MouseEvent) => !ref.current?.contains(e.target as Node) && onClose();
    const onKey = (e: KeyboardEvent) => {
      const k = e.key === "Delete" ? "supr" : e.key.toLowerCase();
      if (k === "escape" || byKey.has(k)) {
        e.preventDefault();
        e.stopPropagation();
      }
      if (k === "escape") onClose();
      else if (byKey.has(k) && !e.ctrlKey && !e.metaKey && !e.altKey) run(byKey.get(k)!);
    };
    const close = () => onClose();
    window.addEventListener("mousedown", outside, true);
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("blur", close);
    window.addEventListener("resize", close);
    window.addEventListener("wheel", close, { passive: true });
    return () => {
      window.removeEventListener("mousedown", outside, true);
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("blur", close);
      window.removeEventListener("resize", close);
      window.removeEventListener("wheel", close);
    };
  });

  const flipX = pos.left + WIDTH * 2 + 12 > window.innerWidth;

  return (
    <motion.div
      ref={ref}
      role="menu"
      onContextMenu={(e) => e.preventDefault()}
      initial={{ opacity: 0, scale: 0.97 }}
      animate={{ opacity: 1, scale: 1 }}
      transition={{ duration: 0.12 }}
      style={{ left: pos.left, top: pos.top, width: WIDTH }}
      className="fixed z-[60] origin-top-left rounded-xl border border-line-strong bg-elevated p-1 shadow-2xl shadow-black/60"
    >
      <div className="flex items-center gap-1.5 px-2.5 pt-1.5 pb-1 text-[11px] text-ink-3">
        <span className="font-mono">#{task.id}</span>
        <span className="truncate">{task.title}</span>
      </div>
      {entries.map((e, i) =>
        e === "sep" ? (
          <div key={`sep${i}`} className="mx-2 my-1 h-px bg-line" />
        ) : (
          <div
            key={e.id}
            className="relative"
            onMouseEnter={(ev) => {
              if (!e.children || e.disabled) return setSub(null);
              const r = ev.currentTarget.getBoundingClientRect();
              setSub({ id: e.id, up: r.top + e.children.length * ROW + 12 > window.innerHeight });
            }}
          >
            <Row
              item={e}
              confirming={e.id === "delete" && confirming}
              open={sub?.id === e.id}
              onClick={() => (e.children ? setSub({ id: e.id, up: sub?.up ?? false }) : run(e))}
            />
            {e.children && sub?.id === e.id && (
              <div
                role="menu"
                style={{ width: WIDTH - 40 }}
                className={`absolute z-10 rounded-xl border border-line-strong bg-elevated p-1 shadow-2xl shadow-black/60 ${
                  flipX ? "right-full mr-1" : "left-full ml-1"
                } ${sub.up ? "bottom-[-5px]" : "top-[-5px]"}`}
              >
                {e.children.map((c) => (
                  <Row key={c.id} item={c} onClick={() => run(c)} />
                ))}
              </div>
            )}
          </div>
        ),
      )}
    </motion.div>
  );
}

function Row(props: { item: Item; open?: boolean; confirming?: boolean; onClick: () => void }) {
  const { item: it, confirming } = props;
  return (
    <button
      role="menuitem"
      onClick={props.onClick}
      disabled={!!it.disabled}
      title={it.disabled}
      className={`flex h-8 w-full items-center gap-2.5 rounded-lg px-2.5 text-left text-[13px] transition disabled:cursor-not-allowed disabled:opacity-40 ${
        confirming
          ? "bg-red-500/90 font-medium text-white"
          : it.danger
            ? "text-red-300 enabled:hover:bg-red-400/10"
            : props.open
              ? "bg-white/[0.06]"
              : "enabled:hover:bg-white/[0.06]"
      }`}
    >
      <span className={`flex w-4 shrink-0 justify-center ${confirming || it.danger ? "" : "text-ink-2"}`}>{it.icon}</span>
      <span className="min-w-0 flex-1 truncate">{confirming ? "Clic otra vez para borrar" : it.label}</span>
      {it.checked && <Check size={13} strokeWidth={3} className="shrink-0 text-accent" />}
      {it.key && !confirming && <span className="shrink-0 font-mono text-[10.5px] text-ink-3">{it.key}</span>}
      {it.children && <ChevronRight size={13} className="shrink-0 text-ink-3" />}
    </button>
  );
}
