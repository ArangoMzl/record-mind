//! Proyectos que conoce Claude Code, para que el tablero y la captura los ofrezcan aunque
//! todavía no tengan tareas ni una sesión abierta.
//!
//! Claude Code guarda en `~/.claude.json` (o `$CLAUDE_CONFIG_DIR/.claude.json`) una entrada por
//! cada carpeta donde se abrió. Aquí solo se lee; la única escritura (registrar el MCP al
//! instalar la app) está en `claude_setup`.

use std::{
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::Serialize;

use crate::{EngramReader, SessionInfo};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownProject {
    /// Nombre igual al que usa Engram (y por tanto `/pendientes` y el MCP).
    pub name: String,
    pub directory: String,
}

/// Un proyecto del tablero: los que tienen tareas, con sus contadores, más los de Claude Code.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProjectSummary {
    pub name: String,
    /// Tareas en inbox, in_progress, review y done.
    pub counts: [i64; 4],
    /// Carpeta del proyecto, si Claude Code la conoce.
    pub directory: Option<String>,
    /// Rama de git en la que está esa carpeta (commit abreviado si el HEAD está separado).
    pub branch: Option<String>,
}

pub fn config_path() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR") {
        return Some(PathBuf::from(dir).join(".claude.json"));
    }
    dirs::home_dir().map(|h| h.join(".claude.json"))
}

/// Proyectos de Claude Code con nombre de Engram, sin repetir y sin carpetas de trabajo internas.
pub fn known_projects(engram: &mut EngramReader) -> Vec<KnownProject> {
    let Some(config) = config_path() else { return Vec::new() };
    let home = dirs::home_dir().map(|h| h.to_string_lossy().into_owned()).unwrap_or_default();
    let mut out: Vec<KnownProject> = Vec::new();
    for dir in project_dirs(&config) {
        if is_internal(&dir, &home) || !Path::new(&dir).is_dir() {
            continue;
        }
        let name = engram.resolve_project(&dir);
        if !out.iter().any(|p| p.name == name) {
            out.push(KnownProject { name, directory: dir });
        }
    }
    out
}

/// Carpetas registradas en la configuración de Claude Code (claves de `projects`).
fn project_dirs(config: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(config) else { return Vec::new() };
    match serde_json::from_str::<serde_json::Value>(&text) {
        Ok(v) => v
            .get("projects")
            .and_then(|p| p.as_object())
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default(),
        Err(e) => {
            tracing::warn!("no se pudo leer {}: {e}", config.display());
            Vec::new()
        }
    }
}

/// Carpetas que no son proyectos: la carpeta de usuario, la de Claude (`~/.claude`, con sus
/// sesiones de reserva) y todo lo que cuelga de AppData (temporales y espacios de trabajo de
/// Claude Desktop).
fn is_internal(dir: &str, home: &str) -> bool {
    let d = normalize(dir);
    let h = normalize(home);
    d.is_empty()
        || (!h.is_empty() && d == h)
        || d.contains("\\appdata\\")
        || d.contains("\\.claude\\")
        || d.ends_with("\\.claude")
}

fn normalize(dir: &str) -> String {
    dir.trim().replace('/', "\\").trim_end_matches('\\').to_lowercase()
}

/// Una conversación sin actividad en este tiempo ya no cuenta como sesión activa.
pub const ACTIVE_WINDOW: Duration = Duration::from_secs(30 * 60);

/// Carpeta donde Claude Code guarda una conversación por archivo (`<carpeta>/<sesión>.jsonl`).
pub fn projects_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR") {
        return Some(PathBuf::from(dir).join("projects"));
    }
    dirs::home_dir().map(|h| h.join(".claude").join("projects"))
}

/// Sesiones activas, de la más reciente a la más antigua.
///
/// La fuente principal son las conversaciones de Claude Code: cada una es un archivo que se
/// reescribe con cada mensaje, así que su fecha de modificación es la actividad real, aunque
/// haya varias abiertas a la vez. Su id coincide con el de la sesión de Engram. Si no hay
/// ninguna (otra configuración, Claude Code en otra máquina), se recurre a Engram.
pub fn live_sessions(engram: &mut EngramReader) -> Vec<SessionInfo> {
    let found = projects_dir().map(|d| transcripts(&d, ACTIVE_WINDOW)).unwrap_or_default();
    if found.is_empty() {
        return engram.active_session().into_iter().collect();
    }
    let home = dirs::home_dir().map(|h| h.to_string_lossy().into_owned()).unwrap_or_default();
    found
        .into_iter()
        .filter(|t| !is_internal(&t.cwd, &home))
        .map(|t| SessionInfo {
            project: engram.resolve_project(&t.cwd),
            branch: crate::git::current_branch(&t.cwd),
            id: t.id,
            directory: t.cwd,
            started_at: utc_string(t.created),
            last_activity: utc_string(t.modified),
        })
        .collect()
}

struct Transcript {
    id: String,
    cwd: String,
    created: SystemTime,
    modified: SystemTime,
}

/// Conversaciones modificadas dentro de la ventana, la más reciente primero. Solo mira los
/// `.jsonl` de primer nivel: los subagentes guardan los suyos dentro de la carpeta de la sesión.
fn transcripts(projects: &Path, window: Duration) -> Vec<Transcript> {
    let since = SystemTime::now() - window;
    let mut out = Vec::new();
    for dir in std::fs::read_dir(projects).into_iter().flatten().flatten() {
        for file in std::fs::read_dir(dir.path()).into_iter().flatten().flatten() {
            let path = file.path();
            if path.extension().is_none_or(|e| e != "jsonl") {
                continue;
            }
            let Ok(meta) = file.metadata() else { continue };
            let Ok(modified) = meta.modified() else { continue };
            if modified < since {
                continue;
            }
            let Some(cwd) = transcript_cwd(&path) else { continue };
            out.push(Transcript {
                id: path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
                cwd,
                created: meta.created().unwrap_or(modified),
                modified,
            });
        }
    }
    out.sort_by_key(|t| std::cmp::Reverse(t.modified));
    out
}

/// Carpeta de trabajo de una conversación: el primer `cwd` que aparece en sus líneas. Solo se
/// leen los primeros KB; las primeras líneas son pequeñas y el archivo puede pesar megas.
fn transcript_cwd(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    BufReader::new(file.take(512 * 1024))
        .lines()
        .take(50)
        .map_while(Result::ok)
        .find_map(|line| {
            let v: serde_json::Value = serde_json::from_str(&line).ok()?;
            v.get("cwd")?.as_str().filter(|s| !s.is_empty()).map(str::to_owned)
        })
}

/// Fecha en UTC con el formato de SQLite (`AAAA-MM-DD HH:MM:SS`), el mismo que usan las tareas.
fn utc_string(t: SystemTime) -> String {
    let secs = t.duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Días desde 1970 a fecha civil (algoritmo de Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// Une los proyectos con tareas (en su orden, el más reciente primero) con los de Claude Code
/// que aún no tienen ninguna, estos en orden alfabético.
pub fn merge_projects(with_tasks: Vec<(String, [i64; 4])>, known: &[KnownProject]) -> Vec<ProjectSummary> {
    let mut out: Vec<ProjectSummary> = with_tasks
        .into_iter()
        .map(|(name, counts)| {
            let directory = known.iter().find(|k| k.name == name).map(|k| k.directory.clone());
            let branch = directory.as_deref().and_then(crate::git::current_branch);
            ProjectSummary { name, counts, directory, branch }
        })
        .collect();
    let mut rest: Vec<&KnownProject> = known.iter().filter(|k| !out.iter().any(|p| p.name == k.name)).collect();
    rest.sort_by_key(|k| k.name.to_lowercase());
    out.extend(rest.into_iter().map(|k| ProjectSummary {
        name: k.name.clone(),
        counts: [0; 4],
        directory: Some(k.directory.clone()),
        branch: crate::git::current_branch(&k.directory),
    }));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: &str = "C:\\Users\\Seb";

    #[test]
    fn descarta_carpetas_internas() {
        for dir in [
            "C:/Users/Seb",
            "C:\\Users\\Seb\\",
            "C:\\Users\\Seb\\.claude\\spares\\spare-abc",
            "C:/Users/Seb/AppData/Local/Claude-Data/claude_x/scratch-workspaces/1/2/scratch-2026",
            "C:\\Users\\Seb\\AppData\\Local\\Temp\\claude\\probe",
        ] {
            assert!(is_internal(dir, HOME), "{dir} debería descartarse");
        }
        for dir in ["C:/Users/Seb/Desktop/deskia", "D:\\code\\record-mind", "C:\\Users\\Seb\\claude-tools"] {
            assert!(!is_internal(dir, HOME), "{dir} es un proyecto");
        }
    }

    #[test]
    fn lee_las_carpetas_de_la_configuracion() {
        let dir = std::env::temp_dir().join(format!("rm-claude-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let config = dir.join(".claude.json");
        std::fs::write(&config, r#"{"numStartups": 3, "projects": {"C:/a/uno": {"x": 1}, "C:/a/dos": {}}}"#).unwrap();
        let mut dirs = project_dirs(&config);
        dirs.sort();
        assert_eq!(dirs, ["C:/a/dos", "C:/a/uno"]);
        std::fs::write(&config, "no es json").unwrap();
        assert!(project_dirs(&config).is_empty());
        assert!(project_dirs(&dir.join("no-existe.json")).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn fechas_utc_con_formato_sqlite() {
        assert_eq!(utc_string(UNIX_EPOCH), "1970-01-01 00:00:00");
        assert_eq!(utc_string(UNIX_EPOCH + Duration::from_secs(1_794_627_399)), "2026-11-14 03:36:39");
        assert_eq!(utc_string(UNIX_EPOCH + Duration::from_secs(951_782_400)), "2000-02-29 00:00:00");
    }

    #[test]
    fn sesiones_activas_desde_las_conversaciones() {
        let root = std::env::temp_dir().join(format!("rm-transcripts-{}", std::process::id()));
        let proj = root.join("C--code-tienda");
        std::fs::create_dir_all(proj.join("aaaa/subagents")).unwrap();
        let line = |cwd: &str| format!("{{\"type\":\"queue-operation\"}}\n{{\"type\":\"user\",\"cwd\":\"{cwd}\"}}\n");
        std::fs::write(proj.join("aaaa.jsonl"), line("C:\\\\code\\\\tienda")).unwrap();
        // Sin cwd todavía: no se puede saber de qué proyecto es.
        std::fs::write(proj.join("bbbb.jsonl"), "{\"type\":\"queue-operation\"}\n").unwrap();
        // Los subagentes no son sesiones.
        std::fs::write(proj.join("aaaa/subagents/cccc.jsonl"), line("C:\\\\code\\\\tienda")).unwrap();
        std::fs::write(proj.join("notas.txt"), "x").unwrap();

        let found = transcripts(&root, ACTIVE_WINDOW);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, "aaaa");
        assert_eq!(found[0].cwd, "C:\\code\\tienda");
        // Fuera de la ventana no cuenta.
        assert!(transcripts(&root, Duration::ZERO).is_empty());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn une_proyectos_con_tareas_y_conocidos() {
        let known = vec![
            KnownProject { name: "record-mind".into(), directory: "C:/d/record-mind".into() },
            KnownProject { name: "engram-view".into(), directory: "C:/d/engram-view".into() },
            KnownProject { name: "deskia".into(), directory: "C:/d/deskia".into() },
        ];
        let merged = merge_projects(vec![("tienda".into(), [1, 0, 0, 0]), ("record-mind".into(), [0, 0, 0, 2])], &known);
        let names: Vec<&str> = merged.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["tienda", "record-mind", "deskia", "engram-view"]);
        assert_eq!(merged[0].directory, None);
        assert_eq!(merged[1].directory.as_deref(), Some("C:/d/record-mind"));
        assert_eq!(merged[2].counts, [0; 4]);
    }
}
