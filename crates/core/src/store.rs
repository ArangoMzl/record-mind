use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::{
    Error, Result,
    model::{Attachment, MemoryLink, MemoryRelation, NewTask, Status, Task, TaskDetail, TaskEvent},
};

const NOW: &str = "strftime('%Y-%m-%dT%H:%M:%fZ','now')";

/// Migraciones en orden; `PRAGMA user_version` guarda cuántas se aplicaron.
const MIGRATIONS: &[&str] = &[r#"
CREATE TABLE tasks (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    project       TEXT    NOT NULL,
    title         TEXT    NOT NULL,
    note          TEXT    NOT NULL DEFAULT '',
    status        TEXT    NOT NULL DEFAULT 'inbox'
                  CHECK (status IN ('inbox','in_progress','review','done')),
    priority      INTEGER NOT NULL DEFAULT 0,
    position      REAL    NOT NULL DEFAULT 0,
    session_id    TEXT,
    resolution    TEXT,
    files_changed TEXT    NOT NULL DEFAULT '[]',
    created_at    TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at    TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE INDEX tasks_project_status ON tasks(project, status);

CREATE TABLE attachments (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id    INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    file       TEXT    NOT NULL,
    mime       TEXT    NOT NULL,
    created_at TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE INDEX attachments_task ON attachments(task_id);

CREATE TABLE task_events (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id    INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    kind       TEXT    NOT NULL,
    actor      TEXT    NOT NULL,
    message    TEXT    NOT NULL DEFAULT '',
    created_at TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE INDEX task_events_task ON task_events(task_id);

CREATE TABLE task_memories (
    task_id    INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    engram_ref TEXT    NOT NULL,
    relation   TEXT    NOT NULL CHECK (relation IN ('consulted','created')),
    created_at TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    PRIMARY KEY (task_id, engram_ref, relation)
);
"#,
    // 2: cola de la próxima pasada de /pendientes (las tarjetas que marcas en el tablero).
    r#"
ALTER TABLE tasks ADD COLUMN queued_at TEXT;
"#,
];

#[derive(Debug, Clone, Default)]
pub struct TaskFilter {
    pub project: Option<String>,
    /// Vacío = todos los estados.
    pub statuses: Vec<Status>,
    /// Solo las marcadas para la próxima pasada de Claude.
    pub queued_only: bool,
}

/// Base de tareas de record-mind (lectura y escritura).
pub struct Store {
    conn: Connection,
    root: PathBuf,
}

impl Store {
    /// Abre (o crea) la base en `root/record-mind.db`.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        std::fs::create_dir_all(&root)?;
        let conn = Connection::open(root.join("record-mind.db"))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        let mut store = Store { conn, root };
        store.migrate()?;
        Ok(store)
    }

    pub fn open_default() -> Result<Self> {
        let root = crate::paths::data_dir();
        if std::env::var_os("RECORD_MIND_HOME").is_none() {
            if let Some(legacy) = crate::paths::legacy_data_dir() {
                migrate_legacy(&legacy, &root)?;
            }
        }
        Self::open(root)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn migrate(&mut self) -> Result<()> {
        let version: i64 = self.conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
            let tx = self.conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", (i + 1) as i64)?;
            tx.commit()?;
        }
        Ok(())
    }

    // ── Escritura ────────────────────────────────────────────────────────

    pub fn create_task(&mut self, new: NewTask, actor: &str) -> Result<Task> {
        let note = new.note.trim().to_string();
        let title = new
            .title
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| derive_title(&note));
        if title.is_empty() {
            return Err(Error::Invalid("la tarea necesita un título o una nota".into()));
        }
        let project = new.project.trim().to_string();
        if project.is_empty() {
            return Err(Error::Invalid("la tarea necesita un proyecto".into()));
        }
        for src in &new.attachments {
            if !src.is_file() {
                return Err(Error::Invalid(format!("no existe la imagen {}", src.display())));
            }
        }

        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO tasks (project, title, note, priority, session_id, position)
             VALUES (?1, ?2, ?3, ?4, ?5, (SELECT COALESCE(MAX(position), 0) + 1 FROM tasks))",
            params![project, title, note, new.priority, new.session_id],
        )?;
        let id = tx.last_insert_rowid();
        insert_event(&tx, id, "created", actor, "")?;
        tx.commit()?;

        for src in &new.attachments {
            self.add_attachment_file(id, src)?;
        }
        self.task(id)
    }

    /// Copia una imagen a `attachments/<tarea>/` y la registra.
    pub fn add_attachment_file(&mut self, task_id: i64, src: &Path) -> Result<Attachment> {
        let bytes = std::fs::read(src)?;
        let ext = src
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("png")
            .to_ascii_lowercase();
        self.add_attachment_bytes(task_id, &bytes, &ext)
    }

    /// Guarda bytes de una imagen (p. ej. pegada del portapapeles).
    pub fn add_attachment_bytes(&mut self, task_id: i64, bytes: &[u8], ext: &str) -> Result<Attachment> {
        self.ensure_exists(task_id)?;
        let mime = mime_for(ext).ok_or_else(|| Error::Invalid(format!("formato de imagen no soportado: .{ext}")))?;
        let dir = self.root.join("attachments").join(task_id.to_string());
        std::fs::create_dir_all(&dir)?;
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM attachments WHERE task_id = ?1", [task_id], |r| r.get(0))?;
        let rel = format!("attachments/{task_id}/{}.{ext}", n + 1);
        std::fs::write(self.abs(&rel), bytes)?;
        self.conn.execute(
            "INSERT INTO attachments (task_id, file, mime) VALUES (?1, ?2, ?3)",
            params![task_id, rel, mime],
        )?;
        let id = self.conn.last_insert_rowid();
        self.touch(task_id)?;
        let created_at = self
            .conn
            .query_row("SELECT created_at FROM attachments WHERE id = ?1", [id], |r| r.get(0))?;
        Ok(Attachment { id, task_id, path: self.abs(&rel), mime: mime.into(), created_at })
    }

    /// Ruta absoluta de un archivo guardado con ruta relativa portable (`a/b/c.png`).
    fn abs(&self, rel: &str) -> PathBuf {
        rel.split('/').fold(self.root.clone(), |p, part| p.join(part))
    }

    /// Claude toma la tarea: Capturado → Claude trabajando.
    pub fn claim(&mut self, id: i64, actor: &str) -> Result<Task> {
        let from = self.status_of(id)?;
        // Al tomarla sale de la cola.
        self.conn.execute("UPDATE tasks SET queued_at = NULL WHERE id = ?1", [id])?;
        match from {
            Status::Inbox => self.set_status(id, Status::InProgress, "claimed", actor, "")?,
            // Ya estaba en curso (movida a mano antes de esta regla, o la retoma otra sesión):
            // se registra quién la tiene ahora.
            Status::InProgress => {
                insert_event(&self.conn, id, "claimed", actor, "")?;
                self.touch(id)?;
            }
            to_err => return Err(Error::InvalidTransition { id, from: to_err, to: Status::InProgress }),
        }
        self.task(id)
    }

    /// Claude terminó: pasa a revisión con su resumen.
    pub fn complete(
        &mut self,
        id: i64,
        actor: &str,
        summary: &str,
        files_changed: &[String],
        memories_created: &[String],
        memories_consulted: &[String],
    ) -> Result<Task> {
        let from = self.status_of(id)?;
        if !matches!(from, Status::Inbox | Status::InProgress) {
            return Err(Error::InvalidTransition { id, from, to: Status::Review });
        }
        let summary = summary.trim();
        if summary.is_empty() {
            return Err(Error::Invalid("falta el resumen de lo que se hizo".into()));
        }
        let tx = self.conn.transaction()?;
        tx.execute(
            &format!(
                "UPDATE tasks SET status = 'review', resolution = ?2, files_changed = ?3, updated_at = {NOW}
                 WHERE id = ?1"
            ),
            params![id, summary, serde_json::to_string(files_changed)?],
        )?;
        for (refs, rel) in [
            (memories_created, MemoryRelation::Created),
            (memories_consulted, MemoryRelation::Consulted),
        ] {
            for r in refs.iter().map(|r| r.trim()).filter(|r| !r.is_empty()) {
                tx.execute(
                    "INSERT OR IGNORE INTO task_memories (task_id, engram_ref, relation) VALUES (?1, ?2, ?3)",
                    params![id, r, rel.as_str()],
                )?;
            }
        }
        insert_event(&tx, id, "completed", actor, summary)?;
        tx.commit()?;
        self.task(id)
    }

    /// Una persona aprueba: Para revisar → Hecho.
    pub fn approve(&mut self, id: i64, actor: &str) -> Result<Task> {
        let from = self.status_of(id)?;
        if from != Status::Review {
            return Err(Error::InvalidTransition { id, from, to: Status::Done });
        }
        self.set_status(id, Status::Done, "approved", actor, "")?;
        self.task(id)
    }

    /// Una persona devuelve la tarea a Capturado con una nota de lo que falta.
    pub fn reopen(&mut self, id: i64, actor: &str, note: &str) -> Result<Task> {
        let from = self.status_of(id)?;
        if from == Status::Inbox {
            return Err(Error::InvalidTransition { id, from, to: Status::Inbox });
        }
        self.set_status(id, Status::Inbox, "returned", actor, note.trim())?;
        self.task(id)
    }

    /// Movimiento manual (arrastrar en el tablero). «Claude trabajando» queda reservado a
    /// `claim` (lo hace el agente) y para sacar una tarea de ahí se usa `release`.
    pub fn move_to(&mut self, id: i64, to: Status, actor: &str) -> Result<Task> {
        let from = self.status_of(id)?;
        if from == to {
            return self.task(id);
        }
        if to == Status::InProgress {
            return Err(Error::Invalid(
                "solo Claude pasa una tarea a «Claude trabajando» (cuando la toma con /pendientes)".into(),
            ));
        }
        if from == Status::InProgress {
            return Err(Error::Invalid(
                "la tarea está en curso; usa «Liberar» para devolverla a Capturado".into(),
            ));
        }
        self.set_status(id, to, "moved", actor, &format!("{} → {}", from.as_str(), to.as_str()))?;
        self.task(id)
    }

    /// Marca (o desmarca) una tarea capturada para la próxima pasada de `/pendientes`.
    pub fn set_queued(&mut self, id: i64, queued: bool) -> Result<Task> {
        let from = self.status_of(id)?;
        if queued && from != Status::Inbox {
            return Err(Error::Invalid(format!(
                "solo las tareas en «Capturado» se pueden poner en cola (la #{id} está en «{from}»)"
            )));
        }
        self.conn.execute(
            &format!("UPDATE tasks SET queued_at = CASE WHEN ?2 THEN COALESCE(queued_at, {NOW}) END WHERE id = ?1"),
            params![id, queued],
        )?;
        self.task(id)
    }

    /// Vacía la cola (de un proyecto o de todos). Devuelve cuántas tareas se desmarcaron.
    pub fn clear_queue(&mut self, project: Option<&str>) -> Result<usize> {
        Ok(self.conn.execute(
            "UPDATE tasks SET queued_at = NULL WHERE queued_at IS NOT NULL AND (?1 IS NULL OR project = ?1)",
            [project],
        )?)
    }

    /// Saca una tarea de «Claude trabajando» y la devuelve a Capturado (p. ej. si la sesión
    /// de Claude se cortó a mitad de camino).
    pub fn release(&mut self, id: i64, actor: &str, reason: &str) -> Result<Task> {
        let from = self.status_of(id)?;
        if from != Status::InProgress {
            return Err(Error::InvalidTransition { id, from, to: Status::Inbox });
        }
        self.set_status(id, Status::Inbox, "released", actor, reason.trim())?;
        self.task(id)
    }

    pub fn comment(&mut self, id: i64, actor: &str, message: &str) -> Result<TaskEvent> {
        self.ensure_exists(id)?;
        let message = message.trim();
        if message.is_empty() {
            return Err(Error::Invalid("el comentario está vacío".into()));
        }
        insert_event(&self.conn, id, "comment", actor, message)?;
        let ev_id = self.conn.last_insert_rowid();
        self.touch(id)?;
        self.conn
            .query_row(
                "SELECT id, task_id, kind, actor, message, created_at FROM task_events WHERE id = ?1",
                [ev_id],
                event_from_row,
            )
            .map_err(Into::into)
    }

    pub fn delete(&mut self, id: i64) -> Result<()> {
        self.ensure_exists(id)?;
        self.conn.execute("DELETE FROM tasks WHERE id = ?1", [id])?;
        let dir = self.root.join("attachments").join(id.to_string());
        if dir.is_dir() {
            std::fs::remove_dir_all(dir)?;
        }
        Ok(())
    }

    // ── Lectura ──────────────────────────────────────────────────────────

    pub fn task(&self, id: i64) -> Result<Task> {
        let mut task = self
            .conn
            .query_row(&format!("{TASK_SELECT} WHERE id = ?1"), [id], task_from_row)
            .optional()?
            .ok_or(Error::NotFound(id))?;
        task.attachments = self.attachments(id)?;
        Ok(task)
    }

    pub fn detail(&self, id: i64) -> Result<TaskDetail> {
        let task = self.task(id)?;
        let mut stmt = self.conn.prepare(
            "SELECT id, task_id, kind, actor, message, created_at FROM task_events
             WHERE task_id = ?1 ORDER BY id",
        )?;
        let events = stmt.query_map([id], event_from_row)?.collect::<rusqlite::Result<_>>()?;
        let mut stmt = self.conn.prepare(
            "SELECT engram_ref, relation, created_at FROM task_memories WHERE task_id = ?1 ORDER BY created_at",
        )?;
        let memories = stmt
            .query_map([id], |r| {
                let rel: String = r.get(1)?;
                Ok(MemoryLink {
                    engram_ref: r.get(0)?,
                    relation: if rel == "created" { MemoryRelation::Created } else { MemoryRelation::Consulted },
                    created_at: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(TaskDetail { task, events, memories })
    }

    pub fn list(&self, filter: &TaskFilter) -> Result<Vec<Task>> {
        let mut sql = format!("{TASK_SELECT} WHERE 1 = 1");
        let mut args: Vec<String> = Vec::new();
        if let Some(p) = &filter.project {
            args.push(p.clone());
            sql.push_str(&format!(" AND project = ?{}", args.len()));
        }
        if filter.queued_only {
            sql.push_str(" AND queued_at IS NOT NULL");
        }
        if !filter.statuses.is_empty() {
            let marks: Vec<String> = filter
                .statuses
                .iter()
                .map(|s| {
                    args.push(s.as_str().to_string());
                    format!("?{}", args.len())
                })
                .collect();
            sql.push_str(&format!(" AND status IN ({})", marks.join(",")));
        }
        sql.push_str(
            " ORDER BY CASE status WHEN 'in_progress' THEN 0 WHEN 'inbox' THEN 1 WHEN 'review' THEN 2 ELSE 3 END,
              priority DESC, position, id",
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let mut tasks: Vec<Task> = stmt
            .query_map(rusqlite::params_from_iter(args.iter()), task_from_row)?
            .collect::<rusqlite::Result<_>>()?;
        for t in &mut tasks {
            t.attachments = self.attachments(t.id)?;
        }
        Ok(tasks)
    }

    /// Proyectos con tareas y cuántas hay en cada estado.
    pub fn projects(&self) -> Result<Vec<(String, [i64; 4])>> {
        let mut stmt = self.conn.prepare(
            "SELECT project,
                    SUM(status = 'inbox'), SUM(status = 'in_progress'), SUM(status = 'review'), SUM(status = 'done')
             FROM tasks GROUP BY project ORDER BY MAX(updated_at) DESC",
        )?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, [r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?])))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    // ── Internos ─────────────────────────────────────────────────────────

    fn attachments(&self, task_id: i64) -> Result<Vec<Attachment>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, task_id, file, mime, created_at FROM attachments WHERE task_id = ?1 ORDER BY id",
        )?;
        let rows = stmt
            .query_map([task_id], |r| {
                let file: String = r.get(2)?;
                Ok(Attachment {
                    id: r.get(0)?,
                    task_id: r.get(1)?,
                    path: self.abs(&file),
                    mime: r.get(3)?,
                    created_at: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    fn status_of(&self, id: i64) -> Result<Status> {
        let s: Option<String> = self
            .conn
            .query_row("SELECT status FROM tasks WHERE id = ?1", [id], |r| r.get(0))
            .optional()?;
        s.ok_or(Error::NotFound(id))?.parse()
    }

    fn ensure_exists(&self, id: i64) -> Result<()> {
        self.status_of(id).map(|_| ())
    }

    fn set_status(&mut self, id: i64, to: Status, kind: &str, actor: &str, message: &str) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            &format!("UPDATE tasks SET status = ?2, updated_at = {NOW} WHERE id = ?1"),
            params![id, to.as_str()],
        )?;
        insert_event(&tx, id, kind, actor, message)?;
        tx.commit()?;
        Ok(())
    }

    fn touch(&self, id: i64) -> Result<()> {
        self.conn
            .execute(&format!("UPDATE tasks SET updated_at = {NOW} WHERE id = ?1"), [id])?;
        Ok(())
    }
}

const TASK_SELECT: &str = "SELECT id, project, title, note, status, priority, session_id, resolution,
    files_changed, created_at, updated_at, queued_at IS NOT NULL FROM tasks";

fn task_from_row(r: &Row) -> rusqlite::Result<Task> {
    let status: String = r.get(4)?;
    let files: String = r.get(8)?;
    Ok(Task {
        id: r.get(0)?,
        project: r.get(1)?,
        title: r.get(2)?,
        note: r.get(3)?,
        status: status.parse().unwrap_or(Status::Inbox),
        priority: r.get(5)?,
        session_id: r.get(6)?,
        resolution: r.get(7)?,
        files_changed: serde_json::from_str(&files).unwrap_or_default(),
        attachments: Vec::new(),
        created_at: r.get(9)?,
        updated_at: r.get(10)?,
        queued: r.get(11)?,
    })
}

fn event_from_row(r: &Row) -> rusqlite::Result<TaskEvent> {
    Ok(TaskEvent {
        id: r.get(0)?,
        task_id: r.get(1)?,
        kind: r.get(2)?,
        actor: r.get(3)?,
        message: r.get(4)?,
        created_at: r.get(5)?,
    })
}

fn insert_event(conn: &Connection, task_id: i64, kind: &str, actor: &str, message: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO task_events (task_id, kind, actor, message) VALUES (?1, ?2, ?3, ?4)",
        params![task_id, kind, actor, message],
    )?;
    Ok(())
}

/// Copia los datos de la ubicación antigua a la nueva si la nueva todavía no tiene base.
/// La base se copia con `VACUUM INTO` (consistente aunque haya WAL pendiente) y las capturas
/// archivo por archivo. La ubicación antigua no se borra.
fn migrate_legacy(legacy: &Path, root: &Path) -> Result<()> {
    let old_db = legacy.join("record-mind.db");
    let new_db = root.join("record-mind.db");
    if new_db.exists() || !old_db.is_file() {
        return Ok(());
    }
    std::fs::create_dir_all(root)?;
    let old = Connection::open_with_flags(&old_db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    old.execute("VACUUM INTO ?1", [new_db.to_string_lossy()])?;
    copy_dir(&legacy.join("attachments"), &root.join("attachments"))?;
    tracing::info!("datos migrados de {} a {}", legacy.display(), root.display());
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    if !from.is_dir() {
        return Ok(());
    }
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let dest = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &dest)?;
        } else {
            std::fs::copy(entry.path(), dest)?;
        }
    }
    Ok(())
}

/// Primera línea de la nota, recortada a 80 caracteres.
fn derive_title(note: &str) -> String {
    let line = note.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    if line.chars().count() <= 80 {
        line.to_string()
    } else {
        let cut: String = line.chars().take(79).collect();
        format!("{}…", cut.trim_end())
    }
}

pub fn mime_for(ext: &str) -> Option<&'static str> {
    Some(match ext.to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> (Store, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "rm-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        (Store::open(&dir).unwrap(), dir)
    }

    #[test]
    fn ciclo_completo_de_una_tarea() {
        let (mut s, dir) = temp_store();
        let img = dir.join("captura.png");
        std::fs::write(&img, b"\x89PNG fake").unwrap();

        let t = s
            .create_task(
                NewTask {
                    project: "demo".into(),
                    note: "Subir el tamaño del título\nse ve muy pequeño en móvil".into(),
                    attachments: vec![img],
                    ..Default::default()
                },
                "user",
            )
            .unwrap();
        assert_eq!(t.title, "Subir el tamaño del título");
        assert_eq!(t.status, Status::Inbox);
        assert_eq!(t.attachments.len(), 1);
        assert!(t.attachments[0].path.is_file());

        // No se puede aprobar algo que no está en revisión.
        assert!(matches!(s.approve(t.id, "user"), Err(Error::InvalidTransition { .. })));

        assert_eq!(s.claim(t.id, "claude").unwrap().status, Status::InProgress);
        let done = s
            .complete(t.id, "claude", "font-size 14→20px", &["src/app.css".into()], &["obs-1".into()], &[])
            .unwrap();
        assert_eq!(done.status, Status::Review);
        assert_eq!(done.files_changed, vec!["src/app.css".to_string()]);

        let back = s.reopen(t.id, "user", "también en el subtítulo").unwrap();
        assert_eq!(back.status, Status::Inbox);

        // Cola: solo en Capturado; tomarla la saca de la cola.
        assert!(s.set_queued(t.id, true).unwrap().queued);
        let queued = s.list(&TaskFilter { queued_only: true, ..Default::default() }).unwrap();
        assert_eq!(queued.len(), 1);

        // «Claude trabajando» solo se alcanza con claim, y solo se sale con release.
        assert!(matches!(s.move_to(t.id, Status::InProgress, "user"), Err(Error::Invalid(_))));
        assert!(!s.claim(t.id, "claude").unwrap().queued);
        assert!(matches!(s.set_queued(t.id, true), Err(Error::Invalid(_))));
        assert!(matches!(s.move_to(t.id, Status::Done, "user"), Err(Error::Invalid(_))));
        assert_eq!(s.release(t.id, "user", "la sesión se cortó").unwrap().status, Status::Inbox);

        let d = s.detail(t.id).unwrap();
        let kinds: Vec<_> = d.events.iter().map(|e| e.kind.as_str()).collect();
        assert_eq!(kinds, ["created", "claimed", "completed", "returned", "claimed", "released"]);
        assert_eq!(d.memories.len(), 1);

        let list = s.list(&TaskFilter { project: Some("demo".into()), statuses: vec![Status::Inbox], ..Default::default() }).unwrap();
        assert_eq!(list.len(), 1);

        s.delete(t.id).unwrap();
        assert!(matches!(s.task(t.id), Err(Error::NotFound(_))));
        let _ = std::fs::remove_dir_all(dir);
    }
}
