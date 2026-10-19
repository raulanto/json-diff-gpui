//! Modal de previsualización y exportación de JSON Schema (Draft 2020-12).

use crate::gui::DiffView;
use ely_gpui_component::{
    buttons::{Button, ButtonVariant},
    data_display::{Badge, Tone},
    overlays::Dialog,
    primitives::IconName,
    theme::{ActiveTheme, ControlSize, TextSize},
};
use gpui::{div, prelude::*, px, Context, Entity, IntoElement};

/// Renderiza el modal de previsualización de JSON Schema usando Dialog y CodeEditor de Ely.
pub fn render_schema_dialog(
    state: &DiffView,
    view: Entity<DiffView>,
    cx: &mut Context<DiffView>,
) -> impl IntoElement {
    let theme = cx.theme();
    let colors = &theme.colors;

    let file_name = state
        .comparison
        .as_ref()
        .and_then(|c| c.files.get(state.editor_file_idx))
        .map(|f| f.name.clone())
        .unwrap_or_else(|| "documento".to_string());

    let view_close = view.clone();
    let view_copy = view.clone();
    let view_export = view.clone();
    let view_close_btn = view.clone();

    Dialog::new(
        "json_schema_dialog",
        format!("JSON Schema — {}", file_name),
        move |_, cx| {
            view_close.update(cx, |this, cx| {
                this.show_schema_dialog = false;
                cx.notify();
            });
        },
    )
    .detail("Esquema estructural generado automáticamente según especificación JSON Schema Draft 2020-12.")
    .child(
        div()
            .flex()
            .flex_col()
            .gap_3()
            // Barra superior de acciones y badges
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(Badge::new("Draft 2020-12").tone(Tone::Accent))
                            .child(Badge::new("Solo Lectura").tone(Tone::Neutral))
                            .child(
                                div()
                                    .text_size(theme.text_size(TextSize::Xs))
                                    .text_color(colors.fg_muted)
                                    .child(format!("Inferencia desde '{}'", file_name)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                Button::new("btn_copy_schema_dialog", "Copiar Schema")
                                    .size(ControlSize::Sm)
                                    .variant(ButtonVariant::Outline)
                                    .icon(IconName::Copy)
                                    .on_click(move |_, _, cx| {
                                        view_copy.update(cx, |this, cx| {
                                            this.copy_schema_to_clipboard(cx);
                                        });
                                    }),
                            )
                            .child(
                                Button::new("btn_export_schema_dialog", "Exportar Schema…")
                                    .size(ControlSize::Sm)
                                    .variant(ButtonVariant::Primary)
                                    .icon(IconName::Save)
                                    .on_click(move |_, _, cx| {
                                        view_export.update(cx, |this, cx| {
                                            this.export_schema_to_disk(cx);
                                        });
                                    }),
                            ),
                    ),
            )
            // Visor del CodeEditor con el Schema generado
            .child(
                div()
                    .h(px(460.0))
                    .w_full()
                    .rounded_md()
                    .border_1()
                    .border_color(colors.border)
                    .bg(colors.surface)
                    .overflow_hidden()
                    .child(if let Some(editor) = &state.schema_editor {
                        div().size_full().child(editor.clone())
                    } else {
                        div()
                            .size_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(colors.fg_muted)
                            .child("No se pudo generar el editor de JSON Schema")
                    }),
            )
            // Pie del diálogo con botón para cerrar
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_end()
                    .pt_1()
                    .child(
                        Button::new("btn_close_schema_dialog", "Cerrar")
                            .size(ControlSize::Sm)
                            .variant(ButtonVariant::Ghost)
                            .on_click(move |_, _, cx| {
                                view_close_btn.update(cx, |this, cx| {
                                    this.show_schema_dialog = false;
                                    cx.notify();
                                });
                            }),
                    ),
            ),
    )
}
