//! Núcleo de comparación: independiente de la interfaz.
//!
//! Cada archivo JSON se normaliza (indentación de 2 espacios, mismo orden de
//! claves) y se aplana a una lista de hojas `ruta -> (línea, valor)`.
//! Después se unen las rutas de todos los archivos y se marca como cambiada
//! cualquier ruta cuyo valor difiera entre archivos (o falte en alguno).

use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const MIN_FILES: usize = 2;
pub const MAX_FILES: usize = 6;

/// Una hoja del JSON: ruta, número de línea (base 1) y valor renderizado.
#[derive(Debug, Clone, PartialEq)]
pub struct Leaf {
    pub path: String,
    pub line: usize,
    pub value: String,
}

/// Un archivo cargado y aplanado.
#[derive(Debug, Clone)]
pub struct LoadedFile {
    pub path: PathBuf,
    pub name: String,
    pub leaves: Vec<Leaf>,
    /// `true` si el texto original coincide con el formato normalizado, de
    /// modo que los números de línea son exactamente los del archivo.
    pub lines_exact: bool,
    /// Contenido de texto en crudo para visualización en editor y visor git.
    pub content: String,
}

/// Tipo inferido de un valor JSON renderizado para visualización e inspección de diferencias.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonType {
    String,
    Number,
    Boolean,
    Null,
    Array,
    Object,
    Other,
}

impl JsonType {
    pub fn name(self) -> &'static str {
        match self {
            Self::String => "String",
            Self::Number => "Number",
            Self::Boolean => "Boolean",
            Self::Null => "Null",
            Self::Array => "Array",
            Self::Object => "Object",
            Self::Other => "Other",
        }
    }
}

/// Detecta el tipo semántico a partir del valor renderizado en formato JSON normalizado.
pub fn detect_json_type(val: &str) -> JsonType {
    let trimmed = val.trim();
    if trimmed == "null" {
        JsonType::Null
    } else if trimmed == "true" || trimmed == "false" {
        JsonType::Boolean
    } else if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
        JsonType::String
    } else if trimmed == "{}" {
        JsonType::Object
    } else if trimmed == "[]" {
        JsonType::Array
    } else if trimmed.parse::<f64>().is_ok() {
        JsonType::Number
    } else {
        JsonType::Other
    }
}

/// Determina si dos valores renderizados representan tipos de datos diferentes.
pub fn is_type_change(val_a: &str, val_b: &str) -> bool {
    detect_json_type(val_a) != detect_json_type(val_b)
}

/// Celda de una fila: valor de una ruta en un archivo concreto.
#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    pub line: usize,
    pub value: String,
}

impl Cell {
    pub fn json_type(&self) -> JsonType {
        detect_json_type(&self.value)
    }
}

#[derive(Debug, Clone)]
pub struct Row {
    pub path: String,
    /// Una celda por archivo; `None` si la ruta no existe en ese archivo.
    pub cells: Vec<Option<Cell>>,
    pub changed: bool,
}

#[derive(Debug, Clone)]
pub struct Comparison {
    pub files: Vec<LoadedFile>,
    pub rows: Vec<Row>,
    pub base_index: usize,
}

impl Comparison {
    pub fn changed_count(&self) -> usize {
        self.rows.iter().filter(|r| r.changed).count()
    }

    pub fn set_base_index(&mut self, base_index: usize) -> Result<(), String> {
        if base_index >= self.files.len() {
            return Err(format!(
                "Índice de base inválido: {base_index} (total de archivos: {})",
                self.files.len()
            ));
        }
        self.base_index = base_index;
        for row in &mut self.rows {
            let base_val = row.cells[base_index].as_ref().map(|c| &c.value);
            row.changed = row
                .cells
                .iter()
                .any(|c| c.as_ref().map(|c| &c.value) != base_val);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------- carga

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

// -------------------------------------------------------------- aplanado

/// Renderiza el valor con el mismo formato que `serde_json::to_string_pretty`
/// y devuelve el texto junto con las hojas y su línea.
pub fn flatten(value: &Value) -> (String, Vec<Leaf>) {
    let mut lines: Vec<String> = Vec::new();
    let mut leaves = Vec::new();
    walk(value, "", 0, None, false, &mut lines, &mut leaves);
    (lines.join("\n"), leaves)
}

fn scalar(v: &Value) -> String {
    // to_string de un Value escalar (o vacío) produce JSON compacto válido.
    v.to_string()
}

fn walk(
    v: &Value,
    path: &str,
    depth: usize,
    key: Option<&str>,
    comma: bool,
    lines: &mut Vec<String>,
    leaves: &mut Vec<Leaf>,
) {
    let indent = "  ".repeat(depth);
    let prefix = match key {
        Some(k) => format!("{indent}{}: ", Value::String(k.to_string())),
        None => indent.clone(),
    };
    let tail = if comma { "," } else { "" };

    match v {
        Value::Object(map) if !map.is_empty() => {
            lines.push(format!("{prefix}{{"));
            let n = map.len();
            for (i, (k, child)) in map.iter().enumerate() {
                let p = join_key(path, k);
                walk(child, &p, depth + 1, Some(k), i + 1 < n, lines, leaves);
            }
            lines.push(format!("{indent}}}{tail}"));
        }
        Value::Array(arr) if !arr.is_empty() => {
            lines.push(format!("{prefix}["));
            let n = arr.len();
            for (i, child) in arr.iter().enumerate() {
                let p = format!("{path}[{i}]");
                walk(child, &p, depth + 1, None, i + 1 < n, lines, leaves);
            }
            lines.push(format!("{indent}]{tail}"));
        }
        other => {
            let rendered = match other {
                Value::Object(_) => "{}".to_string(),
                Value::Array(_) => "[]".to_string(),
                s => scalar(s),
            };
            lines.push(format!("{prefix}{rendered}{tail}"));
            leaves.push(Leaf {
                path: if path.is_empty() {
                    "$".into()
                } else {
                    path.to_string()
                },
                line: lines.len(),
                value: rendered,
            });
        }
    }
}

fn join_key(path: &str, k: &str) -> String {
    if path.is_empty() {
        k.to_string()
    } else {
        format!("{path}.{k}")
    }
}

// ------------------------------------------------------------ comparación

pub fn compare_with_base(files: Vec<LoadedFile>, base_index: usize) -> Result<Comparison, String> {
    if files.len() < MIN_FILES || files.len() > MAX_FILES {
        return Err(format!(
            "Se necesitan entre {MIN_FILES} y {MAX_FILES} archivos (recibidos: {}).",
            files.len()
        ));
    }
    if base_index >= files.len() {
        return Err(format!(
            "Índice de base inválido: {base_index} (total: {}).",
            files.len()
        ));
    }

    // Orden estable: rutas en orden de primera aparición, archivo por archivo.
    let mut order: Vec<String> = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut per_file: Vec<HashMap<&str, &Leaf>> = Vec::new();
    for f in &files {
        let mut m = HashMap::new();
        for leaf in &f.leaves {
            if !seen.contains_key(&leaf.path) {
                seen.insert(leaf.path.clone(), order.len());
                order.push(leaf.path.clone());
            }
            m.insert(leaf.path.as_str(), leaf);
        }
        per_file.push(m);
    }

    let rows = order
        .into_iter()
        .map(|path| {
            let cells: Vec<Option<Cell>> = per_file
                .iter()
                .map(|m| {
                    m.get(path.as_str()).map(|l| Cell {
                        line: l.line,
                        value: l.value.clone(),
                    })
                })
                .collect();
            let base_val = cells[base_index].as_ref().map(|c| &c.value);
            let changed = cells
                .iter()
                .any(|c| c.as_ref().map(|c| &c.value) != base_val);
            Row {
                path,
                cells,
                changed,
            }
        })
        .collect();

    Ok(Comparison {
        files,
        rows,
        base_index,
    })
}

pub fn compare(files: Vec<LoadedFile>) -> Result<Comparison, String> {
    compare_with_base(files, 0)
}

pub fn compare_paths_with_base(paths: &[PathBuf], base_index: usize) -> Result<Comparison, String> {
    let files = paths
        .iter()
        .map(|p| load_file(p))
        .collect::<Result<Vec<_>, _>>()?;
    compare_with_base(files, base_index)
}

pub fn compare_paths(paths: &[PathBuf]) -> Result<Comparison, String> {
    compare_paths_with_base(paths, 0)
}

/// Comprueba si una fila coincide con el texto de búsqueda (en la ruta o en algún valor).
pub fn matches_filter(row: &Row, query: &str) -> bool {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return true;
    }
    if row.path.to_lowercase().contains(&q) {
        return true;
    }
    row.cells.iter().any(|c| match c {
        Some(cell) => cell.value.to_lowercase().contains(&q),
        None => "(no existe)".contains(&q),
    })
}

// ---------------------------------------------------------------- reporte

/// Reporte en texto plano (modo `--cli`).
pub fn text_report(c: &Comparison, only_changed: bool) -> String {
    let mut out = String::new();
    for (i, f) in c.files.iter().enumerate() {
        out.push_str(&format!("[{}] {}", i + 1, f.path.display()));
        if i == c.base_index {
            out.push_str("  (base)");
        }
        if !f.lines_exact {
            out.push_str("  (líneas según formato normalizado)");
        }
        out.push('\n');
    }
    out.push_str(&format!(
        "\n{} de {} campos con diferencias\n\n",
        c.changed_count(),
        c.rows.len()
    ));
    for r in c.rows.iter().filter(|r| !only_changed || r.changed) {
        out.push_str(&format!(
            "{} {}\n",
            if r.changed { "≠" } else { "=" },
            r.path
        ));
        for (i, cell) in r.cells.iter().enumerate() {
            match cell {
                Some(cell) => {
                    out.push_str(&format!("    [{}] L{}: {}\n", i + 1, cell.line, cell.value))
                }
                None => out.push_str(&format!("    [{}] (no existe)\n", i + 1)),
            }
        }
    }
    out
}

/// Exporta el reporte en formato Markdown.
pub fn export_markdown(c: &Comparison, only_changed: bool) -> String {
    let mut out = String::new();
    out.push_str("# Reporte de Diferencias JSON\n\n");
    out.push_str(&format!(
        "**{} de {} campos con diferencias** (Base: [{}] `{}`)\n\n",
        c.changed_count(),
        c.rows.len(),
        c.base_index + 1,
        c.files[c.base_index].name
    ));

    out.push_str("| Estado | Campo |");
    for (i, f) in c.files.iter().enumerate() {
        let is_base = if i == c.base_index { " *(base)*" } else { "" };
        out.push_str(&format!(" [{}] {}{} |", i + 1, f.name, is_base));
    }
    out.push('\n');

    out.push_str("|:---:|:---|");
    for _ in &c.files {
        out.push_str(":---|");
    }
    out.push('\n');

    for r in c.rows.iter().filter(|r| !only_changed || r.changed) {
        out.push_str(&format!(
            "| {} | `{}` |",
            if r.changed { "≠" } else { "=" },
            r.path
        ));
        for cell in &r.cells {
            match cell {
                Some(c) => {
                    out.push_str(&format!(" L{} `{}` |", c.line, c.value.replace('|', "\\|")))
                }
                None => out.push_str(" *(no existe)* |"),
            }
        }
        out.push('\n');
    }
    out
}

/// Exporta el reporte en formato CSV.
pub fn export_csv(c: &Comparison, only_changed: bool) -> String {
    let mut out = String::new();
    out.push_str("Diferencia,Campo");
    for (i, f) in c.files.iter().enumerate() {
        let is_base = if i == c.base_index { " (Base)" } else { "" };
        out.push_str(&format!(
            ",\"[{}] {}{}\"",
            i + 1,
            f.name.replace('"', "\"\""),
            is_base
        ));
    }
    out.push('\n');

    for r in c.rows.iter().filter(|r| !only_changed || r.changed) {
        out.push_str(&format!(
            "{},\"{}\"",
            if r.changed { "SI" } else { "NO" },
            r.path.replace('"', "\"\"")
        ));
        for cell in &r.cells {
            match cell {
                Some(c) => {
                    let text = format!("L{}: {}", c.line, c.value);
                    out.push_str(&format!(",\"{}\"", text.replace('"', "\"\"")));
                }
                None => out.push_str(",\"(no existe)\""),
            }
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(json: &str, name: &str) -> LoadedFile {
        let v: Value = serde_json::from_str(json).unwrap();
        let (rendered, leaves) = flatten(&v);
        LoadedFile {
            path: PathBuf::from(name),
            name: name.into(),
            leaves,
            lines_exact: true,
            content: rendered,
        }
    }

    #[test]
    fn flatten_matches_serde_pretty() {
        let src = r#"{"a":1,"b":{"c":[1,2,{"d":"x\\y"}],"e":{},"f":[]},"g":null}"#;
        let v: Value = serde_json::from_str(src).unwrap();
        let (rendered, _) = flatten(&v);
        assert_eq!(rendered, serde_json::to_string_pretty(&v).unwrap());
    }

    #[test]
    fn line_numbers_are_correct() {
        let v: Value = serde_json::from_str(r#"{"a":1,"b":{"c":2}}"#).unwrap();
        let (_, leaves) = flatten(&v);
        assert_eq!(
            leaves[0],
            Leaf {
                path: "a".into(),
                line: 2,
                value: "1".into()
            }
        );
        assert_eq!(leaves[1].path, "b.c");
        assert_eq!(leaves[1].line, 4);
    }

    #[test]
    fn detects_changes_missing_and_type() {
        let a = load(r#"{"x":"17","y":1,"z":"s"}"#, "a");
        let b = load(r#"{"x":17,"y":1,"w":true}"#, "b");
        let c = compare(vec![a, b]).unwrap();
        let by = |p: &str| c.rows.iter().find(|r| r.path == p).unwrap();
        assert!(by("x").changed); // "17" vs 17
        assert!(!by("y").changed);
        assert!(by("z").changed); // falta en b
        assert!(by("w").changed); // falta en a
    }

    #[test]
    fn three_to_six_files() {
        let mk = |n: i32| load(&format!(r#"{{"k":{n}}}"#), "f");
        assert!(compare(vec![mk(1)]).is_err());
        assert!(compare((0..7).map(mk).collect()).is_err());
        let c = compare(vec![mk(1), mk(1), mk(2), mk(1)]).unwrap();
        assert_eq!(c.changed_count(), 1);
    }

    #[test]
    fn test_base_index_selection() {
        let a = load(r#"{"val":10}"#, "a");
        let b = load(r#"{"val":20}"#, "b");
        let c = load(r#"{"val":20}"#, "c");
        let mut comp = compare(vec![a, b, c]).unwrap();
        // Con base en archivo 0 (a: 10):
        // b (20) y c (20) difieren de 10 -> changed = true
        assert_eq!(comp.changed_count(), 1);
        assert!(comp.rows[0].changed);

        // Cambiamos base al archivo 1 (b: 20):
        comp.set_base_index(1).unwrap();
        assert_eq!(comp.base_index, 1);
        // a (10) difiere de 20 -> changed = true
        assert_eq!(comp.changed_count(), 1);
    }

    #[test]
    fn test_matches_filter() {
        let a = load(r#"{"user.name":"Alice","user.age":30}"#, "a");
        let b = load(r#"{"user.name":"Bob","user.age":30}"#, "b");
        let comp = compare(vec![a, b]).unwrap();

        let name_row = comp.rows.iter().find(|r| r.path == "user.name").unwrap();
        let age_row = comp.rows.iter().find(|r| r.path == "user.age").unwrap();

        assert!(matches_filter(name_row, "name"));
        assert!(matches_filter(name_row, "alice"));
        assert!(matches_filter(name_row, "bob"));
        assert!(!matches_filter(name_row, "age"));

        assert!(matches_filter(age_row, "age"));
        assert!(matches_filter(age_row, "30"));
        assert!(!matches_filter(age_row, "alice"));
    }

    #[test]
    fn test_export_markdown_and_csv() {
        let a = load(r#"{"x":1,"y":"hello"}"#, "a");
        let b = load(r#"{"x":2,"y":"hello"}"#, "b");
        let comp = compare(vec![a, b]).unwrap();

        let md = export_markdown(&comp, true);
        assert!(md.contains("# Reporte de Diferencias JSON"));
        assert!(md.contains("`x`"));
        assert!(!md.contains("`y`")); // only_changed = true

        let csv = export_csv(&comp, false);
        assert!(csv.contains("Diferencia,Campo"));
        assert!(csv.contains("SI,\"x\""));
        assert!(csv.contains("NO,\"y\""));
    }

    #[test]
    fn test_format_and_validate_json() {
        let raw = r#"{"b":2,"a":1}"#;
        assert!(validate_json(raw).is_ok());

        let invalid = r#"{"b":2,"a":}"#;
        assert!(validate_json(invalid).is_err());

        let formatted = format_json(raw).unwrap();
        assert!(formatted.contains("{\n  \"b\": 2,\n  \"a\": 1\n}"));

        let file = load_file_from_str(Path::new("test.json"), raw).unwrap();
        assert_eq!(file.content, raw);
        assert_eq!(file.leaves.len(), 2);
    }

    #[test]
    fn test_detect_json_type_and_type_change() {
        assert_eq!(detect_json_type("\"hello\""), JsonType::String);
        assert_eq!(detect_json_type("\"17\""), JsonType::String);
        assert_eq!(detect_json_type("17"), JsonType::Number);
        assert_eq!(detect_json_type("-3.14"), JsonType::Number);
        assert_eq!(detect_json_type("true"), JsonType::Boolean);
        assert_eq!(detect_json_type("false"), JsonType::Boolean);
        assert_eq!(detect_json_type("null"), JsonType::Null);
        assert_eq!(detect_json_type("{}"), JsonType::Object);
        assert_eq!(detect_json_type("[]"), JsonType::Array);

        // Type changes
        assert!(is_type_change("\"17\"", "17"));
        assert!(is_type_change("true", "\"true\""));
        assert!(is_type_change("null", "\"\""));
        assert!(!is_type_change("17", "18"));
        assert!(!is_type_change("\"a\"", "\"b\""));
    }
}
