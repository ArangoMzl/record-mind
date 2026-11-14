import { AlertTriangle, Check, Keyboard, RotateCcw, Settings, X } from "lucide-react";
import { motion } from "motion/react";
import { useCallback, useEffect, useRef, useState } from "react";
import { api, shortcutLabel, type ShortcutAction, type ShortcutsView } from "../lib/api";

const MODIFIER_CODES = /^(Control|Shift|Alt|Meta|OS)(Left|Right)?$/;
const MODIFIER_KEYS = new Set(["Control", "Shift", "Alt", "AltGraph", "Meta", "OS"]);

/**
 * Modificadores pulsados. En distribuciones con AltGr (español, inglés internacional…) Windows
 * convierte Ctrl+Alt en AltGr y el navegador avisa con `AltGraph` y `ctrlKey`/`altKey` en falso:
 * se cuenta como Ctrl+Alt, que es lo que el atajo global recibe de verdad.
 */
function modifiers(e: KeyboardEvent) {
  const altGr = e.getModifierState?.("AltGraph") ?? false;
  return { ctrl: e.ctrlKey || altGr, alt: e.altKey || altGr, shift: e.shiftKey, meta: e.metaKey };
}

function heldLabels(e: KeyboardEvent): string[] {
  const m = modifiers(e);
  return [m.ctrl && "Ctrl", m.alt && "Alt", m.shift && "⇧", m.meta && "Win"].filter(Boolean) as string[];
}

/** Teclas pulsadas → acelerador de Tauri. `null` si solo hay modificadores todavía. */
function toAccelerator(e: KeyboardEvent): { accel: string } | { error: string } | null {
  // Un modificador solo no termina la grabación. Se mira `key` además de `code`: hay
  // teclados y entradas sintéticas que dejan `code` vacío.
  if (MODIFIER_KEYS.has(e.key) || MODIFIER_CODES.test(e.code)) return null;
  // Sin código físico no se puede formar el atajo: se ignora y se sigue esperando.
  if (!e.code || e.code === "Unidentified") return null;
  const { ctrl, alt, shift, meta } = modifiers(e);
  const parts: string[] = [];
  if (ctrl) parts.push("CommandOrControl");
  if (alt) parts.push("Alt");
  if (shift) parts.push("Shift");
  if (meta) parts.push("Super");
  const fKey = /^F([1-9]|1\d|2[0-4])$/.test(e.code);
  // Sin Ctrl, Alt o Win, el atajo se comería una tecla normal al escribir en cualquier app.
  if (!fKey && !ctrl && !alt && !meta) {
    return { error: "Usa al menos Ctrl, Alt o Win (o una tecla F1–F24)" };
  }
  parts.push(e.code);
  return { accel: parts.join("+") };
}

function Keys({ accel, muted }: { accel: string; muted?: boolean }) {
  return (
    <span className="flex items-center gap-1">
      {shortcutLabel(accel).map((k, i) => (
        <span key={i} className={`kbd ${muted ? "opacity-60" : "text-ink"}`}>
          {k}
        </span>
      ))}
    </span>
  );
}

/** Diálogo de ajustes: por ahora, los atajos globales. */
export function SettingsDialog(props: { onClose: () => void; onChanged: () => void }) {
  const [view, setView] = useState<ShortcutsView | null>(null);
  const [recording, setRecording] = useState<ShortcutAction | null>(null);
  const [held, setHeld] = useState<string[]>([]);
  const [errors, setErrors] = useState<Partial<Record<ShortcutAction, string>>>({});
  const [saved, setSaved] = useState<ShortcutAction | null>(null);
  const recordingRef = useRef<ShortcutAction | null>(null);

  useEffect(() => {
    api.getShortcuts().then(setView);
  }, []);

  const stopRecording = useCallback(() => {
    if (!recordingRef.current) return;
    recordingRef.current = null;
    setRecording(null);
    setHeld([]);
    api.pauseShortcuts(false);
  }, []);

  const startRecording = (action: ShortcutAction) => {
    recordingRef.current = action;
    setRecording(action);
    setHeld([]);
    setErrors((e) => ({ ...e, [action]: undefined }));
    setSaved(null);
    // Si no, pulsar el atajo actual abriría la captura en vez de llegar aquí.
    api.pauseShortcuts(true);
  };

  const apply = useCallback(
    async (action: ShortcutAction, accel: string | null) => {
      try {
        const v = await api.setShortcut(action, accel);
        setView(v);
        setErrors((e) => ({ ...e, [action]: undefined }));
        setSaved(action);
        props.onChanged();
      } catch (err) {
        setErrors((e) => ({ ...e, [action]: String(err) }));
      }
    },
    [props],
  );

  // Teclado: grabar el atajo o, sin grabar, Esc cierra el diálogo.
  useEffect(() => {
    const onDown = (e: KeyboardEvent) => {
      const action = recordingRef.current;
      if (!action) {
        if (e.key === "Escape") {
          e.stopPropagation();
          props.onClose();
        }
        return;
      }
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape" && !e.ctrlKey && !e.altKey && !e.shiftKey && !e.metaKey) return stopRecording();
      const r = toAccelerator(e);
      if (!r) {
        setHeld(heldLabels(e));
        return;
      }
      if ("error" in r) {
        setErrors((x) => ({ ...x, [action]: r.error }));
        return;
      }
      stopRecording();
      apply(action, r.accel);
    };
    const onUp = (e: KeyboardEvent) => {
      if (!recordingRef.current) return;
      setHeld(heldLabels(e));
    };
    // Si la ventana pierde el foco a mitad, se cancela y los atajos vuelven.
    const onBlur = () => stopRecording();
    window.addEventListener("keydown", onDown, true);
    window.addEventListener("keyup", onUp, true);
    window.addEventListener("blur", onBlur);
    return () => {
      window.removeEventListener("keydown", onDown, true);
      window.removeEventListener("keyup", onUp, true);
      window.removeEventListener("blur", onBlur);
    };
  }, [apply, props, stopRecording]);

  // Nunca dejar los atajos apagados al cerrar.
  useEffect(() => () => void api.pauseShortcuts(false), []);

  const rows: { action: ShortcutAction; title: string; hint: string }[] = [
    { action: "capture", title: "Capturar pendiente", hint: "Abre «Nuevo pendiente» desde cualquier app." },
    { action: "snip", title: "Recortar pantalla", hint: "Congela la pantalla y recortas directo, sin abrir antes la captura." },
  ];

  return (
    <motion.div
      className="fixed inset-0 z-[55] grid place-items-center bg-black/55 p-6 backdrop-blur-[2px]"
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      onMouseDown={(e) => e.target === e.currentTarget && props.onClose()}
    >
      <motion.div
        initial={{ opacity: 0, y: 10, scale: 0.98 }}
        animate={{ opacity: 1, y: 0, scale: 1 }}
        exit={{ opacity: 0, y: 10, scale: 0.98 }}
        transition={{ type: "spring", stiffness: 420, damping: 34 }}
        className="w-[560px] max-w-full rounded-2xl border border-line-strong bg-surface shadow-2xl shadow-black/60"
      >
        <header className="flex items-center gap-2 border-b border-line px-5 py-3.5">
          <Settings size={15} className="text-accent" />
          <h2 className="text-[14px] font-semibold">Ajustes</h2>
          <div className="flex-1" />
          <button onClick={props.onClose} className="rounded-lg p-1.5 text-ink-3 hover:bg-white/5 hover:text-ink" title="Cerrar (Esc)">
            <X size={15} />
          </button>
        </header>

        <div className="px-5 py-4">
          <div className="eyebrow mb-2 flex items-center gap-1.5">
            <Keyboard size={12} /> Atajos de teclado
          </div>
          <div className="divide-y divide-line rounded-xl border border-line-strong bg-base">
            {rows.map(({ action, title, hint }) => {
              const accel = view ? (action === "capture" ? view.capture : view.snip) : null;
              const startupError = view ? (action === "capture" ? view.capture_error : view.snip_error) : null;
              const error = errors[action] ?? (recording !== action ? startupError : null);
              const isRec = recording === action;
              return (
                <div key={action} className="px-4 py-3">
                  <div className="flex items-center gap-3">
                    <div className="min-w-0 flex-1">
                      <div className="text-[13.5px] font-medium">{title}</div>
                      <div className="text-[12px] text-ink-3">{hint}</div>
                    </div>
                    <button
                      onClick={() => (isRec ? stopRecording() : startRecording(action))}
                      disabled={!view || (recording !== null && !isRec)}
                      title={isRec ? "Esc para cancelar" : "Clic y pulsa la nueva combinación"}
                      className={`flex min-w-[168px] items-center justify-center gap-2 rounded-xl border px-3 py-2 text-[12.5px] transition disabled:opacity-40 ${
                        isRec
                          ? "border-accent/60 bg-accent-soft text-ink shadow-[0_0_18px_var(--color-accent-glow)]"
                          : "border-line-strong bg-white/[0.03] hover:border-white/25"
                      }`}
                    >
                      {isRec ? (
                        held.length ? (
                          <span className="flex items-center gap-1">
                            {held.map((k) => (
                              <span key={k} className="kbd text-ink">
                                {k}
                              </span>
                            ))}
                            <span className="text-ink-3">+ …</span>
                          </span>
                        ) : (
                          <span className="text-accent">Pulsa la combinación…</span>
                        )
                      ) : accel ? (
                        <Keys accel={accel} />
                      ) : (
                        <span className="text-ink-3">Sin atajo</span>
                      )}
                    </button>
                    <div className="flex w-[30px] justify-center">
                      {action === "capture" && view && view.capture !== view.default_capture && !isRec && (
                        <button
                          onClick={() => apply("capture", view.default_capture)}
                          title={`Volver a ${shortcutLabel(view.default_capture).join(" ")}`}
                          className="rounded-lg p-1.5 text-ink-3 hover:bg-white/5 hover:text-ink"
                        >
                          <RotateCcw size={14} />
                        </button>
                      )}
                      {action === "snip" && view?.snip && !isRec && (
                        <button onClick={() => apply("snip", null)} title="Quitar atajo" className="rounded-lg p-1.5 text-ink-3 hover:bg-white/5 hover:text-ink">
                          <X size={14} />
                        </button>
                      )}
                    </div>
                  </div>
                  {error && (
                    <p className="mt-2 flex items-center gap-1.5 text-[12px] text-red-300">
                      <AlertTriangle size={12} className="shrink-0" /> {error}
                    </p>
                  )}
                  {saved === action && !error && (
                    <p className="mt-2 flex items-center gap-1.5 text-[12px] text-st-done">
                      <Check size={12} /> Guardado: ya funciona en todo Windows.
                    </p>
                  )}
                </div>
              );
            })}
          </div>
          <p className="mt-3 text-[11.5px] text-ink-3">
            Haz clic en un atajo y pulsa la nueva combinación. Funcionan en cualquier app mientras record-mind esté en la bandeja.
          </p>
        </div>
      </motion.div>
    </motion.div>
  );
}
