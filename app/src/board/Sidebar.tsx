import { Brain, LayoutGrid, Plug } from "lucide-react";
import type { Board } from "../lib/api";
import iconUrl from "../../src-tauri/icons/128x128.png";

export function Sidebar({
  board,
  project,
  onProject,
}: {
  board: Board | null;
  project: string | null;
  onProject: (p: string | null) => void;
}) {
  const total = board?.tasks.filter((t) => t.status !== "done").length ?? 0;
  const active = board?.active;

  return (
    <aside className="flex min-h-0 flex-col border-r border-line bg-base/70 backdrop-blur-xl">
      <div className="flex items-center gap-2.5 px-5 pt-5 pb-4">
        <img src={iconUrl} alt="" className="size-8 drop-shadow-[0_0_12px_var(--color-accent-glow)]" />
        <b className="text-[16px] font-bold tracking-tight">
          record<span className="text-accent">·</span>mind
        </b>
      </div>

      {active && (
        <button
          onClick={() => onProject(active.project)}
          className="mx-3 mb-2 rounded-xl border border-accent/25 bg-accent-soft px-3 py-2.5 text-left transition hover:border-accent/45"
          title={active.directory}
        >
          <div className="flex items-center gap-2 text-[10.5px] font-medium tracking-wider text-accent uppercase">
            <span className="size-1.5 animate-pulse-dot rounded-full bg-accent" /> Sesión activa
          </div>
          <div className="mt-0.5 truncate text-[13px] font-medium">{active.project}</div>
        </button>
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
          return (
            <NavItem key={p.name} active={project === p.name} onClick={() => onProject(p.name)} count={open}>
              <span
                className="size-1.5 shrink-0 rounded-full"
                style={{
                  background: p.counts[1] ? "var(--color-st-progress)" : p.counts[2] ? "var(--color-st-review)" : "var(--color-ink-3)",
                }}
              />
              <span className="truncate">{p.name}</span>
            </NavItem>
          );
        })}
        {board && board.projects.length === 0 && <p className="px-2 text-[12px] text-ink-3">Sin proyectos todavía.</p>}
      </nav>

      <footer className="space-y-1.5 border-t border-line px-5 py-4 text-[11.5px] text-ink-3">
        <Status ok={!!board?.mcp_url} icon={<Plug size={12} />} label={board?.mcp_url ? "MCP en línea" : "MCP apagado"} title={board?.mcp_url ?? undefined} />
        <Status ok={!!board?.engram} icon={<Brain size={12} />} label={board?.engram ? "Engram conectado" : "Engram no encontrado"} />
      </footer>
    </aside>
  );
}

function NavItem(props: { active: boolean; onClick: () => void; count: number; children: React.ReactNode }) {
  return (
    <button
      onClick={props.onClick}
      className={`flex items-center gap-2.5 rounded-lg px-2.5 py-1.5 text-left text-[13px] transition ${
        props.active ? "bg-white/[0.07] text-ink" : "text-ink-2 hover:bg-white/[0.04] hover:text-ink"
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
