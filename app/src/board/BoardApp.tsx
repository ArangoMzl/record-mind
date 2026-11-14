import {
  DndContext,
  DragOverlay,
  PointerSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
  type DragStartEvent,
} from "@dnd-kit/core";
import { Plus, Search } from "lucide-react";
import { AnimatePresence, LayoutGroup } from "motion/react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  api,
  onTasksChanged,
  shortcutLabel,
  STATUSES,
  type Board,
  type Status,
  type Task,
  type TaskCard,
  type TaskEdit,
} from "../lib/api";
import { Column } from "./Column";
import { CardView } from "./TaskCardView";
import { DetailDrawer } from "./DetailDrawer";
import { EditDialog } from "./EditDialog";
import { Sidebar } from "./Sidebar";
import { TaskContextMenu, type TaskAction } from "./TaskMenu";
import { Toasts, type Toast } from "./Toasts";

export function BoardApp() {
  const [board, setBoard] = useState<Board | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [project, setProject] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [openId, setOpenId] = useState<number | null>(null);
  const [dragging, setDragging] = useState<TaskCard | null>(null);
  const [toasts, setToasts] = useState<Toast[]>([]);
  const [menu, setMenu] = useState<{ task: TaskCard; x: number; y: number } | null>(null);
  const [editing, setEditing] = useState<Task | null>(null);
  const prev = useRef<Map<number, TaskCard> | null>(null);
  /** Tareas creadas desde aquí (duplicadas) que no deben avisar como «capturada». */
  const quiet = useRef(new Set<number>());
  const searchRef = useRef<HTMLInputElement>(null);

  const pushToast = useCallback((t: Omit<Toast, "id">) => {
    const id = Math.random();
    setToasts((xs) => [...xs.slice(-3), { ...t, id }]);
    setTimeout(() => setToasts((xs) => xs.filter((x) => x.id !== id)), 4200);
  }, []);

  const refresh = useCallback(async () => {
    try {
      const b = await api.board();
      // Avisos de lo que cambió fuera de la app (Claude por MCP, la CLI…).
      if (prev.current) {
        for (const t of b.tasks) {
          const before = prev.current.get(t.id);
          if (!before) {
            if (!quiet.current.delete(t.id)) pushToast({ tone: "inbox", title: `#${t.id} capturada`, body: t.title });
          } else if (before.status !== t.status) {
            if (t.status === "in_progress")
              pushToast({ tone: "progress", title: `Claude tomó #${t.id}`, body: t.title });
            else if (t.status === "review")
              pushToast({ tone: "review", title: `#${t.id} lista para revisar`, body: t.resolution ?? t.title });
          } else if (t.blocked && t.blocked !== before.blocked) {
            pushToast({ tone: "review", title: `Claude pregunta en #${t.id}`, body: t.blocked });
          }
        }
      }
      prev.current = new Map(b.tasks.map((t) => [t.id, t]));
      setBoard(b);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, [pushToast]);

  useEffect(() => {
    refresh();
    const un = onTasksChanged(refresh);
    // Las fechas relativas («hace 3 min») se refrescan solas.
    const tick = setInterval(() => setBoard((b) => (b ? { ...b } : b)), 30_000);
    return () => {
      un.then((f) => f());
      clearInterval(tick);
    };
  }, [refresh]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        searchRef.current?.focus();
      } else if (e.key === "Escape") {
        setOpenId(null);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const visible = useMemo(() => {
    if (!board) return [];
    const q = query.trim().toLowerCase();
    return board.tasks.filter(
      (t) =>
        (!project || t.project === project) &&
        (!q || t.title.toLowerCase().includes(q) || t.note.toLowerCase().includes(q) || `#${t.id}` === q),
    );
  }, [board, project, query]);

  const byStatus = useMemo(() => {
    const m: Record<Status, TaskCard[]> = { inbox: [], in_progress: [], review: [], done: [] };
    for (const t of visible) m[t.status].push(t);
    m.done.sort((a, b) => b.updated_at.localeCompare(a.updated_at));
    return m;
  }, [visible]);

  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 6 } }));

  const onDragStart = (e: DragStartEvent) => {
    setDragging(board?.tasks.find((t) => t.id === e.active.id) ?? null);
  };

  /** Mover a mano (arrastrar o menú). Optimista: se mueve ya y se corrige si falla. */
  const moveTask = async (task: TaskCard, to: Status) => {
    if (task.status === to || to === "in_progress" || task.status === "in_progress") return;
    setBoard((b) => b && { ...b, tasks: b.tasks.map((t) => (t.id === task.id ? { ...t, status: to } : t)) });
    if (prev.current) prev.current.set(task.id, { ...task, status: to });
    try {
      if (to === "done" && task.status === "review") await api.approve(task.id);
      else await api.move(task.id, to);
    } catch (err) {
      pushToast({ tone: "error", title: "No se pudo mover", body: String(err) });
      refresh();
    }
  };

  const onDragEnd = (e: DragEndEvent) => {
    setDragging(null);
    const to = e.over?.id as Status | undefined;
    const task = board?.tasks.find((t) => t.id === e.active.id);
    if (to && task) moveTask(task, to);
  };

  const saveEdit = async (id: number, edit: TaskEdit) => {
    await api.update(id, edit);
    pushToast({ tone: "inbox", title: `#${id} actualizada` });
  };

  const copy = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      pushToast({ tone: "inbox", title: "Copiado al portapapeles", body: text.split("\n")[0] });
    } catch (err) {
      pushToast({ tone: "error", title: "No se pudo copiar", body: String(err) });
    }
  };

  /** Acciones del menú contextual de una tarjeta. */
  const onMenuAction = async (task: TaskCard, a: TaskAction) => {
    const fail = (title: string) => (err: unknown) => {
      pushToast({ tone: "error", title, body: String(err) });
      refresh();
    };
    switch (a.kind) {
      case "open":
        return setOpenId(task.id);
      case "edit":
        return setEditing(task);
      case "priority":
        setBoard((b) => b && { ...b, tasks: b.tasks.map((t) => (t.id === task.id ? { ...t, priority: a.value } : t)) });
        return api.update(task.id, { priority: a.value }).catch(fail("No se pudo cambiar la prioridad"));
      case "queue":
        return setQueued([task.id], !task.queued);
      case "move":
        return moveTask(task, a.to);
      case "approve":
        return quickApprove(task.id);
      case "duplicate":
        try {
          const dup = await api.duplicate(task.id);
          quiet.current.add(dup.id);
          pushToast({ tone: "inbox", title: `#${dup.id} creada`, body: `Copia de #${task.id}` });
        } catch (err) {
          fail("No se pudo duplicar")(err);
        }
        return;
      case "copy": {
        const note = task.note && task.note !== task.title ? `\n\n${task.note}` : "";
        return copy(a.what === "id" ? `#${task.id}` : `#${task.id} ${task.title}${note}`);
      }
      case "delete":
        setBoard((b) => b && { ...b, tasks: b.tasks.filter((t) => t.id !== task.id) });
        prev.current?.delete(task.id);
        if (openId === task.id) setOpenId(null);
        try {
          await api.remove(task.id);
          pushToast({ tone: "inbox", title: `#${task.id} borrada`, body: task.title });
        } catch (err) {
          fail("No se pudo borrar")(err);
        }
        return;
    }
  };

  /** Marca o desmarca tareas para /pendientes (optimista; el vigilante confirma). */
  const setQueued = async (ids: number[], queued: boolean) => {
    if (ids.length === 0) return;
    setBoard((b) => b && { ...b, tasks: b.tasks.map((t) => (ids.includes(t.id) ? { ...t, queued } : t)) });
    try {
      await api.queue(ids, queued);
    } catch (err) {
      pushToast({ tone: "error", title: "No se pudo actualizar la cola", body: String(err) });
      refresh();
    }
  };

  const quickApprove = async (id: number) => {
    try {
      await api.approve(id);
    } catch (err) {
      pushToast({ tone: "error", title: "No se pudo aprobar", body: String(err) });
    }
  };

  // La versión al día de la tarea del menú (si se borró por fuera, el menú se cierra solo).
  const menuTask = menu ? board?.tasks.find((t) => t.id === menu.task.id) : undefined;
  const shortcut = shortcutLabel(board?.shortcut ?? "CommandOrControl+Shift+Space");
  const reviewCount = byStatus.review.length;

  return (
    <div className="relative z-10 grid h-full grid-cols-[248px_minmax(0,1fr)]">
      <Sidebar board={board} project={project} onProject={setProject} />

      <div className="flex min-h-0 flex-col">
        <header className="flex items-center gap-3 border-b border-line px-6 py-3.5">
          <div className="min-w-0">
            <div className="eyebrow">{project ? "Proyecto" : "Todos los proyectos"}</div>
            <h1 className="truncate text-[19px] font-semibold tracking-tight">
              {project ?? "Tablero"}
              {reviewCount > 0 && (
                <span className="ml-3 align-middle text-[12px] font-medium text-st-review">
                  {reviewCount} para revisar
                </span>
              )}
            </h1>
          </div>
          <div className="flex-1" />
          <label className="flex w-72 items-center gap-2 rounded-xl border border-line bg-surface/80 px-3 py-2 text-ink-2 focus-within:border-accent/50">
            <Search size={15} />
            <input
              ref={searchRef}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Buscar pendientes"
              className="min-w-0 flex-1 bg-transparent text-ink outline-none placeholder:text-ink-3"
            />
            <span className="kbd">Ctrl K</span>
          </label>
          <button
            onClick={() => api.openCapture()}
            className="flex items-center gap-2 rounded-xl bg-accent px-3.5 py-2 font-medium text-white shadow-[0_0_24px_var(--color-accent-glow)] transition hover:brightness-110"
          >
            <Plus size={16} strokeWidth={2.5} />
            Capturar
            <span className="flex gap-0.5 opacity-80">
              {shortcut.map((k) => (
                <span key={k} className="rounded bg-white/15 px-1 font-mono text-[10px]">
                  {k}
                </span>
              ))}
            </span>
          </button>
        </header>

        {error && (
          <div className="mx-6 mt-4 rounded-xl border border-red-400/30 bg-red-400/10 px-4 py-2 text-[13px] text-red-300">
            {error}
          </div>
        )}

        <DndContext sensors={sensors} onDragStart={onDragStart} onDragEnd={onDragEnd} onDragCancel={() => setDragging(null)}>
          <LayoutGroup>
            <main className="grid min-h-0 flex-1 grid-cols-4 gap-4 overflow-x-auto p-5">
              {STATUSES.map((s) => (
                <Column
                  key={s.id}
                  status={s}
                  tasks={byStatus[s.id]}
                  showProject={!project}
                  draggingId={dragging?.id ?? null}
                  locked={s.id === "in_progress"}
                  onOpen={setOpenId}
                  onApprove={quickApprove}
                  onToggleQueue={(t) => setQueued([t.id], !t.queued)}
                  onQueue={setQueued}
                  onContextMenu={(task, x, y) => setMenu({ task, x, y })}
                  menuId={menu?.task.id ?? null}
                  empty={board !== null && board.tasks.length === 0}
                  shortcut={shortcut}
                />
              ))}
            </main>
          </LayoutGroup>
          <DragOverlay dropAnimation={{ duration: 180, easing: "cubic-bezier(.2,.8,.2,1)" }}>
            {dragging && <CardView task={dragging} showProject={!project} overlay />}
          </DragOverlay>
        </DndContext>
      </div>

      <AnimatePresence>
        {openId !== null && (
          <DetailDrawer
            key={openId}
            id={openId}
            version={board?.tasks.find((t) => t.id === openId)?.updated_at}
            onClose={() => setOpenId(null)}
            onToast={pushToast}
            onEdit={setEditing}
          />
        )}
      </AnimatePresence>

      {menuTask && menu && (
        <TaskContextMenu
          key={`${menuTask.id}-${menu.x}-${menu.y}`}
          task={menuTask}
          x={menu.x}
          y={menu.y}
          onClose={() => setMenu(null)}
          onAction={(a) => onMenuAction(menuTask, a)}
        />
      )}

      <AnimatePresence>
        {editing && (
          <EditDialog
            key={editing.id}
            task={editing}
            projects={board?.projects.map((p) => p.name) ?? []}
            onClose={() => setEditing(null)}
            onSave={(edit) => saveEdit(editing.id, edit)}
          />
        )}
      </AnimatePresence>

      <Toasts toasts={toasts} />
    </div>
  );
}
