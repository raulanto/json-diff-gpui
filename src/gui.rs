//! Interfaz GPUI moderna con Ely Components: tabla comparadora de 2 a 6 archivos JSON.
//!
//! Permite:
//! - Filtrar solo cambios o ver todos los campos.
//! - Búsqueda en tiempo real por ruta de campo o por valor.
//! - Cambiar el archivo base de comparación dinámicamente con un clic.
//! - Copiar rutas de campo o valores individuales al portapapeles.
//! - Exportar el reporte a Markdown.
//! - Alternar entre tema claro y tema oscuro (Ely Themes).
//! - Mantener alto rendimiento virtualizado mediante `uniform_list`.

use ely_gpui_component::{
    buttons::{Button, ButtonVariant, IconButton, SegmentedControl},
    data_display::{Badge, Tone},
    forms::{Input, InputEvent, TextInput},
    primitives::{Icon, IconName, Tooltip},
    theme::{ActiveTheme, ControlSize, Elevation, IconSize, Mode, Radius, TextSize, Theme},
};
use gpui::{
    div, prelude::*, px, size, uniform_list, App, Bounds, ClipboardItem, Context, Entity,
    ExternalPaths, FontWeight, PathPromptOptions, SharedString, Subscription, TitlebarOptions,
    Window, WindowBounds, WindowOptions,
};
use jsondiff::{
    compare_paths_with_base, export_markdown, matches_filter, Comparison, MAX_FILES, MIN_FILES,
};
use std::ops::Range;
use std::path::PathBuf;

const ROW_H: f32 = 34.0;
const PATH_W: f32 = 320.0;

pub struct DiffView {
    paths: Vec<PathBuf>,
    comparison: Option<Comparison>,
    error: Option<SharedString>,
    only_changed: bool,
    search_query: String,
    search_input: Entity<TextInput>,
    _search_sub: Subscription,
    /// Índices de `comparison.rows` visibles con el filtro actual.
    visible: Vec<usize>,
    status_toast: Option<SharedString>,
}

impl DiffView {
    pub fn new(paths: Vec<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search_input =
            cx.new(|cx| TextInput::new(window, cx).placeholder("Buscar por campo o valor…"));

        let search_sub = cx.subscribe(&search_input, |this, entity, event: &InputEvent, cx| {
            if let InputEvent::Changed = event {
                let query = entity.read(cx).text().to_string();
                this.search_query = query;
                this.refilter();
                cx.notify();
            }
        });

        let mut v = Self {
            paths,
            comparison: None,
            error: None,
            only_changed: true,
            search_query: String::new(),
            search_input,
            _search_sub: search_sub,
            visible: Vec::new(),
            status_toast: None,
        };
        v.reload();
        v
    }

    fn reload(&mut self) {
        self.comparison = None;
        self.error = None;
        self.visible.clear();
        if self.paths.is_empty() {
            return;
        }
        if self.paths.len() < MIN_FILES || self.paths.len() > MAX_FILES {
            self.error = Some(
                format!(
                    "Elige entre {MIN_FILES} y {MAX_FILES} archivos (elegidos: {}).",
                    self.paths.len()
                )
                .into(),
            );
            return;
        }
        match compare_paths_with_base(&self.paths, 0) {
            Ok(c) => {
                self.comparison = Some(c);
                self.refilter();
            }
            Err(e) => self.error = Some(e.into()),
        }
    }

    fn refilter(&mut self) {
        let only = self.only_changed;
        let query = self.search_query.clone();
        self.visible = self
            .comparison
            .as_ref()
            .map(|c| {
                c.rows
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| (!only || r.changed) && matches_filter(r, &query))
                    .map(|(i, _)| i)
                    .collect()
            })
            .unwrap_or_default();
    }

    fn set_paths(&mut self, mut paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        paths.truncate(MAX_FILES);
        self.paths = paths;
        self.reload();
        cx.notify();
    }

    fn pick_files(&mut self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Elige de 2 a 6 archivos JSON".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                this.update(cx, |this, cx| this.set_paths(paths, cx)).ok();
            }
        })
        .detach();
    }

    fn show_toast(&mut self, msg: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.status_toast = Some(msg.into());
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(2200))
                .await;
            this.update(cx, |this, cx| {
                this.status_toast = None;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

impl gpui::Render for DiffView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let colors = &theme.colors;
        let view = cx.entity().clone();

        // ------------------------------------------------ Toolbar superior
        let toolbar = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap_3()
            .p_3()
            .bg(colors.surface)
            .border_b_1()
            .border_color(colors.border)
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
                    .child(
                        SegmentedControl::new(
                            "filter_mode",
                            if self.only_changed { "changed" } else { "all" },
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
                        div().w(px(260.0)).child(
                            Input::new(&self.search_input)
                                .size(ControlSize::Sm)
                                .prefix(
                                    Icon::new(IconName::Search)
                                        .size(IconSize::Sm)
                                        .color(colors.fg_subtle),
                                )
                                .clearable(),
                        ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .when_some(self.status_toast.clone(), |el, toast| {
                        el.child(Badge::new(toast).tone(Tone::Success).dot())
                    })
                    .when_some(self.comparison.as_ref(), |el, c| {
                        let diff_count = c.changed_count();
                        let total = c.rows.len();
                        el.child(
                            Badge::new(format!("{diff_count} de {total} con diferencias"))
                                .tone(if diff_count > 0 {
                                    Tone::Warning
                                } else {
                                    Tone::Success
                                })
                                .dot(),
                        )
                    })
                    .when(!self.search_query.trim().is_empty(), |el| {
                        el.child(
                            Badge::new(format!("{} encontrados", self.visible.len()))
                                .tone(Tone::Info),
                        )
                    })
                    .when(self.comparison.is_some(), |el| {
                        let view = view.clone();
                        el.child(
                            Button::new("export_md", "Copiar Markdown")
                                .size(ControlSize::Sm)
                                .variant(ButtonVariant::Outline)
                                .icon(IconName::Copy)
                                .on_click(move |_, _, cx| {
                                    view.update(cx, |this, cx| {
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
                    })
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
                            "Modo claro"
                        } else {
                            "Modo oscuro"
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
            );

        // ------------------------------------------------ Cuerpo / Contenido
        let body: gpui::AnyElement = if let Some(err) = &self.error {
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
                        .rounded(theme.radius(Radius::Md))
                        .bg(colors.danger.opacity(0.12))
                        .border_1()
                        .border_color(colors.danger)
                        .text_color(colors.danger)
                        .child(err.clone()),
                )
                .into_any_element()
        } else if let Some(c) = &self.comparison {
            let base_ix = c.base_index;

            // Encabezado de columnas
            let mut header = div()
                .flex()
                .flex_row()
                .h(px(ROW_H + 8.0))
                .items_center()
                .bg(colors.sunken)
                .border_b_1()
                .border_color(colors.border)
                .text_color(colors.fg_muted)
                .child(
                    div()
                        .w(px(PATH_W))
                        .px_3()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Campo / Ruta JSON"),
                );

            for (i, f) in c.files.iter().enumerate() {
                let is_base = i == base_ix;
                let label = format!(
                    "[{}] {}{}",
                    i + 1,
                    f.name,
                    if f.lines_exact { "" } else { " *" }
                );
                let full_path = f.path.display().to_string();

                let col_header = div()
                    .flex_1()
                    .min_w_0()
                    .px_3()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .id(("col_title", i))
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .font_weight(if is_base {
                                FontWeight::BOLD
                            } else {
                                FontWeight::MEDIUM
                            })
                            .text_color(if is_base { colors.accent } else { colors.fg })
                            .tooltip(Tooltip::text(full_path))
                            .child(label),
                    )
                    .child(if is_base {
                        div().child(Badge::new("Base").tone(Tone::Accent).dot())
                    } else {
                        let view = view.clone();
                        div().child(
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
                    });

                header = header.child(col_header);
            }

            // Filas virtualizadas mediante uniform_list
            let list = uniform_list(
                "rows",
                self.visible.len(),
                cx.processor(move |this, range: Range<usize>, _window, cx| {
                    let Some(c) = this.comparison.as_ref() else {
                        return Vec::new();
                    };
                    let base_ix = c.base_index;
                    let theme = cx.theme();
                    let colors = &theme.colors;

                    range
                        .map(|ix| {
                            let row = &c.rows[this.visible[ix]];
                            let path_str = row.path.clone();

                            let mut el = div()
                                .id(("row", ix))
                                .flex()
                                .flex_row()
                                .items_center()
                                .h(px(ROW_H))
                                .border_b_1()
                                .border_color(colors.border)
                                .when(row.changed, |d| d.bg(colors.warning.opacity(0.08)))
                                .child(
                                    div()
                                        .w(px(PATH_W))
                                        .px_3()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .gap_2()
                                        .child(
                                            div()
                                                .id(("path_txt", ix))
                                                .flex_1()
                                                .min_w_0()
                                                .overflow_hidden()
                                                .whitespace_nowrap()
                                                .text_ellipsis()
                                                .font_weight(if row.changed {
                                                    FontWeight::SEMIBOLD
                                                } else {
                                                    FontWeight::NORMAL
                                                })
                                                .text_color(if row.changed {
                                                    colors.accent
                                                } else {
                                                    colors.fg
                                                })
                                                .tooltip(Tooltip::text(format!(
                                                    "{}\n(Clic en el icono para copiar)",
                                                    path_str
                                                )))
                                                .child(path_str.clone()),
                                        )
                                        .child(
                                            div()
                                                .id(("copy_p", ix))
                                                .flex_none()
                                                .cursor_pointer()
                                                .opacity(0.4)
                                                .hover(|s| s.opacity(1.0).text_color(colors.accent))
                                                .tooltip(Tooltip::text(format!(
                                                    "Copiar ruta: {}",
                                                    path_str
                                                )))
                                                .on_click(cx.listener({
                                                    let path_str = path_str.clone();
                                                    move |this, _, _, cx| {
                                                        cx.write_to_clipboard(
                                                            ClipboardItem::new_string(
                                                                path_str.clone(),
                                                            ),
                                                        );
                                                        this.show_toast(
                                                            format!("Ruta copiada: {}", path_str),
                                                            cx,
                                                        );
                                                    }
                                                }))
                                                .child(
                                                    Icon::new(IconName::Copy)
                                                        .size(IconSize::Xs)
                                                        .color(colors.fg_muted),
                                                ),
                                        ),
                                );

                            let base_val = row.cells[base_ix].as_ref().map(|c| &c.value);

                            for (i, cell) in row.cells.iter().enumerate() {
                                let differs = row.changed
                                    && i != base_ix
                                    && cell.as_ref().map(|c| &c.value) != base_val;

                                let cell_id = format!("cell_{ix}_{i}");
                                let cell_el = match cell {
                                    Some(c) => {
                                        let cell_val = c.value.clone();
                                        div()
                                            .id(cell_id)
                                            .flex_1()
                                            .min_w_0()
                                            .h_full()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .px_3()
                                            .cursor_pointer()
                                            .hover(|s| s.bg(colors.hover))
                                            .when(differs, |d| {
                                                d.bg(colors.warning.opacity(0.18))
                                                    .border_l_2()
                                                    .border_color(colors.warning)
                                            })
                                            .tooltip(Tooltip::text(format!(
                                                "L{}  {}\n(Clic para copiar valor)",
                                                c.line, cell_val
                                            )))
                                            .on_click(cx.listener({
                                                let cell_val = cell_val.clone();
                                                move |this, _, _, cx| {
                                                    cx.write_to_clipboard(
                                                        ClipboardItem::new_string(cell_val.clone()),
                                                    );
                                                    this.show_toast(
                                                        format!("Valor copiado: {}", cell_val),
                                                        cx,
                                                    );
                                                }
                                            }))
                                            .child(
                                                div()
                                                    .flex_none()
                                                    .px_1p5()
                                                    .py_0p5()
                                                    .rounded_sm()
                                                    .bg(colors.sunken)
                                                    .text_color(colors.fg_muted)
                                                    .text_size(theme.text_size(TextSize::Xs))
                                                    .child(format!("L{}", c.line)),
                                            )
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .overflow_hidden()
                                                    .whitespace_nowrap()
                                                    .text_ellipsis()
                                                    .text_color(if differs {
                                                        colors.fg
                                                    } else {
                                                        colors.fg_muted
                                                    })
                                                    .child(cell_val),
                                            )
                                    }
                                    None => div()
                                        .id(cell_id)
                                        .flex_1()
                                        .min_w_0()
                                        .h_full()
                                        .flex()
                                        .items_center()
                                        .px_3()
                                        .bg(colors.danger.opacity(0.08))
                                        .child(Badge::new("no existe").tone(Tone::Danger)),
                                };

                                el = el.child(cell_el);
                            }
                            el
                        })
                        .collect()
                }),
            )
            .flex_1()
            .size_full();

            let mut col = div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .child(header)
                .child(list);

            if c.files.iter().any(|f| !f.lines_exact) {
                col = col.child(
                    div()
                        .p_2()
                        .text_color(colors.fg_muted)
                        .child("* El archivo no está en formato de 2 espacios; las líneas corresponden al formato normalizado."),
                );
            }
            col.into_any_element()
        } else {
            // Estado vacío / DropZone
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
                                .child("Comparador de JSON"),
                        )
                        .child(
                            div()
                                .text_size(theme.text_size(TextSize::Sm))
                                .text_color(colors.fg_muted)
                                .text_center()
                                .child("Arrastra de 2 a 6 archivos JSON a esta ventana o utiliza el botón para seleccionarlos del disco."),
                        )
                        .child(
                            Button::new("open_empty", "Abrir archivos…")
                                .primary()
                                .icon(IconName::FolderOpen)
                                .on_click(cx.listener(|this, _, _, cx| this.pick_files(cx))),
                        ),
                )
                .into_any_element()
        };

        div()
            .id("root")
            .flex()
            .flex_col()
            .size_full()
            .bg(colors.bg)
            .text_color(colors.fg)
            .text_sm()
            .font_family("monospace")
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                this.set_paths(paths.paths().to_vec(), cx);
            }))
            .child(toolbar)
            .child(body)
    }
}

pub fn run(paths: Vec<PathBuf>) {
    gpui_platform::application()
        .with_assets(ely_gpui_component::Assets)
        .run(move |cx: &mut App| {
            let _ = ely_gpui_component::init(cx);
            Theme::set_mode_now(Mode::Dark, cx);
            let bounds = Bounds::centered(None, size(px(1280.), px(800.)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("JSON Diff".into()),
                        ..Default::default()
                    }),
                    app_id: Some("json-diff".into()),
                    window_min_size: Some(size(px(700.), px(450.))),
                    ..Default::default()
                },
                |window, cx| cx.new(|cx| DiffView::new(paths, window, cx)),
            )
            .expect("no se pudo abrir la ventana");
            cx.activate(true);
        });
}
