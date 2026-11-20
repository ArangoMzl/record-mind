//! Conecta record-mind con Claude Code sin pasos a mano: registra el MCP de la app en la
//! configuración de usuario (lo mismo que `claude mcp add -s user`) e instala las skills
//! `/pendientes` y `/decisiones`. La app lo hace al arrancar, así que basta con instalarla.
//!
//! Respeta lo que decida la persona: si quita el MCP o borra o edita la skill, no se vuelve a
//! tocar. Para saberlo guarda en `settings.json` lo que ya hizo ([`ClaudeSetup`]).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::settings::Settings;

/// Nombre del servidor en `mcpServers` y de la carpeta de la skill.
pub const SERVER_NAME: &str = "record-mind";
const SKILL_NAME: &str = "pendientes";
const DECISIONS_SKILL_NAME: &str = "decisiones";
/// La skill va dentro del binario: siempre coincide con la versión de la app.
pub const SKILL: &str = include_str!("../../../claude/skills/pendientes/SKILL.md");
pub const DECISIONS_SKILL: &str = include_str!("../../../claude/skills/decisiones/SKILL.md");

/// Lo que ya se hizo en la configuración de Claude Code, para no deshacer lo que cambie la persona.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ClaudeSetup {
    /// El MCP ya se registró (o ya estaba): si luego falta es que lo quitaron a propósito.
    pub mcp_registered: bool,
    /// Huella de la última skill que instaló la app. Si el archivo sigue igual se puede
    /// actualizar; si cambió es de la persona.
    pub skill_hash: Option<String>,
    /// Lo mismo para la skill `/decisiones`.
    pub decisions_skill_hash: Option<String>,
    /// Versión de la app en la que la persona eligió «Dejarlo así» ante el aviso de un puente
    /// por stdio viejo: no se vuelve a avisar hasta la siguiente versión.
    pub bridge_dismissed: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Se escribió ahora.
    Installed,
    /// Se reemplazó una versión anterior que la app había instalado.
    Updated,
    /// Ya estaba como debe.
    Present,
    /// La persona lo quitó o lo cambió: no se toca.
    LeftAlone,
    /// No hay Claude Code en este usuario; se reintenta en el próximo arranque.
    NoClaude,
}

#[derive(Debug)]
pub struct Report {
    pub mcp: crate::Result<Outcome>,
    pub skill: crate::Result<Outcome>,
    pub decisions_skill: crate::Result<Outcome>,
    /// `Updated` si el puente por stdio apuntaba a un ejecutable que ya no existe y se cambió
    /// por el de la app.
    pub bridge: crate::Result<Outcome>,
}

/// Carpeta de Claude Code: `$CLAUDE_CONFIG_DIR` o `~/.claude`.
pub fn claude_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR") {
        return Some(PathBuf::from(dir));
    }
    dirs::home_dir().map(|h| h.join(".claude"))
}

/// Registra el MCP en `mcp_url` e instala la skill, y guarda lo hecho en los ajustes de `data_dir`.
pub fn ensure(data_dir: &Path, mcp_url: &str) -> Report {
    let mut settings = Settings::load(data_dir);
    let before = settings.claude.clone();
    let (Some(config), Some(claude)) = (crate::claude_code::config_path(), claude_dir()) else {
        return Report {
            mcp: Ok(Outcome::NoClaude),
            skill: Ok(Outcome::NoClaude),
            decisions_skill: Ok(Outcome::NoClaude),
            bridge: Ok(Outcome::NoClaude),
        };
    };
    let report = Report {
        mcp: ensure_mcp(&config, mcp_url, &mut settings.claude),
        skill: ensure_skill(&claude, SKILL_NAME, SKILL, &mut settings.claude.skill_hash),
        decisions_skill: ensure_skill(&claude, DECISIONS_SKILL_NAME, DECISIONS_SKILL, &mut settings.claude.decisions_skill_hash),
        bridge: repair_missing_bridge(&config, mcp_url),
    };
    if settings.claude != before {
        if let Err(e) = settings.save(data_dir) {
            tracing::warn!("no se pudo guardar el estado de la conexión con Claude Code: {e}");
        }
    }
    report
}

/// Añade `record-mind` a `mcpServers` de `~/.claude.json` si nunca se hizo.
///
/// Solo toca esa clave y conserva el resto del archivo (y su orden). Si el archivo no existe,
/// Claude Code no se ha usado todavía: no se crea, se espera al próximo arranque.
pub fn ensure_mcp(config: &Path, url: &str, state: &mut ClaudeSetup) -> crate::Result<Outcome> {
    if state.mcp_registered {
        let present = read_config(config)?.is_some_and(|v| has_server(&v));
        return Ok(if present { Outcome::Present } else { Outcome::LeftAlone });
    }
    let Some(mut root) = read_config(config)? else { return Ok(Outcome::NoClaude) };
    if has_server(&root) {
        state.mcp_registered = true;
        return Ok(Outcome::Present);
    }
    let obj = root
        .as_object_mut()
        .ok_or_else(|| crate::Error::Invalid(format!("{} no es un objeto JSON", config.display())))?;
    let servers = obj.entry("mcpServers").or_insert_with(|| Value::Object(Map::new()));
    let servers = servers
        .as_object_mut()
        .ok_or_else(|| crate::Error::Invalid("mcpServers no es un objeto".into()))?;
    servers.insert(SERVER_NAME.into(), json!({ "type": "http", "url": url }));
    write_atomic(config, serde_json::to_string_pretty(&root)?.as_bytes())?;
    state.mcp_registered = true;
    Ok(Outcome::Installed)
}

/// Instala o actualiza `skills/<name>/SKILL.md` dentro de la carpeta de Claude Code. `hash` es la
/// huella de la última versión que instaló la app (ver [`ClaudeSetup`]).
pub fn ensure_skill(claude_dir: &Path, name: &str, skill: &str, hash: &mut Option<String>) -> crate::Result<Outcome> {
    let path = claude_dir.join("skills").join(name).join("SKILL.md");
    let ours = fingerprint(skill);
    match std::fs::read_to_string(&path) {
        Ok(current) => {
            let theirs = fingerprint(&current);
            if theirs == ours {
                *hash = Some(ours);
                Ok(Outcome::Present)
            } else if hash.as_deref() == Some(theirs.as_str()) {
                write_atomic(&path, skill.as_bytes())?;
                *hash = Some(ours);
                Ok(Outcome::Updated)
            } else {
                Ok(Outcome::LeftAlone)
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if hash.is_some() {
                Ok(Outcome::LeftAlone)
            } else if !claude_dir.is_dir() {
                Ok(Outcome::NoClaude)
            } else {
                std::fs::create_dir_all(path.parent().expect("la ruta tiene carpeta"))?;
                write_atomic(&path, skill.as_bytes())?;
                *hash = Some(ours);
                Ok(Outcome::Installed)
            }
        }
        Err(e) => Err(e.into()),
    }
}

fn read_config(config: &Path) -> crate::Result<Option<Value>> {
    match std::fs::read_to_string(config) {
        Ok(text) => Ok(Some(serde_json::from_str(&text)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

// ── Puente por stdio ────────────────────────────────────────────────────
//
// Quien registró a mano `record-mind.exe mcp` (README, paso opcional) tiene un puente que no se
// actualiza con la app. No se cambia en silencio, porque se eligió para que Claude funcione con la
// app cerrada: si es más viejo que la app, el tablero avisa y lo cambia con un clic. Solo si el
// ejecutable ya no existe se cambia sin preguntar, porque así no sirve de nada.

/// Un `record-mind` de Claude Code que es un ejecutable de record-mind por stdio.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StdioBridge {
    pub command: String,
    /// La versión que dice el ejecutable (`--version`); `None` si no se pudo saber.
    pub version: Option<String>,
    /// La ruta es absoluta y el archivo ya no existe.
    pub missing: bool,
}

/// El aviso que muestra el tablero.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BridgeNotice {
    pub command: String,
    pub version: String,
    pub app_version: String,
}

/// El puente por stdio de `config`, si lo hay. Otros comandos (`cargo run`, `npx`…) y el MCP por
/// HTTP no cuentan.
pub fn stdio_bridge(config: &Path) -> crate::Result<Option<StdioBridge>> {
    let Some(root) = read_config(config)? else { return Ok(None) };
    let Some(server) = root.get("mcpServers").and_then(|s| s.get(SERVER_NAME)) else { return Ok(None) };
    if server.get("url").is_some() {
        return Ok(None);
    }
    let Some(command) = server.get("command").and_then(Value::as_str) else { return Ok(None) };
    let path = Path::new(command);
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
    if !stem.starts_with("record-mind") {
        return Ok(None);
    }
    let missing = path.is_absolute() && !path.exists();
    let version = if missing { None } else { exe_version(command) };
    Ok(Some(StdioBridge { command: command.to_string(), version, missing }))
}

/// Aviso si el puente por stdio es más viejo que la app y la persona no lo descartó en esta versión.
pub fn bridge_notice(config: &Path, app_version: &str, state: &ClaudeSetup) -> Option<BridgeNotice> {
    if state.bridge_dismissed.as_deref() == Some(app_version) {
        return None;
    }
    let bridge = stdio_bridge(config).ok().flatten()?;
    let version = bridge.version.filter(|v| !bridge.missing && is_older(v, app_version))?;
    Some(BridgeNotice { command: bridge.command, version, app_version: app_version.to_string() })
}

/// Cambia el `record-mind` de Claude Code por el MCP de la app (`url`). Antes guarda una copia del
/// archivo junto a él (`.claude.json.record-mind.bak`) y devuelve su ruta.
pub fn switch_to_app(config: &Path, url: &str) -> crate::Result<PathBuf> {
    let mut root = read_config(config)?
        .ok_or_else(|| crate::Error::Invalid(format!("no existe {}", config.display())))?;
    let servers = root
        .get_mut("mcpServers")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| crate::Error::Invalid("mcpServers no es un objeto".into()))?;
    servers.insert(SERVER_NAME.into(), json!({ "type": "http", "url": url }));
    let name = config.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let backup = config.with_file_name(format!("{name}.record-mind.bak"));
    std::fs::copy(config, &backup)?;
    write_atomic(config, serde_json::to_string_pretty(&root)?.as_bytes())?;
    Ok(backup)
}

/// Si el puente por stdio apunta a un ejecutable que ya no existe, lo cambia por el de la app.
pub fn repair_missing_bridge(config: &Path, url: &str) -> crate::Result<Outcome> {
    match stdio_bridge(config)? {
        Some(b) if b.missing => {
            switch_to_app(config, url)?;
            Ok(Outcome::Updated)
        }
        _ => Ok(Outcome::Present),
    }
}

/// `1.2.3` (también `v1.2.3` o `1.2`) → (1, 2, 3).
pub fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let s = s.trim().trim_start_matches('v');
    let s = s.split(['-', '+']).next()?;
    let mut parts = s.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next().map_or(Some(0), |p| p.parse().ok())?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

/// ¿`version` es anterior a `than`? Si alguna no se entiende, no.
pub fn is_older(version: &str, than: &str) -> bool {
    matches!((parse_version(version), parse_version(than)), (Some(a), Some(b)) if a < b)
}

/// Versión de un ejecutable de record-mind (`record-mind 0.5.0`). Sin ventana de consola.
fn exe_version(command: &str) -> Option<String> {
    let mut cmd = std::process::Command::new(command);
    cmd.arg("--version").stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output().ok()?;
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .find(|w| parse_version(w).is_some())
        .map(str::to_owned)
}

fn has_server(root: &Value) -> bool {
    root.get("mcpServers").and_then(|s| s.get(SERVER_NAME)).is_some()
}

/// Escribe a un temporal junto al destino y lo renombra: Claude Code nunca lee un archivo a medias.
fn write_atomic(path: &Path, bytes: &[u8]) -> crate::Result<()> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = path.with_file_name(format!("{name}.record-mind.tmp"));
    std::fs::write(&tmp, bytes)?;
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(())
}

/// Huella estable (FNV-1a de 64 bits) del texto sin distinguir finales de línea: git puede
/// guardar la skill con CRLF o LF según la máquina donde se compiló.
fn fingerprint(text: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes().filter(|&b| b != b'\r') {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const URL: &str = "http://127.0.0.1:4545/mcp";

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rm-setup-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn registra_el_mcp_sin_tocar_lo_demas() {
        let dir = temp("mcp");
        let config = dir.join(".claude.json");
        let mut state = ClaudeSetup::default();

        // Sin configuración no hay Claude Code: no se crea nada.
        assert_eq!(ensure_mcp(&config, URL, &mut state).unwrap(), Outcome::NoClaude);
        assert!(!config.exists());
        assert!(!state.mcp_registered);

        std::fs::write(&config, r#"{"zeta": 1, "projects": {"C:/x": {}}, "alpha": true}"#).unwrap();
        assert_eq!(ensure_mcp(&config, URL, &mut state).unwrap(), Outcome::Installed);
        assert!(state.mcp_registered);
        let text = std::fs::read_to_string(&config).unwrap();
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["mcpServers"]["record-mind"], json!({ "type": "http", "url": URL }));
        assert_eq!(v["projects"], json!({ "C:/x": {} }));
        // El orden de las claves se conserva.
        assert!(text.find("zeta").unwrap() < text.find("alpha").unwrap());
        assert!(!dir.join(".claude.json.record-mind.tmp").exists());

        // Segunda vez: ya está.
        assert_eq!(ensure_mcp(&config, URL, &mut state).unwrap(), Outcome::Present);

        // Si la persona lo quita, no se vuelve a añadir.
        std::fs::write(&config, r#"{"mcpServers": {}}"#).unwrap();
        assert_eq!(ensure_mcp(&config, URL, &mut state).unwrap(), Outcome::LeftAlone);
        assert_eq!(std::fs::read_to_string(&config).unwrap(), r#"{"mcpServers": {}}"#);

        // Uno registrado a mano (por stdio, por ejemplo) se respeta tal cual.
        let mut fresh = ClaudeSetup::default();
        let manual = r#"{"mcpServers": {"record-mind": {"command": "record-mind.exe", "args": ["mcp"]}}}"#;
        std::fs::write(&config, manual).unwrap();
        assert_eq!(ensure_mcp(&config, URL, &mut fresh).unwrap(), Outcome::Present);
        assert!(fresh.mcp_registered);
        assert_eq!(std::fs::read_to_string(&config).unwrap(), manual);

        // JSON roto: error y archivo intacto.
        std::fs::write(&config, "{roto").unwrap();
        assert!(ensure_mcp(&config, URL, &mut ClaudeSetup::default()).is_err());
        assert_eq!(std::fs::read_to_string(&config).unwrap(), "{roto");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn versiones() {
        assert_eq!(parse_version("0.6.0"), Some((0, 6, 0)));
        assert_eq!(parse_version("v1.2"), Some((1, 2, 0)));
        assert_eq!(parse_version("0.6.0-beta.1"), Some((0, 6, 0)));
        assert_eq!(parse_version("record-mind"), None);
        assert_eq!(parse_version("1.2.3.4"), None);
        assert!(is_older("0.5.0", "0.6.0"));
        assert!(is_older("0.5.5", "0.5.10"));
        assert!(!is_older("0.6.0", "0.6.0"));
        assert!(!is_older("raro", "0.6.0"));
    }

    #[test]
    fn puente_por_stdio() {
        let dir = temp("bridge");
        let config = dir.join(".claude.json");
        let gone = dir.join("no-existe").join("record-mind.exe");
        let gone = gone.to_string_lossy().replace('\\', "/");

        // Por HTTP, sin registrar u otro comando: no es un puente por stdio.
        std::fs::write(&config, format!(r#"{{"mcpServers": {{"record-mind": {{"type": "http", "url": "{URL}"}}}}}}"#)).unwrap();
        assert_eq!(stdio_bridge(&config).unwrap(), None);
        std::fs::write(&config, r#"{"mcpServers": {}}"#).unwrap();
        assert_eq!(stdio_bridge(&config).unwrap(), None);
        std::fs::write(&config, r#"{"mcpServers": {"record-mind": {"command": "cargo", "args": ["run"]}}}"#).unwrap();
        assert_eq!(stdio_bridge(&config).unwrap(), None);
        assert_eq!(repair_missing_bridge(&config, URL).unwrap(), Outcome::Present);

        // Un ejecutable que ya no existe: se cambia solo por el de la app, con copia de seguridad.
        let broken = format!(r#"{{"zeta": 1, "mcpServers": {{"otro": {{}}, "record-mind": {{"type": "stdio", "command": "{gone}", "args": ["mcp"]}}}}, "alpha": 2}}"#);
        std::fs::write(&config, &broken).unwrap();
        let b = stdio_bridge(&config).unwrap().unwrap();
        assert!(b.missing && b.version.is_none());
        assert_eq!(bridge_notice(&config, "0.6.0", &ClaudeSetup::default()), None);
        assert_eq!(repair_missing_bridge(&config, URL).unwrap(), Outcome::Updated);
        let text = std::fs::read_to_string(&config).unwrap();
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["mcpServers"]["record-mind"], json!({ "type": "http", "url": URL }));
        assert_eq!(v["mcpServers"]["otro"], json!({}));
        assert!(text.find("zeta").unwrap() < text.find("alpha").unwrap());
        assert_eq!(std::fs::read_to_string(dir.join(".claude.json.record-mind.bak")).unwrap(), broken);
        assert_eq!(stdio_bridge(&config).unwrap(), None);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Con un `record-mind.exe` de verdad (ruta absoluta):
    /// `RM_TEST_EXE=C:/…/target/debug/record-mind.exe cargo test -p rm-core -- --ignored`.
    #[test]
    #[ignore]
    fn aviso_con_un_ejecutable_real() {
        let exe = std::env::var("RM_TEST_EXE").expect("falta RM_TEST_EXE");
        let exe = std::fs::canonicalize(exe).unwrap().to_string_lossy().replace('\\', "/").replace("//?/", "");
        let dir = temp("real");
        let config = dir.join(".claude.json");
        std::fs::write(&config, format!(r#"{{"mcpServers": {{"record-mind": {{"command": "{exe}", "args": ["mcp"]}}}}}}"#)).unwrap();
        let b = stdio_bridge(&config).unwrap().unwrap();
        let version = b.version.expect("el ejecutable debe decir su versión");
        assert_eq!(bridge_notice(&config, &version, &ClaudeSetup::default()), None);
        let n = bridge_notice(&config, "99.0.0", &ClaudeSetup::default()).unwrap();
        assert_eq!(n.version, version);
        let dismissed = ClaudeSetup { bridge_dismissed: Some("99.0.0".into()), ..Default::default() };
        assert_eq!(bridge_notice(&config, "99.0.0", &dismissed), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn instala_actualiza_y_respeta_la_skill() {
        let dir = temp("skill");
        let claude = dir.join(".claude");
        let path = claude.join("skills").join("pendientes").join("SKILL.md");
        let mut state = ClaudeSetup::default();

        // Sin carpeta de Claude Code no se instala.
        assert_eq!(ensure_skill(&claude, SKILL_NAME, SKILL, &mut state.skill_hash).unwrap(), Outcome::NoClaude);

        std::fs::create_dir_all(&claude).unwrap();
        assert_eq!(ensure_skill(&claude, SKILL_NAME, SKILL, &mut state.skill_hash).unwrap(), Outcome::Installed);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), SKILL);
        assert_eq!(ensure_skill(&claude, SKILL_NAME, SKILL, &mut state.skill_hash).unwrap(), Outcome::Present);

        // Una versión anterior instalada por la app (sin tocar) se actualiza.
        let old = "versión vieja\n";
        std::fs::write(&path, old).unwrap();
        state.skill_hash = Some(fingerprint(old));
        assert_eq!(ensure_skill(&claude, SKILL_NAME, SKILL, &mut state.skill_hash).unwrap(), Outcome::Updated);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), SKILL);

        // Editada por la persona: se deja.
        std::fs::write(&path, "mi versión\n").unwrap();
        assert_eq!(ensure_skill(&claude, SKILL_NAME, SKILL, &mut state.skill_hash).unwrap(), Outcome::LeftAlone);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "mi versión\n");

        // Borrada después de instalarla: no vuelve.
        std::fs::remove_file(&path).unwrap();
        assert_eq!(ensure_skill(&claude, SKILL_NAME, SKILL, &mut state.skill_hash).unwrap(), Outcome::LeftAlone);
        assert!(!path.exists());

        // La misma skill con CRLF cuenta como igual.
        let crlf = SKILL.replace("\r\n", "\n").replace('\n', "\r\n");
        std::fs::write(&path, &crlf).unwrap();
        assert_eq!(ensure_skill(&claude, SKILL_NAME, SKILL, &mut None).unwrap(), Outcome::Present);

        // La de decisiones va en su carpeta, con su propia huella.
        let mut decisions = None;
        assert_eq!(ensure_skill(&claude, DECISIONS_SKILL_NAME, DECISIONS_SKILL, &mut decisions).unwrap(), Outcome::Installed);
        let other = claude.join("skills").join("decisiones").join("SKILL.md");
        assert_eq!(std::fs::read_to_string(&other).unwrap(), DECISIONS_SKILL);
        assert_ne!(decisions, state.skill_hash);

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
