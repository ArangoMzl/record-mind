//! Detecta escrituras en record-mind.db desde cualquier proceso (MCP por stdio, CLI, la app)
//! con `PRAGMA data_version`, que cambia cada vez que otra conexión confirma una transacción.
//! También vigila la carpeta de decisiones: cualquier .md nuevo, cambiado o borrado refresca.

use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use rusqlite::{Connection, OpenFlags};
use tauri::{AppHandle, Emitter};

/// La carpeta de decisiones se revisa cada tantas vueltas (cada ~1,2 s): es leer unos pocos metadatos.
const DECISIONS_EVERY: u32 = 3;

pub fn spawn(app: AppHandle, db: PathBuf, decisions: PathBuf) {
    std::thread::Builder::new()
        .name("rm-watcher".into())
        .spawn(move || {
            let conn = match Connection::open_with_flags(&db, OpenFlags::SQLITE_OPEN_READ_ONLY) {
                Ok(c) => c,
                Err(e) => return tracing::error!("vigilante: no se pudo abrir {}: {e}", db.display()),
            };
            let version = |c: &Connection| c.query_row("PRAGMA data_version", [], |r| r.get::<_, i64>(0)).ok();
            let mut last = version(&conn);
            let mut last_md = signature(&decisions);
            let mut tick = 0u32;
            loop {
                std::thread::sleep(Duration::from_millis(400));
                let now = version(&conn);
                let mut changed = now != last;
                last = now;
                tick = tick.wrapping_add(1);
                if tick.is_multiple_of(DECISIONS_EVERY) {
                    let md = signature(&decisions);
                    changed |= md != last_md;
                    last_md = md;
                }
                if changed {
                    let _ = app.emit("tasks:changed", ());
                }
            }
        })
        .expect("no se pudo iniciar el vigilante");
}

/// Huella de la carpeta: cada .md con su tamaño y fecha de modificación.
fn signature(root: &Path) -> Vec<(PathBuf, u64, Option<SystemTime>)> {
    let mut out = Vec::new();
    for dir in std::fs::read_dir(root).into_iter().flatten().flatten() {
        for file in std::fs::read_dir(dir.path()).into_iter().flatten().flatten() {
            let path = file.path();
            if path.extension().is_some_and(|e| e == "md") {
                let meta = file.metadata().ok();
                out.push((path, meta.as_ref().map_or(0, |m| m.len()), meta.and_then(|m| m.modified().ok())));
            }
        }
    }
    out.sort();
    out
}
