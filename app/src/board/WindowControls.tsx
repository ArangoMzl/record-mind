import { getCurrentWindow } from "@tauri-apps/api/window";
import { Copy, Minus, Square, X } from "lucide-react";
import { useEffect, useState } from "react";

/** Minimizar, maximizar y cerrar propios: la ventana del tablero no tiene marco de Windows. */
export function WindowControls() {
  // Se pide en cada uso y no al pintar: fuera de Tauri (vista previa en el navegador) lanzaría y tumbaría el tablero.
  const win = () => getCurrentWindow();
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    let un: Promise<() => void> | undefined;
    try {
      const w = getCurrentWindow();
      const sync = () => w.isMaximized().then(setMaximized).catch(() => {});
      sync();
      // Maximizar con doble clic, Win+↑ o arrastrando al borde también cambia el tamaño.
      un = w.onResized(sync);
    } catch {
      // Sin Tauri no hay ventana que vigilar.
    }
    return () => {
      un?.then((f) => f()).catch(() => {});
    };
  }, []);

  const base = "grid size-8 place-items-center rounded-lg text-ink-3 transition hover:text-ink";
  return (
    <div className="-mr-2 flex shrink-0 items-center gap-0.5 border-l border-line pl-2">
      <button onClick={() => win().minimize()} title="Minimizar" className={`${base} hover:bg-white/[0.06]`}>
        <Minus size={15} strokeWidth={1.75} />
      </button>
      <button
        onClick={() => win().toggleMaximize()}
        title={maximized ? "Restaurar" : "Maximizar"}
        className={`${base} hover:bg-white/[0.06]`}
      >
        {maximized ? <Copy size={13} strokeWidth={1.75} className="-scale-x-100" /> : <Square size={12.5} strokeWidth={1.75} />}
      </button>
      {/* Cerrar no sale de la app: el tablero se esconde en la bandeja (ver on_window_event). */}
      <button onClick={() => win().close()} title="Cerrar (sigue en la bandeja)" className={`${base} hover:bg-red-500/90 hover:text-white`}>
        <X size={16} strokeWidth={1.75} />
      </button>
    </div>
  );
}
