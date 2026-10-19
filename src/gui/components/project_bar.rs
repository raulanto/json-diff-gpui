//! Barra de pestañas de proyectos (Tabs) para organización y cambio rápido de sesiones.

use crate::gui::DiffView;
use ely_gpui_component::{
    buttons::{Button, ButtonVariant, IconButton},
    forms::Input,
    navigation::{EditorTab, EditorTabs},
    overlays::Dialog,
    primitives::{Icon, IconName},
    theme::{ActiveTheme, ControlSize, IconSize, Mode, Theme},
};
use gpui::{div, prelude::*, px, Context, Entity, IntoElement};

/// Renderiza la barra horizontal de pestañas de proyectos con EditorTabs de Ely Components.
pub fn render_project_bar(
    state: &DiffView,
    view: Entity<DiffView>,
    cx: &mut Context<DiffView>,
) -> impl IntoElement {
    let theme = cx.theme();
    let colors = &theme.colors;
    let active_id = state
        .workspace
        .active_project_id
        .clone()
        .unwrap_or_default();

    let editor_tabs: Vec<EditorTab> = state
        .workspace
        .projects
        .iter()
        .map(|proj| {
            let paths_count = proj.paths.len();
            let label = if paths_count > 0 {
                format!("{} ({paths_count})", proj.name)
            } else {
                proj.name.clone()
            };
            EditorTab::new(proj.id.clone(), label)
                .icon(IconName::Folder)
                .dirty(paths_count == 0)
        })
        .collect();

    let view_select = view.clone();
    let view_close = view.clone();
    let view_reorder = view.clone();

    let project_tabs = EditorTabs::new("workspace_project_tabs", editor_tabs)
        .selected(active_id.clone())
        .on_select(move |id, window, cx| {
            let id_str = id.to_string();
            view_select.update(cx, |this, cx| {
                this.switch_project_tab(&id_str, window, cx);
            });
        })
        .on_close(move |id, window, cx| {
            let id_str = id.to_string();
            view_close.update(cx, |this, cx| {
                this.close_project_tab(&id_str, window, cx);
            });
        })
        .on_reorder(move |from, to, _, cx| {
            view_reorder.update(cx, |this, cx| {
                this.reorder_project_tab(from, to, cx);
            });
        });

    let view_new = view.clone();
    let view_home = view.clone();
    let view_rename = view.clone();
    let view_save = view.clone();
    let view_load = view.clone();
    let view_dup = view.clone();
    let view_bundle = view.clone();
    let view_cmd = view.clone();
    let view_settings = view.clone();

    let active_id_for_rename = if !active_id.is_empty() {
        Some(active_id.clone())
    } else {
        None
    };

    div()
        .id("project_bar")
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .w_full()
        .bg(colors.sunken)
        .border_b_1()
        .border_color(colors.border)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_row()
                .items_center()
                .child(
                    div().px_1().child(
                        IconButton::new("btn_home_project_bar", IconName::House)
                            .size(ControlSize::Sm)
                            .tooltip("Inicio / Crear proyecto")
                            .on_click(move |_, _, cx| {
                                view_home.update(cx, |this, cx| {
                                    this.view_mode = crate::gui::types::ViewMode::Home;
                                    cx.notify();
                                });
                            }),
                    ),
                )
                .child(div().h(px(16.0)).w(px(1.0)).bg(colors.border).mr_1())
                .child(div().flex_1().min_w_0().child(project_tabs))
                .child(
                    div().px_1().child(
                        IconButton::new("btn_new_project_tab", IconName::Plus)
                            .size(ControlSize::Sm)
                            .tooltip("Nueva pestaña de proyecto (Ctrl+T)")
                            .on_click(move |_, window, cx| {
                                view_new.update(cx, |this, cx| {
                                    this.new_project_tab(None, window, cx);
                                });
                            }),
                    ),
                ),
        )
        // Acciones de proyecto y controles globales de aplicación
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .py_1()
                .when_some(active_id_for_rename, |bar, id| {
                    let view_rename = view_rename.clone();
                    bar.child(
                        IconButton::new("btn_rename_project", IconName::Pencil)
                            .size(ControlSize::Sm)
                            .tooltip("Renombrar proyecto activo")
                            .on_click(move |_, window, cx| {
                                view_rename.update(cx, |this, cx| {
                                    this.open_rename_dialog(&id, window, cx);
                                });
                            }),
                    )
                })
                .child(
                    IconButton::new("btn_dup_project", IconName::Copy)
                        .size(ControlSize::Sm)
                        .tooltip("Duplicar proyecto activo")
                        .on_click(move |_, window, cx| {
                            view_dup.update(cx, |this, cx| {
                                this.duplicate_current_project(window, cx);
                            });
                        }),
                )
                .child(
                    IconButton::new("btn_save_bundle", IconName::FolderPlus)
                        .size(ControlSize::Sm)
                        .tooltip("Guardar proyecto con sus archivos JSON en disco…")
                        .on_click(move |_, window, cx| {
                            view_bundle.update(cx, |this, cx| {
                                this.save_current_project_bundle(window, cx);
                            });
                        }),
                )
                .child(
                    IconButton::new("btn_save_project", IconName::Save)
                        .size(ControlSize::Sm)
                        .tooltip("Guardar proyecto actual a archivo JSON…")
                        .on_click(move |_, _, cx| {
                            view_save.update(cx, |this, cx| {
                                this.save_project_to_file(cx);
                            });
                        }),
                )
                .child(
                    IconButton::new("btn_load_project", IconName::FolderOpen)
                        .size(ControlSize::Sm)
                        .tooltip("Cargar proyecto desde archivo JSON…")
                        .on_click(move |_, window, cx| {
                            view_load.update(cx, |this, cx| {
                                this.load_project_from_file(window, cx);
                            });
                        }),
                )
                // Divisor visual vertical entre acciones de proyecto y controles globales
                .child(div().h(px(16.0)).w(px(1.0)).bg(colors.border).mx_1())
                .child(
                    IconButton::new("cmd_palette_btn", IconName::Command)
                        .size(ControlSize::Sm)
                        .tooltip("Paleta de comandos (Ctrl+K)")
                        .on_click(move |_, _, cx| {
                            view_cmd.update(cx, |this, cx| {
                                this.show_command_palette = !this.show_command_palette;
                                this.show_settings = false;
                                cx.notify();
                            });
                        }),
                )
                .child(
                    IconButton::new("settings_btn", IconName::Settings)
                        .size(ControlSize::Sm)
                        .tooltip("Configuración (Ctrl+,)")
                        .on_click(move |_, _, cx| {
                            view_settings.update(cx, |this, cx| {
                                this.show_settings = !this.show_settings;
                                this.show_command_palette = false;
                                cx.notify();
                            });
                        }),
                )
                .child(
                    IconButton::new(
                        "theme_toggle",
                        if cx.theme().is_dark() {
                            IconName::Sun
                        } else {
                            IconName::Moon
                        },
                    )
                    .size(ControlSize::Sm)
                    .tooltip(if cx.theme().is_dark() {
                        "Modo claro (Ctrl+Shift+T)"
                    } else {
                        "Modo oscuro (Ctrl+Shift+T)"
                    })
                    .on_click(move |_, _, cx| {
                        let next = if cx.theme().is_dark() {
                            Mode::Light
                        } else {
                            Mode::Dark
                        };
                        Theme::set_mode(next, cx);
                    }),
                ),
        )
}

/// Diálogo modal para renombrar un proyecto.
pub fn render_rename_dialog(
    state: &DiffView,
    view: Entity<DiffView>,
    cx: &mut Context<DiffView>,
) -> impl IntoElement {
    let theme = cx.theme();
    let colors = &theme.colors;
    let view_close = view.clone();
    let view_confirm = view.clone();

    Dialog::new("rename_project_modal", "Renombrar Proyecto", {
        let view = view_close.clone();
        move |_, cx| {
            view.update(cx, |this, cx| {
                this.renaming_project_id = None;
                cx.notify();
            });
        }
    })
    .detail("Ingresa el nuevo nombre descriptivo para identificar este proyecto de comparación.")
    .child(
        div()
            .flex()
            .flex_col()
            .gap_2()
            .py_2()
            .child(
                div()
                    .text_xs()
                    .text_color(colors.fg_muted)
                    .child("Nombre del proyecto"),
            )
            .child(
                Input::new(&state.project_rename_input)
                    .size(ControlSize::Md)
                    .prefix(
                        Icon::new(IconName::Pencil)
                            .size(IconSize::Sm)
                            .color(colors.fg_muted),
                    )
                    .clearable(),
            ),
    )
    .action({
        let view = view_close.clone();
        move |close| {
            Button::new("btn_cancel_rename", "Cancelar")
                .variant(ButtonVariant::Ghost)
                .on_click(move |_, window, cx| {
                    view.update(cx, |this, cx| {
                        this.renaming_project_id = None;
                        cx.notify();
                    });
                    close(window, cx);
                })
        }
    })
    .action({
        let view = view_confirm.clone();
        move |_close| {
            Button::new("btn_confirm_rename", "Guardar Nombre")
                .variant(ButtonVariant::Primary)
                .icon(IconName::Check)
                .on_click(move |_, window, cx| {
                    view.update(cx, |this, cx| {
                        this.confirm_rename_project(window, cx);
                    });
                })
        }
    })
}
