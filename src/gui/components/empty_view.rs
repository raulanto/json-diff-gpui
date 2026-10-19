//! Pantalla de bienvenida construida con WelcomePage, FileTree y DirectoryListing de Ely Components.

use crate::gui::types::ProjectExplorerView;
use crate::gui::DiffView;
use ely_gpui_component::{
    buttons::{Button, ButtonVariant, IconButton, SegmentedControl},
    editor::WelcomePage,
    lists::{DirEntry, DirectoryListing, FileTree},
    primitives::{Icon, IconName},
    theme::{ActiveTheme, ControlSize, IconSize, Radius, TextSize},
};
use gpui::{div, prelude::*, px, Context, Entity, FontWeight, IntoElement};
use jsondiff::bundles_to_file_tree_paths;

/// Renderiza la vista de bienvenida utilizando WelcomePage de Ely Components,
/// integrando FileTree y DirectoryListing para gestionar proyectos y sus archivos JSON en disco.
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
        "Compara de 2 a 10 archivos JSON, edita con git-marks y gestiona proyectos completos en disco.",
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
    .action(
        IconName::Save,
        "Guardar proyecto y JSON en disco",
        Some("secondary-s"),
    )
    .action(
        IconName::Command,
        "Paleta de comandos",
        Some("secondary-k"),
    )
    .action(
        IconName::Settings,
        "Configuración y catálogo de atajos",
        Some("secondary-,"),
    )
    .on_action(move |index, window, cx| {
        view_actions.update(cx, |this, cx| match index {
            0 => {
                this.pick_files(cx);
                this.view_mode = crate::gui::types::ViewMode::Table;
            }
            1 => {
                this.new_project_tab(None, window, cx);
                this.view_mode = crate::gui::types::ViewMode::Table;
            }
            2 => this.save_current_project_bundle(window, cx),
            3 => {
                this.show_command_palette = true;
                this.show_settings = false;
                cx.notify();
            }
            4 => {
                this.show_settings = true;
                this.show_command_palette = false;
                cx.notify();
            }
            _ => {}
        });
    });

    // Barra de gestión del almacenamiento de proyectos
    let projects_dir = state.workspace.projects_dir();
    let dir_display = projects_dir.display().to_string();

    let view_dir = view.clone();
    let view_reset = view.clone();
    let view_mode_toggle = view.clone();

    let storage_header = div()
        .flex()
        .items_center()
        .justify_between()
        .gap_3()
        .pb_3()
        .border_b_1()
        .border_color(colors.border)
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .min_w_0()
                .child(
                    Icon::new(IconName::Folder)
                        .size(IconSize::Sm)
                        .color(colors.accent),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .child(
                            div()
                                .text_size(theme.text_size(TextSize::Xs))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(colors.fg)
                                .child("Carpeta de Proyectos en Disco:"),
                        )
                        .child(
                            div()
                                .text_size(theme.text_size(TextSize::Xs))
                                .text_color(colors.fg_muted)
                                .child(dir_display),
                        ),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    Button::new("btn_change_projects_dir", "Cambiar ubicación…")
                        .size(ControlSize::Sm)
                        .variant(ButtonVariant::Outline)
                        .icon(IconName::FolderPlus)
                        .on_click(move |_, window, cx| {
                            view_dir.update(cx, |this, cx| {
                                this.choose_projects_dir(window, cx);
                            });
                        }),
                )
                .when(state.workspace.projects_dir.is_some(), |bar| {
                    bar.child(
                        IconButton::new("btn_reset_projects_dir", IconName::RotateCcw)
                            .size(ControlSize::Sm)
                            .tooltip("Restablecer a carpeta predeterminada")
                            .on_click(move |_, _, cx| {
                                view_reset.update(cx, |this, cx| {
                                    this.reset_projects_dir(cx);
                                });
                            }),
                    )
                })
                .child(
                    SegmentedControl::new(
                        "project_view_mode",
                        match state.project_explorer_view {
                            ProjectExplorerView::FileTree => "tree",
                            ProjectExplorerView::DirectoryListing => "list",
                        },
                    )
                    .size(ControlSize::Sm)
                    .segment("tree", "Árbol", Some(IconName::Folder))
                    .segment("list", "Detalles", Some(IconName::Table))
                    .on_change(move |val, _, cx| {
                        view_mode_toggle.update(cx, |this, cx| {
                            this.project_explorer_view = if val == "list" {
                                ProjectExplorerView::DirectoryListing
                            } else {
                                ProjectExplorerView::FileTree
                            };
                            cx.notify();
                        });
                    }),
                ),
        );

    // Contenido del explorador (FileTree o DirectoryListing)
    let view_open_tree = view.clone();
    let view_open_list = view.clone();

    let explorer_content: gpui::AnyElement = match state.project_explorer_view {
        ProjectExplorerView::FileTree => {
            let tree_paths = bundles_to_file_tree_paths(&state.discovered_bundles);
            if tree_paths.is_empty() {
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .py_8()
                    .text_color(colors.fg_muted)
                    .child(
                        Icon::new(IconName::Folder)
                            .size(IconSize::Lg)
                            .color(colors.fg_subtle),
                    )
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .child("No hay proyectos guardados en esta carpeta"),
                    )
                    .child(div().text_size(theme.text_size(TextSize::Xs)).child(
                        "Guarda un proyecto con sus archivos JSON para visualizarlo en el árbol.",
                    ))
                    .into_any_element()
            } else {
                div()
                    .w_full()
                    .h(px(260.0))
                    .child(
                        FileTree::new("welcome_file_tree", tree_paths)
                            .w_full()
                            .h(px(260.0))
                            .on_open(move |path_str, window, cx| {
                                view_open_tree.update(cx, |this, cx| {
                                    this.open_from_file_tree_path(path_str.as_ref(), window, cx);
                                });
                            }),
                    )
                    .into_any_element()
            }
        }
        ProjectExplorerView::DirectoryListing => {
            if state.discovered_bundles.is_empty() {
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .py_8()
                    .text_color(colors.fg_muted)
                    .child(
                        Icon::new(IconName::Table)
                            .size(IconSize::Lg)
                            .color(colors.fg_subtle),
                    )
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .child("No hay proyectos guardados en esta carpeta"),
                    )
                    .into_any_element()
            } else {
                let entries: Vec<DirEntry> = state
                    .discovered_bundles
                    .iter()
                    .map(|b| {
                        let at = jiff::Timestamp::from_second(b.updated_at as i64)
                            .unwrap_or(jiff::Timestamp::UNIX_EPOCH);
                        DirEntry::folder(b.name.clone(), at)
                    })
                    .collect();

                let folder_name = projects_dir
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("projects");

                div()
                    .w_full()
                    .h(px(260.0))
                    .child(
                        DirectoryListing::new("welcome_dir_listing", [folder_name], entries)
                            .on_open(move |entry, window, cx| {
                                let name = entry.name().to_string();
                                view_open_list.update(cx, |this, cx| {
                                    this.open_project_bundle_by_name(&name, window, cx);
                                });
                            }),
                    )
                    .into_any_element()
            }
        }
    };

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
        .child(storage_header)
        .child(explorer_content)
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
