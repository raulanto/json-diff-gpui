//! Pantalla de bienvenida construida con WelcomePage de Ely Components y zona de DropZone interactiva.

use crate::gui::DiffView;
use ely_gpui_component::{
    editor::{Project, RecentProjects, WelcomePage},
    primitives::{Icon, IconName},
    theme::{ActiveTheme, IconSize, Radius, TextSize},
};
use gpui::{div, prelude::*, Context, Entity, FontWeight, IntoElement};

/// Renderiza la vista de bienvenida utilizando WelcomePage de Ely Components,
/// adaptada para abarcar todo el contenedor y ofrecer máxima legibilidad y espacio.
pub fn render_empty_view(
    state: &DiffView,
    view: Entity<DiffView>,
    cx: &mut Context<DiffView>,
) -> impl IntoElement {
    let theme = cx.theme();
    let colors = &theme.colors;

    let view_actions = view.clone();
    let welcome = WelcomePage::new(
        "welcome_main",
        "JSON Diff & Editor",
        "Compara de 2 a 10 archivos JSON, edita con git-marks y genera esquemas semánticos.",
    )
    .action(
        IconName::FolderOpen,
        "Abrir archivos JSON…",
        Some("secondary-o"),
    )
    .action(
        IconName::FolderPlus,
        "Nuevo proyecto de comparación",
        Some("secondary-n"),
    )
    .action(IconName::Command, "Paleta de comandos", Some("secondary-k"))
    .action(
        IconName::Settings,
        "Configuración y catálogo de atajos",
        Some("secondary-,"),
    )
    .on_action(move |index, window, cx| {
        view_actions.update(cx, |this, cx| match index {
            0 => this.pick_files(cx),
            1 => this.new_project_tab(None, window, cx),
            2 => {
                this.show_command_palette = true;
                this.show_settings = false;
                cx.notify();
            }
            3 => {
                this.show_settings = true;
                this.show_command_palette = false;
                cx.notify();
            }
            _ => {}
        });
    });

    // Proyectos del espacio de trabajo con formato limpio y espacioso
    let recent_projects_list: Vec<Project> = state
        .workspace
        .projects
        .iter()
        .map(|proj| {
            let is_active = state.workspace.active_project_id.as_deref() == Some(&proj.id);
            let count = proj.paths.len();
            let subtitle = if count == 0 {
                "Sin archivos cargados".to_string()
            } else {
                format!("{count} archivos JSON configurados")
            };
            Project {
                name: proj.name.clone().into(),
                path: subtitle.into(),
                branch: if is_active {
                    Some("Activo".into())
                } else {
                    None
                },
                opened: "".into(),
                pinned: is_active,
            }
        })
        .collect();

    let view_open = view.clone();
    let view_remove = view.clone();

    let recent_widget = RecentProjects::new("welcome_recent", recent_projects_list)
        .on_open(move |idx, window, cx| {
            view_open.update(cx, |this, cx| {
                if let Some(p) = this.workspace.projects.get(idx) {
                    let id = p.id.clone();
                    this.switch_project_tab(&id, window, cx);
                }
            });
        })
        .on_remove(move |idx, window, cx| {
            view_remove.update(cx, |this, cx| {
                if let Some(p) = this.workspace.projects.get(idx) {
                    let id = p.id.clone();
                    this.close_project_tab(&id, window, cx);
                }
            });
        });

    let view_pick = view.clone();
    let drop_hint = div()
        .id("welcome_dropzone")
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_2()
        .p_6()
        .mt_4()
        .rounded(theme.radius(Radius::Md))
        .border_1()
        .border_dashed()
        .border_color(colors.border_strong)
        .bg(colors.surface.opacity(0.3))
        .cursor_pointer()
        .hover(|el| el.bg(colors.hover).border_color(colors.accent))
        .on_click(move |_, _, cx| {
            view_pick.update(cx, |this, cx| {
                this.pick_files(cx);
            });
        })
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .text_size(theme.text_size(TextSize::Sm))
                .font_weight(FontWeight::MEDIUM)
                .text_color(colors.fg)
                .child(
                    Icon::new(IconName::FolderOpen)
                        .size(IconSize::Sm)
                        .color(colors.accent),
                )
                .child("Arrastra archivos JSON a esta ventana o haz clic para seleccionarlos"),
        )
        .child(
            div()
                .text_size(theme.text_size(TextSize::Xs))
                .text_color(colors.fg_muted)
                .text_center()
                .child(
                    "Soporta de 2 a 10 archivos para comparación matricial simultánea o secuencial",
                ),
        );

    let recent_container = div()
        .flex()
        .flex_col()
        .flex_1()
        .gap_3()
        .child(recent_widget)
        .child(drop_hint);

    let welcome = welcome.recent(recent_container);

    div()
        .id("welcome_container")
        .flex()
        .flex_col()
        .flex_1()
        .size_full()
        .overflow_y_scroll()
        .child(welcome)
}
