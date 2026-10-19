//! Componente de la barra superior de herramientas (Toolbar).

use crate::gui::types::{Density, ViewMode};
use crate::gui::DiffView;
use ely_gpui_component::{
    buttons::{Button, ButtonVariant, IconButton, SegmentedControl},
    data_display::{Badge, Tone},
    forms::Input,
    primitives::{Icon, IconName},
    theme::{ActiveTheme, ControlSize, IconSize, Mode, Theme},
};
use gpui::{div, prelude::*, px, ClipboardItem, Context, Entity, IntoElement};
use jsondiff::{export_markdown, DiffMode};

/// Renderiza la barra de herramientas superior con selectores de vista, filtros, búsqueda, selector JSONPath y acciones rápidas.
pub fn render_toolbar(
    state: &DiffView,
    view: Entity<DiffView>,
    cx: &mut Context<DiffView>,
) -> impl IntoElement {
    let theme = cx.theme();
    let colors = &theme.colors;

    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap_2p5()
        .px_3()
        .py_2()
        .bg(colors.surface)
        .border_b_1()
        .border_color(colors.border)
        // Lado izquierdo: Abrir archivos, limpiar y selector de vistas
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_3()
                .child(
                    Button::new("open", "Abrir archivos…")
                        .primary()
                        .size(ControlSize::Sm)
                        .icon(IconName::FolderOpen)
                        .on_click(cx.listener(|this, _, _, cx| this.pick_files(cx))),
                )
                .when(!state.paths.is_empty(), |bar| {
                    bar.child(
                        Button::new("clear_files", "Limpiar")
                            .size(ControlSize::Sm)
                            .variant(ButtonVariant::Outline)
                            .icon(IconName::Trash2)
                            .on_click(cx.listener(|this, _, _, cx| this.clear_files(cx))),
                    )
                })
                .when(state.comparison.is_some(), |bar| {
                    let view = view.clone();
                    bar.child(
                        SegmentedControl::new(
                            "view_mode",
                            match state.view_mode {
                                ViewMode::Table => "table",
                                ViewMode::GitDiff => "git",
                                ViewMode::Editor => "editor",
                            },
                        )
                        .size(ControlSize::Sm)
                        .segment("table", "Tabla", Some(IconName::Table))
                        .segment("git", "Diff Git", Some(IconName::GitBranch))
                        .segment("editor", "Editor", Some(IconName::Code))
                        .on_change(move |value, _, cx| {
                            let mode = match value.as_ref() {
                                "git" => ViewMode::GitDiff,
                                "editor" => ViewMode::Editor,
                                _ => ViewMode::Table,
                            };
                            view.update(cx, |this, cx| {
                                this.view_mode = mode;
                                this.editor = None;
                                cx.notify();
                            });
                        }),
                    )
                }),
        )
        // Centro: controles dependientes del modo actual (solo en vista Tabla)
        .child(div().flex().flex_row().items_center().gap_3().when(
            state.view_mode == ViewMode::Table && state.comparison.is_some(),
            |bar| {
                let view = view.clone();
                let current_diff_mode = state
                    .comparison
                    .as_ref()
                    .map(|c| c.diff_mode)
                    .unwrap_or_default();
                let current_selector = state
                    .comparison
                    .as_ref()
                    .map(|c| c.selector.as_str())
                    .unwrap_or("$");

                bar.child(
                    SegmentedControl::new(
                        "diff_mode",
                        match current_diff_mode {
                            DiffMode::FixedBase => "fixed",
                            DiffMode::Sequential => "seq",
                        },
                    )
                    .size(ControlSize::Sm)
                    .segment("fixed", "Base fija", Some(IconName::Pin))
                    .segment("seq", "En cadena", Some(IconName::ArrowRight))
                    .on_change({
                        let view = view.clone();
                        move |value, _, cx| {
                            let new_mode = if value.as_ref() == "seq" {
                                DiffMode::Sequential
                            } else {
                                DiffMode::FixedBase
                            };
                            view.update(cx, |this, cx| {
                                if let Some(c) = &mut this.comparison {
                                    c.set_diff_mode(new_mode);
                                    this.refilter();
                                    this.show_toast(
                                        if new_mode == DiffMode::Sequential {
                                            "Modo En cadena: cada archivo se compara con el anterior"
                                        } else {
                                            "Modo Base fija: todos se comparan con la base"
                                        },
                                        cx,
                                    );
                                    cx.notify();
                                }
                            });
                        }
                    }),
                )
                .child(
                    SegmentedControl::new(
                        "filter_mode",
                        if state.only_changed { "changed" } else { "all" },
                    )
                    .size(ControlSize::Sm)
                    .segment("changed", "Solo cambios", Some(IconName::Filter))
                    .segment("all", "Todos", Some(IconName::ListChecks))
                    .on_change({
                        let view = view.clone();
                        move |value, _, cx| {
                            let is_changed = value.as_ref() == "changed";
                            view.update(cx, |this, cx| {
                                this.only_changed = is_changed;
                                this.refilter();
                                cx.notify();
                            });
                        }
                    }),
                )
                .child(
                    div().w(px(200.0)).child(
                        Input::new(&state.search_input)
                            .size(ControlSize::Sm)
                            .prefix(
                                Icon::new(IconName::Search)
                                    .size(IconSize::Sm)
                                    .color(colors.fg_subtle),
                            )
                            .clearable(),
                    ),
                )
                .child(
                    div().w(px(200.0)).child(
                        Input::new(&state.selector_input)
                            .size(ControlSize::Sm)
                            .prefix(
                                Icon::new(IconName::Braces)
                                    .size(IconSize::Sm)
                                    .color(colors.fg_subtle),
                            )
                            .clearable(),
                    ),
                )
                .when(current_selector != "$", |el| {
                    let view = view.clone();
                    el.child(
                        Button::new("reset_selector", "Restaurar $")
                            .size(ControlSize::Sm)
                            .variant(ButtonVariant::Ghost)
                            .icon(IconName::RotateCcw)
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.reset_selector(cx);
                                });
                            }),
                    )
                })
            },
        ))
        // Lado derecho: Notificaciones, badges de estado, exportación y tema
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .when_some(state.status_toast.clone(), |el, toast| {
                    el.child(Badge::new(toast).tone(Tone::Success).dot())
                })
                .when(
                    state.view_mode == ViewMode::Table && state.comparison.is_some(),
                    |el| {
                        let c = state.comparison.as_ref().unwrap();
                        let diff_count = c.changed_count();
                        let total = c.rows.len();
                        let view_md = view.clone();
                        let view_density = view.clone();
                        el.child(
                            Badge::new(format!("{diff_count} de {total} con diferencias"))
                                .tone(if diff_count > 0 {
                                    Tone::Warning
                                } else {
                                    Tone::Success
                                })
                                .dot(),
                        )
                        .child(
                            Button::new("export_md", "Copiar Markdown")
                                .size(ControlSize::Sm)
                                .variant(ButtonVariant::Outline)
                                .icon(IconName::Copy)
                                .on_click(move |_, _, cx| {
                                    view_md.update(cx, |this, cx| {
                                        if let Some(c) = &this.comparison {
                                            let md = export_markdown(c, this.only_changed);
                                            cx.write_to_clipboard(ClipboardItem::new_string(md));
                                            this.show_toast(
                                                "Reporte Markdown copiado al portapapeles",
                                                cx,
                                            );
                                        }
                                    });
                                }),
                        )
                        .child(
                            IconButton::new(
                                "density_btn",
                                if state.density == Density::Compact {
                                    IconName::Rows2
                                } else {
                                    IconName::Minimize2
                                },
                            )
                            .size(ControlSize::Sm)
                            .tooltip(if state.density == Density::Compact {
                                "Cambiar a densidad cómoda (Ctrl+D)"
                            } else {
                                "Cambiar a densidad compacta (Ctrl+D)"
                            })
                            .on_click(move |_, _, cx| {
                                view_density.update(cx, |this, cx| this.toggle_density(cx));
                            }),
                        )
                    },
                )
                .child(
                    IconButton::new("cmd_palette_btn", IconName::Command)
                        .size(ControlSize::Sm)
                        .tooltip("Paleta de comandos (Ctrl+K)")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.show_command_palette = !this.show_command_palette;
                            this.show_settings = false;
                            cx.notify();
                        })),
                )
                .child(
                    IconButton::new("settings_btn", IconName::Settings)
                        .size(ControlSize::Sm)
                        .tooltip("Configuración (Ctrl+,)")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.show_settings = !this.show_settings;
                            this.show_command_palette = false;
                            cx.notify();
                        })),
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
                        "Modo claro (Ctrl+T)"
                    } else {
                        "Modo oscuro (Ctrl+T)"
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
