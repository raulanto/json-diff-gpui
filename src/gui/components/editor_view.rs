//! Vista de Editor de código interactivo (CodeEditor) con marcas Git y acciones de guardado.

use crate::gui::DiffView;
use ely_gpui_component::{
    buttons::{Button, ButtonVariant},
    data_display::{Badge, Tone},
    editor::{FindWidget, GitMark},
    git::{diff, LineKind, Stretch},
    primitives::IconName,
    theme::{ActiveTheme, ControlSize},
};
use gpui::{div, prelude::*, Context, Entity, IntoElement};

/// Calcula marcas en el gutter (líneas añadidas) para el editor comparando contra el archivo base.
pub fn compute_git_marks(base: &str, current: &str) -> Vec<(usize, GitMark)> {
    let stretches = diff(base, current, 0);
    let mut marks = Vec::new();
    for stretch in stretches {
        if let Stretch::Hunk { lines, .. } = stretch {
            for line in lines {
                if line.kind == LineKind::Added {
                    if let Some(new_line) = line.new {
                        if new_line > 0 {
                            marks.push((new_line - 1, GitMark::Added));
                        }
                    }
                }
            }
        }
    }
    marks
}

/// Renderiza la vista del editor de código con pestañas de archivos, FindWidget y barra de herramientas.
pub fn render_editor_view(
    state: &DiffView,
    view: Entity<DiffView>,
    cx: &mut Context<DiffView>,
) -> impl IntoElement {
    let theme = cx.theme();
    let colors = &theme.colors;

    let c = match &state.comparison {
        Some(comp) => comp,
        None => return div().into_any_element(),
    };

    let base_idx = c.base_index;
    let current_file_idx = state.editor_file_idx.min(c.files.len() - 1);
    let is_base = current_file_idx == base_idx;

    // Selector de archivos en el editor
    let mut file_tabs = div().flex().flex_row().items_center().gap_1();
    for (i, f) in c.files.iter().enumerate() {
        let is_active = i == current_file_idx;
        let view = view.clone();
        let label = format!(
            "[{}] {}{}",
            i + 1,
            f.name,
            if i == base_idx { " (Base)" } else { "" }
        );
        file_tabs = file_tabs.child(
            Button::new(("editor_file_tab", i), label)
                .size(ControlSize::Sm)
                .variant(if is_active {
                    ButtonVariant::Primary
                } else {
                    ButtonVariant::Ghost
                })
                .on_click(move |_, window, cx| {
                    view.update(cx, |this, cx| {
                        if this.editor_file_idx != i {
                            this.editor_file_idx = i;
                            this.init_editor_if_needed(window, cx);
                            cx.notify();
                        }
                    });
                }),
        );
    }

    // Sub-barra superior del editor con acciones
    let editor_subbar = div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .px_3()
        .py_2()
        .bg(colors.sunken)
        .border_b_1()
        .border_color(colors.border)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_3()
                .child(file_tabs)
                .child(if is_base {
                    Badge::new("Archivo Base de Referencia")
                        .tone(Tone::Accent)
                        .dot()
                } else {
                    Badge::new("Archivo Comparado (Marcas Git en Gutter)")
                        .tone(Tone::Info)
                        .dot()
                }),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    Button::new("btn_find_editor", "Buscar")
                        .size(ControlSize::Sm)
                        .variant(if state.show_find_widget {
                            ButtonVariant::Primary
                        } else {
                            ButtonVariant::Ghost
                        })
                        .icon(IconName::Search)
                        .on_click({
                            let view = view.clone();
                            move |_, window, cx| {
                                view.update(cx, |this, cx| {
                                    this.toggle_find_widget(window, cx);
                                });
                            }
                        }),
                )
                .child(
                    Button::new("btn_schema_editor", "JSON Schema")
                        .size(ControlSize::Sm)
                        .variant(ButtonVariant::Ghost)
                        .icon(IconName::FileJson)
                        .on_click({
                            let view = view.clone();
                            move |_, window, cx| {
                                view.update(cx, |this, cx| {
                                    this.open_schema_dialog(window, cx);
                                });
                            }
                        }),
                )
                .child(
                    Button::new("fmt_json", "Formatear")
                        .size(ControlSize::Sm)
                        .variant(ButtonVariant::Ghost)
                        .icon(IconName::Sparkles)
                        .on_click({
                            let view = view.clone();
                            move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.format_editor_json(cx);
                                });
                            }
                        }),
                )
                .child(
                    Button::new("validate_json", "Validar")
                        .size(ControlSize::Sm)
                        .variant(ButtonVariant::Ghost)
                        .icon(IconName::Check)
                        .on_click({
                            let view = view.clone();
                            move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.validate_editor_json(cx);
                                });
                            }
                        }),
                )
                .child(
                    Button::new("apply_diff", "Aplicar al Diff")
                        .size(ControlSize::Sm)
                        .primary()
                        .icon(IconName::RefreshCw)
                        .on_click({
                            let view = view.clone();
                            move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.apply_editor_changes(cx);
                                });
                            }
                        }),
                )
                .child(
                    Button::new("save_file", "Guardar")
                        .size(ControlSize::Sm)
                        .variant(ButtonVariant::Outline)
                        .icon(IconName::Save)
                        .on_click({
                            let view = view.clone();
                            move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.save_editor_to_disk(cx);
                                });
                            }
                        }),
                ),
        );

    // Widget flotante de búsqueda integrado con Ely
    let find_widget_element = if state.show_find_widget {
        let mut fw = FindWidget::new(
            "editor_find_widget",
            &state.find_input,
            state.find_current_index,
            state.find_matches.len(),
        )
        .options(state.find_options);

        if state.show_replace {
            fw = fw.replace(&state.replace_input);
        }

        let view_opt = view.clone();
        fw = fw.on_options(move |opts, _, cx| {
            view_opt.update(cx, |this, cx| {
                this.find_options = opts;
                this.update_find_matches(cx);
            });
        });

        let view_step = view.clone();
        fw = fw.on_step(move |forward, _, cx| {
            view_step.update(cx, |this, cx| {
                this.step_find_match(forward, cx);
            });
        });

        let view_repl = view.clone();
        fw = fw.on_replace(move |all, _, cx| {
            view_repl.update(cx, |this, cx| {
                this.replace_find_match(all, cx);
            });
        });

        let view_toggle = view.clone();
        fw = fw.on_toggle_replace(move |show, _, cx| {
            view_toggle.update(cx, |this, cx| {
                this.show_replace = show;
                cx.notify();
            });
        });

        let view_close = view.clone();
        fw = fw.on_close(move |_, cx| {
            view_close.update(cx, |this, cx| {
                this.close_find_widget(cx);
            });
        });

        Some(div().absolute().top_3().right_6().shadow_lg().child(fw))
    } else {
        None
    };

    let editor_element = div()
        .relative()
        .flex_1()
        .min_h_0()
        .size_full()
        .when_some(state.editor.clone(), |el, ed| el.child(ed))
        .children(find_widget_element);

    div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .size_full()
        .child(editor_subbar)
        .child(editor_element)
        .into_any_element()
}
