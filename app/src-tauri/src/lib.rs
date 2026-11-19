//! App de escritorio de record-mind: tablero en vivo, captura rápida con atajo global,
//! icono en la bandeja y servidor MCP embebido para Claude Code.

mod commands;
mod dictation;
mod shortcuts;
mod snip;
mod watcher;

use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use rm_core::{EngramReader, Store};
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, RunEvent, WebviewWindow, WebviewWindowBuilder,
    WindowEvent,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_global_shortcut::ShortcutState;
/// Argumento con el que Windows lanza la app al iniciar sesión: arranca en la bandeja.
const AUTOSTART_ARG: &str = "--autostart";
const DEFAULT_MCP_PORT: u16 = 4545;

pub struct AppState {
    pub store: Arc<Mutex<Store>>,
    pub engram: Mutex<EngramReader>,
    pub mcp_url: Mutex<Option<String>>,
    pub snip: Mutex<snip::SnipState>,
    pub shortcuts: Mutex<shortcuts::ShortcutState>,
}

impl AppState {
    pub fn store(&self) -> std::sync::MutexGuard<'_, Store> {
        self.store.lock().unwrap_or_else(|p| p.into_inner())
    }
    pub fn engram(&self) -> std::sync::MutexGuard<'_, EngramReader> {
        self.engram.lock().unwrap_or_else(|p| p.into_inner())
    }
}

/// El tamaño inicial del tablero (tauri.conf.json) es también su mínimo: solo crece.
///
/// Sin marco, Windows aplica el mínimo al rectángulo exterior, que incluye el borde invisible
/// de redimensionar (~8 px por lado): con el mínimo tal cual, el contenido se podía encoger
/// 16 px. Se suma ese borde. Y en una pantalla más pequeña que el mínimo (p. ej. 1366×768, o
/// 1080p al 150 %) la ventana no cabría ni se podría encoger, así que baja al área de trabajo.
fn set_board_min_size(app: &AppHandle) {
    let Some(w) = app.get_webview_window("main") else { return };
    let Some(cfg) = app.config().app.windows.iter().find(|c| c.label == "main") else { return };
    let (Some(min_w), Some(min_h)) = (cfg.min_width, cfg.min_height) else { return };
    let Ok(scale) = w.scale_factor() else { return };
    let (Ok(outer), Ok(inner)) = (w.outer_size(), w.inner_size()) else { return };
    let frame_w = outer.width.saturating_sub(inner.width) as f64 / scale;
    let frame_h = outer.height.saturating_sub(inner.height) as f64 / scale;

    let (mut want_w, mut want_h) = (min_w, min_h);
    if let Ok(Some(monitor)) = w.current_monitor().or_else(|_| w.primary_monitor()) {
        let area: LogicalSize<f64> = monitor.work_area().size.to_logical(scale);
        if area.width < min_w || area.height < min_h {
            want_w = min_w.min(area.width - frame_w);
            want_h = min_h.min(area.height - frame_h);
            let _ = w.set_size(LogicalSize::new(want_w, want_h));
            let _ = w.center();
        }
    }
    let _ = w.set_min_size(Some(LogicalSize::new(want_w + frame_w, want_h + frame_h)));
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

/// Tamaño de la ventana de captura: el de tauri.conf.json normalmente y uno grande, sin pasarse
/// del monitor, mientras se anota una imagen.
pub fn resize_capture(w: &WebviewWindow, editing: bool) {
    let (width, height) = if editing {
        let (mw, mh) = w
            .current_monitor()
            .ok()
            .flatten()
            .map(|m| {
                let size = m.size().to_logical::<f64>(m.scale_factor());
                (size.width, size.height)
            })
            .unwrap_or((1366.0, 768.0));
        ((mw * 0.92).min(1240.0), (mh * 0.88).min(860.0))
    } else {
        w.app_handle()
            .config()
            .app
            .windows
            .iter()
            .find(|c| c.label == "capture")
            .map(|c| (c.width, c.height))
            .unwrap_or((640.0, 430.0))
    };
    let _ = w.set_size(LogicalSize::new(width, height));
    let _ = w.center();
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
                .with_handler(|app, shortcut, event| {
                    if event.state != ShortcutState::Pressed {
                        return;
                    }
                    match shortcuts::action_for(app, shortcut) {
                        Some(shortcuts::Action::Capture) => show_capture(app),
                        Some(shortcuts::Action::Snip) => {
                            let app = app.clone();
                            tauri::async_runtime::spawn(async move {
                                let _ = snip::start(app).await;
                            });
                        }
                        None => {}
                    }
                })
                .build(),
        )
        .manage(AppState {
            store: store.clone(),
            engram: Mutex::new(EngramReader::open_default()),
            mcp_url: Mutex::new(None),
            snip: Mutex::default(),
            shortcuts: Mutex::default(),
        })
        .invoke_handler(tauri::generate_handler![
            commands::board,
            commands::live_sessions,
            commands::decide_rule,
            commands::start_dictation,
            commands::get_shortcuts,
            commands::set_shortcut,
            commands::pause_shortcuts,
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
            commands::update_task,
            commands::duplicate_task,
            commands::delete_task,
            commands::hide_capture,
            commands::resize_capture,
            commands::start_snip,
            commands::snip_source,
            commands::snip_ready,
            commands::finish_snip,
            commands::take_snips,
            commands::open_board,
            commands::open_capture,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();

            // Las capturas se sirven por el protocolo asset:// solo desde la carpeta de datos.
            app.asset_protocol_scope().allow_directory(&data_dir, true)?;

            // Atajos globales guardados en settings.json (capturar y, si lo hay, recortar).
            shortcuts::init(&handle);

            build_tray(&handle)?;
            set_board_min_size(&handle);

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

            // Conecta con Claude Code sin pasos a mano: registra este MCP en la configuración de
            // usuario e instala la skill /pendientes. El instalador abre la app al terminar, así
            // que queda hecho al instalar; en cada arranque se completa lo que faltara.
            let setup_dir = data_dir.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let report = rm_core::claude_setup::ensure(&setup_dir, &format!("http://{addr}/mcp"));
                for (what, result) in [("MCP", report.mcp), ("skill /pendientes", report.skill)] {
                    match result {
                        Ok(outcome) => tracing::info!("Claude Code, {what}: {outcome:?}"),
                        Err(e) => tracing::warn!("no se pudo configurar {what} en Claude Code: {e}"),
                    }
                }
            });

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
                if window.label() == snip::LABEL {
                    // Alt+F4 durante un recorte: se cancela como con Esc.
                    let _ = snip::finish(window.app_handle(), None);
                } else {
                    let _ = window.hide();
                }
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
    let (capture_text, snip_text) = shortcuts::tray_labels(app);
    let capture = MenuItem::with_id(app, "capture", capture_text, true, None::<&str>)?;
    let snip = MenuItem::with_id(app, "snip", snip_text, true, None::<&str>)?;
    shortcuts::set_tray_items(app, capture.clone(), snip.clone());
    let autostart_on = app.autolaunch().is_enabled().unwrap_or(false);
    let autostart = CheckMenuItem::with_id(app, "autostart", "Iniciar con Windows", true, autostart_on, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Salir", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &open,
            &capture,
            &snip,
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
            "snip" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = snip::start(app).await;
                });
            }
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
