//! Ajustes de la app, en `settings.json` dentro de la carpeta de datos. Un archivo que falta o
//! no se puede leer deja los valores por defecto: nunca impide arrancar.

use std::path::Path;

use serde::{Deserialize, Serialize};

pub const FILE: &str = "settings.json";
pub const DEFAULT_CAPTURE_SHORTCUT: &str = "CommandOrControl+Shift+Space";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Settings {
    pub shortcuts: Shortcuts,
}

/// Atajos globales, en formato de acelerador (`CommandOrControl+Shift+Space`, `Alt+KeyS`…).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Shortcuts {
    /// Abre la ventana de captura. Siempre hay uno.
    pub capture: String,
    /// Recorta la pantalla directamente. Opcional: sin él se recorta desde la captura o la bandeja.
    pub snip: Option<String>,
}

impl Default for Shortcuts {
    fn default() -> Self {
        Shortcuts { capture: DEFAULT_CAPTURE_SHORTCUT.into(), snip: None }
    }
}

impl Settings {
    pub fn load(dir: &Path) -> Settings {
        let path = dir.join(FILE);
        let Ok(text) = std::fs::read_to_string(&path) else { return Settings::default() };
        match serde_json::from_str::<Settings>(&text) {
            Ok(mut s) => {
                if s.shortcuts.capture.trim().is_empty() {
                    s.shortcuts.capture = DEFAULT_CAPTURE_SHORTCUT.into();
                }
                s
            }
            Err(e) => {
                tracing::warn!("ajustes ilegibles en {}, se usan los de por defecto: {e}", path.display());
                Settings::default()
            }
        }
    }

    /// Escribe a un temporal y lo renombra: un corte a mitad no deja el archivo a medias.
    pub fn save(&self, dir: &Path) -> crate::Result<()> {
        std::fs::create_dir_all(dir)?;
        let tmp = dir.join(format!("{FILE}.tmp"));
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, dir.join(FILE))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guarda_y_lee_los_ajustes() {
        let dir = std::env::temp_dir().join(format!("rm-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        // Sin archivo: valores por defecto.
        assert_eq!(Settings::load(&dir), Settings::default());
        assert_eq!(Settings::default().shortcuts.capture, DEFAULT_CAPTURE_SHORTCUT);

        let s = Settings { shortcuts: Shortcuts { capture: "Alt+KeyQ".into(), snip: Some("CommandOrControl+Alt+KeyS".into()) } };
        s.save(&dir).unwrap();
        assert_eq!(Settings::load(&dir), s);
        assert!(!dir.join("settings.json.tmp").exists());

        // Campos que faltan o vacíos: se completan; JSON roto: por defecto.
        std::fs::write(dir.join(FILE), r#"{"shortcuts": {"capture": ""}}"#).unwrap();
        assert_eq!(Settings::load(&dir), Settings::default());
        std::fs::write(dir.join(FILE), "{}").unwrap();
        assert_eq!(Settings::load(&dir), Settings::default());
        std::fs::write(dir.join(FILE), "no es json").unwrap();
        assert_eq!(Settings::load(&dir), Settings::default());

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
