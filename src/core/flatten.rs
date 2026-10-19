//! Normalización y aplanado de documentos JSON a estructuras jerárquicas y hojas con línea.

use crate::core::types::Leaf;
use serde_json::Value;

/// Renderiza el valor con el mismo formato que `serde_json::to_string_pretty`
/// y devuelve el texto junto con las hojas y su número de línea (base 1).
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
