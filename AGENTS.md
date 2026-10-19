# AGENTS.md

Guía para agentes y colaboradores que trabajen en este repositorio.

## Propósito

Comparar de 2 a 6 archivos JSON y mostrar qué campos/líneas cambiaron. Ver `README.md` para el uso.

## Arquitectura

- `src/lib.rs` y `src/core/` (crate `jsondiff`): **toda la lógica del dominio**, agnóstica de GPUI. Arquitectura modular desacoplada:
  - `src/core/types.rs`: modelos de dominio (`LoadedFile`, `Leaf`, `Cell`, `Row`, `DiffMode`, `JsonType`, constantes `MIN_FILES` / `MAX_FILES`).
  - `src/core/flatten.rs`: normalización y aplanado (`flatten`, `walk`, `scalar`, `join_key`) con formato exacto de `serde_json::to_string_pretty`.
  - `src/core/loader.rs`: parsing en memoria (`load_file_from_str`), carga desde disco (`load_file`), formateo normalizado a 2 espacios (`format_json`) y validación sintáctica (`validate_json`).
  - `src/core/selector.rs`: motor extractor estilo JSONPath (`JsonSelector`, `PathSegment`, `SelectorPattern`) con comodines `*`, arrays `[i]`, comodines de array `[*]`, descenso recursivo `..` / `**` y unión `,`.
  - `src/core/comparison.rs`: motor de comparación matricial (`Comparison`, `compare_with_base`, `compare_sequential`), reordenamiento de columnas (`move_file`, `swap_files`), base dinámica y filtro de búsqueda (`matches_filter`).
  - `src/core/report.rs`: generadores estructurados de reportes CLI (`text_report`), Markdown enriquecido (`export_markdown`) y CSV (`export_csv`).
  - `src/core/project.rs`: sistema de sesiones y espacios de trabajo (`Project`, `Workspace`) con gestión de pestañas y persistencia atómica en disco JSON.
  - `src/core/tests.rs`: suite completa de pruebas unitarias agnósticas de GUI.
  - `src/lib.rs`: orquestador raíz que re-exporta toda la API pública de `core` manteniendo 100% de compatibilidad hacia atrás.
- `src/gui.rs` y `src/gui/`: solo presentación (feature `gui`), construida con Ely Components (`ely-gpui-component`) y `gpui-pre 0.3.8`. No debe contener lógica de comparación. Arquitectura modular desacoplada:
  - `src/gui.rs`: punto de entrada público (`run`) y re-exportador de la API gráfica.
  - `src/gui/state.rs`: modelo de estado principal `DiffView` y constructor `new`.
  - `src/gui/file_ops.rs`: gestión de archivos en disco y memoria (`reload`, `refilter`, `add_paths`, `pick_files`, `pick_more_files`, `move_file`, fallback zenity).
  - `src/gui/project_ops.rs`: ciclo de vida de proyectos y pestañas (`Workspace`, creación, duplicación, renombrado, importación y exportación de archivos JSON).
  - `src/gui/editor_ops.rs`: operaciones del editor en vivo (`CodeEditor`), git marks contra base, formateo a 2 espacios, validación sintáctica, selector JSONPath, densidad y toasts.
  - `src/gui/render.rs`: orquestación de renderizado GPUI (`impl Render for DiffView`), modales reactivos, listeners de teclado y drop de archivos.
  - `src/gui/types.rs`: enums y structs de presentación (`Density`, `ViewMode`, `SettingsTab`).
  - `src/gui/shortcuts.rs`: catálogo de atajos (`Shortcut`), listeners globales de teclado y dispatch de comandos.
  - `src/gui/components/project_bar.rs`: barra horizontal de pestañas de proyectos construida con `EditorTabs` y `EditorTab` de Ely Components (cierre nativo, reordenamiento por arrastre, menú de desbordamiento, creación rápida con `+`, renombrado, duplicación y persistencia).
  - `src/gui/components/cell.rs`: funciones reutilizables de renderizado de celdas (micro-badges `+ Nuevo`, `≠ Tipo`, `Ausente`, chips de tipo, gutters numéricos de 32px y tooltips).
  - `src/gui/components/toolbar.rs`: barra superior de herramientas con selectores segmentados, inputs de búsqueda y selector JSONPath, badges y botones de acción.
  - `src/gui/components/table_view.rs`: vista Tabla matricial virtualizada con `uniform_list`, reordenamiento interactivo, base dinámica y estados vacíos reactivos.
  - `src/gui/components/git_view.rs`: vista Diff Git (`DiffViewer` en modo Unified y Split con selector dinámico de destino y estadísticas).
  - `src/gui/components/editor_view.rs`: vista Editor (`CodeEditor` interactivo, gutter con `GitMark` contra la base, formateo, validación sintáctica, aplicación en caliente y guardado en disco).
  - `src/gui/components/empty_view.rs`: pantalla de bienvenida / DropZone con atajos rápidos.
  - `src/gui/components/command_palette.rs`: paleta de comandos interactiva (`CommandPalette` de Ely con búsqueda difusa y grupos categorizados).
  - `src/gui/components/settings_dialog.rs`: modal de Configuración (`Dialog`, `SettingsSection`, `SettingsRow` y `KeyboardShortcutsList` con catálogo reactivo de atajos).
- `src/main.rs`: parseo de argumentos y arranque. Con `--no-default-features` solo existe el modo CLI (soporta `-s|--selector <QUERY>`).

Regla: cualquier cambio de comportamiento se implementa en `lib.rs` con prueba; la UI solo lo consume.


## Comandos

```bash
cargo test --no-default-features      # pruebas del núcleo (rápido, sin GPUI)
cargo run --no-default-features -- --cli a.json b.json
cargo run --no-default-features -- --cli -s "$.data" a.json b.json
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
- Selector interactivo de subestructuras (`JsonSelector`): por defecto `$` (raíz completa), pero configurable dinámicamente (`$.data`, `$.items[*].id`, etc.).
- La base de la comparación puede seleccionarse dinámicamente en modo base fija.

## Dependencias GPUI y Ely

Usa `gpui-pre =0.3.8`, `gpui-pre-platform =0.3.8` (con `font-kit`) y `ely-gpui-component`. Con esta versión ya no se requiere el parche local de `vendor/xattr` que se necesitaba en `gpui 0.2`.

## Estilo

- Español en textos de interfaz, mensajes de error y documentación; identificadores en inglés.
- Sin `unwrap()` en rutas de usuario (carga de archivos): devolver `Result<_, String>` con mensaje claro.
- Mantener el diseño de la tabla: filas de alto fijo (`row_h` según `Density`, 26px en Compacto y 36px en Cómodo) por `uniform_list`; no usar listas no virtualizadas para archivos grandes.
- Ejecutar `cargo fmt` y `cargo test --no-default-features` antes de entregar cambios.

## Ideas pendientes

- [x] Elegir el archivo base (implementado en `lib.rs` y con botón en encabezado de `gui.rs`).
- [x] Copiar ruta/valor al portapapeles (implementado).
- [x] Buscar por nombre de campo o valor (implementado con `SearchInput`/`Input` en `gui.rs` y `matches_filter` en `lib.rs`).
- [x] Exportar el reporte a Markdown/CSV (implementado en `lib.rs` con botón de copia en la UI).
- [x] Comparación en cadena secuencial (1 vs 0, 2 vs 1, 3 vs 2...) y reordenamiento de archivos (hasta 10 JSON).
- [x] Lenguaje selector JSONPath (`$.data`, `$.items[*]`, `$..id`, unión `,`) para extraer y enfocar subestructuras concretas.
- [x] Paleta de comandos interactiva (`CommandPalette`) con búsqueda fuzzy y atajos de teclado globales.
- [x] Modal de Configuración de la aplicación (`Dialog` con secciones generales y catálogo reactivo de atajos).
- [x] Modo de visualización compacto / densidad configurable (`Ctrl+D`, botón en toolbar y ajustes) con ~40% más filas visibles.
- [x] Carga incremental y gestión de archivos (agregar hasta 10 JSON en cualquier momento desde toolbar, drag & drop, atajo `Ctrl+Shift+O` y modal de Configuración sin perder los archivos previos).
- [x] Sistema de proyectos de comparación y pestañas múltiples (Workspace con guardado persistente, exportación/importación JSON, pestañas con cambio rápido `Ctrl+Tab`, duplicación y gestión en configuración).
- [ ] Comparar arreglos por clave (p. ej. por `id`) en vez de por índice.
