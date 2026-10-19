//! Funciones reutilizables para el renderizado de celdas, micro-badges y gutters en la tabla matricial.

use ely_gpui_component::theme::{Radius, TextSize, Theme};
use gpui::{div, prelude::*, px, Div, FontWeight};
use jsondiff::{DiffMode, JsonType};

/// Divide una ruta JSON jerárquica en prefijo padre y clave hoja.
///
/// Ejemplo: `"data.user.name"` -> `("data.user.", "name")`.
#[inline]
pub fn split_path(path_str: &str) -> (&str, &str) {
    match path_str.rfind('.') {
        Some(pos) => (&path_str[..=pos], &path_str[pos + 1..]),
        None => ("", path_str),
    }
}

/// Renderiza el gutter numérico de línea (32px de ancho fijo alineado a la derecha).
pub fn render_line_gutter(line_text: &str, theme: &Theme) -> Div {
    let colors = &theme.colors;
    div()
        .flex_none()
        .w(px(32.0))
        .flex()
        .items_center()
        .justify_end()
        .pr_1()
        .text_size(theme.text_size(TextSize::Xs))
        .text_color(colors.fg_subtle)
        .child(line_text.to_string())
}

/// Micro-badge para campos nuevos respecto a la base o versión previa.
pub fn render_new_badge(theme: &Theme) -> Div {
    let colors = &theme.colors;
    div()
        .flex_none()
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .px_1()
        .py(px(0.5))
        .rounded(theme.radius(Radius::Sm))
        .bg(colors.success.opacity(0.15))
        .text_color(colors.success)
        .text_size(theme.text_size(TextSize::Xs))
        .font_weight(FontWeight::MEDIUM)
        .child(
            div()
                .w(px(4.0))
                .h(px(4.0))
                .rounded_full()
                .bg(colors.success),
        )
        .child("+ Nuevo")
}

/// Micro-badge para cambios de tipo semántico (e.g. String vs Number).
pub fn render_type_changed_badge(theme: &Theme) -> Div {
    let colors = &theme.colors;
    div()
        .flex_none()
        .px_1()
        .py(px(0.5))
        .rounded(theme.radius(Radius::Sm))
        .bg(colors.warning.opacity(0.18))
        .text_color(colors.warning)
        .text_size(theme.text_size(TextSize::Xs))
        .font_weight(FontWeight::MEDIUM)
        .child("≠ Tipo")
}

/// Micro-badge para campos ausentes respecto a la versión de comparación.
pub fn render_absent_badge(theme: &Theme) -> Div {
    let colors = &theme.colors;
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .px_1()
        .py(px(0.5))
        .rounded(theme.radius(Radius::Sm))
        .bg(colors.danger.opacity(0.15))
        .text_color(colors.danger)
        .text_size(theme.text_size(TextSize::Xs))
        .font_weight(FontWeight::MEDIUM)
        .child(div().w(px(4.0)).h(px(4.0)).rounded_full().bg(colors.danger))
        .child("Ausente")
}

/// Símbolo de valor vacío no aplicable (∅).
pub fn render_empty_symbol(theme: &Theme) -> Div {
    let colors = &theme.colors;
    div()
        .flex_none()
        .text_color(colors.fg_subtle)
        .text_size(theme.text_size(TextSize::Xs))
        .child("∅")
}

/// Construye el contenido visual tipado según el `JsonType` inferido.
pub fn render_type_content(
    jtype: JsonType,
    val_str: &str,
    differs_from_ref: bool,
    is_base: bool,
    theme: &Theme,
) -> Div {
    let colors = &theme.colors;
    match jtype {
        JsonType::Boolean => {
            let is_true = val_str == "true";
            div()
                .flex_none()
                .px_1()
                .py(px(0.5))
                .rounded(theme.radius(Radius::Sm))
                .bg(if is_true {
                    colors.success.opacity(0.15)
                } else {
                    colors.warning.opacity(0.15)
                })
                .text_color(if is_true {
                    colors.success
                } else {
                    colors.warning
                })
                .text_size(theme.text_size(TextSize::Xs))
                .font_weight(FontWeight::MEDIUM)
                .child(val_str.to_string())
        }
        JsonType::Null => div()
            .flex_none()
            .px_1()
            .py(px(0.5))
            .rounded(theme.radius(Radius::Sm))
            .bg(colors.sunken)
            .text_color(colors.fg_subtle)
            .text_size(theme.text_size(TextSize::Xs))
            .font_weight(FontWeight::MEDIUM)
            .child("null"),
        JsonType::Number => div()
            .font_weight(FontWeight::MEDIUM)
            .text_size(theme.text_size(TextSize::Xs))
            .text_color(if differs_from_ref {
                colors.warning
            } else if is_base {
                colors.accent
            } else {
                colors.fg
            })
            .child(val_str.to_string()),
        JsonType::String => div()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .text_size(theme.text_size(TextSize::Xs))
            .font_weight(if differs_from_ref {
                FontWeight::MEDIUM
            } else {
                FontWeight::NORMAL
            })
            .text_color(if differs_from_ref {
                colors.fg
            } else {
                colors.fg_muted
            })
            .child(val_str.to_string()),
        JsonType::Array | JsonType::Object => {
            let tag = if jtype == JsonType::Array { "[]" } else { "{}" };
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .min_w_0()
                .overflow_hidden()
                .child(
                    div()
                        .flex_none()
                        .px_1()
                        .py(px(0.5))
                        .rounded(theme.radius(Radius::Sm))
                        .bg(colors.accent.opacity(0.12))
                        .text_color(colors.accent)
                        .text_size(theme.text_size(TextSize::Xs))
                        .font_weight(FontWeight::MEDIUM)
                        .child(tag),
                )
                .child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(theme.text_size(TextSize::Xs))
                        .text_color(colors.fg_muted)
                        .child(val_str.to_string()),
                )
        }
        JsonType::Other => div()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .text_size(theme.text_size(TextSize::Xs))
            .text_color(if differs_from_ref {
                colors.fg
            } else {
                colors.fg_muted
            })
            .child(val_str.to_string()),
    }
}

/// Construye el mensaje completo para el tooltip descriptivo de una celda con valor.
#[allow(clippy::too_many_arguments)]
pub fn build_cell_tooltip(
    is_base: bool,
    diff_mode: DiffMode,
    base_ix: usize,
    line: usize,
    is_new_field: bool,
    type_changed: bool,
    differs_from_ref: bool,
    ref_label: &str,
    ref_val: Option<&str>,
    val_str: &str,
) -> String {
    if is_base {
        if diff_mode == DiffMode::Sequential {
            format!(
                "Origen [1] (inicio de secuencia) | L{}\nClic para copiar valor",
                line
            )
        } else {
            format!(
                "Base fija [{}] | L{}\nClic para copiar valor",
                base_ix + 1,
                line
            )
        }
    } else if is_new_field {
        format!(
            "Nuevo campo (no existía en {}) | L{}\nClic para copiar valor",
            ref_label, line
        )
    } else if type_changed {
        format!(
            "Cambio de tipo vs {}\nAnterior: {}\nActual: {}\nClic para copiar",
            ref_label,
            ref_val.unwrap_or(""),
            val_str
        )
    } else if differs_from_ref {
        format!(
            "Difiere de {}\nAnterior: {}\nActual: {}\nClic para copiar",
            ref_label,
            ref_val.unwrap_or(""),
            val_str
        )
    } else {
        format!("Idéntico a {} | L{}\nClic para copiar", ref_label, line)
    }
}
