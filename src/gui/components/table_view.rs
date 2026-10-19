//! Vista matricial virtualizada (Tabla) para comparar de 2 a 10 archivos JSON.

use super::cell::{
    build_cell_tooltip, render_absent_badge, render_empty_symbol, render_line_gutter,
    render_new_badge, render_type_changed_badge, render_type_content, split_path,
};
use crate::gui::DiffView;
use ely_gpui_component::{
    buttons::{Button, ButtonVariant, IconButton},
    primitives::{Icon, IconName, Tooltip},
    theme::{ActiveTheme, ControlSize, IconSize, Radius, TextSize},
};
use gpui::{
    div, prelude::*, px, uniform_list, ClipboardItem, Context, Entity, FontWeight, IntoElement,
};
use jsondiff::{detect_json_type, is_type_change, DiffMode, MIN_FILES};
use std::ops::Range;

/// Renderiza la vista de tabla con encabezado de columnas, filas virtualizadas y estado vacío reactivo.
pub fn render_table_view(
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

    let base_ix = c.base_index;
    let diff_mode = c.diff_mode;
    let files_count = c.files.len();

    let row_h = state.density.row_height();
    let header_h = state.density.header_height();
    let path_w = state.density.path_width();

    // ------------------------------------------------ Encabezado de columnas
    let mut header = div()
        .flex()
        .flex_row()
        .w_full()
        .h(px(header_h))
        .items_center()
        .bg(colors.sunken)
        .border_b_1()
        .border_color(colors.border)
        .text_color(colors.fg_muted)
        .child(
            div()
                .w(px(path_w))
                .flex_none()
                .px_2p5()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_size(theme.text_size(TextSize::Xs))
                        .child("Campo / Ruta JSON"),
                )
                .when(c.selector != "$", |el| {
                    el.child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1()
                            .px_1p5()
                            .py(px(0.5))
                            .rounded(theme.radius(Radius::Sm))
                            .bg(colors.accent.opacity(0.18))
                            .text_color(colors.accent)
                            .text_size(theme.text_size(TextSize::Xs))
                            .font_weight(FontWeight::MEDIUM)
                            .child(c.selector.clone()),
                    )
                }),
        );

    for (i, f) in c.files.iter().enumerate() {
        let is_base = match diff_mode {
            DiffMode::FixedBase => i == base_ix,
            DiffMode::Sequential => i == 0,
        };
        let label = format!(
            "[{}] {}{}",
            i + 1,
            f.name,
            if f.lines_exact { "" } else { " *" }
        );
        let full_path = f.path.display().to_string();

        let can_move_left = i > 0;
        let can_move_right = i < files_count - 1;

        let mut move_controls = div().flex().flex_row().items_center().gap_0p5();

        if can_move_left {
            let view = view.clone();
            move_controls = move_controls.child(
                IconButton::new(("move_left", i), IconName::ChevronLeft)
                    .size(ControlSize::Sm)
                    .tooltip("Mover archivo hacia la izquierda")
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.move_file(i, i - 1, cx);
                        });
                    }),
            );
        }

        if can_move_right {
            let view = view.clone();
            move_controls = move_controls.child(
                IconButton::new(("move_right", i), IconName::ChevronRight)
                    .size(ControlSize::Sm)
                    .tooltip("Mover archivo hacia la derecha")
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.move_file(i, i + 1, cx);
                        });
                    }),
            );
        }

        if files_count > MIN_FILES {
            let view = view.clone();
            move_controls = move_controls.child(
                IconButton::new(("remove_file", i), IconName::X)
                    .size(ControlSize::Sm)
                    .tooltip("Quitar este archivo de la comparación")
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.remove_file(i, cx);
                        });
                    }),
            );
        }

        let role_badge: gpui::AnyElement = match diff_mode {
            DiffMode::FixedBase => {
                if is_base {
                    div()
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_1()
                                .px_1p5()
                                .py(px(0.5))
                                .rounded(theme.radius(Radius::Sm))
                                .bg(colors.accent.opacity(0.18))
                                .text_color(colors.accent)
                                .text_size(theme.text_size(TextSize::Xs))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(div().w(px(5.0)).h(px(5.0)).rounded_full().bg(colors.accent))
                                .child("Base"),
                        )
                        .into_any_element()
                } else {
                    let view = view.clone();
                    div()
                        .id(("set_base_tip", i))
                        .tooltip(Tooltip::text("Establecer este archivo como base"))
                        .child(
                            Button::new(("set_base", i), "Hacer base")
                                .size(ControlSize::Sm)
                                .variant(ButtonVariant::Ghost)
                                .on_click(move |_, _, cx| {
                                    view.update(cx, |this, cx| {
                                        if let Some(c) = &mut this.comparison {
                                            if c.set_base_index(i).is_ok() {
                                                this.refilter();
                                                this.show_toast(
                                                    format!("Archivo [{}] ahora es la base", i + 1),
                                                    cx,
                                                );
                                                cx.notify();
                                            }
                                        }
                                    });
                                }),
                        )
                        .into_any_element()
                }
            }
            DiffMode::Sequential => {
                if i == 0 {
                    div()
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_1()
                                .px_1p5()
                                .py(px(0.5))
                                .rounded(theme.radius(Radius::Sm))
                                .bg(colors.accent.opacity(0.18))
                                .text_color(colors.accent)
                                .text_size(theme.text_size(TextSize::Xs))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(div().w(px(5.0)).h(px(5.0)).rounded_full().bg(colors.accent))
                                .child("Origen"),
                        )
                        .into_any_element()
                } else {
                    div()
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_1()
                                .px_1p5()
                                .py(px(0.5))
                                .rounded(theme.radius(Radius::Sm))
                                .bg(colors.info.opacity(0.15))
                                .text_color(colors.info)
                                .text_size(theme.text_size(TextSize::Xs))
                                .font_weight(FontWeight::MEDIUM)
                                .child(format!("vs [{}]", i)),
                        )
                        .into_any_element()
                }
            }
        };

        let col_header = div()
            .flex_1()
            .min_w_0()
            .px_2()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap_1p5()
            .child(
                div()
                    .id(("col_title", i))
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(theme.text_size(TextSize::Xs))
                    .font_weight(if is_base {
                        FontWeight::BOLD
                    } else {
                        FontWeight::MEDIUM
                    })
                    .text_color(if is_base { colors.accent } else { colors.fg })
                    .tooltip(Tooltip::text(full_path))
                    .child(label),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .child(role_badge)
                    .child(move_controls),
            );

        header = header.child(col_header);
    }

    // ------------------------------------------------ Filas virtualizadas mediante uniform_list (Zero-Copy Arc)
    let list_indices = std::sync::Arc::clone(&state.cached_visible);
    let rows_data = std::sync::Arc::clone(&state.cached_rows);
    let list_view = view.clone();
    let is_empty = list_indices.is_empty();

    let list = uniform_list(
        "diff_rows",
        list_indices.len(),
        move |range: Range<usize>, _, cx| {
            let theme = cx.theme();
            let colors = &theme.colors;

            range
                .map(|ix| {
                    let row_idx = list_indices[ix];
                    let row = &rows_data[row_idx];
                    let is_diff = row.changed;

                    let mut row_el = div()
                        .id(("row", row_idx))
                        .flex()
                        .flex_row()
                        .w_full()
                        .h(px(row_h))
                        .items_center()
                        .border_b_1()
                        .border_color(colors.border.opacity(0.35))
                        .bg(if is_diff {
                            colors.warning.opacity(0.04)
                        } else {
                            colors.surface
                        })
                        .hover(|s| s.bg(colors.hover.opacity(0.45)));

                    // Columna de ruta
                    let (parent_path, leaf_name) = split_path(&row.path);

                    let path_cell = div()
                        .id(("path_cell", row_idx))
                        .w(px(path_w))
                        .flex_none()
                        .px_2p5()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1p5()
                        .cursor_pointer()
                        .tooltip(Tooltip::text(format!(
                            "Ruta: {}\nClic para copiar",
                            row.path
                        )))
                        .on_click({
                            let view = list_view.clone();
                            move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    if let Some(r) = this.cached_rows.get(row_idx) {
                                        let p = r.path.clone();
                                        cx.write_to_clipboard(ClipboardItem::new_string(p.clone()));
                                        this.show_toast(format!("Ruta copiada: {}", p), cx);
                                    }
                                });
                            }
                        })
                        .child(div().flex_none().w(px(5.0)).h(px(5.0)).rounded_full().bg(
                            if is_diff {
                                colors.warning
                            } else {
                                colors.success
                            },
                        ))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .flex()
                                .flex_row()
                                .items_center()
                                .child(
                                    div()
                                        .text_color(colors.fg_subtle)
                                        .text_size(theme.text_size(TextSize::Xs))
                                        .child(parent_path.to_string()),
                                )
                                .child(
                                    div()
                                        .font_weight(if is_diff {
                                            FontWeight::SEMIBOLD
                                        } else {
                                            FontWeight::NORMAL
                                        })
                                        .text_color(if is_diff {
                                            colors.fg
                                        } else {
                                            colors.fg_muted
                                        })
                                        .text_size(theme.text_size(TextSize::Xs))
                                        .child(leaf_name.to_string()),
                                ),
                        );

                    row_el = row_el.child(path_cell);

                    // Columnas de valores
                    for file_col in 0..files_count {
                        let (is_base, ref_cell, ref_label) = match diff_mode {
                            DiffMode::FixedBase => (
                                file_col == base_ix,
                                row.cells.get(base_ix).and_then(|c| c.as_ref()),
                                format!("[{}]", base_ix + 1),
                            ),
                            DiffMode::Sequential => {
                                if file_col == 0 {
                                    (true, None, "[1]".to_string())
                                } else {
                                    (
                                        false,
                                        row.cells.get(file_col - 1).and_then(|c| c.as_ref()),
                                        format!("[{}]", file_col),
                                    )
                                }
                            }
                        };

                        let cell = row.cells.get(file_col).and_then(|c| c.as_ref());

                        let cell_el = if let Some(c) = cell {
                            let val_str = &c.value;
                            let line_num = c.line;

                            let jtype = detect_json_type(val_str);
                            let (differs_from_ref, type_changed, is_new_field) = if is_base {
                                (false, false, false)
                            } else {
                                match ref_cell {
                                    Some(b) => (
                                        b.value != *val_str,
                                        is_type_change(&b.value, val_str),
                                        false,
                                    ),
                                    None => (true, false, true),
                                }
                            };

                            let tooltip_msg = build_cell_tooltip(
                                is_base,
                                diff_mode,
                                base_ix,
                                line_num,
                                is_new_field,
                                type_changed,
                                differs_from_ref,
                                &ref_label,
                                ref_cell.map(|b| b.value.as_str()),
                                val_str,
                            );

                            let mut val_content = div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_1()
                                .min_w_0()
                                .overflow_hidden();

                            if is_new_field {
                                val_content = val_content.child(render_new_badge(&theme));
                            }

                            if type_changed {
                                val_content = val_content.child(render_type_changed_badge(&theme));
                            }

                            val_content = val_content.child(render_type_content(
                                jtype,
                                val_str,
                                differs_from_ref,
                                is_base,
                                &theme,
                            ));

                            let mut cell_box = div()
                                .id(("val_cell", row_idx * 10 + file_col))
                                .flex_1()
                                .min_w_0()
                                .h(px(row_h - 2.0))
                                .mx(px(2.0))
                                .px_1p5()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_1p5()
                                .cursor_pointer()
                                .rounded(theme.radius(Radius::Sm))
                                .tooltip(Tooltip::text(tooltip_msg))
                                .on_click({
                                    let view = list_view.clone();
                                    move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            if let Some(r) = this.cached_rows.get(row_idx) {
                                                if let Some(Some(c)) = r.cells.get(file_col) {
                                                    let val = c.value.clone();
                                                    cx.write_to_clipboard(
                                                        ClipboardItem::new_string(val.clone()),
                                                    );
                                                    this.show_toast(
                                                        format!("Valor copiado: {}", val),
                                                        cx,
                                                    );
                                                }
                                            }
                                        });
                                    }
                                });

                            if differs_from_ref {
                                cell_box = cell_box
                                    .bg(colors.warning.opacity(0.10))
                                    .border_1()
                                    .border_color(colors.warning.opacity(0.35));
                            } else if is_base {
                                cell_box = cell_box.bg(colors.sunken.opacity(0.30));
                            }

                            let line_info = format!("L{}", line_num);
                            cell_box
                                .child(render_line_gutter(&line_info, &theme))
                                .child(val_content)
                        } else {
                            if ref_cell.is_some() {
                                div()
                                    .id(("val_cell_empty", row_idx * 10 + file_col))
                                    .flex_1()
                                    .min_w_0()
                                    .h(px(row_h - 2.0))
                                    .mx(px(2.0))
                                    .px_1p5()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1p5()
                                    .rounded(theme.radius(Radius::Sm))
                                    .bg(colors.danger.opacity(0.08))
                                    .border_1()
                                    .border_color(colors.danger.opacity(0.25))
                                    .tooltip(Tooltip::text(format!(
                                        "Campo ausente en este archivo (presente en {})",
                                        ref_label
                                    )))
                                    .child(render_line_gutter("-", &theme))
                                    .child(render_absent_badge(&theme))
                            } else {
                                div()
                                    .id(("val_cell_empty", row_idx * 10 + file_col))
                                    .flex_1()
                                    .min_w_0()
                                    .h(px(row_h - 2.0))
                                    .mx(px(2.0))
                                    .px_1p5()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1p5()
                                    .tooltip(Tooltip::text("Campo ausente"))
                                    .child(render_line_gutter("-", &theme))
                                    .child(render_empty_symbol(&theme))
                            }
                        };

                        row_el = row_el.child(cell_el);
                    }

                    row_el.into_any_element()
                })
                .collect()
        },
    )
    .size_full();

    // ------------------------------------------------ Estado vacío de resultados
    let body_content = if is_empty {
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .flex_1()
            .size_full()
            .p_8()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_3()
                    .p_6()
                    .rounded(theme.radius(Radius::Md))
                    .border_1()
                    .border_color(colors.border)
                    .bg(colors.surface)
                    .child(
                        Icon::new(IconName::ListChecks)
                            .size(IconSize::Lg)
                            .color(colors.success),
                    )
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(colors.fg)
                            .child(if c.rows.is_empty() {
                                format!(
                                    "El selector '{}' no coincidió con ningún campo en los archivos JSON.",
                                    c.selector
                                )
                            } else if state.only_changed {
                                if c.changed_count() == 0 {
                                    "¡Los archivos son idénticos! No hay diferencias en los campos seleccionados."
                                        .to_string()
                                } else {
                                    "No hay diferencias que coincidan con la búsqueda."
                                        .to_string()
                                }
                            } else {
                                "No se encontraron campos que coincidan con la búsqueda."
                                    .to_string()
                            }),
                    )
                    .when(c.selector != "$", |el| {
                        let view = view.clone();
                        el.child(
                            Button::new(
                                "btn_reset_empty",
                                "Restaurar estructura completa ($)",
                            )
                            .size(ControlSize::Sm)
                            .variant(ButtonVariant::Primary)
                            .icon(IconName::RotateCcw)
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.reset_selector(cx);
                                });
                            }),
                        )
                    })
                    .when(state.only_changed && !c.rows.is_empty(), |el| {
                        let view = view.clone();
                        el.child(
                            Button::new("btn_show_all", "Ver todos los campos")
                                .size(ControlSize::Sm)
                                .variant(ButtonVariant::Outline)
                                .on_click(move |_, _, cx| {
                                    view.update(cx, |this, cx| {
                                        this.only_changed = false;
                                        this.refilter();
                                        cx.notify();
                                    });
                                }),
                        )
                    }),
            )
            .into_any_element()
    } else {
        list.into_any_element()
    };

    div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .size_full()
        .child(header)
        .child(div().flex_1().min_h_0().size_full().child(body_content))
        .into_any_element()
}
