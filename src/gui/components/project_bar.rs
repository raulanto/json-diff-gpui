//! Barra de pestañas de proyectos (Tabs) para organización y cambio rápido de sesiones.

use crate::gui::DiffView;
use ely_gpui_component::{
    buttons::{Button, ButtonVariant, IconButton},
    data_display::{Badge, Tone},
    overlays::Dialog,
    primitives::{Icon, IconName},
    theme::{ActiveTheme, ControlSize, Mode, TextSize, Theme},
};
use gpui::{div, prelude::*, px, Context, Entity, IntoElement};
use jsondiff::MIN_FILES;

/// Renderiza la barra horizontal de pestañas de proyectos (Tabs) de comparación.
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
    let projects_count = state.workspace.projects.len();

    let tabs = state
        .workspace
        .projects
        .iter()
        .enumerate()
        .map(|(idx, proj)| {
            let is_active = proj.id == active_id;
            let proj_id = proj.id.clone();
            let proj_name = proj.name.clone();
            let paths_count = proj.paths.len();

            let view_click = view.clone();
            let view_close = view.clone();
            let view_rename = view.clone();

            let mut tab = div()
                .id(("project_tab", idx))
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .py_1()
                .cursor_pointer()
                .rounded_t_md()
                .text_size(theme.text_size(TextSize::Sm));

            if is_active {
                tab = tab
                    .bg(colors.bg)
                    .text_color(colors.fg)
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .border_t_2()
                    .border_color(colors.accent);
            } else {
                tab = tab
                    .bg(colors.sunken)
                    .text_color(colors.fg_muted)
                    .hover(|s| s.bg(colors.hover).text_color(colors.fg));
            }

            // Al hacer clic se cambia al proyecto
            tab = tab.on_click(move |_, window, cx| {
                view_click.update(cx, |this, cx| {
                    this.switch_project_tab(&proj_id, window, cx);
                });
            });

            // Icono de proyecto
            tab = tab.child(Icon::new(if is_active {
                IconName::FolderOpen
            } else {
                IconName::Folder
            }));

            // Nombre del proyecto
            tab = tab.child(
                div()
                    .max_w(px(160.0))
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(proj_name),
            );

            // Contador de archivos
            let badge_tone = if paths_count >= MIN_FILES {
                Tone::Success
            } else if paths_count == 1 {
                Tone::Warning
            } else {
                Tone::Neutral
            };
            tab = tab.child(Badge::new(format!("{paths_count}")).tone(badge_tone).dot());

            // Botón discreto para renombrar
            let rename_id = proj.id.clone();
            tab = tab.child(
                IconButton::new(("btn_tab_rename", idx), IconName::Pencil)
                    .size(ControlSize::Sm)
                    .tooltip("Renombrar proyecto")
                    .on_click(move |_, window, cx| {
                        view_rename.update(cx, |this, cx| {
                            this.open_rename_dialog(&rename_id, window, cx);
                        });
                    }),
            );

            // Botón para cerrar pestaña (si hay más de 1 proyecto abierto)
            if projects_count > 1 {
                let close_id = proj.id.clone();
                tab = tab.child(
                    IconButton::new(("btn_tab_close", idx), IconName::X)
                        .size(ControlSize::Sm)
                        .tooltip("Cerrar pestaña")
                        .on_click(move |_, window, cx| {
                            view_close.update(cx, |this, cx| {
                                this.close_project_tab(&close_id, window, cx);
                            });
                        }),
                );
            }

            tab
        });

    let view_new = view.clone();
    let view_save = view.clone();
    let view_load = view.clone();
    let view_dup = view.clone();
    let view_cmd = view.clone();
    let view_settings = view.clone();

    div()
        .id("project_bar")
        .flex()
        .items_center()
        .justify_between()
        .px_2()
        .pt_1()
        .bg(colors.sunken)
        .border_b_1()
        .border_color(colors.border)
        .child(
            div()
                .id("project_tabs_scroll_container")
                .flex()
                .items_center()
                .gap_1()
                .overflow_x_scroll()
                .children(tabs)
                // Botón Nueva Pestaña
                .child(
                    IconButton::new("btn_new_project_tab", IconName::Plus)
                        .size(ControlSize::Sm)
                        .tooltip("Nueva pestaña de proyecto (Ctrl+T)")
                        .on_click(move |_, window, cx| {
                            view_new.update(cx, |this, cx| {
                                this.new_project_tab(None, window, cx);
                            });
                        }),
                ),
        )
        // Acciones de proyecto y controles globales de aplicación
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .py_1()
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
    _cx: &mut Context<DiffView>,
) -> impl IntoElement {
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
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(state.project_rename_input.clone()),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("btn_cancel_rename", "Cancelar")
                            .variant(ButtonVariant::Ghost)
                            .on_click(move |_, _, cx| {
                                view_close.update(cx, |this, cx| {
                                    this.renaming_project_id = None;
                                    cx.notify();
                                });
                            }),
                    )
                    .child(
                        Button::new("btn_confirm_rename", "Guardar Nombre")
                            .variant(ButtonVariant::Primary)
                            .icon(IconName::Check)
                            .on_click(move |_, window, cx| {
                                view_confirm.update(cx, |this, cx| {
                                    this.confirm_rename_project(window, cx);
                                });
                            }),
                    ),
            ),
    )
}
