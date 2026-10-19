//! Visor de diferencias estilo Git (DiffViewer) integrado en la aplicación.

use crate::gui::DiffView;
use ely_gpui_component::{
    buttons::{Button, ButtonVariant},
    data_display::{Badge, Tone},
    git::DiffViewer,
    primitives::{Icon, IconName},
    theme::{ActiveTheme, ControlSize, IconSize, TextSize},
};
use gpui::{div, prelude::*, Context, Entity, IntoElement};

/// Renderiza el visor de diferencias estilo Git comparando el archivo base contra un archivo objetivo.
pub fn render_git_view(
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
    let target_idx = if state.git_target_idx < c.files.len() && state.git_target_idx != base_idx {
        state.git_target_idx
    } else {
        (0..c.files.len()).find(|&i| i != base_idx).unwrap_or(0)
    };

    let base_file = &c.files[base_idx];
    let target_file = &c.files[target_idx];

    // Sub-barra informativa y selectores de destino
    let git_subbar = div()
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
                .gap_2()
                .child(
                    Badge::new(format!("Base: [{}] {}", base_idx + 1, base_file.name))
                        .tone(Tone::Accent)
                        .dot(),
                )
                .child(
                    Icon::new(IconName::GitBranch)
                        .size(IconSize::Sm)
                        .color(colors.fg_muted),
                )
                .child(
                    Badge::new(format!(
                        "Objetivo: [{}] {}",
                        target_idx + 1,
                        target_file.name
                    ))
                    .tone(Tone::Info)
                    .dot(),
                ),
        )
        .when(c.files.len() > 2, |bar| {
            let mut group = div().flex().flex_row().items_center().gap_1();
            for (i, f) in c.files.iter().enumerate() {
                if i == base_idx {
                    continue;
                }
                let is_sel = i == target_idx;
                let view = view.clone();
                let label = format!("[{}] {}", i + 1, f.name);
                group = group.child(
                    Button::new(("sel_target", i), label)
                        .size(ControlSize::Sm)
                        .variant(if is_sel {
                            ButtonVariant::Primary
                        } else {
                            ButtonVariant::Ghost
                        })
                        .on_click(move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                this.git_target_idx = i;
                                this.git_open_stretches.clear();
                                cx.notify();
                            });
                        }),
                );
            }
            bar.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_size(theme.text_size(TextSize::Xs))
                            .text_color(colors.fg_muted)
                            .child("Comparar base contra:"),
                    )
                    .child(group),
            )
        });

    let viewer = DiffViewer::new(
        ("git-diff-viewer", target_idx),
        format!("{}.json", target_file.name),
        &base_file.content,
        &target_file.content,
    )
    .layout(state.git_layout)
    .open(state.git_open_stretches.clone())
    .on_open({
        let view = view.clone();
        move |stretch, _, cx| {
            view.update(cx, |this, cx| {
                if !this.git_open_stretches.contains(&stretch) {
                    this.git_open_stretches.push(stretch);
                    cx.notify();
                }
            });
        }
    })
    .on_layout({
        let view = view.clone();
        move |layout, _, cx| {
            view.update(cx, |this, cx| {
                this.git_layout = layout;
                cx.notify();
            });
        }
    });

    div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .size_full()
        .child(git_subbar)
        .child(div().flex_1().min_h_0().size_full().child(viewer))
        .into_any_element()
}
