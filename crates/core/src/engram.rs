//! Lectura de la base de Engram. Siempre en modo SOLO LECTURA: Engram mantiene índices,
//! versiones y una cola de sincronización, así que cualquier escritura debe pasar por Engram.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub project: String,
    pub directory: String,
    pub started_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: i64,
    pub sync_id: Option<String>,
    pub kind: String,
    pub title: String,
    pub snippet: String,
    pub project: Option<String>,
    pub topic_key: Option<String>,
    pub created_at: String,
}

pub struct EngramReader {
    path: PathBuf,
    conn: Option<Connection>,
}

impl EngramReader {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        EngramReader { path: path.into(), conn: None }
    }

    pub fn open_default() -> Self {
        Self::new(crate::paths::engram_db_path())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// `true` si existe la base de Engram en disco.
    pub fn available(&self) -> bool {
        self.path.is_file()
    }

    fn conn(&mut self) -> Option<&Connection> {
        if self.conn.is_none() && self.available() {
            let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
            match Connection::open_with_flags(&self.path, flags) {
                Ok(c) => {
                    let _ = c.busy_timeout(std::time::Duration::from_secs(2));
                    self.conn = Some(c);
                }
                Err(e) => tracing::warn!("no se pudo abrir Engram en {}: {e}", self.path.display()),
            }
        }
        self.conn.as_ref()
    }

    /// Sesión de Claude Code abierta con actividad más reciente (últimas 12 horas).
    /// La concesión de Engram (`runtime_lease_expires_at`) no siempre se renueva y hay sesiones
    /// que nunca se cierran, así que se usa la última señal de vida: inicio, concesión,
    /// último prompt o última memoria.
    pub fn active_session(&mut self) -> Option<SessionInfo> {
        let conn = self.conn()?;
        conn.query_row(
            "SELECT id, project, directory, started_at FROM (
                SELECT s.id, s.project, s.directory, s.started_at,
                       MAX(substr(s.started_at, 1, 19),
                           COALESCE(s.runtime_lease_expires_at, ''),
                           COALESCE((SELECT MAX(created_at) FROM user_prompts p WHERE p.session_id = s.id), ''),
                           COALESCE((SELECT MAX(created_at) FROM observations o WHERE o.session_id = s.id), '')
                       ) AS last_seen
                FROM sessions s
                WHERE s.ended_at IS NULL
             )
             WHERE last_seen > datetime('now', '-12 hours')
             ORDER BY last_seen DESC LIMIT 1",
            [],
            |r| {
                Ok(SessionInfo { id: r.get(0)?, project: r.get(1)?, directory: r.get(2)?, started_at: r.get(3)? })
            },
        )
        .optional()
        .unwrap_or_else(|e| {
            tracing::warn!("Engram: error leyendo sesiones: {e}");
            None
        })
    }

    /// Nombre de proyecto para una carpeta, igual al que usa Engram.
    /// Busca la sesión cuya carpeta contiene `cwd` (la más específica); si no hay, usa el
    /// nombre de la carpeta en minúsculas.
    pub fn resolve_project(&mut self, cwd: &str) -> String {
        let norm = normalize_dir(cwd);
        if let Some(conn) = self.conn() {
            let alias: Option<String> = conn
                .query_row(
                    "SELECT project FROM project_aliases WHERE kind = 'cwd' AND lower(alias_key) = ?1",
                    [&norm],
                    |r| r.get(0),
                )
                .optional()
                .ok()
                .flatten();
            if let Some(p) = alias {
                return p;
            }
            let found: Option<String> = conn
                .query_row(
                    "SELECT project FROM sessions
                     WHERE ?1 = lower(replace(directory, '/', '\\'))
                        OR ?1 LIKE lower(replace(directory, '/', '\\')) || '\\%'
                     ORDER BY length(directory) DESC, started_at DESC LIMIT 1",
                    [&norm],
                    |r| r.get(0),
                )
                .optional()
                .ok()
                .flatten();
            if let Some(p) = found {
                return p;
            }
        }
        norm.rsplit('\\').find(|s| !s.is_empty()).unwrap_or("sin-proyecto").to_string()
    }

    /// Memorias de Engram relacionadas con un texto (búsqueda de texto completo).
    /// Incluye las del proyecto y las de alcance personal/global.
    pub fn related_memories(&mut self, project: &str, text: &str, limit: usize) -> Vec<Memory> {
        let Some(query) = fts_query(text) else { return Vec::new() };
        let Some(conn) = self.conn() else { return Vec::new() };
        let sql = "SELECT o.id, o.sync_id, o.type, o.title, substr(o.content, 1, 320), o.project, o.topic_key, o.created_at
                   FROM observations_fts
                   JOIN observations o ON o.id = observations_fts.rowid
                   WHERE observations_fts MATCH ?1
                     AND o.deleted_at IS NULL
                     AND (o.project = ?2 OR o.scope <> 'project')
                   ORDER BY observations_fts.rank
                   LIMIT ?3";
        let run = || -> rusqlite::Result<Vec<Memory>> {
            let mut stmt = conn.prepare(sql)?;
            stmt.query_map(rusqlite::params![query, project, limit as i64], memory_from_row)?
                .collect()
        };
        run().unwrap_or_else(|e| {
            tracing::warn!("Engram: búsqueda fallida: {e}");
            Vec::new()
        })
    }

    /// Busca una memoria por id numérico o por sync_id.
    pub fn memory(&mut self, reference: &str) -> Option<Memory> {
        let conn = self.conn()?;
        let sql = "SELECT id, sync_id, type, title, substr(content, 1, 320), project, topic_key, created_at
                   FROM observations WHERE (CAST(id AS TEXT) = ?1 OR sync_id = ?1) AND deleted_at IS NULL";
        conn.query_row(sql, [reference.trim().trim_start_matches('#')], memory_from_row)
            .optional()
            .ok()
            .flatten()
    }
}

fn memory_from_row(r: &rusqlite::Row) -> rusqlite::Result<Memory> {
    Ok(Memory {
        id: r.get(0)?,
        sync_id: r.get(1)?,
        kind: r.get(2)?,
        title: r.get(3)?,
        snippet: r.get(4)?,
        project: r.get(5)?,
        topic_key: r.get(6)?,
        created_at: r.get(7)?,
    })
}

fn normalize_dir(dir: &str) -> String {
    dir.trim().replace('/', "\\").trim_end_matches('\\').to_lowercase()
}

const STOPWORDS: &[&str] = &[
    "para", "esta", "este", "esto", "estos", "estas", "como", "pero", "tiene", "hace", "hacer", "falta",
    "toca", "porque", "cuando", "donde", "desde", "hasta", "sobre", "entre", "todo", "toda", "todos",
    "algo", "también", "tambien", "solo", "sólo", "debe", "deben", "puede", "pueden", "quiero", "queda",
    "that", "this", "with", "from", "have", "should", "does", "into", "when", "what",
];

/// Convierte un texto libre en una consulta FTS5 `"a" OR "b"` con las palabras útiles.
/// El índice de Engram usa trigramas, así que se descartan palabras de menos de 4 letras.
fn fts_query(text: &str) -> Option<String> {
    let mut words: Vec<String> = Vec::new();
    for w in text.split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-') {
        let w = w.trim_matches('-').to_lowercase();
        if w.chars().count() < 4 || STOPWORDS.contains(&w.as_str()) || words.contains(&w) {
            continue;
        }
        words.push(w);
        if words.len() == 10 {
            break;
        }
    }
    if words.is_empty() {
        None
    } else {
        Some(words.iter().map(|w| format!("\"{w}\"")).collect::<Vec<_>>().join(" OR "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consulta_fts_descarta_ruido() {
        assert_eq!(
            fts_query("Toca subir el tamaño de esta letra, no carga la imagen").as_deref(),
            Some("\"subir\" OR \"tamaño\" OR \"letra\" OR \"carga\" OR \"imagen\"")
        );
        assert_eq!(fts_query("ok sí"), None);
    }

    #[test]
    fn normaliza_carpetas() {
        assert_eq!(normalize_dir("C:/Users/Seb/Proyecto/"), "c:\\users\\seb\\proyecto");
    }
}
