//! Carga, parsing en memoria, formateo y validación de archivos JSON.

use crate::core::flatten::flatten;
use crate::core::types::LoadedFile;
use serde_json::Value;
use std::fs;
use std::path::Path;

/// Carga y parsea un archivo JSON a partir de una cadena de texto en memoria.
pub fn load_file_from_str(path: &Path, text: &str) -> Result<LoadedFile, String> {
    let clean = text.trim_start_matches('\u{feff}');
    let value: Value = serde_json::from_str(clean)
        .map_err(|e| format!("JSON inválido en {}: {e}", path.display()))?;
    let (rendered, leaves) = flatten(&value);
    let lines_exact = clean.trim_end().replace("\r\n", "\n") == rendered;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    Ok(LoadedFile {
        path: path.to_path_buf(),
        name,
        leaves,
        lines_exact,
        content: text.to_string(),
    })
}

/// Carga y parsea un archivo JSON desde el sistema de archivos.
pub fn load_file(path: &Path) -> Result<LoadedFile, String> {
    let text =
        fs::read_to_string(path).map_err(|e| format!("No se pudo leer {}: {e}", path.display()))?;
    load_file_from_str(path, &text)
}

/// Formatea e indenta una cadena JSON con 2 espacios.
pub fn format_json(text: &str) -> Result<String, String> {
    let clean = text.trim_start_matches('\u{feff}');
    let value: Value =
        serde_json::from_str(clean).map_err(|e| format!("Error al analizar JSON: {e}"))?;
    serde_json::to_string_pretty(&value).map_err(|e| format!("Error al formatear JSON: {e}"))
}

/// Valida si una cadena de texto contiene JSON sintácticamente correcto.
pub fn validate_json(text: &str) -> Result<(), String> {
    let clean = text.trim_start_matches('\u{feff}');
    let _: Value = serde_json::from_str(clean).map_err(|e| format!("JSON inválido: {e}"))?;
    Ok(())
}
