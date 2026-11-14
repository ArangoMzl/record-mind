import { Check, ChevronDown, Plus, Search } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useMemo, useRef, useState } from "react";

/**
 * Selector de proyecto propio (la app no usa selects ni datalists nativos): lista con buscador,
 * marca la sesión activa de Claude Code y permite crear un proyecto escribiendo su nombre.
 */
export function ProjectPicker(props: {
  value: string;
  projects: string[];
  onChange: (project: string) => void;
  /** Proyectos con una conversación activa de Claude Code. */
  active?: string[];
  disabled?: boolean;
  /** chip: compacto, para cabeceras · field: ancho completo, para formularios. */
  variant?: "chip" | "field";
}) {
  const { value, active, variant = "chip" } = props;
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [hi, setHi] = useState(0);
  const ref = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const all = useMemo(() => {
    const list = [...props.projects];
    if (value && !list.includes(value)) list.unshift(value);
    return list;
  }, [props.projects, value]);

  const q = query.trim();
  const matches = q ? all.filter((p) => p.toLowerCase().includes(q.toLowerCase())) : all;
  const canCreate = q !== "" && !all.some((p) => p.toLowerCase() === q.toLowerCase());
  const options = [...matches.map((p) => ({ name: p, create: false })), ...(canCreate ? [{ name: q, create: true }] : [])];

  const show = () => {
    if (props.disabled) return;
    setQuery("");
    setHi(Math.max(0, all.indexOf(value)));
    setOpen(true);
  };

  const close = (refocus = true) => {
    setOpen(false);
    if (refocus) triggerRef.current?.focus();
  };

  const pick = (name: string) => {
    props.onChange(name);
    close();
  };

  useEffect(() => {
    if (!open) return;
    // El buscador toma el foco ya montado: autoFocus pierde contra el foco que deja el clic.
    const focusTimer = setTimeout(() => inputRef.current?.focus(), 0);
    const outside = (e: MouseEvent) => !ref.current?.contains(e.target as Node) && close(false);
    // En fase de captura, antes que la ventana: Esc cierra la lista y nada más.
    const esc = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      close();
    };
    window.addEventListener("mousedown", outside);
    window.addEventListener("keydown", esc, true);
    return () => {
      clearTimeout(focusTimer);
      window.removeEventListener("mousedown", outside);
      window.removeEventListener("keydown", esc, true);
    };
  }, [open]);

  // Mantiene visible la opción resaltada al moverse con el teclado.
  useEffect(() => {
    listRef.current?.querySelector(`[data-i="${hi}"]`)?.scrollIntoView({ block: "nearest" });
  }, [hi]);

  const onKey = (e: React.KeyboardEvent) => {
    // Nada de lo que se teclea aquí debe llegar a la ventana (Enter guarda, Esc cierra).
    e.stopPropagation();
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setHi((i) => Math.min(options.length - 1, i + 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setHi((i) => Math.max(0, i - 1));
    } else if (e.key === "Enter") {
      e.preventDefault();
      const o = options[hi];
      if (o) pick(o.name);
    } else if (e.key === "Tab") {
      close(false);
    }
  };

  const isActive = (p: string) => !!active?.includes(p);

  return (
    <div ref={ref} data-picker-open={open || undefined} className={`relative ${variant === "field" ? "w-full" : ""}`}>
      <button
        ref={triggerRef}
        type="button"
        disabled={props.disabled}
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => (open ? close() : show())}
        className={
          variant === "chip"
            ? `flex items-center gap-1.5 rounded-lg border py-1 pr-1.5 pl-2 text-[12px] transition ${
                open ? "border-accent/50 bg-white/[0.07]" : "border-white/10 bg-white/[0.04] hover:border-white/20"
              }`
            : `flex w-full items-center gap-2 rounded-xl border bg-base px-3 py-2 text-left text-[13.5px] transition disabled:opacity-50 ${
                open ? "border-accent/50" : "border-line-strong hover:border-white/20"
              }`
        }
      >
        {isActive(value) && <span className="size-1.5 shrink-0 animate-pulse-dot rounded-full bg-accent" title="Sesión activa de Claude Code" />}
        <span className={`truncate ${variant === "chip" ? "max-w-44" : "flex-1"} ${value ? "text-ink" : "text-ink-3"}`}>
          {value || "Elige proyecto"}
        </span>
        <ChevronDown size={13} className={`shrink-0 text-ink-3 transition ${open ? "rotate-180" : ""}`} />
      </button>

      <AnimatePresence>
        {open && (
          <motion.div
            initial={{ opacity: 0, y: -4, scale: 0.98 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: -4, scale: 0.98 }}
            transition={{ duration: 0.12 }}
            className={`absolute top-full left-0 z-30 mt-1.5 origin-top-left overflow-hidden rounded-xl border border-line-strong bg-elevated shadow-2xl shadow-black/60 ${
              variant === "field" ? "w-full" : "w-64"
            }`}
          >
            <div className="flex items-center gap-2 border-b border-line px-2.5">
              <Search size={13} className="shrink-0 text-ink-3" />
              <input
                ref={inputRef}
                value={query}
                onChange={(e) => {
                  setQuery(e.target.value);
                  setHi(0);
                }}
                onKeyDown={onKey}
                placeholder="Buscar o crear proyecto…"
                className="min-w-0 flex-1 bg-transparent py-2 text-[12.5px] text-ink outline-none placeholder:text-ink-3"
              />
            </div>
            <div ref={listRef} role="listbox" className="max-h-52 overflow-y-auto p-1">
              {options.map((o, i) => (
                <button
                  key={(o.create ? "+" : "") + o.name}
                  type="button"
                  role="option"
                  aria-selected={o.name === value}
                  data-i={i}
                  onMouseEnter={() => setHi(i)}
                  onClick={() => pick(o.name)}
                  className={`flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5 text-left text-[12.5px] transition ${
                    i === hi ? "bg-white/[0.07] text-ink" : "text-ink-2"
                  }`}
                >
                  {o.create ? (
                    <>
                      <Plus size={13} className="shrink-0 text-accent" />
                      <span className="truncate">
                        Crear <span className="text-ink">«{o.name}»</span>
                      </span>
                    </>
                  ) : (
                    <>
                      <span className="flex w-3.5 shrink-0 justify-center">
                        {isActive(o.name) && <span className="size-1.5 rounded-full bg-accent shadow-[0_0_8px_var(--color-accent)]" />}
                      </span>
                      <span className="min-w-0 flex-1 truncate">{o.name}</span>
                      {isActive(o.name) && <span className="shrink-0 text-[11px] text-ink-3">sesión activa</span>}
                      {o.name === value && <Check size={13} className="shrink-0 text-accent" />}
                    </>
                  )}
                </button>
              ))}
              {options.length === 0 && <p className="px-2.5 py-2 text-[12px] text-ink-3">Escribe un nombre para crear un proyecto</p>}
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
