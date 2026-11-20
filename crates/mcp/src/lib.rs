//! Servidor MCP de record-mind: le da a Claude Code las herramientas para tomar,
//! resolver y dejar en revisión las tareas del tablero.

use std::{
    fmt::Write as _,
    net::SocketAddr,
    sync::{Arc, Mutex, MutexGuard},
};

use base64::{Engine, engine::general_purpose::STANDARD as B64};
use rm_core::{EngramReader, MemoryRelation, NewTask, Status, Store, Task, TaskDetail, TaskFilter, claude_code, decisions};
use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig},
    schemars, tool, tool_handler, tool_router,
};
use serde::Deserialize;

const ACTOR: &str = "claude";
/// Imágenes más grandes que esto no se envían a Claude (se indica la ruta).
const MAX_IMAGE_BYTES: u64 = 4_500_000;

const INSTRUCTIONS: &str = "record-mind es el tablero de micro-pendientes del usuario: correcciones pequeñas \
(textos, estilos, imágenes que no cargan, detalles que faltan) capturadas con nota y captura de pantalla. \
Flujo: list_tasks (pasa tu cwd) → get_task (incluye capturas y memorias de Engram relacionadas) → claim_task → \
resuelve y verifica → complete_task con un resumen honesto. Nunca marques una tarea como hecha: \
complete_task la deja «Para revisar» y la persona la aprueba o la devuelve con una nota. \
Al terminar una pasada, find_patterns muestra errores que se repiten; si hay uno claro, propón una regla para \
CLAUDE.md con suggest_rule (el usuario la aprueba en el tablero; nunca edites CLAUDE.md por tu cuenta). \
Decisiones: cuando una conversación deja cosas que el usuario debe decidir, la skill /decisiones las registra con \
record_decisions. Si en una conversación se resuelve una decisión abierta del proyecto (list_decisions), ciérrala \
con resolve_decision.";

#[derive(Clone)]
pub struct RecordMind {
    store: Arc<Mutex<Store>>,
    engram: Arc<Mutex<EngramReader>>,
    tool_router: ToolRouter<Self>,
}

// ── Parámetros ──────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ScopeArgs {
    /// Carpeta de trabajo actual (tu cwd). Se usa para deducir el proyecto igual que Engram.
    pub cwd: Option<String>,
    /// Nombre exacto del proyecto. Tiene prioridad sobre cwd.
    pub project: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListArgs {
    #[serde(flatten)]
    pub scope: ScopeArgs,
    /// Estados a incluir: inbox, in_progress, review, done. Por defecto inbox e in_progress.
    pub status: Option<Vec<String>>,
    /// true = solo las que el usuario marcó en el tablero para esta pasada («en cola»).
    pub only_queued: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IdArgs {
    /// Número de la tarea (por ejemplo 12 para #12).
    pub id: i64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CompleteArgs {
    /// Número de la tarea.
    pub id: i64,
    /// Qué se cambió y cómo se verificó, en 1–4 frases. Si algo quedó pendiente, dilo aquí.
    pub summary: String,
    /// Rutas de los archivos modificados, relativas a la raíz del proyecto.
    pub files_changed: Option<Vec<String>>,
    /// IDs o sync_id de memorias que guardaste en Engram (mem_save) a raíz de esta tarea.
    pub memories_created: Option<Vec<MemoryRef>>,
    /// IDs o sync_id de memorias de Engram que consultaste y te sirvieron.
    pub memories_consulted: Option<Vec<MemoryRef>>,
}

/// Referencia a una memoria de Engram: id numérico (12) o sync_id ("obs-…").
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum MemoryRef {
    Id(i64),
    SyncId(String),
}

fn refs(list: Option<Vec<MemoryRef>>) -> Vec<String> {
    list.unwrap_or_default()
        .into_iter()
        .map(|r| match r {
            MemoryRef::Id(n) => n.to_string(),
            MemoryRef::SyncId(s) => s,
        })
        .collect()
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CommentArgs {
    /// Número de la tarea.
    pub id: i64,
    /// Avance, duda o bloqueo. La persona lo verá en el historial de la tarjeta.
    pub message: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateArgs {
    #[serde(flatten)]
    pub scope: ScopeArgs,
    /// Descripción del pendiente: qué está mal y dónde.
    pub note: String,
    /// Título corto opcional; si falta se usa la primera línea de la nota.
    pub title: Option<String>,
    /// 0 normal, 1 alta, 2 urgente.
    pub priority: Option<i64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SuggestRuleArgs {
    #[serde(flatten)]
    pub scope: ScopeArgs,
    /// La regla para CLAUDE.md: una frase imperativa, concreta y comprobable
    /// (p. ej. «Comprueba los tamaños de letra también en el breakpoint móvil»).
    pub rule: String,
    /// Qué error se repite y dónde se vio, en 1–2 frases. El usuario lo lee para decidir.
    pub reason: String,
    /// Tareas que lo demuestran (números).
    pub task_ids: Option<Vec<i64>>,
}

// ── Herramientas ────────────────────────────────────────────────────────

#[tool_router]
impl RecordMind {
    pub fn new(store: Store, engram: EngramReader) -> Self {
        Self::shared(Arc::new(Mutex::new(store)), engram)
    }

    /// Comparte la base con otro dueño (la app de escritorio embebe el servidor así).
    pub fn shared(store: Arc<Mutex<Store>>, engram: EngramReader) -> Self {
        RecordMind {
            store,
            engram: Arc::new(Mutex::new(engram)),
            tool_router: Self::tool_router() + Self::rules_router() + Self::decisions_router(),
        }
    }

    pub fn open_default() -> anyhow::Result<Self> {
        Ok(Self::new(Store::open_default()?, EngramReader::open_default()))
    }

    #[tool(description = "Lista las tareas pendientes del tablero. Pasa tu cwd para ver solo las del proyecto actual. \
        Por defecto muestra las capturadas (inbox) y las que están en curso.")]
    async fn list_tasks(&self, Parameters(args): Parameters<ListArgs>) -> Result<CallToolResult, McpError> {
        let project = self.project_for(&args.scope);
        let statuses = match args.status {
            Some(list) if !list.is_empty() => match list.iter().map(|s| s.parse()).collect::<Result<Vec<Status>, _>>() {
                Ok(s) => s,
                Err(e) => return Ok(fail(e)),
            },
            _ => vec![Status::Inbox, Status::InProgress],
        };
        let filter = TaskFilter {
            project: project.clone(),
            statuses: statuses.clone(),
            queued_only: args.only_queued.unwrap_or(false),
        };
        let tasks = match self.store().list(&filter) {
            Ok(t) => t,
            Err(e) => return Ok(fail(e)),
        };

        let mut out = String::new();
        let scope = project.as_deref().unwrap_or("todos los proyectos");
        let states: Vec<_> = statuses.iter().map(|s| s.as_str()).collect();
        let _ = writeln!(out, "Proyecto: {scope} · estados: {} · {} tarea(s)", states.join(", "), tasks.len());
        let queued: Vec<String> = tasks.iter().filter(|t| t.queued).map(|t| format!("#{}", t.id)).collect();
        if !queued.is_empty() {
            let _ = writeln!(
                out,
                "En cola (el usuario las marcó en el tablero para esta pasada): {}",
                queued.join(", ")
            );
        }
        if tasks.is_empty() {
            out.push_str("\nNo hay tareas con esos filtros.");
        }
        for t in &tasks {
            let _ = write!(out, "\n#{} [{}] {}", t.id, t.status.as_str(), t.title);
            if t.queued {
                out.push_str(" · ★ en cola");
            }
            if project.is_none() {
                let _ = write!(out, " · {}", t.project);
            }
            if t.priority > 0 {
                let _ = write!(out, " · prioridad {}", t.priority);
            }
            if !t.attachments.is_empty() {
                let _ = write!(out, " · {} captura(s)", t.attachments.len());
            }
        }
        if !tasks.is_empty() {
            out.push_str("\n\nUsa get_task para ver la nota, las capturas y las memorias relacionadas.");
        }
        Ok(CallToolResult::success(vec![ContentBlock::text(out)]))
    }

    #[tool(description = "Devuelve una tarea completa: nota del usuario, capturas de pantalla (como imágenes), \
        historial (incluidas las notas con las que la devolvieron) y memorias de Engram relacionadas.")]
    async fn get_task(&self, Parameters(IdArgs { id }): Parameters<IdArgs>) -> Result<CallToolResult, McpError> {
        let detail = match self.store().detail(id) {
            Ok(d) => d,
            Err(e) => return Ok(fail(e)),
        };
        let mut blocks = vec![ContentBlock::text(self.render_detail(&detail))];
        for a in &detail.task.attachments {
            let size = std::fs::metadata(&a.path).map(|m| m.len()).unwrap_or(0);
            if size == 0 || size > MAX_IMAGE_BYTES {
                blocks.push(ContentBlock::text(format!(
                    "(Captura demasiado grande o ilegible, ábrela con Read: {})",
                    a.path.display()
                )));
                continue;
            }
            match std::fs::read(&a.path) {
                Ok(bytes) => blocks.push(ContentBlock::image(B64.encode(bytes), a.mime.clone())),
                Err(e) => blocks.push(ContentBlock::text(format!("(No se pudo leer {}: {e})", a.path.display()))),
            }
        }
        Ok(CallToolResult::success(blocks))
    }

    #[tool(description = "Toma una tarea antes de empezar a trabajarla: pasa de «Capturado» a «Claude trabajando» \
        y el usuario lo ve en vivo en el tablero.")]
    async fn claim_task(&self, Parameters(IdArgs { id }): Parameters<IdArgs>) -> Result<CallToolResult, McpError> {
        Ok(match self.store().claim(id, ACTOR) {
            Ok(t) => ok(format!("Tomaste #{} «{}». Ahora está en «{}».", t.id, t.title, t.status)),
            Err(e) => fail(e),
        })
    }

    #[tool(description = "Marca una tarea como resuelta por Claude: queda «Para revisar» con tu resumen, los archivos \
        tocados y las memorias de Engram usadas o creadas. El usuario la aprueba o la devuelve con una nota.")]
    async fn complete_task(&self, Parameters(a): Parameters<CompleteArgs>) -> Result<CallToolResult, McpError> {
        let created = refs(a.memories_created);
        let consulted = refs(a.memories_consulted);
        let res = self.store().complete(
            a.id,
            ACTOR,
            &a.summary,
            a.files_changed.as_deref().unwrap_or_default(),
            &created,
            &consulted,
        );
        Ok(match res {
            Ok(t) => ok(format!(
                "#{} «{}» quedó «{}». El usuario la revisará en el tablero.",
                t.id, t.title, t.status
            )),
            Err(e) => fail(e),
        })
    }

    #[tool(description = "Agrega un comentario al historial de una tarea: avance, duda para el usuario o motivo de bloqueo.")]
    async fn add_comment(&self, Parameters(a): Parameters<CommentArgs>) -> Result<CallToolResult, McpError> {
        Ok(match self.store().comment(a.id, ACTOR, &a.message) {
            Ok(_) => ok(format!("Comentario agregado a #{}.", a.id)),
            Err(e) => fail(e),
        })
    }

    #[tool(description = "Crea un pendiente nuevo en el tablero. Úsalo cuando detectes algo que corregir y no sea \
        parte de lo que estás haciendo ahora, para que no se pierda.")]
    async fn create_task(&self, Parameters(a): Parameters<CreateArgs>) -> Result<CallToolResult, McpError> {
        let Some(project) = self.project_for(&a.scope) else {
            return Ok(fail("indica cwd o project para saber a qué proyecto pertenece"));
        };
        let new = NewTask {
            project,
            title: a.title,
            note: a.note,
            priority: a.priority.unwrap_or(0).clamp(0, 2),
            session_id: None,
            attachments: Vec::new(),
        };
        Ok(match self.store().create_task(new, ACTOR) {
            Ok(t) => ok(format!("Creada #{} «{}» en {}.", t.id, t.title, t.project)),
            Err(e) => fail(e),
        })
    }
}

#[tool_router(router = rules_router)]
impl RecordMind {
    #[tool(description = "Evidencia de errores que se repiten en un proyecto: tareas que el usuario devolvió (con sus \
        notas), temas que aparecen en varias tareas y reglas ya sugeridas. Úsalo al terminar una pasada de \
        /pendientes para decidir si vale la pena sugerir una regla para CLAUDE.md.")]
    async fn find_patterns(&self, Parameters(scope): Parameters<ScopeArgs>) -> Result<CallToolResult, McpError> {
        let Some(project) = self.project_for(&scope) else {
            return Ok(fail("indica cwd o project"));
        };
        let p = match self.store().patterns(&project) {
            Ok(p) => p,
            Err(e) => return Ok(fail(e)),
        };
        let mut out = String::new();
        let _ = writeln!(out, "# Patrones en {} ({} tareas)", p.project, p.total_tasks);
        out.push_str("\n## Tareas devueltas por el usuario (la señal más fuerte)\n");
        if p.returned.is_empty() {
            out.push_str("Ninguna.\n");
        }
        for t in &p.returned {
            let _ = writeln!(out, "- #{} [{}] {} — nota: {}", t.id, t.status.as_str(), t.title, t.note.replace('\n', " "));
            for n in &t.return_notes {
                let _ = writeln!(out, "  · devuelta: {n}");
            }
        }
        out.push_str("\n## Palabras que se repiten en varias tareas\n");
        if p.themes.is_empty() {
            out.push_str("Ninguna.\n");
        }
        for th in &p.themes {
            let ids: Vec<String> = th.task_ids.iter().map(|i| format!("#{i}")).collect();
            let _ = writeln!(out, "- «{}» en {}", th.word, ids.join(", "));
        }
        out.push_str("\n## Reglas ya sugeridas o aceptadas (no las repitas)\n");
        if p.existing.is_empty() {
            out.push_str("Ninguna.\n");
        }
        for r in &p.existing {
            let _ = writeln!(out, "- [{}] {}", r.status, r.rule);
        }
        out.push_str(
            "\nSugiere una regla con suggest_rule solo si hay un error que de verdad se repite: la misma causa en \
             2 o más tareas, o una devolución que deja clara una convención del proyecto. Una palabra repetida no \
             basta por sí sola: confirma con las notas (get_task). Máximo 2 reglas por pasada. Nunca edites \
             CLAUDE.md tú: el usuario la aprueba en el tablero y la app la escribe.",
        );
        Ok(ok(out))
    }

    #[tool(description = "Sugiere una regla para el CLAUDE.md del proyecto a partir de un error que se repite. Queda \
        pendiente en el tablero; el usuario la acepta (y la app la escribe en CLAUDE.md) o la descarta.")]
    async fn suggest_rule(&self, Parameters(a): Parameters<SuggestRuleArgs>) -> Result<CallToolResult, McpError> {
        let Some(project) = self.project_for(&a.scope) else {
            return Ok(fail("indica cwd o project"));
        };
        let ids = a.task_ids.unwrap_or_default();
        Ok(match self.store().suggest_rule(&project, &a.rule, &a.reason, &ids) {
            Ok(r) => ok(format!(
                "Regla sugerida para {} (sugerencia {}). El usuario la verá en «Reglas sugeridas» del tablero; \
                 no la escribas en CLAUDE.md.",
                r.project, r.id
            )),
            Err(e) => fail(e),
        })
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListDecisionsArgs {
    #[serde(flatten)]
    pub scope: ScopeArgs,
    /// true = incluye también las ya tomadas (por defecto solo las abiertas).
    pub include_resolved: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DecisionInput {
    /// La decisión como pregunta corta y concreta («¿Guardamos los md en ~/.record-mind o en el repo?»).
    pub question: String,
    /// Por qué hay que decidirlo y qué opciones hay, en 1–3 frases.
    pub context: Option<String>,
    /// Lo que recomiendas tú, en una frase, si tienes una recomendación.
    pub recommendation: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RecordDecisionsArgs {
    #[serde(flatten)]
    pub scope: ScopeArgs,
    /// Id de la conversación de Claude Code, si lo conoces. Si falta se usa la conversación del
    /// proyecto con actividad más reciente: así volver a registrar actualiza el mismo archivo.
    pub session_id: Option<String>,
    /// Título corto de la conversación (de qué se trabajó).
    pub title: String,
    /// Resumen de la conversación en 2–5 frases: qué se hizo y qué quedó a medias.
    pub summary: String,
    /// Decisiones que siguen abiertas: lo que el usuario debe decidir y aún no decidió.
    pub decisions: Vec<DecisionInput>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ResolveDecisionArgs {
    /// Id de la decisión tal como lo da list_decisions (p. ej. «record-mind/2026-11-19-e37b0b5e#2»).
    pub id: String,
    /// Qué se decidió, en una frase.
    pub decision: String,
}

#[tool_router(router = decisions_router)]
impl RecordMind {
    #[tool(description = "Lista las decisiones pendientes que quedaron en conversaciones de Claude Code del proyecto \
        (cosas que el usuario debe decidir), con su id. Úsalo antes de record_decisions para no repetirlas y para \
        cerrar con resolve_decision las que ya se resolvieron en esta conversación.")]
    async fn list_decisions(&self, Parameters(a): Parameters<ListDecisionsArgs>) -> Result<CallToolResult, McpError> {
        let Some(project) = self.project_for(&a.scope) else {
            return Ok(fail("indica cwd o project"));
        };
        let all = a.include_resolved.unwrap_or(false);
        let files = decisions::list(&self.data_dir(), Some(&project));
        let mut out = String::new();
        let open: usize = files.iter().map(|f| f.open_count()).sum();
        let _ = writeln!(out, "Decisiones de {project}: {open} abierta(s) en {} conversación(es).", files.len());
        for f in &files {
            let shown: Vec<_> = f.decisions.iter().filter(|d| all || !d.resolved).collect();
            if shown.is_empty() {
                continue;
            }
            let _ = writeln!(out, "\n## {} ({})", f.title, f.updated_at);
            for d in shown {
                let mark = if d.resolved { "x" } else { " " };
                let _ = write!(out, "- [{mark}] {} — id: {}", d.question, d.id);
                if !d.context.is_empty() {
                    let _ = write!(out, "\n  {}", d.context);
                }
                if let Some(r) = &d.recommendation {
                    let _ = write!(out, "\n  Recomendación: {r}");
                }
                if let Some(r) = &d.resolution {
                    let _ = write!(out, "\n  Decisión: {r}");
                }
                out.push('\n');
            }
        }
        if open == 0 && !all {
            out.push_str("\nNo hay decisiones abiertas.");
        }
        Ok(ok(out))
    }

    #[tool(description = "Registra las decisiones que quedaron pendientes en esta conversación, con un resumen. \
        Quedan en un .md que el usuario ve en la pestaña «Decisiones» del tablero. Si esta conversación ya tenía \
        archivo, lo actualiza y no repite preguntas. Las deudas técnicas no van aquí: créalas con create_task.")]
    async fn record_decisions(&self, Parameters(a): Parameters<RecordDecisionsArgs>) -> Result<CallToolResult, McpError> {
        let Some(project) = self.project_for(&a.scope) else {
            return Ok(fail("indica cwd o project"));
        };
        let session_id = a.session_id.filter(|s| !s.trim().is_empty()).or_else(|| {
            claude_code::live_sessions(&mut self.engram()).into_iter().find(|s| s.project == project).map(|s| s.id)
        });
        let input = decisions::RecordInput {
            project,
            session_id,
            title: a.title,
            summary: a.summary,
            decisions: a
                .decisions
                .into_iter()
                .map(|d| decisions::NewDecision { question: d.question, context: d.context, recommendation: d.recommendation })
                .collect(),
        };
        Ok(match decisions::record(&self.data_dir(), input, &decisions::now()) {
            Ok(r) => ok(format!(
                "{} {} · {} nueva(s){} · {} abierta(s) en total. Archivo: {}",
                if r.updated { "Actualizado" } else { "Creado" },
                r.file.key,
                r.added,
                if r.skipped > 0 { format!(", {} ya estaban", r.skipped) } else { String::new() },
                r.file.open_count(),
                r.file.path.display()
            )),
            Err(e) => fail(e),
        })
    }

    #[tool(description = "Marca como tomada una decisión pendiente (de esta u otra conversación) porque en esta \
        conversación quedó resuelta. Usa el id de list_decisions.")]
    async fn resolve_decision(&self, Parameters(a): Parameters<ResolveDecisionArgs>) -> Result<CallToolResult, McpError> {
        Ok(match decisions::resolve(&self.data_dir(), &a.id, Some(&a.decision), "Claude", &decisions::now()) {
            Ok(d) => ok(format!("Decisión tomada: «{}» → {}", d.question, d.resolution.unwrap_or_default())),
            Err(e) => fail(e),
        })
    }
}

// El router del campo: une las herramientas de tareas, las de reglas y las de decisiones (por
// defecto el macro usaría `Self::tool_router()`, que solo tiene las de tareas).
#[tool_handler(router = self.tool_router)]
impl ServerHandler for RecordMind {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("record-mind", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }
}

// ── Internos ────────────────────────────────────────────────────────────

impl RecordMind {
    fn store(&self) -> MutexGuard<'_, Store> {
        self.store.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn data_dir(&self) -> std::path::PathBuf {
        self.store().root().to_path_buf()
    }

    fn engram(&self) -> MutexGuard<'_, EngramReader> {
        self.engram.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn project_for(&self, scope: &ScopeArgs) -> Option<String> {
        if let Some(p) = scope.project.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
            return Some(p.to_string());
        }
        let cwd = scope.cwd.as_deref().map(str::trim).filter(|c| !c.is_empty())?;
        Some(self.engram().resolve_project(cwd))
    }

    fn render_detail(&self, d: &TaskDetail) -> String {
        let t: &Task = &d.task;
        let mut s = String::new();
        let _ = writeln!(s, "# Tarea #{} · {}", t.id, t.title);
        let _ = writeln!(
            s,
            "Proyecto: {} · Estado: {} ({}) · Prioridad: {} · Capturada: {}",
            t.project,
            t.status,
            t.status.as_str(),
            t.priority,
            t.created_at
        );
        let _ = writeln!(s, "\n## Nota del usuario\n{}", if t.note.is_empty() { "(sin nota)" } else { &t.note });

        let returns: Vec<_> = d.events.iter().filter(|e| e.kind == "returned" && !e.message.is_empty()).collect();
        if !returns.is_empty() {
            s.push_str("\n## La devolvieron con estas notas (lo más importante)\n");
            for e in returns {
                let _ = writeln!(s, "- {} · {}", e.created_at, e.message);
            }
        }
        if let Some(r) = &t.resolution {
            let _ = writeln!(s, "\n## Resolución anterior\n{r}");
            if !t.files_changed.is_empty() {
                let _ = writeln!(s, "Archivos: {}", t.files_changed.join(", "));
            }
        }

        let _ = writeln!(
            s,
            "\n## Capturas\n{}",
            match t.attachments.len() {
                0 => "Sin capturas.".to_string(),
                n => format!("{n} imagen(es) a continuación."),
            }
        );

        s.push_str("\n## Historial\n");
        for e in &d.events {
            let msg = if e.message.is_empty() { String::new() } else { format!(" — {}", e.message) };
            let _ = writeln!(s, "- {} · {} · {}{}", e.created_at, e.actor, e.kind, msg);
        }

        let mut engram = self.engram();
        if engram.available() {
            let linked: Vec<&str> = d.memories.iter().map(|m| m.engram_ref.as_str()).collect();
            if !d.memories.is_empty() {
                s.push_str("\n## Memorias de Engram enlazadas\n");
                for m in &d.memories {
                    let rel = match m.relation {
                        MemoryRelation::Created => "creada",
                        MemoryRelation::Consulted => "consultada",
                    };
                    match engram.memory(&m.engram_ref) {
                        Some(mem) => {
                            let _ = writeln!(s, "- [{rel}] #{} [{}] {}", mem.id, mem.kind, mem.title);
                        }
                        None => {
                            let _ = writeln!(s, "- [{rel}] {}", m.engram_ref);
                        }
                    }
                }
            }
            let related: Vec<_> = engram
                .related_memories(&t.project, &format!("{} {}", t.title, t.note), 5)
                .into_iter()
                .filter(|m| {
                    !linked.contains(&m.id.to_string().as_str())
                        && !m.sync_id.as_deref().is_some_and(|sid| linked.contains(&sid))
                })
                .collect();
            s.push_str("\n## Memorias de Engram posiblemente relacionadas\n");
            if related.is_empty() {
                s.push_str("Ninguna por coincidencia de texto. Si el problema no es trivial, prueba mem_search de Engram.\n");
            } else {
                for m in &related {
                    let snippet: String = m.snippet.split_whitespace().collect::<Vec<_>>().join(" ");
                    let snippet: String = snippet.chars().take(220).collect();
                    let _ = writeln!(s, "- #{} [{}] {} — {}…", m.id, m.kind, m.title, snippet);
                }
                s.push_str("Lee las útiles completas con mem_get_observation(id) de Engram.\n");
            }
        }
        s
    }
}

fn ok(text: impl Into<String>) -> CallToolResult {
    CallToolResult::success(vec![ContentBlock::text(text.into())])
}

fn fail(e: impl std::fmt::Display) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(format!("Error: {e}"))])
}

// ── Transportes ─────────────────────────────────────────────────────────

/// MCP por stdio: Claude Code lanza el proceso por su cuenta.
pub async fn serve_stdio(server: RecordMind) -> anyhow::Result<()> {
    let running = server.serve(rmcp::transport::stdio()).await?;
    running.waiting().await?;
    Ok(())
}

/// MCP por HTTP (Streamable HTTP) en `addr`, ruta `/mcp`.
pub async fn serve_http(server: RecordMind, addr: SocketAddr) -> anyhow::Result<()> {
    use rmcp::transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    };
    let service: StreamableHttpService<RecordMind, LocalSessionManager> = StreamableHttpService::new(
        move || Ok(server.clone()),
        Default::default(),
        StreamableHttpServerConfig::default(),
    );
    let router = axum::Router::new().nest_service("/mcp", service);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("record-mind MCP escuchando en http://{addr}/mcp");
    axum::serve(listener, router)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
