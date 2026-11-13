//! Detecta escrituras en record-mind.db desde cualquier proceso (MCP por stdio, CLI, la app)
//! con `PRAGMA data_version`, que cambia cada vez que otra conexión confirma una transacción.

use std::{path::PathBuf, time::Duration};

use rusqlite::{Connection, OpenFlags};
use tauri::{AppHandle, Emitter};

pub fn spawn(app: AppHandle, db: PathBuf) {
    std::thread::Builder::new()
        .name("rm-watcher".into())
        .spawn(move || {
            let conn = match Connection::open_with_flags(&db, OpenFlags::SQLITE_OPEN_READ_ONLY) {
                Ok(c) => c,
                Err(e) => return tracing::error!("vigilante: no se pudo abrir {}: {e}", db.display()),
            };
            let version = |c: &Connection| c.query_row("PRAGMA data_version", [], |r| r.get::<_, i64>(0)).ok();
            let mut last = version(&conn);
            loop {
                std::thread::sleep(Duration::from_millis(400));
                let now = version(&conn);
                if now != last {
                    last = now;
                    let _ = app.emit("tasks:changed", ());
                }
            }
        })
        .expect("no se pudo iniciar el vigilante");
}
