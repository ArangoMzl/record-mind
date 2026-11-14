import { Brain, LayoutGrid, Plug, Settings } from "lucide-react";
import { shortcutLabel, timeAgo, type Board } from "../lib/api";
import iconUrl from "../../src-tauri/icons/128x128.png";

export function Sidebar({
  board,
  project,
  onProject,
  onSettings,
}: {
  board: Board | null;
  project: string | null;
  onProject: (p: string | null) => void;
  onSettings: () => void;
}) {
  const total = board?.tasks.filter((t) => t.status !== "done").length ?? 0;
  // Un renglón por proyecto: su conversación más reciente y cuántas tiene activas.
  const live: { project: string; directory: string; last_activity: string; count: number }[] = [];
  for (const s of board?.sessions ?? []) {
    const row = live.find((x) => x.project === s.project);
    if (row) row.count += 1;
    else live.push({ project: s.project, directory: s.directory, last_activity: s.last_activity, count: 1 });
  }

  return (
    <aside className="flex min-h-0 flex-col border-r border-line bg-base/70 backdrop-blur-xl">
      <div className="flex items-center gap-2.5 px-5 pt-5 pb-4">
        <img src={iconUrl} alt="" className="size-8 drop-shadow-[0_0_12px_var(--color-accent-glow)]" />
        <b className="text-[16px] font-bold tracking-tight">
          record<span className="text-accent">·</span>mind
        </b>
      </div>

      {live.length > 0 && (
        <div className="mx-3 mb-2 rounded-xl border border-accent/25 bg-accent-soft px-1.5 pt-2.5 pb-1.5">
          <div className="flex items-center gap-2 px-1.5 text-[10.5px] font-medium tracking-wider text-accent uppercase">
            <span className="size-1.5 animate-pulse-dot rounded-full bg-accent" />
            {live.length === 1 ? "Sesión activa" : "Sesiones activas"}
          </div>
          <div className="mt-1 flex flex-col">
            {live.map((s) => (
              <button
                key={s.project}
                onClick={() => onProject(s.project)}
                title={`${s.directory}${s.count > 1 ? ` · ${s.count} conversaciones` : ""}`}
                className={`flex items-baseline gap-2 rounded-lg px-1.5 py-1 text-left transition hover:bg-white/[0.06] ${
                  project === s.project ? "bg-white/[0.06]" : ""
                }`}
              >
                <span className="min-w-0 flex-1 truncate text-[13px] font-medium">{s.project}</span>
                {s.count > 1 && <span className="font-mono text-[10.5px] text-ink-3">×{s.count}</span>}
                <span className="shrink-0 text-[10.5px] text-ink-3">{timeAgo(s.last_activity)}</span>
              </button>
            ))}
          </div>
        </div>
      )}

      <div className="eyebrow px-5 pt-3 pb-2">Explorar</div>
      <nav className="flex flex-col gap-0.5 px-3">
        <NavItem active={project === null} onClick={() => onProject(null)} count={total}>
          <LayoutGrid size={15} /> Todos los proyectos
        </NavItem>
      </nav>

      <div className="eyebrow px-5 pt-5 pb-2">Proyectos</div>
      <nav className="flex min-h-0 flex-1 flex-col gap-0.5 overflow-y-auto px-3 pb-3">
        {board?.projects.map((p) => {
          const open = p.counts[0] + p.counts[1] + p.counts[2];
          // Los proyectos de Claude Code que aún no tienen tareas se ven apagados.
          const empty = p.counts.every((c) => c === 0);
          return (
            <NavItem
              key={p.name}
              active={project === p.name}
              onClick={() => onProject(p.name)}
              count={open}
              muted={empty}
              title={p.directory ?? undefined}
            >
              <span
                className={`size-1.5 shrink-0 rounded-full ${empty ? "border border-ink-3" : ""}`}
                style={
                  empty
                    ? undefined
                    : { background: p.counts[1] ? "var(--color-st-progress)" : p.counts[2] ? "var(--color-st-review)" : "var(--color-ink-3)" }
                }
              />
              <span className="truncate">{p.name}</span>
            </NavItem>
          );
        })}
        {board && board.projects.length === 0 && <p className="px-2 text-[12px] text-ink-3">Sin proyectos todavía.</p>}
      </nav>

      <footer className="space-y-1.5 border-t border-line px-5 py-4 text-[11.5px] text-ink-3">
        <button
          onClick={onSettings}
          className="-mx-2 mb-1.5 flex w-[calc(100%+1rem)] items-center gap-2 rounded-lg px-2 py-1.5 text-[12.5px] text-ink-2 transition hover:bg-white/[0.05] hover:text-ink"
        >
          <Settings size={13} /> Ajustes
          {board?.shortcut && (
            <span className="ml-auto flex gap-0.5">
              {shortcutLabel(board.shortcut).map((k, i) => (
                <span key={i} className="kbd px-1 py-0 text-[9.5px]">
                  {k}
                </span>
              ))}
            </span>
          )}
        </button>
        <Status ok={!!board?.mcp_url} icon={<Plug size={12} />} label={board?.mcp_url ? "MCP en línea" : "MCP apagado"} title={board?.mcp_url ?? undefined} />
        <Status ok={!!board?.engram} icon={<Brain size={12} />} label={board?.engram ? "Engram conectado" : "Engram no encontrado"} />
      </footer>
    </aside>
  );
}

function NavItem(props: {
  active: boolean;
  onClick: () => void;
  count: number;
  muted?: boolean;
  title?: string;
  children: React.ReactNode;
}) {
  return (
    <button
      onClick={props.onClick}
      title={props.title}
      className={`flex items-center gap-2.5 rounded-lg px-2.5 py-1.5 text-left text-[13px] transition ${
        props.active
          ? "bg-white/[0.07] text-ink"
          : props.muted
            ? "text-ink-3 hover:bg-white/[0.04] hover:text-ink-2"
            : "text-ink-2 hover:bg-white/[0.04] hover:text-ink"
      }`}
    >
      {props.children}
      {props.count > 0 && <span className="ml-auto font-mono text-[11px] text-ink-3">{props.count}</span>}
    </button>
  );
}

function Status({ ok, icon, label, title }: { ok: boolean; icon: React.ReactNode; label: string; title?: string }) {
  return (
    <div className="flex items-center gap-2" title={title}>
      <span className={ok ? "text-emerald-400" : "text-ink-3"}>{icon}</span>
      <span className={ok ? "text-ink-2" : ""}>{label}</span>
      {title && <span className="ml-auto truncate font-mono text-[10px]">{title.replace("http://", "")}</span>}
    </div>
  );
}
