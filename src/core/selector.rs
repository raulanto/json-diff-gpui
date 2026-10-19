//! Motor extractor y selector de estructuras JSON (estilo JSONPath).

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
pub fn match_pattern_segments(pattern: &[PathSegment], target: &[PathSegment]) -> bool {
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
