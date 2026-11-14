//! Atajos globales configurables: capturar pendiente (siempre) y recortar pantalla (opcional).
//! Se guardan en `settings.json` (rm-core) y se pueden cambiar con la app abierta.

use std::str::FromStr;

use rm_core::settings::{DEFAULT_CAPTURE_SHORTCUT, Settings, Shortcuts};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Wry, menu::MenuItem};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

use crate::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Capture,
    Snip,
}

#[derive(Default)]
pub struct ShortcutState {
    current: Shortcuts,
    /// Mientras se graba un atajo nuevo no hay ninguno registrado: si no, pulsar el actual
    /// abriría la captura en vez de llegar al grabador.
    paused: bool,
    capture_error: Option<String>,
    snip_error: Option<String>,
    /// Entradas de la bandeja que muestran el atajo.
    tray: Option<(MenuItem<Wry>, MenuItem<Wry>)>,
}

#[derive(Serialize)]
pub struct ShortcutsView {
    capture: String,
    snip: Option<String>,
    default_capture: &'static str,
    /// Por qué no se pudo registrar al arrancar (por ejemplo, otra app ya lo usa).
    capture_error: Option<String>,
    snip_error: Option<String>,
}

const IN_USE: &str = "Esa combinación ya la usa Windows u otra aplicación";

fn state(app: &AppHandle) -> std::sync::MutexGuard<'_, ShortcutState> {
    app.state::<AppState>().inner().shortcuts.lock().unwrap_or_else(|p| p.into_inner())
}

fn parse(accel: &str) -> Result<Shortcut, String> {
    Shortcut::from_str(accel).map_err(|_| format!("«{accel}» no es una combinación válida"))
}

/// Carga los atajos guardados y los registra. Llamar en `setup`, antes de crear la bandeja.
pub fn init(app: &AppHandle) {
    let root = app.state::<AppState>().store().root().to_path_buf();
    let current = Settings::load(&root).shortcuts;
    state(app).current = current;
    register_current(app);
}

/// Registra los atajos actuales y apunta los que fallan.
fn register_current(app: &AppHandle) {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let mut s = state(app);
    let try_register = |accel: &str| -> Option<String> {
        match parse(accel) {
            Err(e) => Some(e),
            Ok(sc) => gs.register(sc).err().map(|e| {
                tracing::warn!("no se pudo registrar el atajo {accel}: {e}");
                IN_USE.to_string()
            }),
        }
    };
    s.capture_error = try_register(&s.current.capture.clone());
    s.snip_error = s.current.snip.clone().and_then(|a| try_register(&a));
    s.paused = false;
}

/// Qué acción corresponde a un atajo pulsado.
pub fn action_for(app: &AppHandle, pressed: &Shortcut) -> Option<Action> {
    let s = state(app);
    if parse(&s.current.capture).is_ok_and(|sc| &sc == pressed) {
        return Some(Action::Capture);
    }
    if s.current.snip.as_deref().and_then(|a| parse(a).ok()).is_some_and(|sc| &sc == pressed) {
        return Some(Action::Snip);
    }
    None
}

pub fn view(app: &AppHandle) -> ShortcutsView {
    let s = state(app);
    ShortcutsView {
        capture: s.current.capture.clone(),
        snip: s.current.snip.clone(),
        default_capture: DEFAULT_CAPTURE_SHORTCUT,
        capture_error: s.capture_error.clone(),
        snip_error: s.snip_error.clone(),
    }
}

pub fn capture_shortcut(app: &AppHandle) -> String {
    state(app).current.capture.clone()
}

/// Cambia un atajo. Solo se guarda si Windows deja registrarlo; si no, se queda el anterior.
/// `None` quita el de recortar (el de capturar es obligatorio).
pub fn set(app: &AppHandle, action: Action, accel: Option<String>) -> Result<ShortcutsView, String> {
    let accel = accel.map(|a| a.trim().to_string()).filter(|a| !a.is_empty());
    let mut next = state(app).current.clone();
    match action {
        Action::Capture => next.capture = accel.clone().ok_or("El atajo de captura no se puede quitar")?,
        Action::Snip => next.snip = accel.clone(),
    }

    if let Some(a) = &accel {
        let new = parse(a)?;
        let other = match action {
            Action::Capture => next.snip.as_deref(),
            Action::Snip => Some(next.capture.as_str()),
        };
        if other.and_then(|o| parse(o).ok()).is_some_and(|o| o == new) {
            return Err(match action {
                Action::Capture => "Esa combinación ya es la de recortar pantalla".into(),
                Action::Snip => "Esa combinación ya es la de capturar pendiente".into(),
            });
        }
        // Prueba a registrarla sola antes de tocar nada.
        let gs = app.global_shortcut();
        let _ = gs.unregister_all();
        if gs.register(new).is_err() {
            register_current(app);
            return Err(IN_USE.into());
        }
        let _ = gs.unregister(new);
    }

    let root = app.state::<AppState>().store().root().to_path_buf();
    let mut settings = Settings::load(&root);
    settings.shortcuts = next.clone();
    settings.save(&root).map_err(|e| e.to_string())?;

    state(app).current = next;
    register_current(app);
    update_tray(app);
    Ok(view(app))
}

/// Pausa (o reanuda) todos los atajos mientras el diálogo de ajustes graba uno nuevo.
pub fn pause(app: &AppHandle, paused: bool) {
    if paused {
        let _ = app.global_shortcut().unregister_all();
        state(app).paused = true;
    } else if state(app).paused {
        register_current(app);
    }
}

/// Textos de la bandeja con los atajos actuales.
pub fn tray_labels(app: &AppHandle) -> (String, String) {
    let s = state(app);
    (
        format!("Capturar pendiente   {}", label(&s.current.capture)),
        match &s.current.snip {
            Some(a) => format!("Recortar pantalla   {}", label(a)),
            None => "Recortar pantalla".into(),
        },
    )
}

pub fn set_tray_items(app: &AppHandle, capture: MenuItem<Wry>, snip: MenuItem<Wry>) {
    state(app).tray = Some((capture, snip));
}

fn update_tray(app: &AppHandle) {
    let (capture_text, snip_text) = tray_labels(app);
    if let Some((capture, snip)) = state(app).tray.clone() {
        let _ = capture.set_text(capture_text);
        let _ = snip.set_text(snip_text);
    }
}

/// `CommandOrControl+Shift+KeyS` → `Ctrl+Shift+S`, para la bandeja.
pub fn label(accel: &str) -> String {
    accel
        .split('+')
        .map(|k| match k {
            "CommandOrControl" | "CmdOrCtrl" | "Control" | "Ctrl" => "Ctrl".to_string(),
            "Super" | "Command" | "Cmd" | "Meta" => "Win".to_string(),
            "Alt" | "Option" => "Alt".to_string(),
            "Space" => "Espacio".to_string(),
            k if k.len() == 4 && k.starts_with("Key") => k[3..].to_string(),
            k if k.len() == 6 && k.starts_with("Digit") => k[5..].to_string(),
            k => k.to_string(),
        })
        .collect::<Vec<_>>()
        .join("+")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn etiquetas_legibles() {
        assert_eq!(label("CommandOrControl+Shift+Space"), "Ctrl+Shift+Espacio");
        assert_eq!(label("Alt+KeyS"), "Alt+S");
        assert_eq!(label("Super+Shift+Digit4"), "Win+Shift+4");
        assert_eq!(label("F9"), "F9");
    }

    #[test]
    fn acepta_los_codigos_del_navegador() {
        for a in ["CommandOrControl+Shift+Space", "Alt+KeyS", "CommandOrControl+Alt+Digit1", "F9", "Shift+ArrowUp", "Alt+Numpad5"] {
            assert!(parse(a).is_ok(), "{a}");
        }
        assert!(parse("Ctrl+Nada").is_err());
        assert_eq!(parse("CommandOrControl+KeyS").unwrap(), parse("ctrl+s").unwrap());
    }
}
