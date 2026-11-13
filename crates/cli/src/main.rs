//! `record-mind`: captura y revisión de tareas desde la terminal, y servidor MCP para Claude Code.

use std::{net::SocketAddr, path::PathBuf};

use anyhow::Result;
use clap::{Parser, Subcommand};
use rm_core::{EngramReader, NewTask, Status, Store, Task, TaskFilter};

#[derive(Parser)]
#[command(name = "record-mind", version, about = "Tablero de micro-pendientes conectado a Claude Code y Engram")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Captura un pendiente nuevo.
    Add {
        /// Nota: qué hay que corregir.
        note: Vec<String>,
        /// Título corto (por defecto, la primera línea de la nota).
        #[arg(short, long)]
        title: Option<String>,
        /// Captura de pantalla (se puede repetir).
        #[arg(short, long = "image")]
        images: Vec<PathBuf>,
        /// Proyecto. Si falta: la sesión activa de Claude Code en Engram, o la carpeta actual.
        #[arg(short, long)]
        project: Option<String>,
        /// 0 normal, 1 alta, 2 urgente.
        #[arg(long, default_value_t = 0)]
        priority: i64,
    },
    /// Lista tareas.
    #[command(alias = "ls")]
    List {
        #[arg(short, long)]
        project: Option<String>,
        /// Incluye las que están en revisión y las hechas.
        #[arg(short, long)]
        all: bool,
    },
    /// Muestra una tarea con su historial.
    Show { id: i64 },
    /// Aprueba una tarea en revisión (→ Hecho).
    Approve { id: i64 },
    /// Devuelve una tarea a Capturado con una nota de lo que falta.
    Reopen { id: i64, note: Vec<String> },
    /// Mueve una tarea a otro estado (inbox, review, done). «in_progress» lo pone solo Claude.
    Move { id: i64, status: String },
    /// Saca una tarea de «Claude trabajando» y la devuelve a Capturado.
    Release { id: i64, reason: Vec<String> },
    /// Marca tareas para la próxima pasada de /pendientes (o las desmarca con --off).
    Queue {
        ids: Vec<i64>,
        #[arg(long)]
        off: bool,
    },
    /// Borra una tarea y sus capturas.
    Rm { id: i64 },
    /// Proyectos con tareas.
    Projects,
    /// Servidor MCP por stdio (lo lanza Claude Code).
    Mcp,
    /// Servidor MCP por HTTP en 127.0.0.1.
    Serve {
        #[arg(long, default_value_t = 4545)]
        port: u16,
    },
    /// Rutas de datos y estado de la conexión con Engram.
    Where,
}

#[tokio::main]
async fn main() -> Result<()> {
    // stdout es el canal del protocolo MCP: los logs siempre van a stderr.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn,record_mind=info,rm_mcp=info".into()),
        )
        .init();

    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Mcp => return rm_mcp::serve_stdio(rm_mcp::RecordMind::open_default()?).await,
        Cmd::Serve { port } => {
            let addr = SocketAddr::from(([127, 0, 0, 1], port));
            eprintln!("record-mind MCP → http://{addr}/mcp");
            return rm_mcp::serve_http(rm_mcp::RecordMind::open_default()?, addr).await;
        }
        _ => {}
    }

    let mut store = Store::open_default()?;
    let mut engram = EngramReader::open_default();

    match cli.cmd {
        Cmd::Add { note, title, images, project, priority } => {
            let note = note.join(" ");
            let (project, session_id) = match project {
                Some(p) => (p, None),
                None => match engram.active_session() {
                    Some(s) => (s.project, Some(s.id)),
                    None => (engram.resolve_project(&std::env::current_dir()?.to_string_lossy()), None),
                },
            };
            let t = store.create_task(
                NewTask { project, title, note, priority, session_id, attachments: images },
                "user",
            )?;
            println!("✓ #{} {} · {} · {} captura(s)", t.id, t.title, t.project, t.attachments.len());
        }
        Cmd::List { project, all } => {
            let statuses = if all { vec![] } else { vec![Status::Inbox, Status::InProgress, Status::Review] };
            let tasks = store.list(&TaskFilter { project: project.clone(), statuses, ..Default::default() })?;
            if tasks.is_empty() {
                println!("Sin tareas.");
            }
            let mut current: Option<Status> = None;
            for t in &tasks {
                if current != Some(t.status) {
                    current = Some(t.status);
                    println!("\n{}", t.status.label().to_uppercase());
                }
                print_row(t, project.is_none());
            }
        }
        Cmd::Show { id } => {
            let d = store.detail(id)?;
            let t = &d.task;
            println!("#{} {}\n{} · {} · prioridad {}", t.id, t.title, t.project, t.status, t.priority);
            if !t.note.is_empty() {
                println!("\n{}", t.note);
            }
            for a in &t.attachments {
                println!("📎 {}", a.path.display());
            }
            if let Some(r) = &t.resolution {
                println!("\nResolución: {r}");
                if !t.files_changed.is_empty() {
                    println!("Archivos: {}", t.files_changed.join(", "));
                }
            }
            if !d.memories.is_empty() {
                println!("\nMemorias de Engram:");
                for m in &d.memories {
                    let title = engram.memory(&m.engram_ref).map(|x| x.title).unwrap_or_default();
                    println!("  [{}] {} {}", m.relation.as_str(), m.engram_ref, title);
                }
            }
            println!("\nHistorial:");
            for e in &d.events {
                let msg = if e.message.is_empty() { String::new() } else { format!(" — {}", e.message) };
                println!("  {} {} {}{}", &e.created_at[..19.min(e.created_at.len())], e.actor, e.kind, msg);
            }
        }
        Cmd::Approve { id } => {
            let t = store.approve(id, "user")?;
            println!("✓ #{} → {}", t.id, t.status);
        }
        Cmd::Reopen { id, note } => {
            let t = store.reopen(id, "user", &note.join(" "))?;
            println!("↺ #{} → {}", t.id, t.status);
        }
        Cmd::Move { id, status } => {
            let t = store.move_to(id, status.parse()?, "user")?;
            println!("→ #{} {}", t.id, t.status);
        }
        Cmd::Queue { ids, off } => {
            for id in ids {
                let t = store.set_queued(id, !off)?;
                println!("{} #{} {}", if t.queued { "★" } else { "·" }, t.id, t.title);
            }
        }
        Cmd::Release { id, reason } => {
            let t = store.release(id, "user", &reason.join(" "))?;
            println!("↺ #{} → {}", t.id, t.status);
        }
        Cmd::Rm { id } => {
            store.delete(id)?;
            println!("✗ #{id} borrada");
        }
        Cmd::Projects => {
            for (p, [inbox, doing, review, done]) in store.projects()? {
                println!("{p:<28} capturado {inbox} · trabajando {doing} · revisar {review} · hecho {done}");
            }
        }
        Cmd::Where => {
            println!("Datos:   {}", store.root().display());
            println!("Engram:  {} ({})", engram.path().display(), if engram.available() { "encontrado" } else { "no encontrado" });
            match engram.active_session() {
                Some(s) => println!("Sesión activa: {} · {} · {}", s.project, s.directory, s.id),
                None => println!("Sesión activa: ninguna"),
            }
        }
        Cmd::Mcp | Cmd::Serve { .. } => unreachable!(),
    }
    Ok(())
}

fn print_row(t: &Task, show_project: bool) {
    let mut line = format!("  #{:<4} {}", t.id, t.title);
    if show_project {
        line.push_str(&format!("  · {}", t.project));
    }
    if t.priority > 0 {
        line.push_str(&format!("  {}", "!".repeat(t.priority as usize)));
    }
    if t.queued {
        line.push_str("  ★ en cola");
    }
    if !t.attachments.is_empty() {
        line.push_str(&format!("  📎{}", t.attachments.len()));
    }
    println!("{line}");
}
