//! Errores repetidos → reglas para `CLAUDE.md`.
//!
//! Claude junta la evidencia con [`Store::patterns`] (tareas devueltas, temas que se repiten,
//! reglas que ya hay) y, si ve un error que vuelve, lo propone con [`Store::suggest_rule`]. La
//! regla queda pendiente hasta que la persona la acepta en el tablero: solo entonces se escribe en
//! el `CLAUDE.md` del proyecto ([`add_rule_to_claude_md`]). Claude nunca la escribe por su cuenta.

use std::path::{Path, PathBuf};

use rusqlite::{OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

use crate::{Error, Result, Status, Store, store::TASK_SELECT};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuleSuggestion {
    pub id: i64,
    pub project: String,
    pub rule: String,
    /// Por qué: qué error se repite y en qué tareas.
    pub reason: String,
    pub task_ids: Vec<i64>,
    /// pending, accepted o rejected.
    pub status: String,
    pub created_at: String,
    pub decided_at: Option<String>,
}

/// Una tarea que la persona devolvió, con las notas de cada devolución.
#[derive(Debug, Clone, Serialize)]
pub struct ReturnedTask {
    pub id: i64,
    pub title: String,
    pub note: String,
    pub status: Status,
    pub return_notes: Vec<String>,
}

/// Una palabra que aparece en varias tareas del proyecto.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Theme {
    pub word: String,
    pub task_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Patterns {
    pub project: String,
    pub total_tasks: usize,
    pub returned: Vec<ReturnedTask>,
    pub themes: Vec<Theme>,
    /// Reglas ya sugeridas (pendientes o aceptadas), para no repetirlas.
    pub existing: Vec<RuleSuggestion>,
}

/// Temas: palabras presentes en al menos 2 tareas, de la más repetida a la menos (máximo 15).
pub fn themes(tasks: &[(i64, String)]) -> Vec<Theme> {
    let mut found: Vec<Theme> = Vec::new();
    for (id, text) in tasks {
        for word in crate::text::keywords(text) {
            match found.iter_mut().find(|t| t.word == word) {
                Some(t) => {
                    if !t.task_ids.contains(id) {
                        t.task_ids.push(*id);
                    }
                }
                None => found.push(Theme { word, task_ids: vec![*id] }),
            }
        }
    }
    found.retain(|t| t.task_ids.len() >= 2);
    // Estable: a igual número de tareas, conserva el orden de aparición.
    found.sort_by_key(|t| std::cmp::Reverse(t.task_ids.len()));
    found.truncate(15);
    found
}

impl Store {
    /// La evidencia con la que Claude decide si sugerir una regla.
    pub fn patterns(&self, project: &str) -> Result<Patterns> {
        let mut stmt = self.conn.prepare(&format!("{TASK_SELECT} WHERE project = ?1 ORDER BY id"))?;
        let tasks = stmt
            .query_map([project], crate::store::task_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut ev = self.conn.prepare(
            "SELECT message FROM task_events WHERE task_id = ?1 AND kind = 'returned' AND message <> '' ORDER BY id",
        )?;
        let mut returned = Vec::new();
        let mut texts = Vec::new();
        for t in &tasks {
            let notes = ev.query_map([t.id], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
            texts.push((t.id, format!("{} {} {}", t.title, t.note, notes.join(" "))));
            if !notes.is_empty() {
                returned.push(ReturnedTask {
                    id: t.id,
                    title: t.title.clone(),
                    note: t.note.clone(),
                    status: t.status,
                    return_notes: notes,
                });
            }
        }

        let existing = self
            .rule_suggestions(Some(project))?
            .into_iter()
            .filter(|r| r.status != "rejected")
            .collect();
        Ok(Patterns { project: project.to_string(), total_tasks: tasks.len(), returned, themes: themes(&texts), existing })
    }

    /// Guarda una regla sugerida. No acepta reglas vacías ni repetidas en el proyecto.
    pub fn suggest_rule(&mut self, project: &str, rule: &str, reason: &str, task_ids: &[i64]) -> Result<RuleSuggestion> {
        let rule = rule.trim();
        let project = project.trim();
        if rule.is_empty() || project.is_empty() {
            return Err(Error::Invalid("la regla y el proyecto no pueden estar vacíos".into()));
        }
        let dup: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM rule_suggestions
                 WHERE project = ?1 AND lower(trim(rule)) = lower(?2) AND status <> 'rejected'",
                params![project, rule],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(id) = dup {
            return Err(Error::Invalid(format!("esa regla ya está sugerida o aceptada (sugerencia {id})")));
        }
        for id in task_ids {
            self.task(*id)?;
        }
        self.conn.execute(
            "INSERT INTO rule_suggestions (project, rule, reason, task_ids) VALUES (?1, ?2, ?3, ?4)",
            params![project, rule, reason.trim(), serde_json::to_string(task_ids)?],
        )?;
        self.rule(self.conn.last_insert_rowid())
    }

    pub fn rule(&self, id: i64) -> Result<RuleSuggestion> {
        self.conn
            .query_row(&format!("{RULE_SELECT} WHERE id = ?1"), [id], rule_from_row)
            .optional()?
            .ok_or_else(|| Error::Invalid(format!("la sugerencia {id} no existe")))
    }

    /// Sugerencias del proyecto (o de todos), las más nuevas primero.
    pub fn rule_suggestions(&self, project: Option<&str>) -> Result<Vec<RuleSuggestion>> {
        let mut stmt = self
            .conn
            .prepare(&format!("{RULE_SELECT} WHERE ?1 IS NULL OR project = ?1 ORDER BY id DESC"))?;
        let rows = stmt.query_map([project], rule_from_row)?.collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// La persona acepta (quizá con el texto retocado) o descarta una sugerencia pendiente.
    pub fn decide_rule(&mut self, id: i64, accepted: bool, rule: Option<&str>) -> Result<RuleSuggestion> {
        let current = self.rule(id)?;
        if current.status != "pending" {
            return Err(Error::Invalid(format!("la sugerencia {id} ya está decidida")));
        }
        let text = rule.map(str::trim).filter(|r| !r.is_empty()).unwrap_or(&current.rule);
        self.conn.execute(
            "UPDATE rule_suggestions SET status = ?2, rule = ?3,
                    decided_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?1",
            params![id, if accepted { "accepted" } else { "rejected" }, text],
        )?;
        self.rule(id)
    }
}

const RULE_SELECT: &str =
    "SELECT id, project, rule, reason, task_ids, status, created_at, decided_at FROM rule_suggestions";

fn rule_from_row(r: &Row) -> rusqlite::Result<RuleSuggestion> {
    let ids: String = r.get(4)?;
    Ok(RuleSuggestion {
        id: r.get(0)?,
        project: r.get(1)?,
        rule: r.get(2)?,
        reason: r.get(3)?,
        task_ids: serde_json::from_str(&ids).unwrap_or_default(),
        status: r.get(5)?,
        created_at: r.get(6)?,
        decided_at: r.get(7)?,
    })
}

// ── CLAUDE.md ───────────────────────────────────────────────────────────

pub const CLAUDE_MD_SECTION: &str = "## Reglas aprendidas (record-mind)";

/// Añade la regla como viñeta al final de la sección de record-mind, creándola si no existe.
/// `None` si la regla ya estaba. El resto del archivo no se toca.
pub fn insert_rule(contents: &str, rule: &str) -> Option<String> {
    let rule = rule.trim().trim_start_matches(['-', '*', ' ']).trim();
    let bullet = format!("- {rule}");
    if contents.lines().any(|l| l.trim().eq_ignore_ascii_case(&bullet)) {
        return None;
    }
    let nl = if contents.contains("\r\n") { "\r\n" } else { "\n" };
    let lines: Vec<&str> = contents.lines().collect();
    let Some(start) = lines.iter().position(|l| l.trim() == CLAUDE_MD_SECTION) else {
        let mut out = contents.trim_end().to_string();
        if !out.is_empty() {
            out.push_str(nl);
            out.push_str(nl);
        }
        out.push_str(&format!(
            "{CLAUDE_MD_SECTION}{nl}{nl}Errores que se repetían, aprobados desde el tablero de record-mind.{nl}{nl}{bullet}{nl}"
        ));
        return Some(out);
    };
    // Fin de la sección: el siguiente título de nivel 1 o 2, o el final del archivo.
    let end = lines[start + 1..]
        .iter()
        .position(|l| l.starts_with("# ") || l.starts_with("## "))
        .map_or(lines.len(), |i| start + 1 + i);
    // Justo después de la última línea con contenido de la sección.
    let mut at = end;
    while at > start + 1 && lines[at - 1].trim().is_empty() {
        at -= 1;
    }
    let mut out: Vec<String> = lines[..at].iter().map(|l| l.to_string()).collect();
    out.push(bullet);
    if at < lines.len() {
        out.push(String::new());
        out.extend(lines[end..].iter().map(|l| l.to_string()));
    }
    Some(out.join(nl) + nl)
}

/// Escribe la regla en `<carpeta>/CLAUDE.md`. Devuelve la ruta y si hubo que añadirla.
pub fn add_rule_to_claude_md(dir: &Path, rule: &str) -> Result<(PathBuf, bool)> {
    if !dir.is_dir() {
        return Err(Error::Invalid(format!("la carpeta del proyecto no existe: {}", dir.display())));
    }
    let path = dir.join("CLAUDE.md");
    let current = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
    };
    match insert_rule(&current, rule) {
        Some(next) => {
            std::fs::write(&path, next)?;
            Ok((path, true))
        }
        None => Ok((path, false)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NewTask;

    #[test]
    fn crea_la_seccion_si_no_existe() {
        let out = insert_rule("# Proyecto\n\nUsa pnpm.\n", "Revisa el título en móvil").unwrap();
        assert_eq!(
            out,
            "# Proyecto\n\nUsa pnpm.\n\n## Reglas aprendidas (record-mind)\n\nErrores que se repetían, aprobados desde el tablero de record-mind.\n\n- Revisa el título en móvil\n"
        );
        assert!(insert_rule("", "Una regla").unwrap().starts_with(CLAUDE_MD_SECTION));
    }

    #[test]
    fn agrega_al_final_de_la_seccion_sin_tocar_lo_demas() {
        let md = "# P\n\n## Reglas aprendidas (record-mind)\n\nIntro.\n\n- Primera\n\n## Otra\n\nTexto\n";
        let out = insert_rule(md, "- Segunda").unwrap();
        assert_eq!(out, "# P\n\n## Reglas aprendidas (record-mind)\n\nIntro.\n\n- Primera\n- Segunda\n\n## Otra\n\nTexto\n");
        // Repetida (sin importar mayúsculas): no cambia nada.
        assert_eq!(insert_rule(&out, "Segunda"), None);
        assert_eq!(insert_rule(&out, "segunda"), None);
        // Respeta los finales de línea de Windows.
        let crlf = insert_rule("# P\r\n", "Regla").unwrap();
        assert!(crlf.contains("\r\n- Regla\r\n") && !crlf.replace("\r\n", "").contains('\n'));
    }

    #[test]
    fn temas_que_se_repiten() {
        let t = themes(&[
            (1, "El título se ve pequeño en móvil".into()),
            (2, "Título del footer pequeño".into()),
            (3, "La imagen del logo no carga".into()),
        ]);
        assert_eq!(t, vec![Theme { word: "título".into(), task_ids: vec![1, 2] }, Theme { word: "pequeño".into(), task_ids: vec![1, 2] }]);
    }

    #[test]
    fn sugerir_y_decidir_reglas() {
        let dir = std::env::temp_dir().join(format!("rm-rules-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut s = Store::open(&dir).unwrap();
        let t = s.create_task(NewTask { project: "web".into(), note: "Título pequeño en móvil".into(), ..Default::default() }, "user").unwrap();
        s.claim(t.id, "claude").unwrap();
        s.complete(t.id, "claude", "subí el tamaño", &[], &[], &[]).unwrap();
        s.reopen(t.id, "user", "sigue pequeño en móvil, revisa el breakpoint").unwrap();

        let p = s.patterns("web").unwrap();
        assert_eq!(p.total_tasks, 1);
        assert_eq!(p.returned[0].return_notes, ["sigue pequeño en móvil, revisa el breakpoint"]);

        let r = s.suggest_rule("web", "Comprueba los tamaños en el breakpoint móvil", "Devuelta por lo mismo", &[t.id]).unwrap();
        assert_eq!(r.status, "pending");
        assert!(s.suggest_rule("web", "comprueba los tamaños en el breakpoint móvil", "", &[]).is_err());
        assert!(s.suggest_rule("web", "Otra", "", &[999]).is_err());
        assert_eq!(s.patterns("web").unwrap().existing.len(), 1);

        let done = s.decide_rule(r.id, true, Some("Comprueba los tamaños de letra en el breakpoint móvil")).unwrap();
        assert_eq!(done.status, "accepted");
        assert_eq!(done.rule, "Comprueba los tamaños de letra en el breakpoint móvil");
        assert!(s.decide_rule(r.id, false, None).is_err());

        let (path, added) = add_rule_to_claude_md(&dir, &done.rule).unwrap();
        assert!(added && std::fs::read_to_string(&path).unwrap().contains("- Comprueba los tamaños"));
        assert!(!add_rule_to_claude_md(&dir, &done.rule).unwrap().1);
        drop(s);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
