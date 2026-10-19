//! Suite completa de pruebas unitarias del núcleo.

use super::*;
use serde_json::Value;
use std::path::{Path, PathBuf};

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
    assert!(matches_filter(name_row, "NAME"));
    assert!(matches_filter(name_row, "alice"));
    assert!(matches_filter(name_row, "ALICE"));
    assert!(matches_filter(name_row, "bob"));
    assert!(matches_filter(name_row, "BoB"));
    assert!(!matches_filter(name_row, "age"));

    assert!(matches_filter_normalized(name_row, "alice"));
    assert!(matches_filter_normalized(name_row, "bob"));
    assert!(!matches_filter_normalized(name_row, "charlie"));

    assert!(matches_filter(age_row, "age"));
    assert!(matches_filter(age_row, "AGE"));
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

#[test]
fn test_project_lifecycle_and_json_serialization() {
    let mut proj = Project::new("Mi Proyecto");
    assert_eq!(proj.name, "Mi Proyecto");
    assert_eq!(proj.selector, "$");
    assert_eq!(proj.diff_mode, DiffMode::FixedBase);
    assert_eq!(proj.paths.len(), 0);

    let p1 = PathBuf::from("a.json");
    let p2 = PathBuf::from("b.json");
    assert!(proj.add_path(p1.clone()));
    assert!(proj.add_path(p2.clone()));
    // No permite duplicados:
    assert!(!proj.add_path(p1.clone()));
    assert_eq!(proj.paths.len(), 2);

    // Mover rutas
    proj.move_path(0, 1);
    assert_eq!(proj.paths[0], p2);
    assert_eq!(proj.paths[1], p1);

    // Serialización y deserialización
    let json = proj.to_json().unwrap();
    let loaded = Project::from_json(&json).unwrap();
    assert_eq!(loaded.id, proj.id);
    assert_eq!(loaded.name, "Mi Proyecto");
    assert_eq!(loaded.paths.len(), 2);
    assert_eq!(loaded.paths[0], p2);
    assert_eq!(loaded.paths[1], p1);
}

#[test]
fn test_workspace_tabs_and_navigation() {
    let mut ws = Workspace::default_initial();
    assert_eq!(ws.projects.len(), 1);
    assert_eq!(ws.active_index(), 0);
    let id1 = ws.active_project().unwrap().id.clone();

    // Crear nueva pestaña
    let id2 = ws.new_tab(Some("Segundo Proyecto".into())).id.clone();
    assert_eq!(ws.projects.len(), 2);
    assert_eq!(ws.active_project_id.as_deref(), Some(id2.as_str()));
    assert_eq!(ws.active_index(), 1);

    // Renombrar
    assert!(ws.rename_project(&id2, "Proyecto Renombrado"));
    assert_eq!(ws.active_project().unwrap().name, "Proyecto Renombrado");

    // Duplicar
    let id3 = ws.duplicate_project(&id2).unwrap();
    assert_eq!(ws.projects.len(), 3);
    assert_eq!(
        ws.active_project().unwrap().name,
        "Proyecto Renombrado (Copia)"
    );

    // Navegación siguiente / anterior
    ws.next_tab();
    assert_eq!(ws.active_index(), 0); // Vuelve a 0 cíclicamente
    ws.prev_tab();
    assert_eq!(ws.active_index(), 2);

    // Cerrar pestaña
    let closed = ws.close_tab(&id3).unwrap();
    assert_eq!(closed.id, id3);
    assert_eq!(ws.projects.len(), 2);

    // Seleccionar pestaña específica
    assert!(ws.select_tab(&id1));
    assert_eq!(ws.active_index(), 0);

    // Reordenar pestañas
    assert!(ws.reorder_tabs(0, 1));
    assert_eq!(ws.projects[0].id, id2);
    assert_eq!(ws.projects[1].id, id1);
}

#[test]
fn test_json_schema_generation() {
    let json_text = r#"{
        "id": 101,
        "name": "Antigravity",
        "active": true,
        "ratio": 3.14,
        "tags": ["code", "rust"],
        "metadata": {
            "version": 2
        }
    }"#;

    let schema_str = generate_schema_from_str(json_text, Some("TestEntity")).unwrap();
    let schema: serde_json::Value = serde_json::from_str(&schema_str).unwrap();

    assert_eq!(
        schema["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(schema["title"], "TestEntity");
    assert_eq!(schema["type"], "object");

    let props = &schema["properties"];
    assert_eq!(props["id"]["type"], "integer");
    assert_eq!(props["name"]["type"], "string");
    assert_eq!(props["active"]["type"], "boolean");
    assert_eq!(props["ratio"]["type"], "number");
    assert_eq!(props["tags"]["type"], "array");
    assert_eq!(props["tags"]["items"]["type"], "string");
    assert_eq!(props["metadata"]["type"], "object");
    assert_eq!(
        props["metadata"]["properties"]["version"]["type"],
        "integer"
    );

    let req = schema["required"].as_array().unwrap();
    assert!(req.contains(&serde_json::Value::String("id".to_string())));
    assert!(req.contains(&serde_json::Value::String("name".to_string())));
}
