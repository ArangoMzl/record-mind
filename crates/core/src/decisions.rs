//! Decisiones que quedaron pendientes en conversaciones de Claude Code.
//!
//! Cada conversación deja un `.md` en `<datos>/decisiones/<proyecto>/` con un resumen y una lista
//! de casillas (`- [ ]` abierta, `- [x]` tomada). El archivo es la fuente de verdad: se puede leer
//! y editar fuera de la app, y record-mind lo vuelve a escribir entero con el mismo formato cada
//! vez que cambia algo (registrar, tomar o reabrir una decisión).
//!
//! Claude lo escribe con la skill `/decisiones` (MCP `record_decisions`) y cierra las que se
//! resuelven en otra conversación con `resolve_decision`. La persona las marca en el tablero.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

const FOLDER: &str = "decisiones";
const SECTION_SUMMARY: &str = "## Resumen";
const SECTION_DECISIONS: &str = "## Decisiones";
const RECOMMENDATION: &str = "Recomendación:";
const REGISTERED: &str = "Registrada:";
const RESOLUTION: &str = "Decisión:";
const RESOLVED_META: &str = "Decidida:";

/// Una decisión pendiente (o ya tomada) de una conversación.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    /// `<carpeta del proyecto>/<archivo>#<n>`, estable mientras no se reordene el archivo a mano.
    pub id: String,
    pub question: String,
    /// Contexto: por qué hay que decidirlo y qué opciones hay.
    pub context: String,
    /// Lo que Claude recomienda, si lo dijo.
    pub recommendation: Option<String>,
    /// Cuándo se registró (UTC, ISO).
    pub created_at: Option<String>,
    pub resolved: bool,
    /// Qué se decidió (puede ir vacío si solo se marcó).
    pub resolution: Option<String>,
    pub resolved_at: Option<String>,
    /// Dónde se tomó: «tablero» o la conversación de Claude.
    pub resolved_by: Option<String>,
}

/// Un `.md` de decisiones: una conversación.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionFile {
    /// `<carpeta del proyecto>/<archivo sin .md>`.
    pub key: String,
    pub path: PathBuf,
    pub project: String,
    /// Conversación de Claude Code donde salieron.
    pub session_id: Option<String>,
    pub title: String,
    pub summary: String,
    pub created_at: String,
    pub updated_at: String,
    pub decisions: Vec<Decision>,
}

impl DecisionFile {
    pub fn open_count(&self) -> usize {
        self.decisions.iter().filter(|d| !d.resolved).count()
    }
}

/// Lo que manda Claude al registrar.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct NewDecision {
    pub question: String,
    pub context: Option<String>,
    pub recommendation: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct RecordInput {
    pub project: String,
    pub session_id: Option<String>,
    pub title: String,
    pub summary: String,
    pub decisions: Vec<NewDecision>,
}

#[derive(Debug, Clone)]
pub struct Recorded {
    pub file: DecisionFile,
    /// true si ya había un archivo de esa conversación y se actualizó.
    pub updated: bool,
    pub added: usize,
    /// Preguntas que ya estaban (abiertas o tomadas) y no se repitieron.
    pub skipped: usize,
}

/// Ahora en UTC con formato ISO (`2026-11-19T10:00:00Z`).
pub fn now() -> String {
    crate::claude_code::utc_string(std::time::SystemTime::now()).replace(' ', "T") + "Z"
}

/// Carpeta raíz de las decisiones dentro de la carpeta de datos.
pub fn root(data_dir: &Path) -> PathBuf {
    data_dir.join(FOLDER)
}

/// Todas las conversaciones con decisiones (o solo las de un proyecto), la actualizada más
/// recientemente primero. Los archivos que no se pueden leer se omiten.
pub fn list(data_dir: &Path, project: Option<&str>) -> Vec<DecisionFile> {
    let mut out = Vec::new();
    for dir in std::fs::read_dir(root(data_dir)).into_iter().flatten().flatten() {
        if !dir.path().is_dir() {
            continue;
        }
        for file in std::fs::read_dir(dir.path()).into_iter().flatten().flatten() {
            let path = file.path();
            if path.extension().is_none_or(|e| e != "md") {
                continue;
            }
            match read(&path) {
                Ok(f) if project.is_none_or(|p| f.project == p) => out.push(f),
                Ok(_) => {}
                Err(e) => tracing::warn!("decisiones: no se pudo leer {}: {e}", path.display()),
            }
        }
    }
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then_with(|| b.key.cmp(&a.key)));
    out
}

/// Registra las decisiones de una conversación. Si ya hay un archivo de esa misma conversación
/// lo actualiza: cambia título y resumen y añade solo las preguntas nuevas.
pub fn record(data_dir: &Path, input: RecordInput, now: &str) -> Result<Recorded> {
    let project = input.project.trim();
    if project.is_empty() {
        return Err(Error::Invalid("falta el proyecto".into()));
    }
    let session = input.session_id.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let existing = session.and_then(|s| list(data_dir, Some(project)).into_iter().find(|f| f.session_id.as_deref() == Some(s)));
    let updated = existing.is_some();

    let mut file = match existing {
        Some(f) => f,
        None => {
            let title = one_line(&input.title);
            let dir = root(data_dir).join(folder_name(project));
            let base = format!("{}-{}", &now[..10.min(now.len())], session.map(|s| s.chars().take(8).collect()).unwrap_or_else(|| slug(&title)));
            let mut path = dir.join(format!("{base}.md"));
            let mut n = 2;
            while path.exists() {
                path = dir.join(format!("{base}-{n}.md"));
                n += 1;
            }
            DecisionFile {
                key: key_for(&path),
                path,
                project: project.to_string(),
                session_id: session.map(str::to_owned),
                title: if title.is_empty() { "Conversación sin título".into() } else { title },
                summary: String::new(),
                created_at: now.to_string(),
                updated_at: now.to_string(),
                decisions: Vec::new(),
            }
        }
    };

    let title = one_line(&input.title);
    if !title.is_empty() {
        file.title = title;
    }
    if !input.summary.trim().is_empty() {
        file.summary = input.summary.trim().to_string();
    }

    let (mut added, mut skipped) = (0, 0);
    for d in input.decisions {
        let question = one_line(&d.question);
        if question.is_empty() {
            continue;
        }
        if file.decisions.iter().any(|x| same_question(&x.question, &question)) {
            skipped += 1;
            continue;
        }
        file.decisions.push(Decision {
            id: String::new(),
            question,
            context: d.context.as_deref().map(collapse).unwrap_or_default(),
            recommendation: d.recommendation.as_deref().map(collapse).filter(|r| !r.is_empty()),
            created_at: Some(now.to_string()),
            resolved: false,
            resolution: None,
            resolved_at: None,
            resolved_by: None,
        });
        added += 1;
    }
    if !updated && file.decisions.is_empty() {
        return Err(Error::Invalid("no hay decisiones que registrar".into()));
    }
    file.updated_at = now.to_string();
    write(&file)?;
    let file = read(&file.path)?;
    Ok(Recorded { file, updated, added, skipped })
}

/// Marca una decisión como tomada, con lo que se decidió (opcional) y dónde.
pub fn resolve(data_dir: &Path, id: &str, resolution: Option<&str>, by: &str, now: &str) -> Result<Decision> {
    update(data_dir, id, now, |d| {
        d.resolved = true;
        d.resolution = resolution.map(collapse).filter(|r| !r.is_empty());
        d.resolved_at = Some(now.to_string());
        d.resolved_by = Some(one_line(by)).filter(|b| !b.is_empty());
    })
}

/// Vuelve a abrir una decisión tomada.
pub fn reopen(data_dir: &Path, id: &str, now: &str) -> Result<Decision> {
    update(data_dir, id, now, |d| {
        d.resolved = false;
        d.resolution = None;
        d.resolved_at = None;
        d.resolved_by = None;
    })
}

/// Ruta del `.md` de una decisión (o de un archivo, con o sin `#n`), comprobando que esté dentro
/// de la carpeta de decisiones.
pub fn file_path(data_dir: &Path, id: &str) -> Result<PathBuf> {
    let key = id.split('#').next().unwrap_or_default();
    let mut parts = key.split('/');
    let (Some(folder), Some(stem), None) = (parts.next(), parts.next(), parts.next()) else {
        return Err(Error::Invalid(format!("id de decisión no válido: «{id}»")));
    };
    let safe = |s: &str| !s.is_empty() && s != "." && s != ".." && !s.contains(['\\', ':']);
    if !safe(folder) || !safe(stem) {
        return Err(Error::Invalid(format!("id de decisión no válido: «{id}»")));
    }
    let path = root(data_dir).join(folder).join(format!("{stem}.md"));
    if !path.is_file() {
        return Err(Error::Invalid(format!("la decisión «{id}» no existe")));
    }
    Ok(path)
}

fn update(data_dir: &Path, id: &str, now: &str, change: impl FnOnce(&mut Decision)) -> Result<Decision> {
    let path = file_path(data_dir, id)?;
    let n: usize = id
        .rsplit_once('#')
        .and_then(|(_, n)| n.parse().ok())
        .ok_or_else(|| Error::Invalid(format!("id de decisión no válido: «{id}» (falta #número)")))?;
    let mut file = read(&path)?;
    let d = file
        .decisions
        .get_mut(n.wrapping_sub(1))
        .ok_or_else(|| Error::Invalid(format!("la decisión «{id}» no existe")))?;
    change(d);
    file.updated_at = now.to_string();
    write(&file)?;
    let file = read(&path)?;
    Ok(file.decisions[n - 1].clone())
}

// ── Formato ─────────────────────────────────────────────────────────────

/// Lee un `.md` de decisiones.
pub fn read(path: &Path) -> Result<DecisionFile> {
    let text = std::fs::read_to_string(path)?;
    let mut f = parse(&text);
    f.key = key_for(path);
    f.path = path.to_path_buf();
    if f.title.is_empty() {
        f.title = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    }
    if f.project.is_empty() {
        f.project = path
            .parent()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
    }
    for (i, d) in f.decisions.iter_mut().enumerate() {
        d.id = format!("{}#{}", f.key, i + 1);
    }
    Ok(f)
}

/// Interpreta el texto; lo que no entiende lo ignora.
pub fn parse(text: &str) -> DecisionFile {
    let mut f = DecisionFile {
        key: String::new(),
        path: PathBuf::new(),
        project: String::new(),
        session_id: None,
        title: String::new(),
        summary: String::new(),
        created_at: String::new(),
        updated_at: String::new(),
        decisions: Vec::new(),
    };
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    if lines.first().is_some_and(|l| l.trim() == "---") {
        i = 1;
        while i < lines.len() && lines[i].trim() != "---" {
            if let Some((k, v)) = lines[i].split_once(':') {
                let v = v.trim().to_string();
                match k.trim() {
                    "proyecto" => f.project = v,
                    "sesion" | "sesión" => f.session_id = Some(v).filter(|v| !v.is_empty()),
                    "titulo" | "título" => f.title = v,
                    "creado" => f.created_at = v,
                    "actualizado" => f.updated_at = v,
                    _ => {}
                }
            }
            i += 1;
        }
        i += 1;
    }

    #[derive(PartialEq)]
    enum Section {
        None,
        Summary,
        Decisions,
    }
    let mut section = Section::None;
    let mut summary: Vec<&str> = Vec::new();
    let mut context: Vec<String> = Vec::new();
    let flush = |f: &mut DecisionFile, context: &mut Vec<String>| {
        if let Some(d) = f.decisions.last_mut() {
            d.context = context.join(" ");
        }
        context.clear();
    };
    for line in &lines[i.min(lines.len())..] {
        let t = line.trim();
        if let Some(h) = t.strip_prefix("# ").filter(|_| f.title.is_empty()) {
            f.title = h.trim().to_string();
            continue;
        }
        if t.starts_with("## ") {
            flush(&mut f, &mut context);
            section = if t.eq_ignore_ascii_case(SECTION_SUMMARY) {
                Section::Summary
            } else if t.to_lowercase().starts_with(&SECTION_DECISIONS.to_lowercase()) {
                Section::Decisions
            } else {
                Section::None
            };
            continue;
        }
        match section {
            Section::Summary => summary.push(line),
            Section::Decisions => {
                if let Some((done, rest)) = checkbox(t) {
                    flush(&mut f, &mut context);
                    f.decisions.push(Decision {
                        id: String::new(),
                        question: unbold(rest),
                        context: String::new(),
                        recommendation: None,
                        created_at: None,
                        resolved: done,
                        resolution: None,
                        resolved_at: None,
                        resolved_by: None,
                    });
                } else if let Some(d) = f.decisions.last_mut().filter(|_| !t.is_empty()) {
                    if let Some(v) = t.strip_prefix(RECOMMENDATION) {
                        d.recommendation = Some(v.trim().to_string()).filter(|v| !v.is_empty());
                    } else if let Some(v) = t.strip_prefix(REGISTERED) {
                        d.created_at = Some(v.trim().to_string()).filter(|v| !v.is_empty());
                    } else if let Some(v) = t.strip_prefix(RESOLUTION) {
                        d.resolution = Some(v.trim().to_string()).filter(|v| !v.is_empty());
                    } else if let Some(v) = t.strip_prefix(RESOLVED_META) {
                        let mut parts = v.splitn(2, '·');
                        d.resolved_at = parts.next().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
                        d.resolved_by = parts.next().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
                    } else {
                        context.push(t.to_string());
                    }
                }
            }
            Section::None => {}
        }
    }
    flush(&mut f, &mut context);
    f.summary = summary.join("\n").trim().to_string();
    f
}

/// Texto completo del archivo. Siempre el mismo formato, con finales de línea `\n`.
pub fn render(f: &DecisionFile) -> String {
    let mut s = String::new();
    s.push_str("---\n");
    s.push_str(&format!("proyecto: {}\n", one_line(&f.project)));
    if let Some(id) = &f.session_id {
        s.push_str(&format!("sesion: {}\n", one_line(id)));
    }
    s.push_str(&format!("titulo: {}\n", one_line(&f.title)));
    s.push_str(&format!("creado: {}\n", f.created_at));
    s.push_str(&format!("actualizado: {}\n", f.updated_at));
    s.push_str("---\n\n");
    s.push_str(&format!("# {}\n\n", one_line(&f.title)));
    if !f.summary.trim().is_empty() {
        s.push_str(&format!("{SECTION_SUMMARY}\n\n{}\n\n", f.summary.trim()));
    }
    s.push_str(&format!("{SECTION_DECISIONS}\n\n"));
    for d in &f.decisions {
        s.push_str(&format!("- [{}] **{}**\n", if d.resolved { 'x' } else { ' ' }, one_line(&d.question)));
        if !d.context.is_empty() {
            s.push_str(&format!("  {}\n", collapse(&d.context)));
        }
        if let Some(r) = &d.recommendation {
            s.push_str(&format!("  {RECOMMENDATION} {}\n", collapse(r)));
        }
        if let Some(at) = &d.created_at {
            s.push_str(&format!("  {REGISTERED} {at}\n"));
        }
        if d.resolved {
            if let Some(r) = &d.resolution {
                s.push_str(&format!("  {RESOLUTION} {}\n", collapse(r)));
            }
            let meta: Vec<&str> = [d.resolved_at.as_deref(), d.resolved_by.as_deref()].into_iter().flatten().collect();
            if !meta.is_empty() {
                s.push_str(&format!("  {RESOLVED_META} {}\n", meta.join(" · ")));
            }
        }
    }
    s
}

fn write(f: &DecisionFile) -> Result<()> {
    let dir = f.path.parent().ok_or_else(|| Error::Invalid("ruta sin carpeta".into()))?;
    std::fs::create_dir_all(dir)?;
    let tmp = f.path.with_extension("md.tmp");
    std::fs::write(&tmp, render(f))?;
    if let Err(e) = std::fs::rename(&tmp, &f.path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(())
}

// ── Utilidades ──────────────────────────────────────────────────────────

/// `- [ ] texto` / `- [x] texto` (también `*`). Devuelve si está marcada y el texto.
fn checkbox(line: &str) -> Option<(bool, &str)> {
    let rest = line.strip_prefix("- ").or_else(|| line.strip_prefix("* "))?;
    let (mark, text) = rest.strip_prefix('[').and_then(|r| r.split_once(']'))?;
    let done = match mark {
        " " | "" => false,
        "x" | "X" => true,
        _ => return None,
    };
    Some((done, text.trim()))
}

fn unbold(s: &str) -> String {
    let s = s.trim();
    s.strip_prefix("**").and_then(|s| s.strip_suffix("**")).unwrap_or(s).trim().to_string()
}

/// Una sola línea, sin espacios repetidos.
fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn collapse(s: &str) -> String {
    one_line(s)
}

fn same_question(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect::<String>();
    norm(a) == norm(b)
}

/// Nombre de carpeta válido en Windows para un proyecto.
pub fn folder_name(project: &str) -> String {
    let s: String = project
        .chars()
        .map(|c| if c.is_control() || r#"<>:"/\|?*"#.contains(c) { '-' } else { c })
        .collect();
    let s = s.trim().trim_matches('.').trim().to_string();
    if s.is_empty() { "sin-proyecto".into() } else { s }
}

/// Título → `decisiones-de-la-sesion` (para el nombre del archivo cuando no hay sesión).
fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.to_lowercase().chars() {
        let c = match c {
            'á' | 'à' | 'ä' => 'a',
            'é' | 'è' | 'ë' => 'e',
            'í' | 'ì' | 'ï' => 'i',
            'ó' | 'ò' | 'ö' => 'o',
            'ú' | 'ù' | 'ü' => 'u',
            'ñ' => 'n',
            c => c,
        };
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out: String = out.trim_end_matches('-').chars().take(40).collect();
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() { "decisiones".into() } else { out }
}

fn key_for(path: &Path) -> String {
    let folder = path.parent().and_then(|p| p.file_name()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    format!("{folder}/{stem}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rm-decisions-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn new(q: &str) -> NewDecision {
        NewDecision { question: q.into(), context: Some("Contexto\nen dos líneas".into()), recommendation: Some("Opción A".into()) }
    }

    #[test]
    fn registra_actualiza_y_resuelve() {
        let dir = temp("flow");
        let input = RecordInput {
            project: "record-mind".into(),
            session_id: Some("e37b0b5e-0336-4c78".into()),
            title: "Decisiones pendientes".into(),
            summary: "Se habló del apartado.".into(),
            decisions: vec![new("¿Dónde guardar los md?"), new("¿Long-poll?")],
        };
        let r = record(&dir, input.clone(), "2026-11-19T10:00:00Z").unwrap();
        assert!(!r.updated);
        assert_eq!((r.added, r.skipped), (2, 0));
        assert_eq!(r.file.key, "record-mind/2026-11-19-e37b0b5e");
        assert_eq!(r.file.decisions[0].id, "record-mind/2026-11-19-e37b0b5e#1");
        assert_eq!(r.file.decisions[0].context, "Contexto en dos líneas");
        assert_eq!(r.file.decisions[0].recommendation.as_deref(), Some("Opción A"));
        assert_eq!(r.file.decisions[0].created_at.as_deref(), Some("2026-11-19T10:00:00Z"));

        // Misma conversación otro día: mismo archivo, sin repetir preguntas.
        let again = RecordInput {
            summary: String::new(),
            decisions: vec![new("¿dónde guardar los MD"), new("¿Hook en Stop?")],
            ..input
        };
        let r = record(&dir, again, "2026-11-20T09:00:00Z").unwrap();
        assert!(r.updated);
        assert_eq!((r.added, r.skipped), (1, 1));
        assert_eq!(r.file.decisions.len(), 3);
        assert_eq!(r.file.summary, "Se habló del apartado.");
        assert_eq!(r.file.updated_at, "2026-11-20T09:00:00Z");
        assert_eq!(list(&dir, None).len(), 1);

        let d = resolve(&dir, "record-mind/2026-11-19-e37b0b5e#2", Some("No por ahora"), "tablero", "2026-11-21T08:00:00Z").unwrap();
        assert!(d.resolved);
        assert_eq!(d.resolution.as_deref(), Some("No por ahora"));
        assert_eq!(d.resolved_by.as_deref(), Some("tablero"));
        let f = &list(&dir, Some("record-mind"))[0];
        assert_eq!(f.open_count(), 2);
        let text = std::fs::read_to_string(&f.path).unwrap();
        assert!(text.contains("- [x] **¿Long-poll?**\n"));
        assert!(text.contains("  Decidida: 2026-11-21T08:00:00Z · tablero\n"));

        let d = reopen(&dir, &d.id, "2026-11-21T09:00:00Z").unwrap();
        assert!(!d.resolved && d.resolution.is_none());

        assert!(resolve(&dir, "record-mind/2026-11-19-e37b0b5e#9", None, "x", "t").is_err());
        assert!(resolve(&dir, "../../etc#1", None, "x", "t").is_err());
        assert!(list(&dir, Some("otro")).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn sin_sesion_y_vacio() {
        let dir = temp("nosession");
        let empty = RecordInput { project: "web".into(), title: "x".into(), ..Default::default() };
        assert!(record(&dir, empty, "2026-11-19T10:00:00Z").is_err());
        let input = RecordInput {
            project: "web".into(),
            title: "Rediseño del menú".into(),
            decisions: vec![new("¿Menú lateral?")],
            ..Default::default()
        };
        let a = record(&dir, input.clone(), "2026-11-19T10:00:00Z").unwrap();
        let b = record(&dir, input, "2026-11-19T11:00:00Z").unwrap();
        assert_eq!(a.file.key, "web/2026-11-19-rediseno-del-menu");
        assert_eq!(b.file.key, "web/2026-11-19-rediseno-del-menu-2");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn lee_un_md_escrito_a_mano() {
        let md = "# Sesión del lunes\n\n## Resumen\n\nAlgo.\n\n## Decisiones pendientes\n\n- [ ] ¿Postgres o SQLite?\n  Depende del hosting.\n- [X] **¿Tailwind?**\n\n## Deudas técnicas\n\n- [ ] Esto no es una decisión\n";
        let f = parse(md);
        assert_eq!(f.title, "Sesión del lunes");
        assert_eq!(f.summary, "Algo.");
        assert_eq!(f.decisions.len(), 2);
        assert_eq!(f.decisions[0].question, "¿Postgres o SQLite?");
        assert_eq!(f.decisions[0].context, "Depende del hosting.");
        assert!(f.decisions[1].resolved);
        // Volver a escribirlo y leerlo da lo mismo.
        let again = parse(&render(&f));
        assert_eq!(again.decisions, f.decisions);
        assert_eq!(again.summary, f.summary);
    }

    #[test]
    fn nombres_de_carpeta() {
        assert_eq!(folder_name("mi:proyecto/x"), "mi-proyecto-x");
        assert_eq!(folder_name("  .. "), "sin-proyecto");
        assert_eq!(slug("¿Dónde guardar?"), "donde-guardar");
    }
}
