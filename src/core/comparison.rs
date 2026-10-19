//! Motor de comparación matricial, alineación de rutas, modos de diff y filtros.

use crate::core::loader::load_file;
use crate::core::selector::JsonSelector;
use crate::core::types::{Cell, DiffMode, Leaf, LoadedFile, Row, MAX_FILES, MIN_FILES};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// Estructura principal que almacena el resultado de una comparación entre 2 y 10 archivos JSON.
#[derive(Debug, Clone)]
pub struct Comparison {
    pub files: Vec<LoadedFile>,
    pub rows: Vec<Row>,
    pub base_index: usize,
    pub diff_mode: DiffMode,
    pub selector: String,
}

impl Comparison {
    /// Número de filas que presentan diferencias según el modo de comparación activo.
    pub fn changed_count(&self) -> usize {
        self.rows.iter().filter(|r| r.changed).count()
    }

    /// Expresión del selector JSONPath activo.
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
        let mut seen: HashSet<&str> = HashSet::new();
        let mut per_file: Vec<HashMap<&str, &Leaf>> = Vec::new();

        for f in &self.files {
            let mut m = HashMap::new();
            for leaf in &f.leaves {
                if selector.matches(&leaf.path) {
                    if seen.insert(leaf.path.as_str()) {
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

    /// Cambia el modo de comparación (Base fija o En cadena).
    pub fn set_diff_mode(&mut self, mode: DiffMode) {
        self.diff_mode = mode;
        self.recompute_changes();
    }

    /// Cambia el índice del archivo base en modo Base fija.
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

/// Compara una lista de archivos cargados especificando el índice del archivo base.
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
    let mut seen: HashSet<&str> = HashSet::new();
    let mut per_file: Vec<HashMap<&str, &Leaf>> = Vec::new();
    for f in &files {
        let mut m = HashMap::new();
        for leaf in &f.leaves {
            if seen.insert(leaf.path.as_str()) {
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

/// Compara una lista de archivos cargados usando el primer archivo (índice 0) como base.
pub fn compare(files: Vec<LoadedFile>) -> Result<Comparison, String> {
    compare_with_base(files, 0)
}

/// Compara una lista de archivos cargados en modo en cadena secuencial (1 vs 0, 2 vs 1, etc.).
pub fn compare_sequential(files: Vec<LoadedFile>) -> Result<Comparison, String> {
    let mut c = compare_with_base(files, 0)?;
    c.set_diff_mode(DiffMode::Sequential);
    Ok(c)
}

/// Carga los archivos desde sus rutas y realiza la comparación con una base específica.
pub fn compare_paths_with_base(paths: &[PathBuf], base_index: usize) -> Result<Comparison, String> {
    let files = paths
        .iter()
        .map(|p| load_file(p))
        .collect::<Result<Vec<_>, _>>()?;
    compare_with_base(files, base_index)
}

/// Carga los archivos desde sus rutas y realiza la comparación con el primer archivo como base.
pub fn compare_paths(paths: &[PathBuf]) -> Result<Comparison, String> {
    compare_paths_with_base(paths, 0)
}

/// Búsqueda de subcadena case-insensitive sin alocaciones en heap para textos ASCII.
#[inline]
fn contains_case_insensitive(haystack: &str, needle_lower: &str) -> bool {
    if needle_lower.is_empty() {
        return true;
    }
    if haystack.len() < needle_lower.len() {
        return false;
    }
    if haystack.is_ascii() && needle_lower.is_ascii() {
        let needle_bytes = needle_lower.as_bytes();
        haystack
            .as_bytes()
            .windows(needle_bytes.len())
            .any(|window| {
                window
                    .iter()
                    .zip(needle_bytes.iter())
                    .all(|(&h, &n)| h.to_ascii_lowercase() == n)
            })
    } else {
        haystack.to_lowercase().contains(needle_lower)
    }
}

/// Comprueba si una fila coincide con un query de búsqueda ya normalizado en minúsculas (zero-allocation).
#[inline]
pub fn matches_filter_normalized(row: &Row, query_lower: &str) -> bool {
    if query_lower.is_empty() {
        return true;
    }
    if contains_case_insensitive(&row.path, query_lower) {
        return true;
    }
    row.cells.iter().any(|c| match c {
        Some(cell) => contains_case_insensitive(&cell.value, query_lower),
        None => "(no existe)".contains(query_lower),
    })
}

/// Comprueba si una fila coincide con el texto de búsqueda (en la ruta o en algún valor).
pub fn matches_filter(row: &Row, query: &str) -> bool {
    let q = query.trim().to_lowercase();
    matches_filter_normalized(row, &q)
}
