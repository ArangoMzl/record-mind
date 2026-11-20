//! Comandos que invoca la interfaz.

use base64::{Engine, engine::general_purpose::STANDARD as B64};
use rm_core::{
    Decision, DecisionFile, Memory, MemoryRelation, NewTask, ProjectSummary, RuleSuggestion, SessionInfo, Status, Task,
    TaskDetail, TaskEdit, TaskFilter, claude_code, claude_setup, decisions, rules, settings::Settings,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::{AppState, shortcuts};

type Res<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Agente que tiene tomada la tarea ahora mismo (según el historial).
#[derive(Serialize, Clone)]
pub struct Worker {
    pub actor: String,
    /// Cuándo la tomó.
    pub since: String,
    /// Último evento del agente en esta tarea.
    pub last_activity: String,
    /// Último mensaje que dejó (avance o pregunta).
    pub last_message: Option<String>,
}

/// Recorre el historial: un `claimed` abre el turno del agente y cualquier movimiento
/// posterior (mover, liberar, completar, devolver, aprobar) lo cierra.
pub fn current_worker(d: &TaskDetail) -> Option<Worker> {
    if d.task.status != Status::InProgress {
        return None;
    }
    let mut w: Option<Worker> = None;
    for e in &d.events {
        match e.kind.as_str() {
            "claimed" => {
                w = Some(Worker {
                    actor: e.actor.clone(),
                    since: e.created_at.clone(),
                    last_activity: e.created_at.clone(),
                    last_message: None,
                })
            }
            "moved" | "released" | "completed" | "returned" | "approved" => w = None,
            _ => {
                if let Some(w) = w.as_mut().filter(|w| w.actor == e.actor) {
                    w.last_activity = e.created_at.clone();
                    if !e.message.is_empty() {
                        w.last_message = Some(e.message.clone());
                    }
                }
            }
        }
    }
    w
}

#[derive(Serialize)]
pub struct TaskCard {
    #[serde(flatten)]
    pub task: Task,
    /// Quién la está trabajando de verdad. `None` en «Claude trabajando» = nadie la tomó.
    pub worker: Option<Worker>,
    /// Memorias de Engram enlazadas (consultadas + creadas).
    pub memory_count: usize,
    pub memories_created: usize,
    /// Última pregunta o bloqueo que dejó Claude, si es lo más reciente del historial.
    pub blocked: Option<String>,
    /// Veces que la devolvieron.
    pub returns: usize,
    /// Desde cuándo está en su columna (UTC), para filtrar el tablero por día.
    pub status_since: String,
}

#[derive(Serialize)]
pub struct Board {
    pub tasks: Vec<TaskCard>,
    /// Proyectos con tareas y los que conoce Claude Code (estos con contadores a cero).
    pub projects: Vec<ProjectSummary>,
    /// Reglas para CLAUDE.md que Claude sugirió y esperan tu decisión.
    pub rules: Vec<RuleSuggestion>,
    /// Conversaciones con decisiones pendientes (o ya tomadas), la actualizada más recientemente primero.
    pub decisions: Vec<DecisionFile>,
    /// Conversaciones de Claude Code con actividad reciente, la más reciente primero.
    pub sessions: Vec<SessionInfo>,
    pub mcp_url: Option<String>,
    pub engram: bool,
    /// Atajo de captura actual (acelerador de Tauri).
    pub shortcut: String,
}

#[tauri::command]
pub fn board(app: AppHandle, state: State<AppState>) -> Res<Board> {
    let store = state.store();
    let tasks = store.list(&TaskFilter::default()).map_err(err)?;
    let mut cards = Vec::with_capacity(tasks.len());
    for task in tasks {
        let d = store.detail(task.id).map_err(err)?;
        cards.push(card(d));
    }
    let with_tasks = store.projects().map_err(err)?;
    let rules = store
        .rule_suggestions(None)
        .map_err(err)?
        .into_iter()
        .filter(|r| r.status == "pending")
        .collect();
    let decisions = decisions::list(store.root(), None);
    drop(store);
    let mut engram = state.engram();
    let known = claude_code::known_projects(&mut engram);
    let projects = claude_code::merge_projects(with_tasks, &known);
    Ok(Board {
        tasks: cards,
        projects,
        rules,
        decisions,
        sessions: claude_code::live_sessions(&mut engram),
        mcp_url: state.mcp_url.lock().unwrap().clone(),
        engram: engram.available(),
        shortcut: shortcuts::capture_shortcut(&app),
    })
}

#[tauri::command]
pub fn get_shortcuts(app: AppHandle) -> shortcuts::ShortcutsView {
    shortcuts::view(&app)
}

/// Cambia un atajo; devuelve un error legible si no es válido o Windows no deja registrarlo.
#[tauri::command]
pub fn set_shortcut(app: AppHandle, action: shortcuts::Action, shortcut: Option<String>) -> Res<shortcuts::ShortcutsView> {
    shortcuts::set(&app, action, shortcut)
}

#[tauri::command]
pub fn pause_shortcuts(app: AppHandle, paused: bool) {
    shortcuts::pause(&app, paused);
}

#[derive(Serialize)]
pub struct RuleDecision {
    pub rule: RuleSuggestion,
    /// CLAUDE.md donde quedó escrita (solo al aceptar).
    pub path: Option<String>,
    /// false si la regla ya estaba en el archivo.
    pub added: bool,
}

/// Aceptar escribe la regla en el CLAUDE.md del proyecto **antes** de marcarla: si no se puede
/// escribir, la sugerencia sigue pendiente. Descartar solo la marca.
#[tauri::command]
pub fn decide_rule(state: State<AppState>, id: i64, accept: bool, rule: Option<String>) -> Res<RuleDecision> {
    if !accept {
        let rule = state.store().decide_rule(id, false, None).map_err(err)?;
        return Ok(RuleDecision { rule, path: None, added: false });
    }
    let current = state.store().rule(id).map_err(err)?;
    let text = rule.as_deref().map(str::trim).filter(|r| !r.is_empty()).unwrap_or(&current.rule).to_string();
    let dir = claude_code::known_projects(&mut state.engram())
        .into_iter()
        .find(|p| p.name == current.project)
        .map(|p| p.directory)
        .ok_or_else(|| {
            format!(
                "No sé en qué carpeta está «{}». Abre Claude Code en esa carpeta una vez y vuelve a intentarlo.",
                current.project
            )
        })?;
    let (path, added) = rules::add_rule_to_claude_md(std::path::Path::new(&dir), &text).map_err(err)?;
    let rule = state.store().decide_rule(id, true, Some(&text)).map_err(err)?;
    Ok(RuleDecision { rule, path: Some(path.to_string_lossy().into_owned()), added })
}

/// Marca una decisión como tomada desde el tablero (con lo que se decidió, si lo escribiste).
/// Descartar es tomarla con «Descartada».
#[tauri::command]
pub fn resolve_decision(state: State<AppState>, id: String, resolution: Option<String>, discard: bool) -> Res<Decision> {
    let dir = state.store().root().to_path_buf();
    let text = if discard { Some("Descartada".to_string()) } else { resolution };
    decisions::resolve(&dir, &id, text.as_deref(), "tablero", &decisions::now()).map_err(err)
}

#[tauri::command]
pub fn reopen_decision(state: State<AppState>, id: String) -> Res<Decision> {
    let dir = state.store().root().to_path_buf();
    decisions::reopen(&dir, &id, &decisions::now()).map_err(err)
}

/// Abre el .md de una conversación con el programa predeterminado de Windows.
#[tauri::command]
pub fn open_decision_file(state: State<AppState>, id: String) -> Res<()> {
    let dir = state.store().root().to_path_buf();
    let path = decisions::file_path(&dir, &id).map_err(err)?;
    std::process::Command::new("explorer").arg(&path).spawn().map_err(err)?;
    Ok(())
}

/// Aviso si Claude Code usa un puente por stdio (`record-mind.exe mcp`) más viejo que la app.
/// Solo si el MCP de la app está en marcha, que es a donde se cambiaría. Ejecuta el `.exe` con
/// `--version`, así que el tablero lo pide una vez al abrir, no en cada recarga.
#[tauri::command]
pub async fn bridge_notice(state: State<'_, AppState>) -> Res<Option<claude_setup::BridgeNotice>> {
    if state.mcp_url.lock().unwrap().is_none() {
        return Ok(None);
    }
    let dir = state.store().root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        let config = claude_code::config_path()?;
        let settings = Settings::load(&dir);
        claude_setup::bridge_notice(&config, env!("CARGO_PKG_VERSION"), &settings.claude)
    })
    .await
    .map_err(err)
}

/// «Usar el de la app»: cambia el `record-mind` de Claude Code por el MCP de la app. Devuelve la
/// copia de seguridad de `.claude.json`.
#[tauri::command]
pub fn fix_bridge(state: State<AppState>) -> Res<String> {
    let url = state.mcp_url.lock().unwrap().clone().ok_or("El MCP de la app no está en marcha.")?;
    let config = claude_code::config_path().ok_or("No encuentro la configuración de Claude Code.")?;
    let backup = claude_setup::switch_to_app(&config, &url).map_err(err)?;
    Ok(backup.to_string_lossy().into_owned())
}

/// «Dejarlo así»: no vuelve a avisar hasta la siguiente versión de la app.
#[tauri::command]
pub fn dismiss_bridge(state: State<AppState>) -> Res<()> {
    let dir = state.store().root().to_path_buf();
    let mut settings = Settings::load(&dir);
    settings.claude.bridge_dismissed = Some(env!("CARGO_PKG_VERSION").to_string());
    settings.save(&dir).map_err(err)
}

/// Solo las sesiones activas: el tablero las consulta cada poco sin recargar todas las tareas,
/// porque abrir o cerrar una conversación no escribe en la base de tareas.
#[tauri::command]
pub fn live_sessions(state: State<AppState>) -> Vec<SessionInfo> {
    claude_code::live_sessions(&mut state.engram())
}

/// Rama actual de cada carpeta, en el mismo orden. Cambiar de rama no toca la base de tareas,
/// así que el tablero la vuelve a pedir junto con las sesiones.
#[tauri::command]
pub fn git_branches(directories: Vec<String>) -> Vec<Option<String>> {
    directories.iter().map(rm_core::git::current_branch).collect()
}

fn card(d: TaskDetail) -> TaskCard {
    // Una pregunta del agente (último evento, con «?») bloquea la tarjeta hasta que respondas;
    // los demás comentarios son avances y se muestran bajo «Claude está en esto».
    let blocked = match (d.task.status, d.events.last()) {
        (Status::InProgress, Some(e))
            if e.kind == "comment" && e.actor != "user" && e.message.contains(['?', '¿']) =>
        {
            Some(e.message.clone())
        }
        _ => None,
    };
    TaskCard {
        worker: current_worker(&d),
        memory_count: d.memories.len(),
        memories_created: d.memories.iter().filter(|m| m.relation == MemoryRelation::Created).count(),
        returns: d.events.iter().filter(|e| e.kind == "returned").count(),
        blocked,
        status_since: d.status_since().to_owned(),
        task: d.task,
    }
}

#[derive(Serialize)]
pub struct LinkedMemory {
    pub reference: String,
    pub relation: MemoryRelation,
    pub memory: Option<Memory>,
}

#[derive(Serialize)]
pub struct TaskDetailView {
    #[serde(flatten)]
    pub detail: TaskDetail,
    pub worker: Option<Worker>,
    pub linked: Vec<LinkedMemory>,
    pub related: Vec<Memory>,
}

#[tauri::command]
pub fn task_detail(state: State<AppState>, id: i64) -> Res<TaskDetailView> {
    let detail = state.store().detail(id).map_err(err)?;
    let mut engram = state.engram();
    let linked: Vec<LinkedMemory> = detail
        .memories
        .iter()
        .map(|m| LinkedMemory {
            reference: m.engram_ref.clone(),
            relation: m.relation,
            memory: engram.memory(&m.engram_ref),
        })
        .collect();
    let linked_ids: Vec<i64> = linked.iter().filter_map(|l| l.memory.as_ref().map(|m| m.id)).collect();
    let related = engram
        .related_memories(&detail.task.project, &format!("{} {}", detail.task.title, detail.task.note), 4)
        .into_iter()
        .filter(|m| !linked_ids.contains(&m.id))
        .collect();
    Ok(TaskDetailView { worker: current_worker(&detail), detail, linked, related })
}

#[derive(Serialize)]
pub struct CaptureContext {
    /// Conversaciones de Claude Code con actividad reciente, la más reciente primero.
    pub sessions: Vec<SessionInfo>,
    pub projects: Vec<String>,
}

#[tauri::command]
pub fn capture_context(state: State<AppState>) -> Res<CaptureContext> {
    let with_tasks = state.store().projects().map_err(err)?;
    let mut engram = state.engram();
    let known = claude_code::known_projects(&mut engram);
    let mut projects: Vec<String> = claude_code::merge_projects(with_tasks, &known).into_iter().map(|p| p.name).collect();
    let sessions = claude_code::live_sessions(&mut engram);
    // Los proyectos con sesión activa van primero, en el mismo orden que las sesiones.
    let mut live: Vec<String> = Vec::new();
    for s in &sessions {
        if !live.contains(&s.project) {
            live.push(s.project.clone());
        }
    }
    projects.retain(|p| !live.contains(p));
    live.extend(projects);
    Ok(CaptureContext { sessions, projects: live })
}

#[derive(Deserialize)]
pub struct ImageInput {
    /// Contenido en base64 (sin el prefijo data:).
    pub data: String,
    /// png, jpg, webp o gif.
    pub ext: String,
}

#[derive(Deserialize)]
pub struct CaptureInput {
    pub project: String,
    pub note: String,
    pub title: Option<String>,
    pub priority: i64,
    pub images: Vec<ImageInput>,
}

#[tauri::command]
pub fn create_task(state: State<AppState>, input: CaptureInput) -> Res<Task> {
    // La tarea queda enlazada a la conversación activa de su proyecto, si la hay.
    let session_id = claude_code::live_sessions(&mut state.engram())
        .into_iter()
        .find(|s| s.project == input.project)
        .map(|s| s.id);
    let images: Vec<(Vec<u8>, String)> = input
        .images
        .iter()
        .map(|i| B64.decode(i.data.as_bytes()).map(|b| (b, i.ext.to_ascii_lowercase())))
        .collect::<Result<_, _>>()
        .map_err(|e| format!("imagen inválida: {e}"))?;

    let mut store = state.store();
    let task = store
        .create_task(
            NewTask {
                project: input.project,
                title: input.title,
                note: input.note,
                priority: input.priority.clamp(0, 2),
                session_id,
                attachments: Vec::new(),
            },
            "user",
        )
        .map_err(err)?;
    for (bytes, ext) in images {
        store.add_attachment_bytes(task.id, &bytes, &ext).map_err(err)?;
    }
    store.task(task.id).map_err(err)
}

#[tauri::command]
pub fn move_task(state: State<AppState>, id: i64, status: String) -> Res<Task> {
    let to: Status = status.parse().map_err(err)?;
    let mut store = state.store();
    // A mano solo se mueve entre Para revisar y Hecho; el resto de transiciones son de Claude
    // (o de «Devolver con nota», que va por reopen_task).
    let from = store.task(id).map_err(err)?.status;
    if !matches!((from, to), (Status::Review, Status::Done) | (Status::Done, Status::Review)) {
        return Err("solo se puede mover entre Para revisar y Hecho".into());
    }
    store.move_to(id, to, "user").map_err(err)
}

#[tauri::command]
pub fn release_task(state: State<AppState>, id: i64, reason: String) -> Res<Task> {
    state.store().release(id, "user", &reason).map_err(err)
}

/// Marca o desmarca tareas para la próxima pasada de `/pendientes`.
#[tauri::command]
pub fn queue_tasks(state: State<AppState>, ids: Vec<i64>, queued: bool) -> Res<()> {
    let mut store = state.store();
    for id in ids {
        store.set_queued(id, queued).map_err(err)?;
    }
    Ok(())
}

#[tauri::command]
pub fn clear_queue(state: State<AppState>, project: Option<String>) -> Res<usize> {
    state.store().clear_queue(project.as_deref()).map_err(err)
}

#[tauri::command]
pub fn approve_task(state: State<AppState>, id: i64) -> Res<Task> {
    state.store().approve(id, "user").map_err(err)
}

#[tauri::command]
pub fn reopen_task(state: State<AppState>, id: i64, note: String) -> Res<Task> {
    state.store().reopen(id, "user", &note).map_err(err)
}

#[tauri::command]
pub fn comment_task(state: State<AppState>, id: i64, message: String) -> Res<()> {
    state.store().comment(id, "user", &message).map(|_| ()).map_err(err)
}

#[derive(Deserialize)]
pub struct EditInput {
    pub title: Option<String>,
    pub note: Option<String>,
    pub priority: Option<i64>,
    pub project: Option<String>,
}

#[tauri::command]
pub fn update_task(state: State<AppState>, id: i64, edit: EditInput) -> Res<Task> {
    let edit = TaskEdit { title: edit.title, note: edit.note, priority: edit.priority, project: edit.project };
    state.store().update(id, edit, "user").map_err(err)
}

#[tauri::command]
pub fn duplicate_task(state: State<AppState>, id: i64) -> Res<Task> {
    state.store().duplicate(id, "user").map_err(err)
}

#[tauri::command]
pub fn delete_task(state: State<AppState>, id: i64) -> Res<()> {
    state.store().delete(id).map_err(err)
}

#[tauri::command]
pub fn hide_capture(app: AppHandle) {
    if let Some(w) = app.get_webview_window("capture") {
        let _ = w.hide();
    }
}

#[tauri::command]
pub async fn start_snip(app: AppHandle) -> Res<()> {
    crate::snip::start(app).await
}

#[tauri::command]
pub fn snip_source(app: AppHandle) -> Option<crate::snip::SnipSource> {
    crate::snip::source(&app)
}

#[tauri::command]
pub fn snip_ready(app: AppHandle) {
    crate::snip::ready(&app);
}

// Asíncrono: destruye la ventana que lo invoca y puede crear la de captura.
#[tauri::command]
pub async fn finish_snip(app: AppHandle, rect: Option<crate::snip::SnipRect>) -> Res<()> {
    crate::snip::finish(&app, rect)
}

/// Recortes pendientes de añadir a la captura, en el mismo formato que `create_task`.
#[tauri::command]
pub fn take_snips(app: AppHandle) -> Vec<SnipImage> {
    crate::snip::take(&app).into_iter().map(|data| SnipImage { data, ext: "png" }).collect()
}

#[derive(Serialize)]
pub struct SnipImage {
    data: String,
    ext: &'static str,
}

/// Agranda la ventana de captura mientras se anota una imagen y la devuelve a su tamaño
/// normal al terminar. Asíncrono por la misma razón que `open_capture`.
#[tauri::command]
pub async fn resize_capture(app: AppHandle, editing: bool) {
    if let Some(w) = app.get_webview_window("capture") {
        crate::resize_capture(&w, editing);
    }
}

#[tauri::command]
pub fn open_board(app: AppHandle) {
    crate::show_board(&app);
}

// Asíncrono a propósito: puede crear la ventana de captura, y crear ventanas desde un
// comando síncrono se bloquea en Windows (wry#583).
#[tauri::command]
pub async fn open_capture(app: AppHandle) {
    crate::show_capture(&app);
}

/// Abre el dictado de Windows (Win+H). Espera un instante para que el foco ya esté en la nota.
#[tauri::command]
pub async fn start_dictation() -> Res<()> {
    tokio::time::sleep(std::time::Duration::from_millis(120)).await;
    crate::dictation::open()
}
