//! Avisos de versión nueva: consulta las releases de GitHub, junta las notas de todas las
//! versiones posteriores a la instalada y, si se pide, descarga el instalador, comprueba su
//! SHA-256 y lo abre en modo pasivo (instala encima y vuelve a abrir la app), como hace el
//! actualizador oficial de Tauri con su plantilla NSIS.

use std::{io::Write as _, path::PathBuf, sync::Mutex};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};

const RELEASES_API: &str = "https://api.github.com/repos/ArangoMzl/record-mind/releases?per_page=20";
const CURRENT: &str = env!("CARGO_PKG_VERSION");

#[derive(Default)]
pub struct UpdateState {
    /// Último instalador ofrecido: lo único que `install` acepta descargar.
    installer: Mutex<Option<Installer>>,
}

#[derive(Serialize, Clone)]
pub struct Installer {
    pub name: String,
    pub url: String,
    pub size: u64,
    /// SHA-256 publicado (el `digest` del asset en GitHub o el bloque de las notas).
    pub sha256: String,
}

#[derive(Serialize, Clone)]
pub struct ReleaseNotes {
    pub version: String,
    pub name: String,
    pub notes: String,
    pub url: String,
    pub published_at: String,
}

#[derive(Serialize, Clone)]
pub struct UpdateInfo {
    pub current: String,
    /// La más reciente publicada (puede ser la instalada).
    pub latest: String,
    /// Versiones posteriores a la instalada, de la más nueva a la más antigua.
    pub releases: Vec<ReleaseNotes>,
    /// Instalador de la más reciente; `None` si no hay versión nueva o no trae uno verificable.
    pub installer: Option<Installer>,
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    body: Option<String>,
    html_url: String,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    size: u64,
    #[serde(default)]
    digest: Option<String>,
}

fn client() -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(concat!("record-mind/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
}

fn parse_version(tag: &str) -> Option<semver::Version> {
    semver::Version::parse(tag.trim().trim_start_matches('v')).ok()
}

/// SHA-256 del instalador: el `digest` que calcula GitHub o, si falta, la línea
/// `<hex>  <nombre>` del bloque «SHA-256» de las notas.
fn sha256_for(asset: &GhAsset, body: &str) -> Option<String> {
    let valid = |h: &str| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit());
    if let Some(h) = asset.digest.as_deref().and_then(|d| d.strip_prefix("sha256:")).filter(|h| valid(h)) {
        return Some(h.to_ascii_lowercase());
    }
    body.lines().find_map(|l| {
        let mut parts = l.split_whitespace();
        let (hash, name) = (parts.next()?, parts.next()?);
        (name == asset.name && valid(hash)).then(|| hash.to_ascii_lowercase())
    })
}

/// Compara las releases publicadas con `installed` (la versión de esta app en uso normal).
async fn fetch(installed: &str) -> Result<UpdateInfo, String> {
    let current = semver::Version::parse(installed).map_err(|e| e.to_string())?;
    let releases: Vec<GhRelease> = client()
        .map_err(|e| e.to_string())?
        .get(RELEASES_API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("no se pudo consultar GitHub: {e}"))?
        .json()
        .await
        .map_err(|e| format!("respuesta inesperada de GitHub: {e}"))?;

    let mut stable: Vec<(semver::Version, GhRelease)> = releases
        .into_iter()
        .filter(|r| !r.draft && !r.prerelease)
        .filter_map(|r| parse_version(&r.tag_name).map(|v| (v, r)))
        .collect();
    stable.sort_by(|a, b| b.0.cmp(&a.0));

    let latest = stable.first().map(|(v, _)| v.to_string()).unwrap_or_else(|| current.to_string());
    let installer = stable.first().filter(|(v, _)| *v > current).and_then(|(_, r)| {
        let body = r.body.as_deref().unwrap_or_default();
        let asset = r.assets.iter().find(|a| a.name.ends_with("-setup.exe"))?;
        Some(Installer {
            name: asset.name.clone(),
            url: asset.browser_download_url.clone(),
            size: asset.size,
            sha256: sha256_for(asset, body)?,
        })
    });
    let releases = stable
        .into_iter()
        .filter(|(v, _)| *v > current)
        .map(|(v, r)| ReleaseNotes {
            version: v.to_string(),
            name: r.name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| r.tag_name.clone()),
            notes: r.body.unwrap_or_default(),
            url: r.html_url,
            published_at: r.published_at.unwrap_or_default(),
        })
        .collect();

    Ok(UpdateInfo { current: current.to_string(), latest, releases, installer })
}

#[tauri::command]
pub async fn check_update(app: AppHandle) -> Result<UpdateInfo, String> {
    let info = fetch(CURRENT).await?;
    *app.state::<UpdateState>().installer.lock().unwrap_or_else(|p| p.into_inner()) = info.installer.clone();
    Ok(info)
}

#[derive(Serialize, Clone)]
struct Progress {
    downloaded: u64,
    total: u64,
}

/// Descarga el instalador que ofreció `check_update`, comprueba el SHA-256 y lo abre con
/// `/P /R /UPDATE`: instalación pasiva, en modo actualización, que vuelve a abrir record-mind.
/// La app se cierra para no bloquear sus archivos; el instalador cerraría la que siguiera
/// abierta de todos modos.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    let installer = app
        .state::<UpdateState>()
        .installer
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone()
        .ok_or("no hay ninguna actualización pendiente")?;
    if !installer.url.starts_with("https://github.com/ArangoMzl/record-mind/releases/download/") {
        return Err("el instalador no viene de las releases de record-mind".into());
    }

    let mut res = client()
        .map_err(|e| e.to_string())?
        .get(&installer.url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("no se pudo descargar el instalador: {e}"))?;
    let total = res.content_length().unwrap_or(installer.size);
    let mut data = Vec::with_capacity(total as usize);
    let mut last_emit = 0u64;
    while let Some(chunk) = res.chunk().await.map_err(|e| format!("la descarga se cortó: {e}"))? {
        data.extend_from_slice(&chunk);
        let downloaded = data.len() as u64;
        if downloaded - last_emit >= 128 * 1024 || downloaded == total {
            last_emit = downloaded;
            let _ = app.emit_to("main", "update:progress", Progress { downloaded, total });
        }
    }

    let hash: String = Sha256::digest(&data).iter().map(|b| format!("{b:02x}")).collect();
    if hash != installer.sha256 {
        return Err(format!(
            "el SHA-256 del instalador no coincide con el publicado (esperado {}, obtenido {hash}); no se instaló nada",
            installer.sha256
        ));
    }

    let dir: PathBuf = std::env::temp_dir().join("record-mind-update");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(&installer.name);
    std::fs::File::create(&path)
        .and_then(|mut f| f.write_all(&data))
        .map_err(|e| format!("no se pudo guardar el instalador: {e}"))?;

    std::process::Command::new(&path)
        .args(["/P", "/R", "/UPDATE"])
        .spawn()
        .map_err(|e| format!("no se pudo abrir el instalador: {e}"))?;
    tracing::info!("instalando {} desde {}", installer.name, path.display());
    app.exit(0);
    Ok(())
}

/// Abre una página de las notas (la release en GitHub, el README…) en el navegador.
#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    if !url.starts_with("https://") || url.chars().any(|c| c.is_whitespace() || c == '"') {
        return Err("solo se abren enlaces https".into());
    }
    std::process::Command::new("explorer").arg(&url).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str, digest: Option<&str>) -> GhAsset {
        GhAsset { name: name.into(), browser_download_url: String::new(), size: 0, digest: digest.map(Into::into) }
    }

    #[test]
    fn versions_with_or_without_v() {
        assert_eq!(parse_version("v0.5.2"), semver::Version::parse("0.5.2").ok());
        assert_eq!(parse_version("0.10.0"), semver::Version::parse("0.10.0").ok());
        assert!(parse_version("latest").is_none());
        assert!(parse_version("v0.10.0") > parse_version("v0.9.9"));
    }

    #[test]
    fn sha256_from_digest_or_notes() {
        let h = "5445c1c80c1fa56a14bcc103fa0835952b3217159551c2047f63e8377204eefd";
        let a = asset("x_setup.exe", Some(&format!("sha256:{h}")));
        assert_eq!(sha256_for(&a, "").as_deref(), Some(h));

        let body = format!("## SHA-256\n\n```\n{h}  x-setup.exe\n85a8  record-mind.exe\n```\n");
        assert_eq!(sha256_for(&asset("x-setup.exe", None), &body).as_deref(), Some(h));
        assert_eq!(sha256_for(&asset("otro-setup.exe", None), &body), None);
        assert_eq!(sha256_for(&asset("x-setup.exe", Some("sha256:corto")), ""), None);
    }

    /// Contra GitHub de verdad: `cargo test -p record-mind-app -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore]
    async fn real_releases_since_0_5_0() {
        let info = fetch("0.5.0").await.unwrap();
        println!("latest {} · {:?}", info.latest, info.releases.iter().map(|r| &r.version).collect::<Vec<_>>());
        println!("{:?}", info.installer.as_ref().map(|i| (&i.name, &i.sha256, i.size)));
        assert!(info.releases.len() >= 2);
        assert!(info.releases.windows(2).all(|w| parse_version(&w[0].version) > parse_version(&w[1].version)));
        assert!(info.installer.is_some());
        let none = fetch(&info.latest).await.unwrap();
        assert!(none.releases.is_empty() && none.installer.is_none());
    }
}
