//! Pantalla de bienvenida y estado vacío (DropZone).

use crate::gui::DiffView;
use ely_gpui_component::{
    buttons::{Button, ButtonVariant},
    primitives::{Icon, IconName},
    theme::{ActiveTheme, Elevation, IconSize, Radius, TextSize},
    typography::KbdCombo,
};
use gpui::{div, prelude::*, px, Context, FontWeight, IntoElement};

/// Renderiza la vista de bienvenida cuando no hay archivos cargados.
pub fn render_empty_view(cx: &mut Context<DiffView>) -> impl IntoElement {
    let theme = cx.theme();
    let colors = &theme.colors;

    div()
        .flex()
        .flex_col()
        .flex_1()
        .items_center()
        .justify_center()
        .p_8()
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_4()
                .p_8()
                .max_w(px(540.0))
                .rounded(theme.radius(Radius::Lg))
                .border_1()
                .border_dashed()
                .border_color(colors.border_strong)
                .bg(colors.surface)
                .shadow(theme.elevation(Elevation::Raised))
                .child(
                    Icon::new(IconName::FolderOpen)
                        .size(IconSize::Lg)
                        .color(colors.accent),
                )
                .child(
                    div()
                        .text_size(theme.text_size(TextSize::Lg))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(colors.fg)
                        .child("Comparador y Editor de JSON"),
                )
                .child(
                    div()
                        .text_size(theme.text_size(TextSize::Sm))
                        .text_color(colors.fg_muted)
                        .text_center()
                        .child(
                            "Arrastra de 2 a 10 archivos JSON a esta ventana o utiliza el botón para seleccionarlos del disco.\n\
                             Incluye vista matricial, comparador de cambios estilo Git y editor de código en vivo.",
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_3()
                        .child(
                            Button::new("open_empty", "Abrir archivos…")
                                .primary()
                                .icon(IconName::FolderOpen)
                                .on_click(cx.listener(|this, _, _, cx| this.pick_files(cx))),
                        )
                        .child(
                            Button::new("cmd_empty", "Paleta de comandos")
                                .variant(ButtonVariant::Outline)
                                .icon(IconName::Command)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.show_command_palette = true;
                                    this.show_settings = false;
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("settings_empty", "Configuración")
                                .variant(ButtonVariant::Ghost)
                                .icon(IconName::Settings)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.show_settings = true;
                                    this.show_command_palette = false;
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_4()
                        .pt_2()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .text_size(theme.text_size(TextSize::Xs))
                                .text_color(colors.fg_muted)
                                .child("Paleta:")
                                .child(KbdCombo::new("secondary-k")),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .text_size(theme.text_size(TextSize::Xs))
                                .text_color(colors.fg_muted)
                                .child("Abrir:")
                                .child(KbdCombo::new("secondary-o")),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .text_size(theme.text_size(TextSize::Xs))
                                .text_color(colors.fg_muted)
                                .child("Densidad:")
                                .child(KbdCombo::new("secondary-d")),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .text_size(theme.text_size(TextSize::Xs))
                                .text_color(colors.fg_muted)
                                .child("Ajustes:")
                                .child(KbdCombo::new("secondary-,"))
                                .child(div().text_xs().text_color(colors.fg_subtle).child("(o F2)")),
                        ),
                ),
        )
}
