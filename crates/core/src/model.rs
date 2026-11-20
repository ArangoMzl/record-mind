use std::{fmt, path::PathBuf, str::FromStr};

use serde::{Deserialize, Serialize};

/// Columnas del tablero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Capturado, esperando a Claude.
    Inbox,
    /// Claude lo está resolviendo.
    InProgress,
    /// Claude terminó; falta que una persona lo revise.
    Review,
    /// Aprobado por una persona.
    Done,
}

impl Status {
    pub const ALL: [Status; 4] = [Status::Inbox, Status::InProgress, Status::Review, Status::Done];

    pub fn as_str(self) -> &'static str {
        match self {
            Status::Inbox => "inbox",
            Status::InProgress => "in_progress",
            Status::Review => "review",
            Status::Done => "done",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Status::Inbox => "Capturado",
            Status::InProgress => "Claude trabajando",
            Status::Review => "Para revisar",
            Status::Done => "Hecho",
        }
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

impl FromStr for Status {
    type Err = crate::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().replace(['-', ' '], "_").as_str() {
            "inbox" | "todo" | "capturado" | "pendiente" => Ok(Status::Inbox),
            "in_progress" | "doing" | "progress" | "trabajando" => Ok(Status::InProgress),
            "review" | "revisar" | "revision" => Ok(Status::Review),
            "done" | "hecho" | "resuelto" => Ok(Status::Done),
            other => Err(crate::Error::Invalid(format!(
                "estado desconocido «{other}» (usa inbox, in_progress, review o done)"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub id: i64,
    pub task_id: i64,
    /// Ruta absoluta del archivo en disco.
    pub path: PathBuf,
    pub mime: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: i64,
    pub project: String,
    pub title: String,
    pub note: String,
    pub status: Status,
    pub priority: i64,
    /// Sesión de Engram/Claude Code en la que se capturó.
    pub session_id: Option<String>,
    /// Resumen de lo que hizo Claude al resolverla.
    pub resolution: Option<String>,
    pub files_changed: Vec<String>,
    pub attachments: Vec<Attachment>,
    pub created_at: String,
    pub updated_at: String,
    /// Marcada en el tablero para la próxima pasada de `/pendientes`.
    pub queued: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEvent {
    pub id: i64,
    pub task_id: i64,
    /// created, claimed, completed, approved, returned, comment, moved
    pub kind: String,
    /// "user", "claude" u otro agente
    pub actor: String,
    pub message: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryRelation {
    /// Claude la consultó para resolver la tarea.
    Consulted,
    /// Claude la guardó en Engram como resultado de la tarea.
    Created,
}

impl MemoryRelation {
    pub fn as_str(self) -> &'static str {
        match self {
            MemoryRelation::Consulted => "consulted",
            MemoryRelation::Created => "created",
        }
    }
}

/// Enlace entre una tarea y una memoria de Engram (por id o sync_id).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryLink {
    pub engram_ref: String,
    pub relation: MemoryRelation,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDetail {
    #[serde(flatten)]
    pub task: Task,
    pub events: Vec<TaskEvent>,
    pub memories: Vec<MemoryLink>,
}

impl TaskDetail {
    /// Desde cuándo está en su columna actual, según el historial. `updated_at` no sirve para
    /// esto: editarla, comentarla o marcarla para `/pendientes` también la toca. Volver a tomar
    /// una tarea que ya estaba en curso no cuenta como cambio.
    pub fn status_since(&self) -> &str {
        let mut current: Option<Status> = None;
        let mut since = self.task.created_at.as_str();
        for e in &self.events {
            let to = match e.kind.as_str() {
                "created" | "returned" | "released" => Some(Status::Inbox),
                "claimed" => Some(Status::InProgress),
                "completed" => Some(Status::Review),
                "approved" => Some(Status::Done),
                // El mensaje es «origen → destino».
                "moved" => e.message.rsplit('→').next().and_then(|s| s.parse().ok()),
                _ => None,
            };
            if let Some(to) = to.filter(|&to| current != Some(to)) {
                current = Some(to);
                since = &e.created_at;
            }
        }
        // Historial incompleto (datos antiguos o tocados a mano): lo mejor que hay es el último cambio.
        if current != Some(self.task.status) {
            return &self.task.updated_at;
        }
        since
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(kind: &str, message: &str, at: &str) -> TaskEvent {
        TaskEvent { id: 0, task_id: 1, kind: kind.into(), actor: "user".into(), message: message.into(), created_at: at.into() }
    }

    fn detail(status: Status, events: Vec<TaskEvent>) -> TaskDetail {
        TaskDetail {
            task: Task {
                id: 1,
                project: "demo".into(),
                title: "t".into(),
                note: String::new(),
                status,
                priority: 0,
                session_id: None,
                resolution: None,
                files_changed: Vec::new(),
                attachments: Vec::new(),
                created_at: "2026-11-10T08:00:00.000Z".into(),
                updated_at: "2026-11-19T09:00:00.000Z".into(),
                queued: false,
            },
            events,
            memories: Vec::new(),
        }
    }

    #[test]
    fn status_since_ignora_ediciones_y_comentarios() {
        let d = detail(
            Status::Done,
            vec![
                event("created", "", "2026-11-10T08:00:00.000Z"),
                event("claimed", "", "2026-11-11T08:00:00.000Z"),
                event("comment", "voy", "2026-11-11T08:05:00.000Z"),
                event("claimed", "", "2026-11-11T09:00:00.000Z"),
                event("completed", "listo", "2026-11-12T08:00:00.000Z"),
                event("approved", "", "2026-11-13T08:00:00.000Z"),
                // Editada días después de aprobarla: sigue siendo de ese día.
                event("edited", "título", "2026-11-19T09:00:00.000Z"),
            ],
        );
        assert_eq!(d.status_since(), "2026-11-13T08:00:00.000Z");

        let mut d = d;
        d.task.status = Status::InProgress;
        d.events.truncate(4);
        // Volver a tomarla no reinicia el reloj.
        assert_eq!(d.status_since(), "2026-11-11T08:00:00.000Z");
    }

    #[test]
    fn status_since_con_movimientos_a_mano() {
        let d = detail(
            Status::Review,
            vec![
                event("created", "", "2026-11-10T08:00:00.000Z"),
                event("moved", "inbox → review", "2026-11-14T08:00:00.000Z"),
                event("moved", "review → done", "2026-11-15T08:00:00.000Z"),
                event("moved", "done → review", "2026-11-16T08:00:00.000Z"),
            ],
        );
        assert_eq!(d.status_since(), "2026-11-16T08:00:00.000Z");
        // Sin historial que cuadre, el último cambio.
        assert_eq!(detail(Status::Done, vec![]).status_since(), "2026-11-19T09:00:00.000Z");
    }
}

#[derive(Debug, Clone, Default)]
pub struct NewTask {
    pub project: String,
    /// Si va vacío se deriva de la primera línea de la nota.
    pub title: Option<String>,
    pub note: String,
    pub priority: i64,
    pub session_id: Option<String>,
    /// Imágenes a copiar dentro de la carpeta de datos.
    pub attachments: Vec<PathBuf>,
}
