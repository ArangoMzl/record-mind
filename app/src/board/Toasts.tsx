import { AnimatePresence, motion } from "motion/react";

export interface Toast {
  id: number;
  tone: "inbox" | "progress" | "review" | "error";
  title: string;
  body?: string;
}

const TONE: Record<Toast["tone"], string> = {
  inbox: "var(--color-st-inbox)",
  progress: "var(--color-st-progress)",
  review: "var(--color-st-review)",
  error: "#f87171",
};

export function Toasts({ toasts }: { toasts: Toast[] }) {
  return (
    <div className="pointer-events-none fixed right-5 bottom-5 z-[60] flex w-80 flex-col gap-2">
      <AnimatePresence initial={false}>
        {toasts.map((t) => (
          <motion.div
            key={t.id}
            layout
            initial={{ opacity: 0, x: 40, scale: 0.96 }}
            animate={{ opacity: 1, x: 0, scale: 1 }}
            exit={{ opacity: 0, x: 40 }}
            transition={{ type: "spring", stiffness: 380, damping: 30 }}
            className="flex gap-3 rounded-xl border border-line-strong bg-elevated/95 p-3 shadow-2xl shadow-black/50 backdrop-blur"
          >
            <span className="mt-1.5 size-2 shrink-0 rounded-full" style={{ background: TONE[t.tone], boxShadow: `0 0 10px ${TONE[t.tone]}` }} />
            <div className="min-w-0">
              <div className="text-[13px] font-medium">{t.title}</div>
              {t.body && <div className="line-clamp-2 text-[12px] text-ink-2">{t.body}</div>}
            </div>
          </motion.div>
        ))}
      </AnimatePresence>
    </div>
  );
}
