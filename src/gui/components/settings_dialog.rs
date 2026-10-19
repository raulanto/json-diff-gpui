//! Modal de configuración de la aplicación (Settings Dialog) con catálogo de atajos.

use crate::gui::shortcuts::shortcuts_list;
use crate::gui::types::{Density, SettingsTab};
use crate::gui::DiffView;
use ely_gpui_component::{
    buttons::{Button, ButtonVariant, SegmentedControl},
    data_display::{Badge, Tone},
    overlays::Dialog,
    primitives::IconName,
    settings::{KeyboardShortcutsList, SettingsRow, SettingsSection},
    theme::{ActiveTheme, ControlSize, Mode, TextSize, Theme},
};
use gpui::{div, prelude::*, px, Context, Entity, IntoElement};
use jsondiff::DiffMode;

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
                .child(
                    SettingsSection::new("Capacidad y Soporte")
                        .description("Límites del motor y características admitidas.")
                        .row(
                            SettingsRow::new("Límite de archivos")
                                .description("Comparación matricial de 2 a 10 archivos JSON simultáneos.")
                                .control(
                                    Badge::new("2 - 10 archivos")
                                        .tone(Tone::Info),
                                ),
                        ),
                )
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
    .detail("Preferencias visuales, modo de comparación y catálogo de atajos de teclado.")
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
                        SettingsTab::Shortcuts => "shortcuts",
                    },
                )
                .size(ControlSize::Sm)
                .segment(
                    "general",
                    "General y Preferencias",
                    Some(IconName::Settings),
                )
                .segment("shortcuts", "Atajos de Teclado", Some(IconName::Keyboard))
                .on_change(move |val, _, cx| {
                    let tab = if val.as_ref() == "shortcuts" {
                        SettingsTab::Shortcuts
                    } else {
                        SettingsTab::General
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
