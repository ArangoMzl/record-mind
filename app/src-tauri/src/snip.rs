//! Recorte de pantalla nativo: congela el monitor donde está el ratón, abre encima una ventana
//! a pantalla completa con esa imagen y, cuando eliges un área, la deja como captura en la
//! ventana de «Nuevo pendiente».
//!
//! La captura completa se guarda en memoria (BGRA) para recortar sin pérdidas, y en disco como
//! BMP solo para que la ventana de recorte la muestre: un BMP se escribe al instante, mientras que
//! codificar un PNG de 4K tarda. Solo el área elegida se codifica a PNG.

use std::{path::PathBuf, time::Duration};

use base64::{Engine, engine::general_purpose::STANDARD as B64};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewWindowBuilder};

use crate::AppState;

pub const LABEL: &str = "snip";

struct Frame {
    width: u32,
    height: u32,
    /// Píxeles BGRA de arriba abajo, como los entrega GDI.
    bgra: Vec<u8>,
    file: PathBuf,
}

#[derive(Default)]
pub struct SnipState {
    frame: Option<Frame>,
    /// Si el recorte salió de la ventana de captura, vuelve a ella aunque se cancele.
    reopen_capture: bool,
    /// Recortes listos que la ventana de captura recoge al abrirse (PNG en base64).
    pending: Vec<String>,
}

#[derive(Serialize)]
pub struct SnipSource {
    path: String,
    width: u32,
    height: u32,
}

/// Área elegida, en píxeles físicos de la captura.
#[derive(Deserialize)]
pub struct SnipRect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

fn state(app: &AppHandle) -> std::sync::MutexGuard<'_, SnipState> {
    app.state::<AppState>().inner().snip.lock().unwrap_or_else(|p| p.into_inner())
}

/// Empieza un recorte. Asíncrono: espera a que la ventana de captura desaparezca de la
/// pantalla y crea otra ventana (crearla desde un comando síncrono se bloquea en Windows).
pub async fn start(app: AppHandle) -> Result<(), String> {
    if app.get_webview_window(LABEL).is_some() {
        return Ok(());
    }

    let capture = app.get_webview_window("capture");
    let reopen = capture.as_ref().is_some_and(|w| w.is_visible().unwrap_or(false));
    if let Some(w) = capture.filter(|_| reopen) {
        let _ = w.hide();
        // Lo que tarda Windows en quitarla de la pantalla; si no, sale en la captura.
        tokio::time::sleep(Duration::from_millis(220)).await;
    }

    state(&app).reopen_capture = reopen;
    let result = grab_and_open(&app).await;
    if let Err(e) = &result {
        tracing::warn!("no se pudo recortar la pantalla: {e}");
        let mut s = state(&app);
        if let Some(f) = s.frame.take() {
            let _ = std::fs::remove_file(f.file);
        }
        s.reopen_capture = false;
        drop(s);
        if let Some(w) = app.get_webview_window(LABEL) {
            let _ = w.destroy();
        }
        if reopen {
            crate::show_capture(&app);
        }
    }
    result
}

async fn grab_and_open(app: &AppHandle) -> Result<(), String> {
    // El monitor donde está el ratón.
    let cursor = app.cursor_position().map_err(|e| e.to_string())?;
    let monitor = app
        .monitor_from_point(cursor.x, cursor.y)
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten())
        .ok_or("no se encontró ningún monitor")?;
    let pos = *monitor.position();
    let size = *monitor.size();

    let data_dir = app.state::<AppState>().store().root().to_path_buf();
    let frame = tauri::async_runtime::spawn_blocking(move || -> Result<Frame, String> {
        let mut bgra = grab(pos.x, pos.y, size.width, size.height).map_err(|e| e.to_string())?;
        // GDI deja el canal alfa a 0: se marca opaco para que el BMP se vea igual en cualquier lector.
        bgra.as_chunks_mut::<4>().0.iter_mut().for_each(|px| px[3] = 255);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let file = data_dir.join(format!("snip-{stamp}.bmp"));
        std::fs::write(&file, bmp(size.width, size.height, &bgra)).map_err(|e| e.to_string())?;
        Ok(Frame { width: size.width, height: size.height, bgra, file })
    })
    .await
    .map_err(|e| e.to_string())??;
    state(app).frame = Some(frame);

    // Nace oculta: la interfaz la muestra cuando la imagen ya cargó, así no hay destello.
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == LABEL)
        .ok_or("falta la ventana de recorte en tauri.conf.json")?;
    let w = WebviewWindowBuilder::from_config(app, config)
        .map_err(|e| e.to_string())?
        .build()
        .map_err(|e| e.to_string())?;
    let _ = w.set_position(PhysicalPosition::new(pos.x, pos.y));
    let _ = w.set_size(PhysicalSize::new(size.width, size.height));
    Ok(())
}

pub fn source(app: &AppHandle) -> Option<SnipSource> {
    state(app).frame.as_ref().map(|f| SnipSource {
        path: f.file.to_string_lossy().into_owned(),
        width: f.width,
        height: f.height,
    })
}

/// La imagen ya se ve: se muestra la ventana encima de todo.
pub fn ready(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Termina el recorte: con un área la deja lista para la captura; sin ella, se cancela.
pub fn finish(app: &AppHandle, rect: Option<SnipRect>) -> Result<(), String> {
    if let Some(w) = app.get_webview_window(LABEL) {
        // destroy y no close: la ventana no se reutiliza y así WebView2 libera su memoria.
        let _ = w.destroy();
    }
    let (frame, reopen) = {
        let mut s = state(app);
        (s.frame.take(), std::mem::take(&mut s.reopen_capture))
    };
    let Some(frame) = frame else { return Ok(()) };
    let _ = std::fs::remove_file(&frame.file);

    let picked = rect.is_some();
    if let Some(r) = rect {
        let png = crop_png(&frame, &r)?;
        state(app).pending.push(B64.encode(png));
    }
    if picked || reopen {
        crate::show_capture(app);
    }
    Ok(())
}

/// Recortes que aún no recogió la ventana de captura.
pub fn take(app: &AppHandle) -> Vec<String> {
    std::mem::take(&mut state(app).pending)
}

fn crop_png(f: &Frame, r: &SnipRect) -> Result<Vec<u8>, String> {
    let x = r.x.min(f.width.saturating_sub(1));
    let y = r.y.min(f.height.saturating_sub(1));
    let w = r.width.clamp(1, f.width - x);
    let h = r.height.clamp(1, f.height - y);
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for row in y..y + h {
        let start = ((row * f.width + x) * 4) as usize;
        for px in f.bgra[start..start + (w * 4) as usize].as_chunks::<4>().0 {
            rgba.extend_from_slice(&[px[2], px[1], px[0], 255]);
        }
    }
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(&rgba).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

/// BMP de 32 bits de arriba abajo (alto negativo), justo el formato que entrega GDI.
fn bmp(width: u32, height: u32, bgra: &[u8]) -> Vec<u8> {
    let header = 14 + 40;
    let mut b = Vec::with_capacity(header + bgra.len());
    b.extend_from_slice(b"BM");
    b.extend_from_slice(&((header + bgra.len()) as u32).to_le_bytes());
    b.extend_from_slice(&0u32.to_le_bytes());
    b.extend_from_slice(&(header as u32).to_le_bytes());
    b.extend_from_slice(&40u32.to_le_bytes());
    b.extend_from_slice(&(width as i32).to_le_bytes());
    b.extend_from_slice(&(-(height as i32)).to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&32u16.to_le_bytes());
    b.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
    b.extend_from_slice(&(bgra.len() as u32).to_le_bytes());
    b.extend_from_slice(&[0u8; 16]);
    b.extend_from_slice(bgra);
    b
}

/// Copia un rectángulo de la pantalla (coordenadas físicas del escritorio virtual) en BGRA.
#[cfg(windows)]
fn grab(x: i32, y: i32, width: u32, height: u32) -> anyhow::Result<Vec<u8>> {
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, CreateCompatibleBitmap, CreateCompatibleDC,
        DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, ReleaseDC, SRCCOPY, SelectObject,
    };

    let (w, h) = (width as i32, height as i32);
    let mut buf = vec![0u8; (width * height * 4) as usize];
    // SAFETY: cada objeto GDI que se crea aquí se libera antes de salir, en orden inverso.
    unsafe {
        let screen = GetDC(None);
        anyhow::ensure!(!screen.is_invalid(), "no se pudo abrir la pantalla");
        let mem = CreateCompatibleDC(Some(screen));
        let bitmap = CreateCompatibleBitmap(screen, w, h);
        let old = SelectObject(mem, bitmap.into());
        let blit = BitBlt(mem, 0, 0, w, h, Some(screen), x, y, SRCCOPY | CAPTUREBLT);
        SelectObject(mem, old);

        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let lines = GetDIBits(mem, bitmap, 0, height, Some(buf.as_mut_ptr().cast()), &mut info, DIB_RGB_COLORS);

        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);

        blit?;
        anyhow::ensure!(lines == h, "la captura de pantalla vino incompleta");
    }
    Ok(buf)
}

#[cfg(not(windows))]
fn grab(_x: i32, _y: i32, _width: u32, _height: u32) -> anyhow::Result<Vec<u8>> {
    anyhow::bail!("el recorte de pantalla solo está disponible en Windows")
}
