# AGENTS.md

Guía para agentes y colaboradores que trabajen en este repositorio.

## Propósito

Comparar de 2 a 6 archivos JSON y mostrar qué campos/líneas cambiaron. Ver `README.md` para el uso.

## Arquitectura

- `src/lib.rs` (crate `jsondiff`): **toda la lógica**. Sin dependencia de GPUI.
  - `flatten` renderiza el JSON con el mismo formato que `serde_json::to_string_pretty` y devuelve hojas `Leaf { path, line, value }`.
  - `load_file_from_str` parsea texto en memoria preservando el contenido original.
  - `format_json` indenta y normaliza JSON a 2 espacios.
  - `validate_json` comprueba la sintaxis JSON sin dependencias gráficas.
  - `detect_json_type` y `is_type_change` clasifican tipos de datos y detectan mutaciones entre versiones (e.g. string vs número).
  - `compare_with_base` une rutas en orden de primera aparición y marca `Row.changed` respecto a la base seleccionada.
  - `matches_filter` filtra filas por ruta o valor.
  - `export_markdown` y `export_csv` generan reportes tabulares estructurados.
  - `text_report` alimenta el modo `--cli`.
- `src/gui.rs`: solo presentación (feature `gui`), construida con Ely Components (`ely-gpui-component`) y `gpui-pre 0.3.8`. No debe contener lógica de comparación. Integra:
  - Vista Tabla (Matriz virtualizada con `uniform_list`, celdas enriquecidas con `Badge`, gutter de líneas, chips de tipo, resaltado sutil de diferencias contra la base y distinción jerárquica de rutas).
  - Vista Diff Git (`DiffViewer` en modo Unified y Split con estadísticas `DiffStat`).
  - Vista Editor (`CodeEditor` interactivo, gutter con `GitMark` contra la base, formateo, validación sintáctica, aplicación en caliente y guardado en disco).
- `src/main.rs`: parseo de argumentos y arranque. Con `--no-default-features` solo existe el modo CLI.

Regla: cualquier cambio de comportamiento se implementa en `lib.rs` con prueba; la UI solo lo consume.


## Comandos

```bash
cargo test --no-default-features      # pruebas del núcleo (rápido, sin GPUI)
cargo run --no-default-features -- --cli a.json b.json
cargo check                           # type-check incluida la UI
cargo run --release                   # abre la ventana
```

## Invariantes importantes

- `flatten` debe producir exactamente el mismo texto que `serde_json::to_string_pretty` (hay un test). De eso dependen los números de línea.
- `serde_json` se usa con `preserve_order`; sin esa feature las líneas no coincidirían con el archivo original.
- Se compara por **ruta**, no por posición de línea.
- Valores comparados como JSON renderizado: `"17"` ≠ `17`, `""` ≠ `null`. Es intencional.
- Límites: `MIN_FILES = 2`, `MAX_FILES = 10` (en `lib.rs`; la UI los importa).
- Soporta modos de comparación: `DiffMode::FixedBase` (contra archivo base único) y `DiffMode::Sequential` (en cadena: col 1 vs col 0, col 2 vs col 1, col 3 vs col 2...).
- Reordenamiento interactivo de columnas con `move_file` / `swap_files` directamente en la UI.
- La base de la comparación puede seleccionarse dinámicamente en modo base fija.

## Dependencias GPUI y Ely

Usa `gpui-pre =0.3.8`, `gpui-pre-platform =0.3.8` (con `font-kit`) y `ely-gpui-component`. Con esta versión ya no se requiere el parche local de `vendor/xattr` que se necesitaba en `gpui 0.2`.

## Estilo

- Español en textos de interfaz, mensajes de error y documentación; identificadores en inglés.
- Sin `unwrap()` en rutas de usuario (carga de archivos): devolver `Result<_, String>` con mensaje claro.
- Mantener el diseño de la tabla: filas de alto fijo (`ROW_H`) por `uniform_list`; no usar listas no virtualizadas para archivos grandes.
- Ejecutar `cargo fmt` y `cargo test --no-default-features` antes de entregar cambios.

## Ideas pendientes

- [x] Elegir el archivo base (implementado en `lib.rs` y con botón en encabezado de `gui.rs`).
- [x] Copiar ruta/valor al portapapeles (implementado).
- [x] Buscar por nombre de campo o valor (implementado con `SearchInput`/`Input` en `gui.rs` y `matches_filter` en `lib.rs`).
- [x] Exportar el reporte a Markdown/CSV (implementado en `lib.rs` con botón de copia en la UI).
- [x] Comparación en cadena secuencial (1 vs 0, 2 vs 1, 3 vs 2...) y reordenamiento de archivos (hasta 10 JSON).
- [ ] Comparar arreglos por clave (p. ej. por `id`) en vez de por índice.
