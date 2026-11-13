//! App de escritorio de record-mind: tablero en vivo, captura rápida con atajo global,
//! icono en la bandeja y servidor MCP embebido para Claude Code.

mod commands;
mod watcher;

use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use rm_core::{EngramReader, Store};
use tauri::{
    AppHandle, Emitter, Manager, RunEvent, WebviewWindow, WebviewWindowBuilder, WindowEvent,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

pub const CAPTURE_SHORTCUT: &str = "CommandOrControl+Shift+Space";
/// Argumento con el que Windows lanza la app al iniciar sesión: arranca en la bandeja.
const AUTOSTART_ARG: &str = "--autostart";
const DEFAULT_MCP_PORT: u16 = 4545;

pub struct AppState {
    pub store: Arc<Mutex<Store>>,
    pub engram: Mutex<EngramReader>,
    pub mcp_url: Mutex<Option<String>>,
}

impl AppState {
    pub fn store(&self) -> std::sync::MutexGuard<'_, Store> {
        self.store.lock().unwrap_or_else(|p| p.into_inner())
    }
    pub fn engram(&self) -> std::sync::MutexGuard<'_, EngramReader> {
        self.engram.lock().unwrap_or_else(|p| p.into_inner())
    }
}

pub fn show_board(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Muestra la ventana de captura. No existe hasta la primera vez que se abre: así no
/// gasta memoria de WebView2 mientras nadie la usa (`create: false` en tauri.conf.json).
pub fn show_capture(app: &AppHandle) {
    let w = match app.get_webview_window("capture") {
        Some(w) => w,
        None => match create_capture(app) {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!("no se pudo crear la ventana de captura: {e}");
                return;
            }
        },
    };
    let _ = w.center();
    let _ = w.show();
    let _ = w.set_focus();
    // Si la ventana se acaba de crear nadie escucha todavía; la vista carga su contexto al montarse.
    let _ = app.emit_to("capture", "capture:open", ());
}

fn create_capture(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == "capture")
        .ok_or(tauri::Error::WindowNotFound)?;
    WebviewWindowBuilder::from_config(app, config)?.build()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn,record_mind_app=info,rm_mcp=info".into()),
        )
        .init();

    let store = Store::open_default().expect("no se pudo abrir la base de record-mind");
    let data_dir = store.root().to_path_buf();
    let store = Arc::new(Mutex::new(store));

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_board(app)))
        .plugin(tauri_plugin_autostart::Builder::new().arg(AUTOSTART_ARG).build())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        show_capture(app);
                    }
                })
                .build(),
        )
        .manage(AppState {
            store: store.clone(),
            engram: Mutex::new(EngramReader::open_default()),
            mcp_url: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            commands::board,
            commands::task_detail,
            commands::capture_context,
            commands::create_task,
            commands::move_task,
            commands::release_task,
            commands::queue_tasks,
            commands::clear_queue,
            commands::approve_task,
            commands::reopen_task,
            commands::comment_task,
            commands::delete_task,
            commands::hide_capture,
            commands::open_board,
            commands::open_capture,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();

            // Las capturas se sirven por el protocolo asset:// solo desde la carpeta de datos.
            app.asset_protocol_scope().allow_directory(&data_dir, true)?;

            // Atajo global de captura.
            if let Err(e) = app.global_shortcut().register(CAPTURE_SHORTCUT) {
                tracing::warn!("no se pudo registrar el atajo {CAPTURE_SHORTCUT}: {e}");
            }

            build_tray(&handle)?;

            // El tablero nace oculto (tauri.conf.json); al iniciar con Windows se queda en
            // la bandeja con el atajo listo, y en un arranque normal se muestra.
            if !std::env::args().any(|a| a == AUTOSTART_ARG) {
                show_board(&handle);
            }

            // Vigila la base: cualquier escritura (Claude por MCP, la CLI o la propia app)
            // refresca el tablero al instante.
            watcher::spawn(handle.clone(), data_dir.join("record-mind.db"));

            // Servidor MCP embebido (Streamable HTTP) compartiendo la misma base.
            let port = std::env::var("RECORD_MIND_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(DEFAULT_MCP_PORT);
            let addr = SocketAddr::from(([127, 0, 0, 1], port));
            let server = rm_mcp::RecordMind::shared(store.clone(), EngramReader::open_default());
            let mcp_handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                let url = format!("http://{addr}/mcp");
                match tokio::net::TcpListener::bind(addr).await {
                    Ok(probe) => {
                        drop(probe);
                        *mcp_handle.state::<AppState>().mcp_url.lock().unwrap() = Some(url);
                        if let Err(e) = rm_mcp::serve_http(server, addr).await {
                            tracing::error!("servidor MCP detenido: {e}");
                            *mcp_handle.state::<AppState>().mcp_url.lock().unwrap() = None;
                        }
                    }
                    Err(e) => tracing::warn!("puerto MCP {port} ocupado, se omite el servidor embebido: {e}"),
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            // Cerrar el tablero lo esconde en la bandeja; la captura siempre se oculta.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .expect("error al iniciar record-mind")
        .run(|_app, event| {
            if let RunEvent::ExitRequested { api, code, .. } = event {
                // Sin ventanas visibles la app sigue viva en la bandeja; solo «Salir» la cierra.
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Abrir tablero", true, None::<&str>)?;
    let capture = MenuItem::with_id(app, "capture", "Capturar pendiente   Ctrl+Shift+Espacio", true, None::<&str>)?;
    let autostart_on = app.autolaunch().is_enabled().unwrap_or(false);
    let autostart = CheckMenuItem::with_id(app, "autostart", "Iniciar con Windows", true, autostart_on, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Salir", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &open,
            &capture,
            &PredefinedMenuItem::separator(app)?,
            &autostart,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().expect("falta el icono de la app"))
        .tooltip("record-mind")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "open" => show_board(app),
            "capture" => show_capture(app),
            "autostart" => {
                let launcher = app.autolaunch();
                let was_on = launcher.is_enabled().unwrap_or(false);
                let result = if was_on { launcher.disable() } else { launcher.enable() };
                if let Err(e) = result {
                    tracing::warn!("no se pudo cambiar el inicio con Windows: {e}");
                }
                // El menú marca la casilla solo al hacer clic; la dejamos con el estado real.
                let _ = autostart.set_checked(launcher.is_enabled().unwrap_or(was_on));
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_board(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}
