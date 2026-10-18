# json-diff-gpui

Herramienta de escritorio en **Rust + [GPUI](https://www.gpui.rs/)** para comparar de **2 a 10 archivos JSON** y ver de inmediato **qué campos y qué líneas cambiaron**.

Pensada para los respaldos de guardado de solicitudes (`*_promotor_enviado_*.json`, `*_promotor_solventado_*.json`, …): se generan varias versiones del mismo archivo y esta herramienta señala dónde buscar.

## Qué hace

- **Tres modos de visualización integrados**:
  - **Tabla (Matriz)**: compara de 2 a 10 archivos JSON a la vez con columnas independientes, rutas aplanadas y números de línea exactos.
  - **Diff Estilo Git (`DiffViewer`)**: visor unificado (`Unified`) o dividido (`Split` lado a lado) con estadísticas de cambios (`+agregados -eliminados`), resaltado de palabras y plegado de líneas idénticas.
  - **Editor en Vivo (`CodeEditor`)**: edición directa del JSON con resaltado de sintaxis, números de línea, marcas en el gutter respecto al archivo base, formateo automático (pretty-print con 2 espacios), validación sintáctica, aplicación de cambios en memoria al diff al instante y guardado en disco.
- **Lenguaje selector de subestructuras (JSONPath)**:
  - Permite acotar la comparación a campos o subárboles concretos: `$.data`, `$.user.profile`, `$.items[*]`, `$.items[*].id`, `$..id`, `$.data, $.meta`.
  - Por defecto evalúa la raíz completa (`$`), pero puedes cambiar el selector dinámicamente desde la barra superior o pasar `-s|--selector` en la terminal.
  - Botón de restauración rápida a `$` para alternar con un clic entre la vista enfocada y el documento completo.
- **Modos de comparación flexibles**:
  - **Base fija**: todos los archivos se contrastan con un único archivo base seleccionado.
  - **En cadena (secuencial)**: cada archivo se compara contra el anterior (1 vs 0, 2 vs 1, 3 vs 2...), ideal para ver la evolución paso a paso de múltiples versiones.
- **Reordenamiento interactivo**: flechas en cada columna para mover y reordenar archivos a la izquierda o derecha en tiempo real.
- **Selector de archivo base dinámico**: cambia el archivo base con un clic en «Hacer base» en la columna que desees o selecciónalo para contrastar en el Diff Git.
- **Búsqueda en tiempo real**: filtra campos o valores al instante mediante el campo de búsqueda integrado en la tabla.
- **Copiar al portapapeles y exportar**: copia la ruta del campo o valores individuales al portapapeles, o exporta el reporte tabular a Markdown.
- **Limpieza y remoción de archivos**: botón «Limpiar» en la barra superior para descargar todos los archivos y volver a la pantalla de bienvenida; botones individuales (`✕`) en cada columna para descartar archivos específicos cuando hay más de 2; y botón de reinicio en estados de error.
- **Tema claro / oscuro**: alterna entre modos visuales usando el sistema de temas de Ely Components.
- Interruptor **solo cambios / todo** mediante control segmentado.
- Abrir archivos con el diálogo del sistema o **arrastrándolos** a la ventana.
- Modo `--cli` para terminal (sin ventana).


## Uso

```bash
# Ventana vacía; elige los archivos con «Abrir archivos…» o arrástralos
cargo run --release

# Archivos por argumento (2 a 10)
cargo run --release -- a.json b.json c.json

# Solo terminal
cargo run --release -- --cli a.json b.json
cargo run --release -- --cli --all a.json b.json   # incluye campos sin cambios
cargo run --release -- --cli -s "$.data" a.json b.json  # enfocar en la subestructura data
cargo run --release -- --cli -s "$.items[*].id" a.json b.json  # enfocar en IDs de items
```

Ejemplo de salida `--cli`:

```
[1] a.json  (base)
[2] b.json

27 de 130 campos con diferencias

≠ data.STATUS_SOLI
    [1] L71: 2
    [2] L71: 102
≠ data.ACESOR
    [1] L111: 995157
    [2] L114: "995157"
```

## Cómo se compara

1. Cada JSON se parsea y se renderiza en formato normalizado (2 espacios, **mismo orden de claves que el archivo**).
2. Se aplana a hojas `ruta → (línea, valor)`, p. ej. `data.ORIGEN_CAPTACION.NOMBRE`, `lista[2].campo`.
3. Se unen las rutas de todos los archivos y una ruta cuenta como **cambiada** si su valor difiere entre archivos o falta en alguno.

Los números de línea son los del archivo original cuando este ya está indentado con 2 espacios (lo habitual). Si no lo está, el nombre del archivo aparece con `*` y las líneas corresponden al formato normalizado.

Se compara por **ruta**, no por posición: un campo que cambió de lugar (p. ej. `ACESOR` en L111 → L114) se reporta como el mismo campo, no como borrado + añadido.

## Requisitos

- Rust estable reciente (`rustup`).
- Dependencias de sistema de GPUI en Linux: `libxkbcommon`, `libxkbcommon-x11`, `wayland`, `xcb`, `fontconfig`, `vulkan`. En Windows y macOS no hace falta nada adicional (en Windows se necesita el toolchain MSVC).

## Pruebas

El núcleo no depende de GPUI, así que se prueba sin compilar la interfaz:

```bash
cargo test --no-default-features
```

## Estructura

```
src/lib.rs     Núcleo: carga, aplanado, comparación, filtros, reporte y exportación (+ tests)
src/gui.rs     Interfaz GPUI + Ely Components (tabla virtualizada con uniform_list)
src/main.rs    Argumentos, modo --cli y arranque de la ventana
```

## Limitaciones

- Los arreglos se comparan por índice (`lista[0]`, `lista[1]`…); si reordenas elementos se verán como cambios.
- Los valores largos se truncan con `…` en la tabla (visibles completos en el tooltip).
