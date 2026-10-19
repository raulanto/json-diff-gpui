//! Generador e inferidor de JSON Schema a partir de documentos JSON.
//!
//! Produce especificaciones JSON Schema compatibles con Draft 2020-12
//! con inferencia recursiva de objetos, propiedades requeridas, tipos de arreglos y escalares.

use serde_json::{json, Map, Value};

/// Genera un JSON Schema raíz completo a partir de un valor JSON dado.
pub fn infer_schema(value: &Value) -> Value {
    infer_schema_with_title(value, "JSONSchema")
}

/// Genera un JSON Schema con un título descriptivo personalizado.
pub fn infer_schema_with_title(value: &Value, title: &str) -> Value {
    let root_schema = infer_type_schema(value);
    if let Value::Object(map) = root_schema {
        let mut full_map = Map::new();
        full_map.insert(
            "$schema".to_string(),
            Value::String("https://json-schema.org/draft/2020-12/schema".to_string()),
        );
        full_map.insert("title".to_string(), Value::String(title.to_string()));
        for (k, v) in map {
            full_map.insert(k, v);
        }
        Value::Object(full_map)
    } else {
        root_schema
    }
}

/// Infiere el sub-esquema de tipo para un valor JSON.
fn infer_type_schema(value: &Value) -> Value {
    match value {
        Value::Null => json!({ "type": "null" }),
        Value::Bool(_) => json!({ "type": "boolean" }),
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                json!({ "type": "integer" })
            } else {
                json!({ "type": "number" })
            }
        }
        Value::String(_) => json!({ "type": "string" }),
        Value::Array(arr) => {
            if arr.is_empty() {
                json!({ "type": "array" })
            } else {
                let mut item_schemas: Vec<Value> = Vec::new();
                for item in arr {
                    let s = infer_type_schema(item);
                    if !item_schemas.contains(&s) {
                        item_schemas.push(s);
                    }
                }
                if item_schemas.len() == 1 {
                    json!({
                        "type": "array",
                        "items": item_schemas.into_iter().next().unwrap()
                    })
                } else {
                    json!({
                        "type": "array",
                        "items": { "anyOf": item_schemas }
                    })
                }
            }
        }
        Value::Object(obj) => {
            if obj.is_empty() {
                json!({ "type": "object" })
            } else {
                let mut properties = Map::new();
                for (k, v) in obj {
                    properties.insert(k.clone(), infer_type_schema(v));
                }
                json!({
                    "type": "object",
                    "properties": properties
                })
            }
        }
    }
}

/// Genera el texto JSON Schema formateado a 2 espacios a partir de una cadena JSON.
pub fn generate_schema_from_str(json_str: &str, title: Option<&str>) -> Result<String, String> {
    let value: Value = serde_json::from_str(json_str)
        .map_err(|e| format!("JSON inválido para generar schema: {e}"))?;
    let title = title.unwrap_or("GeneratedSchema");
    let schema = infer_schema_with_title(&value, title);
    serde_json::to_string_pretty(&schema)
        .map_err(|e| format!("Error al serializar schema a JSON: {e}"))
}
