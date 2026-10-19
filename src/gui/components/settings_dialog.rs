//! Modal de configuración de la aplicación (Settings Dialog) con catálogo de atajos.

use crate::gui::shortcuts::shortcuts_list;
use crate::gui::types::{Density, SettingsTab};
use crate::gui::DiffView;
use ely_gpui_component::{
    buttons::{Button, ButtonVariant, IconButton, SegmentedControl},
    data_display::{Badge, Tone},
    overlays::Dialog,
    primitives::{Icon, IconName},
    settings::{KeyboardShortcutsList, SettingsRow, SettingsSection},
    theme::{ActiveTheme, ControlSize, Mode, TextSize, Theme},
};
use gpui::{div, prelude::*, px, Context, Entity, IntoElement};
use jsondiff::{DiffMode, MAX_FILES, MIN_FILES};

/// Renderiza el modal de Configuración de la aplicación usando Dialog y componentes de Ely.
pub fn render_settings_dialog(
    state: &DiffView,
    view: Entity<DiffView>,
    cx: &mut Context<DiffView>,
) -> impl IntoElement {
    let theme = cx.theme();
    let colors = &theme.colors;
    let shortcuts = shortcuts_list();

    let tab_content = match state.settings_tab {
        SettingsTab::General => {
            let current_diff_mode = state
                .comparison
                .as_ref()
                .map(|c| c.diff_mode)
                .unwrap_or(DiffMode::FixedBase);
            let current_filter = if state.only_changed { "changed" } else { "all" };
            let current_theme = if cx.theme().is_dark() {
                "dark"
            } else {
                "light"
            };

            let view1 = view.clone();
            let view2 = view.clone();
            let view3 = view.clone();
            let view_density = view.clone();

            div()
                .flex()
                .flex_col()
                .gap_4()
                .child(
                    SettingsSection::new("Apariencia y Visualización")
                        .description("Personaliza el tema visual, tamaño y densidad de filas.")
                        .row(
                            SettingsRow::new("Tema de la interfaz")
                                .description("Alterna entre tema oscuro moderno o tema claro de alto contraste.")
                                .control(
                                    SegmentedControl::new("settings_theme", current_theme)
                                        .size(ControlSize::Sm)
                                        .segment("dark", "Oscuro", Some(IconName::Moon))
                                        .segment("light", "Claro", Some(IconName::Sun))
                                        .on_change(move |val, _, cx| {
                                            let mode = if val.as_ref() == "dark" {
                                                Mode::Dark
                                            } else {
                                                Mode::Light
                                            };
                                            Theme::set_mode(mode, cx);
                                        }),
                                ),
                        )
                        .row(
                            SettingsRow::new("Densidad de la tabla")
                                .description("Ajusta el espaciado de filas para visualizar muchas más líneas simultáneas (Ctrl+D).")
                                .control(
                                    SegmentedControl::new(
                                        "settings_density",
                                        match state.density {
                                            Density::Compact => "compact",
                                            Density::Comfortable => "comfortable",
                                        },
                                    )
                                    .size(ControlSize::Sm)
                                    .segment("compact", "Compacta (26px)", Some(IconName::Rows2))
                                    .segment("comfortable", "Cómoda (36px)", Some(IconName::Maximize2))
                                    .on_change(move |val, _, cx| {
                                        let compact = val.as_ref() == "compact";
                                        view_density.update(cx, |this, cx| {
                                            this.density = if compact {
                                                Density::Compact
                                            } else {
                                                Density::Comfortable
                                            };
                                            this.show_toast(
                                                if compact {
                                                    "Densidad: Compacta (~40% más filas)"
                                                } else {
                                                    "Densidad: Cómoda (espaciado generoso)"
                                                },
                                                cx,
                                            );
                                            cx.notify();
                                        });
                                    }),
                                ),
                        ),
                )
                .child(
                    SettingsSection::new("Comportamiento de Comparación")
                        .description("Configura la estrategia de diff por defecto entre múltiples versiones JSON.")
                        .row(
                            SettingsRow::new("Modo de comparación inicial")
                                .description("Base fija compara todos los archivos contra el primero; En cadena compara paso a paso (2 vs 1, 3 vs 2).")
                                .control(
                                    SegmentedControl::new(
                                        "settings_diff_mode",
                                        match current_diff_mode {
                                            DiffMode::FixedBase => "fixed",
                                            DiffMode::Sequential => "seq",
                                        },
                                    )
                                    .size(ControlSize::Sm)
                                    .segment("fixed", "Base fija", Some(IconName::Pin))
                                    .segment("seq", "En cadena", Some(IconName::ArrowRight))
                                    .on_change(move |val, _, cx| {
                                        let new_mode = if val.as_ref() == "seq" {
                                            DiffMode::Sequential
                                        } else {
                                            DiffMode::FixedBase
                                        };
                                        view1.update(cx, |this, cx| {
                                            if let Some(c) = &mut this.comparison {
                                                c.set_diff_mode(new_mode);
                                                this.refilter();
                                                cx.notify();
                                            }
                                        });
                                    }),
                                ),
                        )
                        .row(
                            SettingsRow::new("Filtro predeterminado")
                                .description("Mostrar solo filas con cambios o todas las propiedades.")
                                .control(
                                    SegmentedControl::new("settings_filter", current_filter)
                                        .size(ControlSize::Sm)
                                        .segment("changed", "Solo cambios", Some(IconName::Filter))
                                        .segment("all", "Todos", Some(IconName::ListChecks))
                                        .on_change(move |val, _, cx| {
                                            let is_changed = val.as_ref() == "changed";
                                            view2.update(cx, |this, cx| {
                                                this.only_changed = is_changed;
                                                this.refilter();
                                                cx.notify();
                                            });
                                        }),
                                ),
                        )
                        .row(
                            SettingsRow::new("Selector de subestructura")
                                .description("Restablecer consulta JSONPath a la raíz '$' completa.")
                                .control(
                                    Button::new("btn_reset_sel_settings", "Restaurar a raíz ($)")
                                        .size(ControlSize::Sm)
                                        .variant(ButtonVariant::Outline)
                                        .icon(IconName::RotateCcw)
                                        .on_click(move |_, _, cx| {
                                            view3.update(cx, |this, cx| {
                                                this.reset_selector(cx);
                                            });
                                        }),
                                ),
                        ),
                )
                .child({
                    let view_add = view.clone();
                    let view_clear = view.clone();
                    let can_add = state.paths.len() < MAX_FILES;
                    let file_count = state.paths.len();

                    let mut section = SettingsSection::new("Gestión de Archivos y Capacidad")
                        .description("Controla los archivos JSON en la comparación matricial (admite de 2 a 10 archivos).")
                        .row(
                            SettingsRow::new("Agregar archivos a la comparación")
                                .description(if can_add {
                                    format!("Incorpora archivos adicionales sin descartar los actuales ({file_count} de {MAX_FILES}).")
                                } else {
                                    format!("Se ha alcanzado la capacidad máxima de {MAX_FILES} archivos simultáneos.")
                                })
                                .control(
                                    Button::new(
                                        "btn_add_files_settings",
                                        if can_add { "Agregar archivos…" } else { "Límite alcanzado" },
                                    )
                                    .size(ControlSize::Sm)
                                    .variant(if can_add { ButtonVariant::Primary } else { ButtonVariant::Outline })
                                    .icon(IconName::FolderPlus)
                                    .on_click(move |_, _, cx| {
                                        if can_add {
                                            view_add.update(cx, |this, cx| {
                                                this.pick_more_files(cx);
                                            });
                                        }
                                    }),
                                ),
                        )
                        .row(
                            SettingsRow::new("Archivos activos")
                                .description("Estado actual de la sesión de comparación.")
                                .control(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .child(
                                            Badge::new(format!("{file_count} / {MAX_FILES}"))
                                                .tone(if file_count >= MIN_FILES {
                                                    Tone::Success
                                                } else if file_count == 1 {
                                                    Tone::Warning
                                                } else {
                                                    Tone::Neutral
                                                }),
                                        )
                                        .when(file_count > 0, |parent| {
                                            parent.child(
                                                Button::new("btn_clear_settings", "Limpiar todo")
                                                    .size(ControlSize::Sm)
                                                    .variant(ButtonVariant::Ghost)
                                                    .icon(IconName::Trash2)
                                                    .on_click(move |_, _, cx| {
                                                        view_clear.update(cx, |this, cx| {
                                                            this.clear_files(cx);
                                                        });
                                                    }),
                                            )
                                        }),
                                ),
                        );

                    if !state.paths.is_empty() {
                        let files_box = div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .p_2()
                            .rounded_md()
                            .bg(colors.sunken)
                            .border_1()
                            .border_color(colors.border)
                            .children(state.paths.iter().enumerate().map(|(idx, path)| {
                                let filename = path
                                    .file_name()
                                    .map(|f| f.to_string_lossy().to_string())
                                    .unwrap_or_else(|| format!("Archivo {}", idx + 1));
                                let view_rm = view.clone();
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap_2()
                                    .py_1()
                                    .px_2()
                                    .rounded_sm()
                                    .hover(|s| s.bg(colors.hover))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                Badge::new(format!("#{}", idx + 1))
                                                    .tone(if idx == 0 { Tone::Accent } else { Tone::Neutral }),
                                            )
                                            .child(
                                                div()
                                                    .text_size(theme.text_size(TextSize::Sm))
                                                    .text_color(colors.fg)
                                                    .child(filename),
                                            ),
                                    )
                                    .child(
                                        IconButton::new(("btn_rm_file_settings", idx), IconName::X)
                                            .size(ControlSize::Sm)
                                            .tooltip("Quitar archivo")
                                            .on_click(move |_, _, cx| {
                                                view_rm.update(cx, |this, cx| {
                                                    this.remove_file(idx, cx);
                                                });
                                            }),
                                    )
                            }));

                        section = section.row(
                            SettingsRow::new("Lista de archivos cargados")
                                .description("Puedes descartar archivos individualmente.")
                                .control(files_box),
                        );
                    }

                    section
                })
                .into_any_element()
        }
        SettingsTab::Projects => {
            let active_id = state
                .workspace
                .active_project_id
                .clone()
                .unwrap_or_default();
            let view_new_proj = view.clone();
            let view_export = view.clone();
            let view_import = view.clone();

            div()
                .flex()
                .flex_col()
                .gap_4()
                .child(
                    SettingsSection::new("Proyectos de Comparación")
                        .description(
                            "Organiza diferentes conjuntos de archivos JSON en sesiones independientes para reutilizarlas.",
                        )
                        .row(
                            SettingsRow::new("Crear nuevo proyecto")
                                .description(
                                    "Inicia una sesión limpia de comparación en una nueva pestaña (Ctrl+T).",
                                )
                                .control(
                                    Button::new("btn_new_proj_settings", "Nuevo Proyecto")
                                        .size(ControlSize::Sm)
                                        .variant(ButtonVariant::Primary)
                                        .icon(IconName::Plus)
                                        .on_click(move |_, window, cx| {
                                            view_new_proj.update(cx, |this, cx| {
                                                this.new_project_tab(None, window, cx);
                                                this.show_settings = false;
                                            });
                                        }),
                                ),
                        )
                        .row(
                            SettingsRow::new("Copia de seguridad del espacio de trabajo")
                                .description(
                                    "Exporta todos los proyectos a un archivo JSON o importa un workspace guardado.",
                                )
                                .control(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .child(
                                            Button::new("btn_export_ws", "Exportar Workspace…")
                                                .size(ControlSize::Sm)
                                                .variant(ButtonVariant::Outline)
                                                .icon(IconName::Save)
                                                .on_click(move |_, _, cx| {
                                                    view_export.update(cx, |this, cx| {
                                                        this.export_workspace_dialog(cx);
                                                    });
                                                }),
                                        )
                                        .child(
                                            Button::new("btn_import_ws", "Importar Workspace…")
                                                .size(ControlSize::Sm)
                                                .variant(ButtonVariant::Outline)
                                                .icon(IconName::FolderOpen)
                                                .on_click(move |_, window, cx| {
                                                    view_import.update(cx, |this, cx| {
                                                        this.import_workspace_dialog(window, cx);
                                                    });
                                                }),
                                        ),
                                ),
                        ),
                )
                .child({
                    let projects_list = state
                        .workspace
                        .projects
                        .iter()
                        .enumerate()
                        .map(|(idx, proj)| {
                            let is_active = proj.id == active_id;
                            let proj_id = proj.id.clone();
                            let proj_name = proj.name.clone();
                            let paths_len = proj.paths.len();

                            let view_activate = view.clone();
                            let view_rename = view.clone();
                            let view_dup = view.clone();
                            let view_del = view.clone();

                            let rename_id = proj.id.clone();
                            let dup_id = proj.id.clone();
                            let del_id = proj.id.clone();

                            div()
                                .id(("settings_proj_row", idx))
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap_3()
                                .p_3()
                                .rounded_md()
                                .bg(if is_active {
                                    colors.surface
                                } else {
                                    colors.sunken
                                })
                                .border_1()
                                .border_color(if is_active {
                                    colors.accent
                                } else {
                                    colors.border
                                })
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_3()
                                        .child(
                                            div()
                                                .text_color(if is_active {
                                                    colors.accent
                                                } else {
                                                    colors.fg_muted
                                                })
                                                .child(
                                                    Icon::new(if is_active {
                                                        IconName::FolderOpen
                                                    } else {
                                                        IconName::Folder
                                                    })
                                                ),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap_0p5()
                                                .child(
                                                    div()
                                                        .flex()
                                                        .items_center()
                                                        .gap_2()
                                                        .child(
                                                            div()
                                                                .font_weight(
                                                                    gpui::FontWeight::SEMIBOLD,
                                                                )
                                                                .text_size(
                                                                    theme.text_size(TextSize::Sm),
                                                                )
                                                                .text_color(colors.fg)
                                                                .child(proj_name),
                                                        )
                                                        .when(is_active, |el| {
                                                            el.child(
                                                                Badge::new("Activo")
                                                                    .tone(Tone::Accent),
                                                            )
                                                        }),
                                                )
                                                .child(
                                                    div()
                                                        .text_size(
                                                            theme.text_size(TextSize::Xs),
                                                        )
                                                        .text_color(colors.fg_muted)
                                                        .child(format!(
                                                            "{paths_len} archivo(s) JSON • Selector: '{}' • Modo: {:?}",
                                                            proj.selector, proj.diff_mode
                                                        )),
                                                ),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .when(!is_active, |el| {
                                            el.child(
                                                Button::new(("btn_open_proj", idx), "Abrir")
                                                    .size(ControlSize::Sm)
                                                    .variant(ButtonVariant::Primary)
                                                    .on_click(move |_, window, cx| {
                                                        view_activate.update(cx, |this, cx| {
                                                            this.switch_project_tab(
                                                                &proj_id, window, cx,
                                                            );
                                                            this.show_settings = false;
                                                        });
                                                    }),
                                            )
                                        })
                                        .child(
                                            IconButton::new(
                                                ("btn_settings_rename", idx),
                                                IconName::Pencil,
                                            )
                                            .size(ControlSize::Sm)
                                            .tooltip("Renombrar")
                                            .on_click(move |_, window, cx| {
                                                view_rename.update(cx, |this, cx| {
                                                    this.open_rename_dialog(
                                                        &rename_id, window, cx,
                                                    );
                                                });
                                            }),
                                        )
                                        .child(
                                            IconButton::new(
                                                ("btn_settings_dup", idx),
                                                IconName::Copy,
                                            )
                                            .size(ControlSize::Sm)
                                            .tooltip("Duplicar")
                                            .on_click(move |_, _window, cx| {
                                                view_dup.update(cx, |this, cx| {
                                                    this.workspace.duplicate_project(&dup_id);
                                                    this.show_toast(
                                                        "Proyecto duplicado",
                                                        cx,
                                                    );
                                                    cx.notify();
                                                });
                                            }),
                                        )
                                        .when(state.workspace.projects.len() > 1, |el| {
                                            el.child(
                                                IconButton::new(
                                                    ("btn_settings_del", idx),
                                                    IconName::Trash2,
                                                )
                                                .size(ControlSize::Sm)
                                                .tooltip("Eliminar proyecto")
                                                .on_click(move |_, window, cx| {
                                                    view_del.update(cx, |this, cx| {
                                                        this.close_project_tab(
                                                            &del_id, window, cx,
                                                        );
                                                    });
                                                }),
                                            )
                                        }),
                                )
                        });

                    div()
                        .id("settings_projects_scroll_container")
                        .flex()
                        .flex_col()
                        .gap_2()
                        .max_h(px(280.0))
                        .overflow_y_scroll()
                        .children(projects_list)
                })
                .into_any_element()
        }
        SettingsTab::Shortcuts => div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_size(theme.text_size(TextSize::Sm))
                    .text_color(colors.fg_muted)
                    .child(
                        "Escribe para filtrar atajos por nombre, acción o combinación de teclas:",
                    ),
            )
            .child(
                div()
                    .id("settings_shortcuts_scroll")
                    .max_h(px(320.0))
                    .overflow_y_scroll()
                    .child(KeyboardShortcutsList::new(
                        shortcuts,
                        &state.settings_shortcuts_search,
                    )),
            )
            .into_any_element(),
    };

    let view_tabs = view.clone();
    Dialog::new("settings_dialog", "Configuración de la Aplicación", {
        let view = view.clone();
        move |_, cx| {
            view.update(cx, |this, cx| {
                this.show_settings = false;
                cx.notify();
            });
        }
    })
    .detail("Preferencias visuales, proyectos y catálogo de atajos de teclado.")
    .child(
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                SegmentedControl::new(
                    "settings_tab_select",
                    match state.settings_tab {
                        SettingsTab::General => "general",
                        SettingsTab::Projects => "projects",
                        SettingsTab::Shortcuts => "shortcuts",
                    },
                )
                .size(ControlSize::Sm)
                .segment(
                    "general",
                    "General y Preferencias",
                    Some(IconName::Settings),
                )
                .segment("projects", "Proyectos Guardados", Some(IconName::Folder))
                .segment("shortcuts", "Atajos de Teclado", Some(IconName::Keyboard))
                .on_change(move |val, _, cx| {
                    let tab = match val.as_ref() {
                        "projects" => SettingsTab::Projects,
                        "shortcuts" => SettingsTab::Shortcuts,
                        _ => SettingsTab::General,
                    };
                    view_tabs.update(cx, |this, cx| {
                        this.settings_tab = tab;
                        cx.notify();
                    });
                }),
            )
            .child(tab_content),
    )
}
