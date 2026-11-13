//! Comandos que invoca la interfaz.

use base64::{Engine, engine::general_purpose::STANDARD as B64};
use rm_core::{Memory, MemoryRelation, NewTask, SessionInfo, Status, Task, TaskDetail, TaskFilter};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::{AppState, CAPTURE_SHORTCUT};

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
}

#[derive(Serialize)]
pub struct ProjectCount {
    pub name: String,
    pub counts: [i64; 4],
}

#[derive(Serialize)]
pub struct Board {
    pub tasks: Vec<TaskCard>,
    pub projects: Vec<ProjectCount>,
    pub active: Option<SessionInfo>,
    pub mcp_url: Option<String>,
    pub engram: bool,
    pub shortcut: &'static str,
}

#[tauri::command]
pub fn board(state: State<AppState>) -> Res<Board> {
    let store = state.store();
    let tasks = store.list(&TaskFilter::default()).map_err(err)?;
    let mut cards = Vec::with_capacity(tasks.len());
    for task in tasks {
        let d = store.detail(task.id).map_err(err)?;
        cards.push(card(d));
    }
    let projects = store
        .projects()
        .map_err(err)?
        .into_iter()
        .map(|(name, counts)| ProjectCount { name, counts })
        .collect();
    drop(store);
    let mut engram = state.engram();
    Ok(Board {
        tasks: cards,
        projects,
        active: engram.active_session(),
        mcp_url: state.mcp_url.lock().unwrap().clone(),
        engram: engram.available(),
        shortcut: CAPTURE_SHORTCUT,
    })
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
    pub active: Option<SessionInfo>,
    pub projects: Vec<String>,
}

#[tauri::command]
pub fn capture_context(state: State<AppState>) -> Res<CaptureContext> {
    let mut projects: Vec<String> = state.store().projects().map_err(err)?.into_iter().map(|(p, _)| p).collect();
    let active = state.engram().active_session();
    if let Some(a) = &active {
        projects.retain(|p| p != &a.project);
        projects.insert(0, a.project.clone());
    }
    Ok(CaptureContext { active, projects })
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
    let active = state.engram().active_session();
    let session_id = active.filter(|s| s.project == input.project).map(|s| s.id);
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
    state.store().move_to(id, to, "user").map_err(err)
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
pub fn open_board(app: AppHandle) {
    crate::show_board(&app);
}

// Asíncrono a propósito: puede crear la ventana de captura, y crear ventanas desde un
// comando síncrono se bloquea en Windows (wry#583).
#[tauri::command]
pub async fn open_capture(app: AppHandle) {
    crate::show_capture(&app);
}
