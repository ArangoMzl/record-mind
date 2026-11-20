import { Download, ExternalLink, Loader2, ShieldCheck, Sparkles, X } from "lucide-react";
import { motion } from "motion/react";
import { Fragment, useEffect, useState, type ReactNode } from "react";
import { api, onUpdateProgress, parseDate, type UpdateInfo, type UpdateProgress } from "../lib/api";

/**
 * Versión nueva: las notas de cada release posterior a la instalada y el botón para instalarla.
 * La instalación descarga el instalador de GitHub, comprueba su SHA-256 y lo abre en modo pasivo;
 * record-mind se cierra y vuelve a abrirse ya actualizada (las tareas viven en ~/.record-mind).
 */
export function UpdateDialog(props: { info: UpdateInfo; onClose: () => void; onRecheck: () => Promise<void> }) {
  const { info } = props;
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<UpdateProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);
  const newest = info.releases[0];

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !installing) {
        e.stopPropagation();
        props.onClose();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [props, installing]);

  useEffect(() => {
    const un = onUpdateProgress(setProgress);
    return () => {
      un.then((f) => f());
    };
  }, []);

  const install = async () => {
    setInstalling(true);
    setError(null);
    setProgress(null);
    try {
      await api.installUpdate();
    } catch (err) {
      setError(String(err));
      setInstalling(false);
    }
  };

  const recheck = async () => {
    setChecking(true);
    await props.onRecheck();
    setChecking(false);
  };

  const pct = progress && progress.total > 0 ? Math.min(100, Math.round((progress.downloaded / progress.total) * 100)) : 0;

  return (
    <motion.div
      className="fixed inset-0 z-[55] grid place-items-center bg-black/55 p-6 backdrop-blur-[2px]"
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      onMouseDown={(e) => e.target === e.currentTarget && !installing && props.onClose()}
    >
      <motion.div
        initial={{ opacity: 0, y: 10, scale: 0.98 }}
        animate={{ opacity: 1, y: 0, scale: 1 }}
        exit={{ opacity: 0, y: 10, scale: 0.98 }}
        transition={{ type: "spring", stiffness: 420, damping: 34 }}
        className="flex max-h-[85vh] w-[640px] max-w-full flex-col rounded-2xl border border-line-strong bg-surface shadow-2xl shadow-black/60"
      >
        <header className="flex items-center gap-2 border-b border-line px-5 py-3.5">
          <Sparkles size={15} className="text-accent" />
          <h2 className="text-[14px] font-semibold">{newest ? `record-mind ${newest.version} disponible` : "record-mind está al día"}</h2>
          <span className="text-[12px] text-ink-3">tienes la {info.current}</span>
          <div className="flex-1" />
          <button
            onClick={props.onClose}
            disabled={installing}
            className="rounded-lg p-1.5 text-ink-3 hover:bg-white/5 hover:text-ink disabled:opacity-40"
            title="Cerrar (Esc)"
          >
            <X size={15} />
          </button>
        </header>

        <div className="min-h-0 flex-1 space-y-3 overflow-y-auto px-5 py-4">
          {info.releases.length === 0 && (
            <div className="rounded-xl border border-dashed border-line-strong px-4 py-6 text-center text-[12.5px] text-ink-3">
              Tienes la última versión publicada ({info.latest}).
            </div>
          )}
          {info.releases.map((r, i) => (
            <article key={r.version} className="rounded-xl border border-line-strong bg-base p-4">
              <div className="mb-2.5 flex items-center gap-2">
                <span className="rounded-md bg-accent-soft px-1.5 py-0.5 font-mono text-[11.5px] font-semibold text-accent">v{r.version}</span>
                {i === 0 && <span className="text-[11px] font-medium tracking-wider text-ink-2 uppercase">Más reciente</span>}
                <div className="flex-1" />
                {r.published_at && (
                  <span className="text-[11.5px] text-ink-3">
                    {parseDate(r.published_at).toLocaleDateString("es", { day: "numeric", month: "short", year: "numeric" })}
                  </span>
                )}
                <button
                  onClick={() => api.openUrl(r.url)}
                  className="flex items-center gap-1 rounded-md px-1.5 py-0.5 text-[11.5px] text-ink-3 hover:bg-white/5 hover:text-ink"
                  title="Ver la release en GitHub"
                >
                  GitHub <ExternalLink size={11} />
                </button>
              </div>
              <Markdown source={releaseBody(r.notes)} />
            </article>
          ))}
        </div>

        <footer className="flex items-center gap-3 border-t border-line px-5 py-3">
          {newest ? (
            <>
              {installing ? (
                <div className="flex min-w-0 flex-1 items-center gap-3">
                  <Loader2 size={14} className="shrink-0 animate-spin text-accent" />
                  <div className="h-1.5 min-w-0 flex-1 overflow-hidden rounded-full bg-white/[0.07]">
                    <div className="h-full rounded-full bg-accent transition-[width] duration-200" style={{ width: `${pct}%` }} />
                  </div>
                  <span className="shrink-0 font-mono text-[11.5px] text-ink-2">
                    {progress && pct >= 100 ? "Abriendo el instalador…" : progress ? `${pct}%` : "Descargando…"}
                  </span>
                </div>
              ) : (
                <>
                  <span className="flex min-w-0 flex-1 items-center gap-1.5 text-[11.5px] text-ink-3">
                    {info.installer ? (
                      <>
                        <ShieldCheck size={13} className="shrink-0 text-emerald-400" />
                        <span className="truncate" title="Se comprueba el SHA-256 publicado, se instala encima y record-mind se vuelve a abrir. Tus tareas (en ~/.record-mind) no se tocan.">
                          SHA-256 verificado · conserva tus tareas
                        </span>
                      </>
                    ) : (
                      <span>Esta release no trae un instalador verificable: descárgalo desde GitHub.</span>
                    )}
                  </span>
                  <button onClick={props.onClose} className="shrink-0 rounded-xl px-3 py-1.5 text-[12.5px] text-ink-2 hover:bg-white/5 hover:text-ink">
                    Ahora no
                  </button>
                  {info.installer ? (
                    <button
                      onClick={install}
                      className="flex shrink-0 items-center gap-1.5 rounded-xl bg-accent px-3 py-1.5 text-[12.5px] font-medium text-white shadow-[0_0_18px_var(--color-accent-glow)] transition hover:brightness-110"
                    >
                      <Download size={13} /> Instalar {newest.version}
                    </button>
                  ) : (
                    <button
                      onClick={() => api.openUrl(newest.url)}
                      className="flex shrink-0 items-center gap-1.5 rounded-xl bg-accent px-3 py-1.5 text-[12.5px] font-medium text-white transition hover:brightness-110"
                    >
                      <ExternalLink size={13} /> Abrir en GitHub
                    </button>
                  )}
                </>
              )}
            </>
          ) : (
            <>
              <div className="flex-1" />
              <button
                onClick={recheck}
                disabled={checking}
                className="flex items-center gap-1.5 rounded-xl px-3 py-1.5 text-[12.5px] text-ink-2 hover:bg-white/5 hover:text-ink disabled:opacity-40"
              >
                {checking && <Loader2 size={13} className="animate-spin" />} Buscar de nuevo
              </button>
            </>
          )}
        </footer>
        {error && <p className="border-t border-line px-5 py-2.5 text-[12px] text-red-300">{error}</p>}
      </motion.div>
    </motion.div>
  );
}

/** Quita de las notas lo que ya hace la app: el bloque de sumas SHA-256 (se comprueban solas). */
function releaseBody(md: string): string {
  return md.replace(/\n#{1,6}\s*SHA-?256[\s\S]*?(?=\n#{1,6}\s|$)/i, "").trim();
}

/**
 * Markdown mínimo de las notas de release (títulos, listas, citas, código, negrita, enlaces),
 * pintado con elementos de React: nada de HTML crudo y los enlaces se abren en el navegador.
 */
function Markdown({ source }: { source: string }) {
  const blocks: ReactNode[] = [];
  const lines = source.replace(/\r\n/g, "\n").split("\n");
  let list: { ordered: boolean; items: string[] } | null = null;
  let para: string[] = [];

  const flush = () => {
    if (para.length) {
      blocks.push(
        <p key={blocks.length} className="text-[13px] leading-relaxed text-ink-2">
          {inline(para.join(" "))}
        </p>,
      );
      para = [];
    }
    if (list) {
      const items = list.items.map((it, i) => (
        <li key={i} className="pl-1">
          {inline(it)}
        </li>
      ));
      blocks.push(
        list.ordered ? (
          <ol key={blocks.length} className="list-decimal space-y-1 pl-5 text-[13px] leading-relaxed text-ink-2 marker:text-ink-3">
            {items}
          </ol>
        ) : (
          <ul key={blocks.length} className="list-disc space-y-1 pl-5 text-[13px] leading-relaxed text-ink-2 marker:text-accent">
            {items}
          </ul>
        ),
      );
      list = null;
    }
  };

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    if (line.trim().startsWith("```")) {
      flush();
      const code: string[] = [];
      for (i++; i < lines.length && !lines[i].trim().startsWith("```"); i++) code.push(lines[i]);
      blocks.push(
        <pre key={blocks.length} className="overflow-x-auto rounded-lg border border-line bg-surface px-3 py-2 font-mono text-[11.5px] text-ink-2">
          {code.join("\n")}
        </pre>,
      );
      continue;
    }
    const heading = /^(#{1,6})\s+(.*)$/.exec(line);
    const bullet = /^\s*[-*+]\s+(.*)$/.exec(line);
    const ordered = /^\s*\d+[.)]\s+(.*)$/.exec(line);
    if (heading) {
      flush();
      blocks.push(
        <h3 key={blocks.length} className="pt-1 text-[12px] font-semibold tracking-wider text-ink uppercase">
          {inline(heading[2])}
        </h3>,
      );
    } else if (bullet || ordered) {
      if (para.length) flush();
      const isOrdered = !bullet;
      if (list && list.ordered !== isOrdered) flush();
      list ??= { ordered: isOrdered, items: [] };
      list.items.push((bullet ?? ordered)![1]);
    } else if (line.startsWith(">")) {
      flush();
      blocks.push(
        <blockquote key={blocks.length} className="border-l-2 border-line-strong pl-3 text-[12.5px] leading-relaxed text-ink-3">
          {inline(line.replace(/^>\s?/, ""))}
        </blockquote>,
      );
    } else if (!line.trim()) {
      flush();
    } else if (list && /^\s{2,}\S/.test(line)) {
      // Continuación de la viñeta anterior.
      list.items[list.items.length - 1] += " " + line.trim();
    } else {
      if (list) flush();
      para.push(line.trim());
    }
  }
  flush();
  return <div className="space-y-2.5">{blocks}</div>;
}

const INLINE = /(`[^`]+`)|(\*\*[^*]+\*\*)|(\[[^\]]+\]\([^)\s]+\))|(\*[^*\s][^*]*\*|_[^_\s][^_]*_)/g;

function inline(text: string): ReactNode {
  const out: ReactNode[] = [];
  let last = 0;
  for (const m of text.matchAll(INLINE)) {
    const at = m.index ?? 0;
    if (at > last) out.push(text.slice(last, at));
    const tok = m[0];
    const k = out.length;
    if (m[1]) {
      out.push(
        <code key={k} className="rounded bg-white/[0.07] px-1 py-px font-mono text-[11.5px] text-ink">
          {tok.slice(1, -1)}
        </code>,
      );
    } else if (m[2]) {
      out.push(
        <strong key={k} className="font-semibold text-ink">
          {inline(tok.slice(2, -2))}
        </strong>,
      );
    } else if (m[3]) {
      const [, label, href] = /^\[([^\]]+)\]\(([^)\s]+)\)$/.exec(tok)!;
      out.push(
        <button key={k} onClick={() => api.openUrl(href)} className="text-accent underline-offset-2 hover:underline" title={href}>
          {label}
        </button>,
      );
    } else {
      out.push(<em key={k}>{inline(tok.slice(1, -1))}</em>);
    }
    last = at + tok.length;
  }
  if (last < text.length) out.push(text.slice(last));
  return out.map((n, i) => <Fragment key={i}>{n}</Fragment>);
}
