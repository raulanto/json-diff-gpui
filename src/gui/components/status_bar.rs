//! Barra de estado inferior (pie de la aplicación) con CursorPosition de Ely Components.

use crate::gui::types::{Density, ViewMode};
use crate::gui::DiffView;
use ely_gpui_component::{
    editor::{CursorPosition, Encoding, IndentSettings, LanguageMode},
    primitives::IconName,
    shell::{StatusBar, StatusBarItem},
};
use gpui::{Context, Entity, IntoElement};

/// Renderiza la barra de estado en el pie de la aplicación.
pub fn render_status_bar(
    state: &DiffView,
    view: Entity<DiffView>,
    cx: &mut Context<DiffView>,
) -> impl IntoElement {
    let mut bar = StatusBar::new();

    // 1. Proyecto activo
    let proj_name = state
        .workspace
        .active_project()
        .map(|p| p.name.clone())
        .unwrap_or_else(|| "Proyecto".to_string());
    let view_proj = view.clone();
    bar = bar.left(
        StatusBarItem::new("sb_project")
            .icon(IconName::Folder)
            .label(proj_name)
            .tooltip("Proyecto actual (clic para abrir comandos)")
            .on_click(move |_, _, cx| {
                view_proj.update(cx, |this, cx| {
                    this.show_command_palette = true;
                    this.show_settings = false;
                    cx.notify();
                });
            }),
    );

    // 2. Modo de visualización actual
    let mode_label = match state.view_mode {
        ViewMode::Home => "Inicio / Proyectos",
        ViewMode::Table => "Tabla",
        ViewMode::GitDiff => "Git Diff",
        ViewMode::Editor => "Editor",
    };
    bar = bar.left(
        StatusBarItem::new("sb_mode")
            .label(mode_label)
            .tooltip("Modo de vista activo"),
    );

    // 3. Conteo de diferencias si hay comparación activa
    if let Some(c) = &state.comparison {
        let diff_count = c.rows.iter().filter(|r| r.changed).count();
        let total = c.rows.len();
        bar = bar.left(
            StatusBarItem::new("sb_diffs")
                .label(format!("{diff_count} dif. / {total} campos"))
                .tooltip("Diferencias detectadas"),
        );
    }

    // 4. Toast de estado si existe
    if let Some(toast) = &state.status_toast {
        bar = bar.left(
            StatusBarItem::new("sb_toast")
                .icon(IconName::Info)
                .label(toast.clone())
                .tooltip("Mensaje de estado"),
        );
    }

    // 5. Controles del lado derecho: Lenguaje, Identación (Spaces: 2), Codificación (UTF-8) y Densidad
    bar = bar.right(LanguageMode::new("sb_lang", "JSON"));
    bar = bar.right(IndentSettings::new("sb_indent", true, 2));
    bar = bar.right(Encoding::new("sb_enc", "UTF-8"));

    let view_density = view.clone();
    let density_text = match state.density {
        Density::Comfortable => "Cómodo",
        Density::Compact => "Compacto",
    };
    bar = bar.right(
        StatusBarItem::new("sb_density")
            .label(density_text)
            .tooltip("Alternar densidad de filas (Ctrl+D)")
            .on_click(move |_, _, cx| {
                view_density.update(cx, |this, cx| {
                    this.toggle_density(cx);
                });
            }),
    );

    // 6. CursorPosition interactivo según el modo actual
    let (line, col, selected) = match state.view_mode {
        ViewMode::Editor => {
            if let Some(editor) = &state.editor {
                let ed = editor.read(cx);
                let (l, c) = ed.position();
                let sel = ed.primary();
                let s = if !sel.is_empty() {
                    sel.range().len()
                } else {
                    0
                };
                (l.max(1), c.max(1), s)
            } else {
                (1, 1, 0)
            }
        }
        ViewMode::Table => {
            let total = state.visible.len().max(1);
            (total, 1, 0)
        }
        ViewMode::GitDiff | ViewMode::Home => (1, 1, 0),
    };

    let mut cursor = CursorPosition::new("sb_cursor_position", line, col);
    if selected > 0 {
        cursor = cursor.selected(selected);
    }

    let view_cursor = view.clone();
    let cursor = cursor.on_click(move |_, cx| {
        view_cursor.update(cx, |this, cx| {
            this.show_command_palette = true;
            this.show_settings = false;
            cx.notify();
        });
    });

    bar = bar.right(cursor);

    bar
}
