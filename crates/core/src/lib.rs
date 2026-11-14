//! Núcleo de record-mind: tareas, adjuntos, historial y lectura (solo lectura) de Engram.

pub mod claude_code;
pub mod engram;
pub mod model;
pub mod paths;
pub mod store;

pub use claude_code::{KnownProject, ProjectSummary};
pub use engram::{EngramReader, Memory, SessionInfo};
pub use model::{Attachment, MemoryLink, MemoryRelation, NewTask, Status, Task, TaskDetail, TaskEvent};
pub use store::{Store, TaskEdit, TaskFilter};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("base de datos: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("archivo: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("la tarea #{0} no existe")]
    NotFound(i64),
    #[error("la tarea #{id} está en «{from}» y no puede pasar a «{to}»")]
    InvalidTransition { id: i64, from: Status, to: Status },
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, Error>;
