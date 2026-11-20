import {
  DndContext,
  DragOverlay,
  PointerSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
  type DragStartEvent,
} from "@dnd-kit/core";
import { LayoutGrid, MessageSquareText, Plug, Plus, Search, Sparkles, X } from "lucide-react";
import { AnimatePresence, LayoutGroup, motion } from "motion/react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  api,
  canMove,
  inPeriod,
  isMovable,
  onTasksChanged,
  PERIODS,
  shortcutLabel,
  STATUSES,
  type Board,
  type BridgeNotice,
  type Period,
  type Status,
  type Task,
  type TaskCard,
  type TaskEdit,
  type UpdateInfo,
} from "../lib/api";
import { UpdateDialog } from "./UpdateDialog";
import { BranchTag } from "./BranchTag";
import { Column } from "./Column";
import { CardView } from "./TaskCardView";
import { DetailDrawer } from "./DetailDrawer";
import { EditDialog } from "./EditDialog";
import { Sidebar } from "./Sidebar";
import { TaskContextMenu, type TaskAction } from "./TaskMenu";
import { SettingsDialog } from "./SettingsDialog";
import { RulesDialog } from "./RulesDialog";
import { DecisionsView, type DecisionFilter } from "./DecisionsView";
import { Toasts, type Toast } from "./Toasts";
import { WindowControls } from "./WindowControls";

export function BoardApp() {
  const [board, setBoard] = useState<Board | null>(null);
  // Para el intervalo, que no debe reiniciarse con cada cambio del tablero.
  const boardRef = useRef<Board | null>(null);
  boardRef.current = board;
  const [error, setError] = useState<string | null>(null);
  const [project, setProject] = useState<string | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [rulesOpen, setRulesOpen] = useState(false);
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [updateOpen, setUpdateOpen] = useState(false);
  // Reglas ya vistas, para avisar solo de las que Claude acaba de sugerir.
  const seenRules = useRef<Set<number> | null>(null);
  const [query, setQuery] = useState("");
  const [period, setPeriodState] = useState<Period>(() => {
    try {
      const saved = localStorage.getItem("rm.period");
      if (PERIODS.some((p) => p.id === saved)) return saved as Period;
    } catch {}
    return "all";
  });
  const setPeriod = (p: Period) => {
    setPeriodState(p);
    try {
      localStorage.setItem("rm.period", p);
    } catch {}
  };
  // Tablero o decisiones pendientes; se recuerda entre aperturas.
  const [view, setViewState] = useState<"board" | "decisions">(() => {
    try {
      if (localStorage.getItem("rm.view") === "decisions") return "decisions";
    } catch {}
    return "board";
  });
  const setView = (v: "board" | "decisions") => {
    setViewState(v);
    try {
      localStorage.setItem("rm.view", v);
    } catch {}
  };
  const [decisionFilter, setDecisionFilterState] = useState<DecisionFilter>(() => {
    try {
      if (localStorage.getItem("rm.decisions.filter") === "all") return "all";
    } catch {}
    return "open";
  });
  const setDecisionFilter = (f: DecisionFilter) => {
    setDecisionFilterState(f);
    try {
      localStorage.setItem("rm.decisions.filter", f);
    } catch {}
  };
  // Decisiones ya vistas, para avisar solo de las que Claude acaba de registrar.
  const seenDecisions = useRef<Set<string> | null>(null);
  const [openId, setOpenId] = useState<number | null>(null);
  const [dragging, setDragging] = useState<TaskCard | null>(null);
  const [toasts, setToasts] = useState<Toast[]>([]);
  const [menu, setMenu] = useState<{ task: TaskCard; x: number; y: number } | null>(null);
  const [editing, setEditing] = useState<Task | null>(null);
  const prev = useRef<Map<number, TaskCard> | null>(null);
  /** Tareas creadas desde aquí (duplicadas) que no deben avisar como «capturada». */
  const quiet = useRef(new Set<number>());
  const searchRef = useRef<HTMLInputElement>(null);
  const searchBoxRef = useRef<HTMLDivElement>(null);
  const [searchOpen, setSearchOpen] = useState(false);

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
      if (seenRules.current) {
        for (const r of b.rules) {
          if (!seenRules.current.has(r.id)) pushToast({ tone: "review", title: `Claude sugiere una regla para ${r.project}`, body: r.rule });
        }
      }
      seenRules.current = new Set(b.rules.map((r) => r.id));
      const openDecisions = b.decisions.flatMap((f) => f.decisions.filter((d) => !d.resolved).map((d) => ({ f, d })));
      if (seenDecisions.current) {
        const fresh = openDecisions.filter(({ d }) => !seenDecisions.current!.has(d.id));
        if (fresh.length > 0)
          pushToast({
            tone: "review",
            title: fresh.length === 1 ? `Nueva decisión pendiente en ${fresh[0].f.project}` : `${fresh.length} decisiones pendientes nuevas`,
            body: fresh.length === 1 ? fresh[0].d.question : fresh[0].f.title,
          });
      }
      seenDecisions.current = new Set(b.decisions.flatMap((f) => f.decisions.map((d) => d.id)));
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
    // Cada 30 s: sesiones activas al día (abrir o cerrar una conversación no avisa) y, de paso,
    // se vuelven a pintar los «hace X min».
    // Las ramas también: cambiar de rama en un proyecto no escribe en la base de tareas.
    const tick = setInterval(async () => {
      const sessions = await api.sessions().catch(() => null);
      const dirs = boardRef.current?.projects.flatMap((p) => (p.directory ? [p.directory] : [])) ?? [];
      const branches = dirs.length ? await api.gitBranches(dirs).catch(() => null) : null;
      const byDir = new Map(branches ? dirs.map((d, i) => [d, branches[i]]) : []);
      setBoard((b) =>
        b
          ? {
              ...b,
              sessions: sessions ?? b.sessions,
              projects: b.projects.map((p) => (p.directory && byDir.has(p.directory) ? { ...p, branch: byDir.get(p.directory) ?? null } : p)),
            }
          : b,
      );
    }, 30_000);
    return () => {
      un.then((f) => f());
      clearInterval(tick);
    };
  }, [refresh]);

  /** Consulta las releases de GitHub; avisa una sola vez por versión nueva. Sin red, se calla. */
  const checkUpdate = useCallback(
    async (quiet: boolean) => {
      try {
        const info = await api.checkUpdate();
        setUpdate(info);
        const newest = info.releases[0];
        if (!newest) return;
        let notified: string | null = null;
        try {
          notified = localStorage.getItem("rm.update.notified");
        } catch {}
        if (notified !== newest.version) {
          pushToast({ tone: "progress", title: `record-mind ${newest.version} disponible`, body: "Mira las novedades en la cabecera del tablero." });
          try {
            localStorage.setItem("rm.update.notified", newest.version);
          } catch {}
        }
      } catch (err) {
        if (!quiet) pushToast({ tone: "error", title: "No se pudo buscar actualizaciones", body: String(err) });
      }
    },
    [pushToast],
  );

  useEffect(() => {
    // Al arrancar (sin estorbar la carga del tablero) y cada 6 h mientras siga abierta en la bandeja.
    const first = setTimeout(() => checkUpdate(true), 4_000);
    const every = setInterval(() => checkUpdate(true), 6 * 60 * 60 * 1000);
    return () => {
      clearTimeout(first);
      clearInterval(every);
    };
  }, [checkUpdate]);

  const openUpdate = async () => {
    if (!update) await checkUpdate(false);
    setUpdateOpen(true);
  };

  // Puente por stdio viejo en Claude Code: se revisa una vez, cuando el MCP de la app ya está en marcha.
  const [bridge, setBridge] = useState<BridgeNotice | null>(null);
  const [bridgeBusy, setBridgeBusy] = useState(false);
  const bridgeChecked = useRef(false);
  const mcpUp = !!board?.mcp_url;
  useEffect(() => {
    if (!mcpUp || bridgeChecked.current) return;
    bridgeChecked.current = true;
    api.bridgeNotice().then(setBridge).catch(() => {});
  }, [mcpUp]);

  const fixBridge = async () => {
    setBridgeBusy(true);
    try {
      await api.fixBridge();
      setBridge(null);
      pushToast({ tone: "review", title: "Claude Code ya usa el puente de la app", body: "Reinicia Claude Code para que tome el cambio." });
    } catch (err) {
      pushToast({ tone: "error", title: "No se pudo cambiar el puente", body: String(err) });
    } finally {
      setBridgeBusy(false);
    }
  };

  const dismissBridge = () => {
    setBridge(null);
    api.dismissBridge().catch(() => {});
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        // Con la ventana estrecha el buscador completo está oculto: se abre el desplegable.
        if (searchRef.current?.offsetParent) searchRef.current.focus();
        else setSearchOpen(true);
      } else if (e.key === "Escape") {
        setOpenId(null);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // El desplegable del buscador se cierra al hacer clic fuera.
  useEffect(() => {
    if (!searchOpen) return;
    const onDown = (e: MouseEvent) => {
      if (!searchBoxRef.current?.contains(e.target as Node)) setSearchOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [searchOpen]);

  const visible = useMemo(() => {
    if (!board) return [];
    const q = query.trim().toLowerCase();
    // `now` se toma aquí: el tablero se repinta cada 30 s, así que al pasar la medianoche «Hoy» se pone al día solo.
    const now = new Date();
    return board.tasks.filter(
      (t) =>
        (!project || t.project === project) &&
        inPeriod(t, period, now) &&
        (!q || t.title.toLowerCase().includes(q) || t.note.toLowerCase().includes(q) || `#${t.id}` === q),
    );
  }, [board, project, query, period]);

  const decisionFiles = useMemo(() => {
    if (!board) return [];
    const q = query.trim().toLowerCase();
    const has = (s: string | null) => !!s && s.toLowerCase().includes(q);
    return board.decisions
      .filter((f) => !project || f.project === project)
      .map((f) =>
        !q || has(f.title) || has(f.summary)
          ? f
          : { ...f, decisions: f.decisions.filter((d) => has(d.question) || has(d.context) || has(d.recommendation) || has(d.resolution)) },
      )
      .filter((f) => f.decisions.length > 0);
  }, [board, project, query]);
  const openDecisions = decisionFiles.reduce((n, f) => n + f.decisions.filter((d) => !d.resolved).length, 0);

  const byStatus = useMemo(() => {
    const m: Record<Status, TaskCard[]> = { inbox: [], in_progress: [], review: [], done: [] };
    for (const t of visible) m[t.status].push(t);
    // Lo último aprobado arriba (por cuándo se aprobó, no por la última edición).
    m.done.sort((a, b) => b.status_since.localeCompare(a.status_since));
    return m;
  }, [visible]);

  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 6 } }));

  const onDragStart = (e: DragStartEvent) => {
    setDragging(board?.tasks.find((t) => t.id === e.active.id) ?? null);
  };

  /** Mover a mano (arrastrar o menú). Optimista: se mueve ya y se corrige si falla. */
  const moveTask = async (task: TaskCard, to: Status) => {
    if (!canMove(task.status, to)) return;
    // Llega a la columna ahora: con el filtro «Hoy» no debe desaparecer mientras responde la base.
    const moved = { ...task, status: to, status_since: new Date().toISOString() };
    setBoard((b) => b && { ...b, tasks: b.tasks.map((t) => (t.id === task.id ? { ...t, ...moved } : t)) });
    if (prev.current) prev.current.set(task.id, moved);
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
  const projectBranch = project ? board?.projects.find((p) => p.name === project)?.branch : null;
  const periodInfo = PERIODS.find((p) => p.id === period)!;
  const searchPlaceholder = view === "decisions" ? "Buscar decisiones" : "Buscar pendientes";

  return (
    <div className="relative z-10 grid h-full grid-cols-[248px_minmax(0,1fr)]">
      <Sidebar
        board={board}
        project={project}
        onProject={setProject}
        onSettings={() => setSettingsOpen(true)}
        onRules={() => setRulesOpen(true)}
        view={view}
        onView={setView}
        update={update}
        onUpdate={openUpdate}
      />

      <div className="flex min-h-0 flex-col">
        {/* La ventana no tiene marco: esta cabecera hace de barra de título (se arrastra y con doble clic maximiza). */}
        <header data-tauri-drag-region className="flex items-center gap-3 border-b border-line px-6 py-3.5">
          <div data-tauri-drag-region className="max-w-[40%] min-w-0 shrink-0">
            <div data-tauri-drag-region className="flex min-w-0 items-center gap-2">
              <span data-tauri-drag-region className="eyebrow shrink-0">
                {project ? "Proyecto" : "Todos los proyectos"}
                {view === "board" && period !== "all" && <span className="text-accent"> · {periodInfo.label}</span>}
              </span>
              {projectBranch && <BranchTag branch={projectBranch} size="md" className="text-ink-2" />}
            </div>
            <h1 data-tauri-drag-region className="truncate text-[19px] font-semibold tracking-tight">
              {project ?? (view === "decisions" ? "Decisiones" : "Tablero")}
              {view === "decisions" && openDecisions > 0 && (
                <span className="ml-3 align-middle text-[12px] font-medium text-st-review">
                  {openDecisions} por decidir
                </span>
              )}
              {view === "board" && reviewCount > 0 && (
                <span className="ml-3 align-middle text-[12px] font-medium text-st-review">
                  {reviewCount} para revisar
                </span>
              )}
              {view === "board" && period !== "all" && byStatus.done.length > 0 && (
                <span className="ml-3 align-middle text-[12px] font-medium text-st-done">
                  {byStatus.done.length} {byStatus.done.length === 1 ? "aprobada" : "aprobadas"}
                </span>
              )}
            </h1>
          </div>
          <div data-tauri-drag-region className="flex-1 self-stretch" />
          <div role="tablist" aria-label="Vista" className="flex shrink-0 rounded-xl border border-line bg-surface/80 p-0.5">
            {(
              [
                { id: "board", label: "Tablero", icon: <LayoutGrid size={13} />, hint: "Los pendientes del tablero" },
                { id: "decisions", label: "Decisiones", icon: <MessageSquareText size={13} />, hint: "Lo que quedó por decidir en tus conversaciones con Claude" },
              ] as const
            ).map((v) => (
              <button
                key={v.id}
                role="tab"
                aria-selected={view === v.id}
                onClick={() => setView(v.id)}
                title={v.hint}
                className={`flex items-center gap-1.5 rounded-[10px] px-2.5 py-1.5 text-[12.5px] font-medium whitespace-nowrap transition ${
                  view === v.id ? "bg-white/[0.08] text-ink" : "text-ink-3 hover:text-ink"
                }`}
              >
                {v.icon}
                <span className="max-xl:hidden">{v.label}</span>
                {v.id === "decisions" && openDecisions > 0 && (
                  <span className="rounded-full bg-st-review/20 px-1.5 font-mono text-[10.5px] text-st-review">{openDecisions}</span>
                )}
              </button>
            ))}
          </div>
          {view === "board" ? (
            <div role="radiogroup" aria-label="Periodo" className="flex shrink-0 rounded-xl border border-line bg-surface/80 p-0.5">
              {PERIODS.map((p) => (
                <button
                  key={p.id}
                  role="radio"
                  aria-checked={period === p.id}
                  onClick={() => setPeriod(p.id)}
                  title={p.hint}
                  className={`rounded-[10px] px-2.5 py-1.5 text-[12.5px] font-medium whitespace-nowrap transition ${
                    period === p.id ? "bg-accent-soft text-accent" : "text-ink-3 hover:text-ink"
                  }`}
                >
                  {p.label}
                </button>
              ))}
            </div>
          ) : (
            <div role="radiogroup" aria-label="Qué decisiones mostrar" className="flex shrink-0 rounded-xl border border-line bg-surface/80 p-0.5">
              {(
                [
                  { id: "open", label: "Abiertas", hint: "Solo las que faltan por decidir" },
                  { id: "all", label: "Todas", hint: "También las ya tomadas" },
                ] as const
              ).map((f) => (
                <button
                  key={f.id}
                  role="radio"
                  aria-checked={decisionFilter === f.id}
                  onClick={() => setDecisionFilter(f.id)}
                  title={f.hint}
                  className={`rounded-[10px] px-2.5 py-1.5 text-[12.5px] font-medium whitespace-nowrap transition ${
                    decisionFilter === f.id ? "bg-accent-soft text-accent" : "text-ink-3 hover:text-ink"
                  }`}
                >
                  {f.label}
                </button>
              ))}
            </div>
          )}
          {/* Ventana ancha: el buscador completo. Más estrecha: solo el icono, que lo abre debajo (también con Ctrl K). */}
          <label className="flex w-72 shrink-0 items-center gap-2 rounded-xl border border-line bg-surface/80 px-3 py-2 text-ink-2 focus-within:border-accent/50 max-2xl:hidden">
            <Search size={15} />
            <input
              ref={searchRef}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={searchPlaceholder}
              className="min-w-0 flex-1 bg-transparent text-ink outline-none placeholder:text-ink-3"
            />
            <span className="kbd whitespace-nowrap">Ctrl K</span>
          </label>
          <div ref={searchBoxRef} className="relative shrink-0 2xl:hidden">
            <button
              onClick={() => setSearchOpen((v) => !v)}
              title="Buscar (Ctrl K)"
              aria-label="Buscar"
              aria-expanded={searchOpen}
              className={`relative grid size-[38px] place-items-center rounded-xl border transition ${
                searchOpen || query
                  ? "border-accent/50 bg-accent-soft text-accent"
                  : "border-line bg-surface/80 text-ink-2 hover:border-line-strong hover:text-ink"
              }`}
            >
              <Search size={16} />
              {query && !searchOpen && <span className="absolute -top-0.5 -right-0.5 size-2 rounded-full bg-accent shadow-[0_0_8px_var(--color-accent-glow)]" />}
            </button>
            <AnimatePresence>
              {searchOpen && (
                <motion.label
                  initial={{ opacity: 0, y: -4, scale: 0.98 }}
                  animate={{ opacity: 1, y: 0, scale: 1 }}
                  exit={{ opacity: 0, y: -4, scale: 0.98 }}
                  transition={{ duration: 0.12 }}
                  className="absolute top-full right-0 z-40 mt-2 flex w-80 items-center gap-2 rounded-xl border border-line-strong bg-elevated px-3 py-2.5 text-ink-2 shadow-2xl shadow-black/60 focus-within:border-accent/50"
                >
                  <Search size={15} />
                  <input
                    autoFocus
                    value={query}
                    onChange={(e) => setQuery(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Escape" || e.key === "Enter") {
                        e.stopPropagation();
                        setSearchOpen(false);
                      }
                    }}
                    placeholder={searchPlaceholder}
                    className="min-w-0 flex-1 bg-transparent text-ink outline-none placeholder:text-ink-3"
                  />
                  {query ? (
                    <button onClick={() => setQuery("")} title="Borrar la búsqueda" className="rounded-md p-0.5 text-ink-3 hover:text-ink">
                      <X size={14} />
                    </button>
                  ) : (
                    <span className="kbd whitespace-nowrap">Ctrl K</span>
                  )}
                </motion.label>
              )}
            </AnimatePresence>
          </div>
          {update && update.releases.length > 0 && (
            <button
              onClick={() => setUpdateOpen(true)}
              title={`Tienes la ${update.current}. Ver las novedades e instalar.`}
              className="flex shrink-0 items-center gap-1.5 rounded-xl border border-accent/40 bg-accent-soft px-3 py-2 text-[12.5px] font-medium whitespace-nowrap text-accent transition hover:border-accent/70"
            >
              <Sparkles size={14} />
              {update.releases[0].version}
              <span className="max-xl:hidden">disponible</span>
            </button>
          )}
          <button
            onClick={() => api.openCapture()}
            className="flex shrink-0 items-center gap-2 rounded-xl bg-accent px-3.5 py-2 font-medium whitespace-nowrap text-white shadow-[0_0_24px_var(--color-accent-glow)] transition hover:brightness-110"
          >
            <Plus size={16} strokeWidth={2.5} />
            Capturar
            <span className="flex gap-0.5 opacity-80 max-lg:hidden">
              {shortcut.map((k) => (
                <span key={k} className="rounded bg-white/15 px-1 font-mono text-[10px]">
                  {k}
                </span>
              ))}
            </span>
          </button>
          <WindowControls />
        </header>

        {error && (
          <div className="mx-6 mt-4 rounded-xl border border-red-400/30 bg-red-400/10 px-4 py-2 text-[13px] text-red-300">
            {error}
          </div>
        )}

        {bridge && (
          <div className="mx-5 mt-4 flex items-center gap-3 rounded-xl border border-st-review/30 bg-st-review/[0.07] px-4 py-2.5">
            <Plug size={16} className="shrink-0 text-st-review" />
            <p className="min-w-0 flex-1 text-[12.5px] leading-relaxed text-ink-2">
              <span className="font-medium text-ink">Claude Code usa un puente viejo de record-mind ({bridge.version}).</span>{" "}
              Le faltan funciones de la {bridge.app_version}, como <code className="text-ink">/decisiones</code>. El de la app
              se actualiza solo con cada versión.
            </p>
            <button
              onClick={fixBridge}
              disabled={bridgeBusy}
              title={`Cambia ${bridge.command} por el MCP de la app en la configuración de Claude Code (guarda una copia antes)`}
              className="shrink-0 rounded-xl bg-accent px-3 py-1.5 text-[12.5px] font-medium whitespace-nowrap text-white shadow-[0_0_18px_var(--color-accent-glow)] transition hover:brightness-110 disabled:opacity-40"
            >
              Usar el de la app
            </button>
            <button
              onClick={dismissBridge}
              disabled={bridgeBusy}
              title="No volver a avisar hasta la siguiente versión"
              className="shrink-0 rounded-xl px-3 py-1.5 text-[12.5px] whitespace-nowrap text-ink-2 hover:bg-white/5 hover:text-ink disabled:opacity-40"
            >
              Dejarlo así
            </button>
          </div>
        )}

        {view === "decisions" ? (
          <DecisionsView
            files={decisionFiles}
            filter={decisionFilter}
            showProject={!project}
            empty={board !== null && board.decisions.length === 0}
            onToast={pushToast}
            onChanged={refresh}
          />
        ) : (
        <DndContext sensors={sensors} onDragStart={onDragStart} onDragEnd={onDragEnd} onDragCancel={() => setDragging(null)}>
          <LayoutGroup>
            {/* El mínimo va en la pista y no en la columna: si no caben, se desplaza en horizontal en vez de solaparse. */}
            <main className="grid min-h-0 flex-1 grid-cols-[repeat(4,minmax(200px,1fr))] gap-4 overflow-x-auto p-5">
              {STATUSES.map((s) => (
                <Column
                  key={s.id}
                  status={s}
                  tasks={byStatus[s.id]}
                  showProject={!project}
                  draggingId={dragging?.id ?? null}
                  locked={dragging ? dragging.status !== s.id && !canMove(dragging.status, s.id) : !isMovable(s.id)}
                  onOpen={setOpenId}
                  onApprove={quickApprove}
                  onToggleQueue={(t) => setQueued([t.id], !t.queued)}
                  onQueue={setQueued}
                  onContextMenu={(task, x, y) => setMenu({ task, x, y })}
                  menuId={menu?.task.id ?? null}
                  empty={board !== null && board.tasks.length === 0}
                  periodPhrase={periodInfo.phrase}
                  shortcut={shortcut}
                />
              ))}
            </main>
          </LayoutGroup>
          <DragOverlay dropAnimation={{ duration: 180, easing: "cubic-bezier(.2,.8,.2,1)" }}>
            {dragging && <CardView task={dragging} showProject={!project} overlay />}
          </DragOverlay>
        </DndContext>
        )}
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

      <AnimatePresence>
        {settingsOpen && <SettingsDialog onClose={() => setSettingsOpen(false)} onChanged={refresh} />}
      </AnimatePresence>

      <AnimatePresence>
        {rulesOpen && (
          <RulesDialog
            rules={board?.rules ?? []}
            tasks={board?.tasks ?? []}
            onClose={() => setRulesOpen(false)}
            onOpenTask={(id) => {
              setRulesOpen(false);
              setOpenId(id);
            }}
            onDecided={(d, accepted) => {
              pushToast(
                accepted
                  ? { tone: "review", title: d.added ? "Regla añadida a CLAUDE.md" : "La regla ya estaba en CLAUDE.md", body: d.path ?? d.rule.rule }
                  : { tone: "inbox", title: "Regla descartada", body: d.rule.rule },
              );
              refresh();
            }}
          />
        )}
      </AnimatePresence>

      <AnimatePresence>
        {updateOpen && update && <UpdateDialog info={update} onClose={() => setUpdateOpen(false)} onRecheck={() => checkUpdate(false)} />}
      </AnimatePresence>

      <Toasts toasts={toasts} />
    </div>
  );
}
