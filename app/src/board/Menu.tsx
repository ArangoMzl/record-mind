import { ChevronUp } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useRef, useState } from "react";

export interface MenuItem {
  id: string;
  label: string;
  hint?: string;
  color?: string;
  icon?: React.ReactNode;
  danger?: boolean;
}

/** Menú desplegable propio (se abre hacia arriba, pensado para el pie del panel). */
export function Menu(props: { label: React.ReactNode; items: MenuItem[]; onSelect: (id: string) => void }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => !ref.current?.contains(e.target as Node) && setOpen(false);
    const esc = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        setOpen(false);
      }
    };
    window.addEventListener("mousedown", close);
    window.addEventListener("keydown", esc, true);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("keydown", esc, true);
    };
  }, [open]);

  return (
    <div ref={ref} className="relative">
      <button
        onClick={() => setOpen((o) => !o)}
        className={`flex items-center gap-2 rounded-xl border px-3.5 py-2 text-[13px] font-medium transition ${
          open ? "border-accent/50 bg-white/[0.07]" : "border-line-strong bg-white/[0.03] hover:bg-white/[0.07]"
        }`}
      >
        {props.label}
        <ChevronUp size={14} className={`text-ink-3 transition ${open ? "" : "rotate-180"}`} />
      </button>
      <AnimatePresence>
        {open && (
          <motion.div
            initial={{ opacity: 0, y: 6, scale: 0.97 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: 6, scale: 0.97 }}
            transition={{ duration: 0.14 }}
            className="absolute right-0 bottom-full z-10 mb-2 w-64 origin-bottom-right overflow-hidden rounded-xl border border-line-strong bg-elevated p-1 shadow-2xl shadow-black/60"
          >
            {props.items.map((it) => (
              <button
                key={it.id}
                onClick={() => {
                  setOpen(false);
                  props.onSelect(it.id);
                }}
                className={`flex w-full items-start gap-2.5 rounded-lg px-2.5 py-2 text-left transition hover:bg-white/[0.06] ${
                  it.danger ? "text-red-300" : ""
                }`}
              >
                <span className="mt-[5px] flex shrink-0">
                  {it.icon ?? <span className="size-2 rounded-full" style={{ background: it.color, boxShadow: `0 0 8px ${it.color}` }} />}
                </span>
                <span className="min-w-0">
                  <span className="block text-[13px] font-medium">{it.label}</span>
                  {it.hint && <span className="block text-[11.5px] text-ink-3">{it.hint}</span>}
                </span>
              </button>
            ))}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

/** «Claude está en esto» con brillo y puntos en ola. */
export function WorkingLabel({ text = "Claude está en esto" }: { text?: string }) {
  return (
    <span className="inline-flex items-center gap-1.5 font-medium">
      <span className="text-wave">{text}</span>
      <span className="wave-dots" aria-hidden>
        <i />
        <i />
        <i />
      </span>
    </span>
  );
}
