//! Renderizado visual principal de DiffView en GPUI.

use crate::gui::components;
use crate::gui::shortcuts;
use crate::gui::types::ViewMode;
use crate::gui::DiffView;
use ely_gpui_component::{
    buttons::{Button, ButtonVariant},
    primitives::IconName,
    theme::{ActiveTheme, ControlSize, Radius, TextSize},
};
use gpui::{div, prelude::*, px, Context, ExternalPaths, IntoElement, Window};
use jsondiff::MAX_FILES;
use std::path::PathBuf;

impl gpui::Render for DiffView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.view_mode == ViewMode::Editor && self.editor.is_none() {
            self.init_editor_if_needed(window, cx);
        }

        let view = cx.entity().clone();

        let toolbar = components::toolbar::render_toolbar(self, view.clone(), cx);

        let body: gpui::AnyElement = if let Some(err) = &self.error {
            let theme = cx.theme();
            let colors = &theme.colors;
            let can_add_more = self.paths.len() < MAX_FILES;
            let is_waiting_for_second = self.paths.len() == 1;

            div()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .p_8()
                .gap_3()
                .child(
                    div()
                        .p_4()
                        .max_w(px(520.0))
                        .rounded(theme.radius(Radius::Md))
                        .bg(if is_waiting_for_second {
                            colors.warning.opacity(0.12)
                        } else {
                            colors.danger.opacity(0.12)
                        })
                        .border_1()
                        .border_color(if is_waiting_for_second {
                            colors.warning
                        } else {
                            colors.danger
                        })
                        .text_color(if is_waiting_for_second {
                            colors.warning
                        } else {
                            colors.danger
                        })
                        .child(err.clone()),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .when(can_add_more, |el| {
                            el.child(
                                Button::new("btn_add_more_error", "Agregar otro archivo JSON…")
                                    .primary()
                                    .size(ControlSize::Sm)
                                    .icon(IconName::FolderPlus)
                                    .on_click(cx.listener(|this, _, _, cx| this.pick_more_files(cx))),
                            )
                        })
                        .child(
                            Button::new("btn_clear_error", "Limpiar")
                                .size(ControlSize::Sm)
                                .variant(ButtonVariant::Outline)
                                .icon(IconName::RotateCcw)
                                .on_click(cx.listener(|this, _, _, cx| this.clear_files(cx))),
                        ),
                )
                .when(is_waiting_for_second, |el| {
                    el.child(
                        div()
                            .text_size(theme.text_size(TextSize::Xs))
                            .text_color(colors.fg_muted)
                            .child("Pista: También puedes arrastrar el segundo archivo directamente a esta ventana."),
                    )
                })
                .into_any_element()
        } else if self.comparison.is_some() {
            match self.view_mode {
                ViewMode::Table => {
                    components::table_view::render_table_view(self, view.clone(), cx)
                        .into_any_element()
                }
                ViewMode::GitDiff => {
                    components::git_view::render_git_view(self, view.clone(), cx).into_any_element()
                }
                ViewMode::Editor => {
                    components::editor_view::render_editor_view(self, view.clone(), cx)
                        .into_any_element()
                }
            }
        } else {
            components::empty_view::render_empty_view(cx).into_any_element()
        };

        let cmd_palette = if self.show_command_palette {
            Some(
                components::command_palette::render_command_palette(view.clone())
                    .into_any_element(),
            )
        } else {
            None
        };

        let settings_modal = if self.show_settings {
            Some(
                components::settings_dialog::render_settings_dialog(self, view.clone(), cx)
                    .into_any_element(),
            )
        } else {
            None
        };

        let project_bar = components::project_bar::render_project_bar(self, view.clone(), cx);
        let rename_modal = if self.renaming_project_id.is_some() {
            Some(
                components::project_bar::render_rename_dialog(self, view.clone(), cx)
                    .into_any_element(),
            )
        } else {
            None
        };

        let schema_modal = if self.show_schema_dialog {
            Some(
                components::schema_dialog::render_schema_dialog(self, view.clone(), cx)
                    .into_any_element(),
            )
        } else {
            None
        };

        let theme = cx.theme();
        let colors = &theme.colors;

        div()
            .id("root")
            .flex()
            .flex_col()
            .size_full()
            .bg(colors.bg)
            .text_color(colors.fg)
            .text_sm()
            .font_family("monospace")
            .track_focus(&self.focus_handle)
            .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if shortcuts::handle_global_key(this, event, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if shortcuts::handle_global_key(this, event, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                let dropped: Vec<PathBuf> = paths.paths().to_vec();
                if this.paths.is_empty() {
                    this.set_paths(dropped, cx);
                } else {
                    this.add_paths(dropped, cx);
                }
            }))
            .child(project_bar)
            .child(toolbar)
            .child(body)
            .children(cmd_palette)
            .children(settings_modal)
            .children(rename_modal)
            .children(schema_modal)
    }
}
