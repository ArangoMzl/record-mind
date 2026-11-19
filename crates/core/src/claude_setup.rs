//! Conecta record-mind con Claude Code sin pasos a mano: registra el MCP de la app en la
//! configuración de usuario (lo mismo que `claude mcp add -s user`) e instala la skill
//! `/pendientes`. La app lo hace al arrancar, así que basta con instalarla.
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
/// La skill va dentro del binario: siempre coincide con la versión de la app.
pub const SKILL: &str = include_str!("../../../claude/skills/pendientes/SKILL.md");

/// Lo que ya se hizo en la configuración de Claude Code, para no deshacer lo que cambie la persona.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ClaudeSetup {
    /// El MCP ya se registró (o ya estaba): si luego falta es que lo quitaron a propósito.
    pub mcp_registered: bool,
    /// Huella de la última skill que instaló la app. Si el archivo sigue igual se puede
    /// actualizar; si cambió es de la persona.
    pub skill_hash: Option<String>,
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
        return Report { mcp: Ok(Outcome::NoClaude), skill: Ok(Outcome::NoClaude) };
    };
    let report = Report {
        mcp: ensure_mcp(&config, mcp_url, &mut settings.claude),
        skill: ensure_skill(&claude, &mut settings.claude),
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

/// Instala o actualiza `skills/pendientes/SKILL.md` dentro de la carpeta de Claude Code.
pub fn ensure_skill(claude_dir: &Path, state: &mut ClaudeSetup) -> crate::Result<Outcome> {
    let path = claude_dir.join("skills").join(SKILL_NAME).join("SKILL.md");
    let ours = fingerprint(SKILL);
    match std::fs::read_to_string(&path) {
        Ok(current) => {
            let theirs = fingerprint(&current);
            if theirs == ours {
                state.skill_hash = Some(ours);
                Ok(Outcome::Present)
            } else if state.skill_hash.as_deref() == Some(theirs.as_str()) {
                write_atomic(&path, SKILL.as_bytes())?;
                state.skill_hash = Some(ours);
                Ok(Outcome::Updated)
            } else {
                Ok(Outcome::LeftAlone)
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if state.skill_hash.is_some() {
                Ok(Outcome::LeftAlone)
            } else if !claude_dir.is_dir() {
                Ok(Outcome::NoClaude)
            } else {
                std::fs::create_dir_all(path.parent().expect("la ruta tiene carpeta"))?;
                write_atomic(&path, SKILL.as_bytes())?;
                state.skill_hash = Some(ours);
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
    fn instala_actualiza_y_respeta_la_skill() {
        let dir = temp("skill");
        let claude = dir.join(".claude");
        let path = claude.join("skills").join("pendientes").join("SKILL.md");
        let mut state = ClaudeSetup::default();

        // Sin carpeta de Claude Code no se instala.
        assert_eq!(ensure_skill(&claude, &mut state).unwrap(), Outcome::NoClaude);

        std::fs::create_dir_all(&claude).unwrap();
        assert_eq!(ensure_skill(&claude, &mut state).unwrap(), Outcome::Installed);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), SKILL);
        assert_eq!(ensure_skill(&claude, &mut state).unwrap(), Outcome::Present);

        // Una versión anterior instalada por la app (sin tocar) se actualiza.
        let old = "versión vieja\n";
        std::fs::write(&path, old).unwrap();
        state.skill_hash = Some(fingerprint(old));
        assert_eq!(ensure_skill(&claude, &mut state).unwrap(), Outcome::Updated);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), SKILL);

        // Editada por la persona: se deja.
        std::fs::write(&path, "mi versión\n").unwrap();
        assert_eq!(ensure_skill(&claude, &mut state).unwrap(), Outcome::LeftAlone);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "mi versión\n");

        // Borrada después de instalarla: no vuelve.
        std::fs::remove_file(&path).unwrap();
        assert_eq!(ensure_skill(&claude, &mut state).unwrap(), Outcome::LeftAlone);
        assert!(!path.exists());

        // La misma skill con CRLF cuenta como igual.
        let crlf = SKILL.replace("\r\n", "\n").replace('\n', "\r\n");
        std::fs::write(&path, &crlf).unwrap();
        assert_eq!(ensure_skill(&claude, &mut ClaudeSetup::default()).unwrap(), Outcome::Present);

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
