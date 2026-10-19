//! Paleta de comandos interactiva (CommandPalette) con búsqueda difusa y atajos globales.

use crate::gui::DiffView;
use ely_gpui_component::{
    navigation::{Command, CommandPalette},
    primitives::IconName,
};
use gpui::{Entity, IntoElement};

/// Renderiza la paleta de comandos interactiva de Ely Components.
pub fn render_command_palette(view: Entity<DiffView>) -> impl IntoElement {
    CommandPalette::new("global_command_palette", {
        let view = view.clone();
        move |_, cx| {
            view.update(cx, |this, cx| {
                this.show_command_palette = false;
                cx.notify();
            });
        }
    })
    .group(
        "Navegación y Vistas",
        vec![
            Command::new("view_table", "Ver: Tabla matricial")
                .icon(IconName::Table)
                .keys("secondary-1"),
            Command::new("view_git", "Ver: Diff estilo Git")
                .icon(IconName::GitBranch)
                .keys("secondary-2"),
            Command::new("view_editor", "Ver: Editor en vivo")
                .icon(IconName::Code)
                .keys("secondary-3"),
        ],
    )
    .group(
        "Archivos",
        vec![
            Command::new("open_files", "Abrir y reemplazar archivos JSON…")
                .icon(IconName::FolderOpen)
                .keys("secondary-o"),
            Command::new("add_more_files", "Agregar archivos JSON a la comparación…")
                .icon(IconName::FolderPlus)
                .keys("secondary-shift-o"),
            Command::new("clear_files", "Limpiar archivos cargados")
                .icon(IconName::Trash2)
                .keys("secondary-shift-l"),
            Command::new("export_markdown", "Copiar reporte Markdown")
                .icon(IconName::Copy)
                .keys("secondary-shift-m"),
        ],
    )
    .group(
        "Comparación y Filtros",
        vec![
            Command::new("toggle_diff_mode", "Alternar modo: Base fija / En cadena")
                .icon(IconName::SlidersHorizontal)
                .keys("secondary-m"),
            Command::new("toggle_filter", "Alternar filtro: Solo cambios / Todos")
                .icon(IconName::Filter)
                .keys("secondary-shift-c"),
            Command::new(
                "reset_selector",
                "Restaurar selector JSONPath a la raíz ($)",
            )
            .icon(IconName::RotateCcw),
        ],
    )
    .group(
        "Visualización y Densidad",
        vec![
            Command::new(
                "toggle_density",
                "Alternar densidad de tabla (Compacta / Cómoda)",
            )
            .icon(IconName::Rows2)
            .keys("secondary-d"),
            Command::new("density_compact", "Densidad de tabla: Modo compacto")
                .icon(IconName::Minimize2),
            Command::new("density_comfortable", "Densidad de tabla: Modo cómodo")
                .icon(IconName::Maximize2),
        ],
    )
    .group(
        "Editor de Código",
        vec![
            Command::new("format_json", "Formatear JSON (2 espacios)")
                .icon(IconName::Sparkles)
                .keys("secondary-shift-i"),
            Command::new("validate_json", "Validar sintaxis JSON").icon(IconName::Check),
            Command::new("apply_changes", "Aplicar edición a la comparación").icon(IconName::Play),
            Command::new("save_file", "Guardar cambios en disco")
                .icon(IconName::Save)
                .keys("secondary-s"),
        ],
    )
    .group(
        "Configuración y Preferencias",
        vec![
            Command::new("open_settings", "Abrir Configuración de la aplicación")
                .icon(IconName::Settings)
                .keys("secondary-,"),
            Command::new("toggle_theme", "Alternar tema (Claro / Oscuro)")
                .icon(IconName::Sun)
                .keys("secondary-t"),
        ],
    )
    .on_run({
        let view = view.clone();
        move |val, window, cx| {
            view.update(cx, |this, cx| {
                crate::gui::shortcuts::run_command_action(this, val.as_ref(), window, cx);
            });
        }
    })
}
