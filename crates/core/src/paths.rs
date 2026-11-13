use std::path::PathBuf;

/// Carpeta de datos de record-mind: `RECORD_MIND_HOME` o `~/.record-mind`.
///
/// No usa `%APPDATA%` a propósito: las apps empaquetadas con MSIX (como Claude Desktop)
/// virtualizan las escrituras en `AppData` hacia una carpeta privada del paquete, así que
/// el MCP lanzado desde Claude Desktop y la app abierta desde el menú Inicio verían bases
/// distintas. La carpeta de usuario no se virtualiza (Engram usa `~/.engram` por lo mismo).
pub fn data_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("RECORD_MIND_HOME") {
        return PathBuf::from(p);
    }
    dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".record-mind")
}

/// Ubicación de las versiones 0.1 (`%APPDATA%\record-mind`), para migrar sus datos.
pub fn legacy_data_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("record-mind"))
}

/// Ruta de la base de Engram, con la misma resolución que usa Engram View:
/// `ENGRAM_DB`, luego `ENGRAM_DATA_DIR/engram.db`, luego `~/.engram/engram.db`.
pub fn engram_db_path() -> PathBuf {
    if let Some(p) = std::env::var_os("ENGRAM_DB") {
        return PathBuf::from(p);
    }
    if let Some(dir) = std::env::var_os("ENGRAM_DATA_DIR") {
        return PathBuf::from(dir).join("engram.db");
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".engram")
        .join("engram.db")
}
