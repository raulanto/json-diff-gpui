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
pub const MAX_FILES: usize = 10;

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

/// Segmento de una ruta o patrón de consulta JSON.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathSegment {
    /// Nombre de clave de objeto (p. ej. "data", "user", "name").
    Field(String),
    /// Índice numérico de array (p. ej. "[0]", "[1]").
    Index(usize),
    /// Comodín de nivel único: "*" o "[*]" (coincide con cualquier campo o índice).
    Wildcard,
    /// Descenso recursivo: "**" o ".." (coincide con 0 o más niveles en cualquier profundidad).
    Recursive,
}

/// Patrón individual compilado de una consulta (ej. "$.data.items[*].id").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectorPattern {
    pub segments: Vec<PathSegment>,
}

impl SelectorPattern {
    pub fn matches(&self, target: &[PathSegment]) -> bool {
        match_pattern_segments(&self.segments, target)
    }
}

/// Selector / extractor de estructuras JSON (estilo JSONPath).
///
/// Permite acotar la comparación a subárboles concretos de los documentos JSON.
///
/// Sintaxis soportada:
/// - `"$"` o vacío `""`: selecciona todo el documento JSON (comportamiento por defecto).
/// - `"$.data"` o `"data"`: enfoca la comparación en el objeto/subárbol `data`.
/// - `"$.data.user"`: navegación por propiedades anidadas.
/// - `"$.items[0]"`: acceso por índice a un elemento específico del array.
/// - `"$.items[*]"`: comodín para cualquier elemento del array.
/// - `"$.items[*].id"`: campo `id` en cualquiera de los elementos de `items`.
/// - `"$.*.name"`: propiedad `name` en cualquier objeto de primer nivel.
/// - `"$.**.id"` o `"$..id"`: búsqueda recursiva de campos `id` en cualquier nivel.
/// - `"$.data, $.meta"`: unión de múltiples selectores separados por comas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonSelector {
    /// Expresión original ingresada por el usuario (e.g. "$.data").
    pub raw: String,
    /// Patrones compilados.
    pub patterns: Vec<SelectorPattern>,
}

impl JsonSelector {
    /// Crea el selector raíz por defecto ("$"), que selecciona todo el documento.
    pub fn root() -> Self {
        Self {
            raw: "$".to_string(),
            patterns: vec![SelectorPattern {
                segments: Vec::new(),
            }],
        }
    }

    /// Comprueba si el selector actual corresponde a la raíz ("$").
    pub fn is_root(&self) -> bool {
        self.raw == "$"
            || self.patterns.is_empty()
            || (self.patterns.len() == 1 && self.patterns[0].segments.is_empty())
    }

    /// Parsea una cadena de consulta.
    pub fn parse(input: &str) -> Result<Self, String> {
        let trimmed = input.trim();
        if trimmed.is_empty() || trimmed == "$" {
            return Ok(Self::root());
        }

        let mut patterns = Vec::new();
        for part in trimmed.split(',') {
            let p_trim = part.trim();
            if p_trim.is_empty() {
                continue;
            }
            let pattern = parse_single_pattern(p_trim)?;
            patterns.push(pattern);
        }

        if patterns.is_empty() {
            return Ok(Self::root());
        }

        Ok(Self {
            raw: trimmed.to_string(),
            patterns,
        })
    }

    /// Evalúa si una ruta de hoja (ej. "data.user.name", "items[0].id") coincide con el selector.
    pub fn matches(&self, path: &str) -> bool {
        if self.is_root() {
            return true;
        }
        let segments = parse_concrete_path(path);
        self.patterns.iter().any(|p| p.matches(&segments))
    }
}

impl std::fmt::Display for JsonSelector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.raw)
    }
}

/// Descompone una ruta concreta generada durante el aplanado en segmentos.
pub fn parse_concrete_path(path: &str) -> Vec<PathSegment> {
    let clean = path.trim();
    if clean.is_empty() || clean == "$" {
        return vec![PathSegment::Field("$".to_string())];
    }
    let mut segments = Vec::new();
    let mut chars = clean.chars().peekable();
    let mut acc = String::new();

    while let Some(ch) = chars.next() {
        match ch {
            '.' => {
                if !acc.is_empty() {
                    segments.push(PathSegment::Field(std::mem::take(&mut acc)));
                }
            }
            '[' => {
                if !acc.is_empty() {
                    segments.push(PathSegment::Field(std::mem::take(&mut acc)));
                }
                let mut bracket_content = String::new();
                for inner in chars.by_ref() {
                    if inner == ']' {
                        break;
                    }
                    bracket_content.push(inner);
                }
                let inner_trim = bracket_content.trim();
                if let Ok(idx) = inner_trim.parse::<usize>() {
                    segments.push(PathSegment::Index(idx));
                } else if inner_trim == "*" {
                    segments.push(PathSegment::Wildcard);
                } else {
                    let cleaned = inner_trim.trim_matches(|c| c == '\'' || c == '"');
                    segments.push(PathSegment::Field(cleaned.to_string()));
                }
            }
            _ => {
                acc.push(ch);
            }
        }
    }
    if !acc.is_empty() {
        segments.push(PathSegment::Field(acc));
    }
    segments
}

/// Parsea un patrón individual (ej. "$.data.items[*].id" o "$..id").
pub fn parse_single_pattern(pattern_str: &str) -> Result<SelectorPattern, String> {
    let mut s = pattern_str.trim();
    if let Some(rest) = s.strip_prefix('$') {
        s = rest;
    }
    if let Some(rest) = s.strip_prefix('/') {
        s = rest;
    }

    if s.is_empty() {
        return Ok(SelectorPattern {
            segments: Vec::new(),
        });
    }

    if s.starts_with("..") {
        // Descenso recursivo desde la raíz
    } else if let Some(rest) = s.strip_prefix('.') {
        s = rest;
    }

    let mut segments = Vec::new();
    let mut chars = s.chars().peekable();
    let mut acc = String::new();

    let push_acc = |acc: &mut String, segments: &mut Vec<PathSegment>| {
        if !acc.is_empty() {
            if acc == "*" {
                segments.push(PathSegment::Wildcard);
            } else if acc == "**" {
                if !matches!(segments.last(), Some(PathSegment::Recursive)) {
                    segments.push(PathSegment::Recursive);
                }
            } else {
                segments.push(PathSegment::Field(std::mem::take(acc)));
            }
            acc.clear();
        }
    };

    while let Some(ch) = chars.next() {
        match ch {
            '.' => {
                if chars.peek() == Some(&'.') {
                    chars.next(); // consumir el segundo '.'
                    push_acc(&mut acc, &mut segments);
                    if !matches!(segments.last(), Some(PathSegment::Recursive)) {
                        segments.push(PathSegment::Recursive);
                    }
                } else {
                    push_acc(&mut acc, &mut segments);
                }
            }
            '[' => {
                push_acc(&mut acc, &mut segments);
                let mut bracket_content = String::new();
                let mut closed = false;
                for inner in chars.by_ref() {
                    if inner == ']' {
                        closed = true;
                        break;
                    }
                    bracket_content.push(inner);
                }
                if !closed {
                    return Err(format!(
                        "Corchete '[' sin cerrar en el selector: '{pattern_str}'"
                    ));
                }
                let inner_trim = bracket_content.trim();
                if inner_trim.is_empty() {
                    return Err(format!("Índice vacío '[]' en el selector: '{pattern_str}'"));
                }
                if inner_trim == "*" {
                    segments.push(PathSegment::Wildcard);
                } else if inner_trim == "**" {
                    if !matches!(segments.last(), Some(PathSegment::Recursive)) {
                        segments.push(PathSegment::Recursive);
                    }
                } else if let Ok(idx) = inner_trim.parse::<usize>() {
                    segments.push(PathSegment::Index(idx));
                } else {
                    let cleaned = inner_trim.trim_matches(|c| c == '\'' || c == '"');
                    segments.push(PathSegment::Field(cleaned.to_string()));
                }
            }
            _ => {
                acc.push(ch);
            }
        }
    }
    push_acc(&mut acc, &mut segments);

    Ok(SelectorPattern { segments })
}

/// Compara recursivamente los segmentos del patrón contra la ruta objetivo.
fn match_pattern_segments(pattern: &[PathSegment], target: &[PathSegment]) -> bool {
    if pattern.is_empty() {
        return true;
    }

    match &pattern[0] {
        PathSegment::Recursive => {
            let rest = &pattern[1..];
            if rest.is_empty() {
                return true;
            }
            for i in 0..=target.len() {
                if match_pattern_segments(rest, &target[i..]) {
                    return true;
                }
            }
            false
        }
        PathSegment::Wildcard => {
            if target.is_empty() {
                return false;
            }
            match_pattern_segments(&pattern[1..], &target[1..])
        }
        PathSegment::Field(p_field) => {
            if target.is_empty() {
                return false;
            }
            match &target[0] {
                PathSegment::Field(t_field) => {
                    if p_field.eq_ignore_ascii_case(t_field) {
                        match_pattern_segments(&pattern[1..], &target[1..])
                    } else {
                        false
                    }
                }
                _ => false,
            }
        }
        PathSegment::Index(p_idx) => {
            if target.is_empty() {
                return false;
            }
            match &target[0] {
                PathSegment::Index(t_idx) => {
                    if p_idx == t_idx {
                        match_pattern_segments(&pattern[1..], &target[1..])
                    } else {
                        false
                    }
                }
                _ => false,
            }
        }
    }
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

/// Modo de comparación entre archivos JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiffMode {
    /// Todas las versiones se comparan contra un archivo base fijo (`base_index`).
    #[default]
    FixedBase,
    /// Cada versión se compara contra la inmediatamente anterior (1 vs 0, 2 vs 1, 3 vs 2...).
    Sequential,
}

#[derive(Debug, Clone)]
pub struct Comparison {
    pub files: Vec<LoadedFile>,
    pub rows: Vec<Row>,
    pub base_index: usize,
    pub diff_mode: DiffMode,
    pub selector: String,
}

impl Comparison {
    pub fn changed_count(&self) -> usize {
        self.rows.iter().filter(|r| r.changed).count()
    }

    pub fn selector(&self) -> &str {
        &self.selector
    }

    /// Aplica un selector JSONPath (ej. "$.data", "$.items[*].id", "$") y reconstruye las filas.
    pub fn set_selector(&mut self, selector_str: &str) -> Result<(), String> {
        let sel = JsonSelector::parse(selector_str)?;
        self.selector = if selector_str.trim().is_empty() {
            "$".to_string()
        } else {
            selector_str.trim().to_string()
        };
        self.rebuild_rows_with_selector(&sel);
        Ok(())
    }

    /// Reconstruye las filas de comparación filtrando las hojas según el selector provisto.
    pub fn rebuild_rows_with_selector(&mut self, selector: &JsonSelector) {
        let mut order: Vec<String> = Vec::new();
        let mut seen: HashMap<String, usize> = HashMap::new();
        let mut per_file: Vec<HashMap<&str, &Leaf>> = Vec::new();

        for f in &self.files {
            let mut m = HashMap::new();
            for leaf in &f.leaves {
                if selector.matches(&leaf.path) {
                    if !seen.contains_key(&leaf.path) {
                        seen.insert(leaf.path.clone(), order.len());
                        order.push(leaf.path.clone());
                    }
                    m.insert(leaf.path.as_str(), leaf);
                }
            }
            per_file.push(m);
        }

        self.rows = order
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
                Row {
                    path,
                    cells,
                    changed: false,
                }
            })
            .collect();

        self.recompute_changes();
    }

    pub fn set_diff_mode(&mut self, mode: DiffMode) {
        self.diff_mode = mode;
        self.recompute_changes();
    }

    pub fn set_base_index(&mut self, base_index: usize) -> Result<(), String> {
        if base_index >= self.files.len() {
            return Err(format!(
                "Índice de base inválido: {base_index} (total de archivos: {})",
                self.files.len()
            ));
        }
        self.base_index = base_index;
        self.recompute_changes();
        Ok(())
    }

    /// Recalcula el estado `Row.changed` de todas las filas según el `diff_mode` activo.
    pub fn recompute_changes(&mut self) {
        match self.diff_mode {
            DiffMode::FixedBase => {
                let base_index = self.base_index;
                for row in &mut self.rows {
                    let base_val = row
                        .cells
                        .get(base_index)
                        .and_then(|c| c.as_ref())
                        .map(|c| &c.value);
                    row.changed = row
                        .cells
                        .iter()
                        .any(|c| c.as_ref().map(|c| &c.value) != base_val);
                }
            }
            DiffMode::Sequential => {
                for row in &mut self.rows {
                    let mut changed = false;
                    for i in 1..row.cells.len() {
                        let prev_val = row.cells[i - 1].as_ref().map(|c| &c.value);
                        let curr_val = row.cells[i].as_ref().map(|c| &c.value);
                        if prev_val != curr_val {
                            changed = true;
                            break;
                        }
                    }
                    row.changed = changed;
                }
            }
        }
    }

    /// Mueve un archivo de la posición `from` a la posición `to`, reordenando columnas y datos.
    pub fn move_file(&mut self, from: usize, to: usize) -> Result<(), String> {
        if from >= self.files.len() || to >= self.files.len() {
            return Err(format!(
                "Índice fuera de rango: from={from}, to={to} (total: {})",
                self.files.len()
            ));
        }
        if from == to {
            return Ok(());
        }

        let file = self.files.remove(from);
        self.files.insert(to, file);

        for row in &mut self.rows {
            let cell = row.cells.remove(from);
            row.cells.insert(to, cell);
        }

        if self.base_index == from {
            self.base_index = to;
        } else if from < self.base_index && to >= self.base_index {
            self.base_index -= 1;
        } else if from > self.base_index && to <= self.base_index {
            self.base_index += 1;
        }

        self.recompute_changes();
        Ok(())
    }

    /// Intercambia las posiciones de dos archivos `a` y `b`.
    pub fn swap_files(&mut self, a: usize, b: usize) -> Result<(), String> {
        if a >= self.files.len() || b >= self.files.len() {
            return Err(format!(
                "Índices fuera de rango: a={a}, b={b} (total: {})",
                self.files.len()
            ));
        }
        if a == b {
            return Ok(());
        }

        self.files.swap(a, b);
        for row in &mut self.rows {
            row.cells.swap(a, b);
        }

        if self.base_index == a {
            self.base_index = b;
        } else if self.base_index == b {
            self.base_index = a;
        }

        self.recompute_changes();
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
        diff_mode: DiffMode::FixedBase,
        selector: "$".to_string(),
    })
}

pub fn compare(files: Vec<LoadedFile>) -> Result<Comparison, String> {
    compare_with_base(files, 0)
}

pub fn compare_sequential(files: Vec<LoadedFile>) -> Result<Comparison, String> {
    let mut c = compare_with_base(files, 0)?;
    c.set_diff_mode(DiffMode::Sequential);
    Ok(c)
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
        match c.diff_mode {
            DiffMode::FixedBase => {
                if i == c.base_index {
                    out.push_str("  (base)");
                }
            }
            DiffMode::Sequential => {
                if i == 0 {
                    out.push_str("  (origen)");
                } else {
                    out.push_str(&format!("  (vs [{}])", i));
                }
            }
        }
        if !f.lines_exact {
            out.push_str("  (líneas según formato normalizado)");
        }
        out.push('\n');
    }
    if c.selector != "$" {
        out.push_str(&format!("Selector: {}\n", c.selector));
    }
    out.push_str(&format!(
        "\n{} de {} campos con diferencias ({})\n\n",
        c.changed_count(),
        c.rows.len(),
        match c.diff_mode {
            DiffMode::FixedBase => "Modo: Base fija",
            DiffMode::Sequential => "Modo: En cadena secuencial",
        }
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
    if c.selector != "$" {
        out.push_str(&format!("**Selector de estructura:** `{}`\n\n", c.selector));
    }
    match c.diff_mode {
        DiffMode::FixedBase => {
            out.push_str(&format!(
                "**{} de {} campos con diferencias** (Base: [{}] `{}`)\n\n",
                c.changed_count(),
                c.rows.len(),
                c.base_index + 1,
                c.files[c.base_index].name
            ));
        }
        DiffMode::Sequential => {
            out.push_str(&format!(
                "**{} de {} campos con diferencias** (Modo: En cadena secuencial [1] -> ... -> [{}])\n\n",
                c.changed_count(),
                c.rows.len(),
                c.files.len()
            ));
        }
    }

    out.push_str("| Estado | Campo |");
    for (i, f) in c.files.iter().enumerate() {
        let tag = match c.diff_mode {
            DiffMode::FixedBase => {
                if i == c.base_index {
                    " *(base)*".to_string()
                } else {
                    String::new()
                }
            }
            DiffMode::Sequential => {
                if i == 0 {
                    " *(origen)*".to_string()
                } else {
                    format!(" *(vs [{}])*", i)
                }
            }
        };
        out.push_str(&format!(" [{}] {}{} |", i + 1, f.name, tag));
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
    fn test_files_limits_up_to_ten() {
        let mk = |n: i32| load(&format!(r#"{{"k":{n}}}"#), "f");
        assert!(compare(vec![mk(1)]).is_err());
        assert!(compare((0..11).map(mk).collect()).is_err());
        // 10 archivos es válido:
        let ten = compare((0..10).map(mk).collect());
        assert!(ten.is_ok());
        let c = compare(vec![mk(1), mk(1), mk(2), mk(1)]).unwrap();
        assert_eq!(c.changed_count(), 1);
    }

    #[test]
    fn test_sequential_diff_mode() {
        // a: step 1, status: "init"
        let a = load(r#"{"step":1,"status":"init"}"#, "a");
        // b: step 2, status: "init" (cambia step, no status)
        let b = load(r#"{"step":2,"status":"init"}"#, "b");
        // c: step 2, status: "done" (no cambia step vs b, cambia status vs b)
        let c = load(r#"{"step":2,"status":"done"}"#, "c");

        let mut comp = compare(vec![a, b, c]).unwrap();

        // En FixedBase (base a):
        assert_eq!(comp.diff_mode, DiffMode::FixedBase);
        assert_eq!(comp.changed_count(), 2);

        // Cambiamos a Sequential:
        comp.set_diff_mode(DiffMode::Sequential);
        assert_eq!(comp.diff_mode, DiffMode::Sequential);
        // Ambos campos sufrieron algún cambio a lo largo de la secuencia:
        assert_eq!(comp.changed_count(), 2);

        // Si creamos una secuencia donde nada cambia:
        let a2 = load(r#"{"x":10}"#, "a2");
        let b2 = load(r#"{"x":10}"#, "b2");
        let c2 = load(r#"{"x":10}"#, "c2");
        let comp2 = compare_sequential(vec![a2, b2, c2]).unwrap();
        assert_eq!(comp2.changed_count(), 0);
    }

    #[test]
    fn test_move_and_swap_files() {
        let a = load(r#"{"name":"A"}"#, "a");
        let b = load(r#"{"name":"B"}"#, "b");
        let c = load(r#"{"name":"C"}"#, "c");
        let mut comp = compare(vec![a, b, c]).unwrap();
        assert_eq!(comp.files[0].name, "a");
        assert_eq!(comp.files[1].name, "b");
        assert_eq!(comp.files[2].name, "c");

        // Mover archivo 'a' (pos 0) a pos 2
        comp.move_file(0, 2).unwrap();
        assert_eq!(comp.files[0].name, "b");
        assert_eq!(comp.files[1].name, "c");
        assert_eq!(comp.files[2].name, "a");

        // row.cells se deben haber reordenado
        let row = &comp.rows[0];
        assert_eq!(row.cells[0].as_ref().unwrap().value, "\"B\"");
        assert_eq!(row.cells[1].as_ref().unwrap().value, "\"C\"");
        assert_eq!(row.cells[2].as_ref().unwrap().value, "\"A\"");

        // Intercambiar pos 0 y pos 1
        comp.swap_files(0, 1).unwrap();
        assert_eq!(comp.files[0].name, "c");
        assert_eq!(comp.files[1].name, "b");
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

    #[test]
    fn test_json_selector_root() {
        let sel_root = JsonSelector::parse("$").unwrap();
        assert!(sel_root.is_root());
        assert!(sel_root.matches("data"));
        assert!(sel_root.matches("data.user.name"));
        assert!(sel_root.matches("items[0].id"));
        assert!(sel_root.matches("$"));

        let sel_empty = JsonSelector::parse("").unwrap();
        assert!(sel_empty.is_root());
        assert!(sel_empty.matches("anything"));
    }

    #[test]
    fn test_json_selector_subfields() {
        let sel = JsonSelector::parse("$.data").unwrap();
        assert!(sel.matches("data"));
        assert!(sel.matches("data.name"));
        assert!(sel.matches("data.user.email"));
        assert!(sel.matches("data.items[0]"));
        // No debe coincidir con campos fuera de data
        assert!(!sel.matches("metadata"));
        assert!(!sel.matches("database"));
        assert!(!sel.matches("other.data"));
    }

    #[test]
    fn test_json_selector_array_and_wildcard() {
        let sel_idx = JsonSelector::parse("$.items[0]").unwrap();
        assert!(sel_idx.matches("items[0]"));
        assert!(sel_idx.matches("items[0].id"));
        assert!(!sel_idx.matches("items[1].id"));

        let sel_wild = JsonSelector::parse("$.items[*]").unwrap();
        assert!(sel_wild.matches("items[0].id"));
        assert!(sel_wild.matches("items[1].name"));
        assert!(sel_wild.matches("items[99].status"));
        assert!(!sel_wild.matches("users[0].id"));

        let sel_wild_field = JsonSelector::parse("$.items[*].id").unwrap();
        assert!(sel_wild_field.matches("items[0].id"));
        assert!(sel_wild_field.matches("items[5].id"));
        assert!(!sel_wild_field.matches("items[0].name"));
    }

    #[test]
    fn test_json_selector_recursive() {
        let sel_rec = JsonSelector::parse("$..id").unwrap();
        assert!(sel_rec.matches("id"));
        assert!(sel_rec.matches("user.id"));
        assert!(sel_rec.matches("items[0].id"));
        assert!(sel_rec.matches("data.dept.team.leader.id"));
        assert!(!sel_rec.matches("user.name"));

        let sel_rec_stars = JsonSelector::parse("$.**.id").unwrap();
        assert!(sel_rec_stars.matches("user.id"));
        assert!(sel_rec_stars.matches("items[0].id"));
    }

    #[test]
    fn test_json_selector_multiple_union() {
        let sel = JsonSelector::parse("$.data, $.meta").unwrap();
        assert!(sel.matches("data.user"));
        assert!(sel.matches("meta.version"));
        assert!(!sel.matches("logs[0]"));
    }

    #[test]
    fn test_json_selector_syntax_errors() {
        assert!(JsonSelector::parse("$.items[").is_err());
        assert!(JsonSelector::parse("$.items[]").is_err());
    }

    #[test]
    fn test_comparison_set_selector() {
        let a = load(r#"{"data":{"a":1,"b":2},"meta":{"version":1}}"#, "a");
        let b = load(r#"{"data":{"a":1,"b":3},"meta":{"version":2}}"#, "b");
        let mut comp = compare(vec![a, b]).unwrap();

        // Por defecto ($) están las 3 rutas
        assert_eq!(comp.rows.len(), 3);
        assert_eq!(comp.selector, "$");
        assert_eq!(comp.changed_count(), 2); // b y meta.version

        // Enfocamos en $.data: solo quedan data.a y data.b
        comp.set_selector("$.data").unwrap();
        assert_eq!(comp.selector, "$.data");
        assert_eq!(comp.rows.len(), 2);
        assert_eq!(comp.rows[0].path, "data.a");
        assert_eq!(comp.rows[1].path, "data.b");
        assert_eq!(comp.changed_count(), 1); // solo data.b cambió

        // Enfocamos en subcampo exacto $.data.b
        comp.set_selector("$.data.b").unwrap();
        assert_eq!(comp.rows.len(), 1);
        assert_eq!(comp.rows[0].path, "data.b");
        assert!(comp.rows[0].changed);

        // Restaurar a raíz ($)
        comp.set_selector("$").unwrap();
        assert_eq!(comp.rows.len(), 3);
        assert_eq!(comp.changed_count(), 2);
    }
}
