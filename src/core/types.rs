//! Tipos de datos fundamentales y modelos de dominio del núcleo de comparación.

use std::path::PathBuf;

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

/// Fila comparativa asociada a una ruta o clave específica.
#[derive(Debug, Clone)]
pub struct Row {
    pub path: String,
    /// Una celda por archivo; `None` si la ruta no existe en ese archivo.
    pub cells: Vec<Option<Cell>>,
    pub changed: bool,
}

/// Modo de comparación entre archivos JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum DiffMode {
    /// Todas las versiones se comparan contra un archivo base fijo (`base_index`).
    #[default]
    FixedBase,
    /// Cada versión se compara contra la inmediatamente anterior (1 vs 0, 2 vs 1, 3 vs 2...).
    Sequential,
}
