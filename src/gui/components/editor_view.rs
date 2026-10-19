//! Vista de Editor de código interactivo (CodeEditor) con marcas Git, buscador y panel de JSON Schema.

use crate::gui::DiffView;
use ely_gpui_component::{
    buttons::{Button, ButtonVariant, IconButton},
    data_display::{Badge, Tone},
    editor::{FindWidget, GitMark},
    git::{diff, LineKind, Stretch},
    layout::SplitPane,
    primitives::{Icon, IconName},
    theme::{ActiveTheme, ControlSize, TextSize},
};
use gpui::{div, prelude::*, px, Axis, Context, Entity, IntoElement};

/// Trunca nombres largos conservando inicio, elipsis y sufijo con extensión.
fn truncate_filename(name: &str, max_chars: usize) -> String {
    let char_count = name.chars().count();
    if char_count <= max_chars {
        return name.to_string();
    }
    let ext = if name.ends_with(".json") { ".json" } else { "" };
    let stem = if ext.is_empty() {
        name
    } else {
        &name[..name.len() - ext.len()]
    };
    let keep = max_chars.saturating_sub(ext.len() + 1);
    let head = keep * 2 / 3;
    let tail = keep.saturating_sub(head);

    let stem_chars: Vec<char> = stem.chars().collect();
    if stem_chars.len() <= keep {
        return name.to_string();
    }
    let prefix: String = stem_chars.iter().take(head).collect();
    let suffix: String = stem_chars.iter().skip(stem_chars.len() - tail).collect();
    format!("{prefix}…{suffix}{ext}")
}

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

/// Renderiza la vista del editor de código en dos paneles redimensionables (JSON Editor y JSON Schema).
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
    let current_file_name = c
        .files
        .get(current_file_idx)
        .map(|f| f.name.clone())
        .unwrap_or_else(|| "documento.json".to_string());

    // Selector de archivos en el editor con truncado inteligente
    let mut file_tabs = div().flex().flex_row().items_center().gap_1();
    for (i, f) in c.files.iter().enumerate() {
        let is_active = i == current_file_idx;
        let view = view.clone();
        let short_name = truncate_filename(&f.name, 18);
        let label = format!(
            "[{}] {}{}",
            i + 1,
            short_name,
            if i == base_idx { " *" } else { "" }
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
                            this.update_schema_editor(window, cx);
                            cx.notify();
                        }
                    });
                }),
        );
    }

    // Sub-barra superior del editor JSON (panel izquierdo)
    let left_subbar = div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .px_3()
        .py_1p5()
        .bg(colors.sunken)
        .border_b_1()
        .border_color(colors.border)
        // Lado izquierdo con tabs y badge base
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .min_w_0()
                .flex_1()
                .overflow_hidden()
                .child(file_tabs)
                .child(if is_base {
                    Badge::new("Base").tone(Tone::Accent).dot()
                } else {
                    Badge::new("Git").tone(Tone::Info).dot()
                }),
        )
        // Lado derecho con acciones garantizadas sin desbordamiento
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_1p5()
                .flex_shrink_0()
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
                    IconButton::new("validate_json", IconName::Check)
                        .size(ControlSize::Sm)
                        .tooltip("Validar sintaxis JSON")
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
                    Button::new("apply_diff", "Aplicar")
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
                )
                .child(if state.show_schema_panel {
                    IconButton::new("btn_toggle_schema_panel", IconName::PanelRight)
                        .size(ControlSize::Sm)
                        .tooltip("Ocultar panel lateral de Schema (Ctrl+J)")
                        .on_click({
                            let view = view.clone();
                            move |_, window, cx| {
                                view.update(cx, |this, cx| {
                                    this.toggle_schema_panel(window, cx);
                                });
                            }
                        })
                        .into_any_element()
                } else {
                    Button::new("btn_toggle_schema_panel", "Ver Schema")
                        .size(ControlSize::Sm)
                        .variant(ButtonVariant::Primary)
                        .icon(IconName::PanelRight)
                        .on_click({
                            let view = view.clone();
                            move |_, window, cx| {
                                view.update(cx, |this, cx| {
                                    this.toggle_schema_panel(window, cx);
                                });
                            }
                        })
                        .into_any_element()
                }),
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
        fw = fw.on_close(move |window, cx| {
            view_close.update(cx, |this, cx| {
                this.close_find_widget(Some(window), cx);
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

    let left_panel = div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .size_full()
        .border_r_1()
        .border_color(colors.border)
        .child(left_subbar)
        .child(editor_element);

    // Panel derecho: JSON Schema (Draft 2020-12)
    let right_subbar = div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .px_3()
        .py_1p5()
        .bg(colors.sunken)
        .border_b_1()
        .border_color(colors.border)
        // Título del schema con truncado protegido
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .min_w_0()
                .flex_1()
                .overflow_hidden()
                .child(
                    div()
                        .text_color(colors.accent)
                        .child(Icon::new(IconName::FileJson)),
                )
                .child(
                    div()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_size(theme.text_size(TextSize::Sm))
                        .text_color(colors.fg)
                        .child("JSON Schema"),
                )
                .child(Badge::new("Draft 2020-12").tone(Tone::Accent))
                .child(
                    div()
                        .text_size(theme.text_size(TextSize::Xs))
                        .text_color(colors.fg_muted)
                        .truncate()
                        .max_w(px(160.0))
                        .child(format!("({})", truncate_filename(&current_file_name, 18))),
                ),
        )
        // Acciones del schema + botón de ocultar
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_1p5()
                .flex_shrink_0()
                .child(
                    IconButton::new("btn_sync_schema_panel", IconName::RefreshCw)
                        .size(ControlSize::Sm)
                        .tooltip("Sincronizar schema con el editor")
                        .on_click({
                            let view = view.clone();
                            move |_, window, cx| {
                                view.update(cx, |this, cx| {
                                    this.update_schema_editor(window, cx);
                                    this.show_toast("JSON Schema sincronizado ✓", cx);
                                    cx.notify();
                                });
                            }
                        }),
                )
                .child(
                    Button::new("btn_copy_schema_panel", "Copiar")
                        .size(ControlSize::Sm)
                        .variant(ButtonVariant::Ghost)
                        .icon(IconName::Copy)
                        .on_click({
                            let view = view.clone();
                            move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.copy_schema_to_clipboard(cx);
                                });
                            }
                        }),
                )
                .child(
                    Button::new("btn_export_schema_panel", "Exportar…")
                        .size(ControlSize::Sm)
                        .variant(ButtonVariant::Outline)
                        .icon(IconName::Save)
                        .on_click({
                            let view = view.clone();
                            move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.export_schema_to_disk(cx);
                                });
                            }
                        }),
                )
                .child(
                    IconButton::new("btn_hide_schema_panel", IconName::X)
                        .size(ControlSize::Sm)
                        .tooltip("Ocultar panel de Schema (Ctrl+J)")
                        .on_click({
                            let view = view.clone();
                            move |_, window, cx| {
                                view.update(cx, |this, cx| {
                                    this.toggle_schema_panel(window, cx);
                                });
                            }
                        }),
                ),
        );

    let schema_element = if let Some(schema_ed) = &state.schema_editor {
        div()
            .flex_1()
            .min_h_0()
            .size_full()
            .child(schema_ed.clone())
    } else {
        div()
            .flex_1()
            .min_h_0()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_color(colors.fg_muted)
            .child("Generando esquema estructural…")
    };

    let right_panel = div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .size_full()
        .child(right_subbar)
        .child(schema_element);

    if state.show_schema_panel {
        SplitPane::new("editor_schema_split", Axis::Horizontal, px(240.0))
            .sizes(&[0.55, 0.45])
            .pane(left_panel)
            .pane(right_panel)
            .into_any_element()
    } else {
        left_panel.into_any_element()
    }
}
