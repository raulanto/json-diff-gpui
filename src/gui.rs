//! Interfaz GPUI moderna con Ely Components: tabla comparadora de 2 a 6 archivos JSON,
//! visor de diferencias estilo Git (DiffViewer) y Editor interactivo (CodeEditor).
//!
//! Permite:
//! - Vista Tabla (Matriz): filtrar solo cambios o ver todos los campos, búsqueda en tiempo real,
//!   copia al portapapeles y exportación a Markdown.
//! - Vista Diff Git: comparación estilo Git unificada (Unified) o dividida (Split side-by-side)
//!   con plegado de contexto, resaltado de palabras y estadísticas de cambios (+/-).
//! - Vista Editor: edición directa en vivo del JSON con resaltado de sintaxis, números de línea,
//!   marcas en el gutter respecto a la base, formateo automático (pretty-print), validación sintáctica,
//!   aplicación de cambios al diff en memoria y guardado en disco.
//! - Alternar entre tema claro y tema oscuro (Ely Themes).
//! - Mantener alto rendimiento virtualizado mediante `uniform_list`.

use ely_gpui_component::{
    buttons::{Button, ButtonVariant, IconButton, SegmentedControl},
    data_display::{Badge, Tone},
    editor::{CodeEditor, GitMark, LineNumbers},
    forms::{Input, InputEvent, TextInput},
    git::{diff, DiffLayout, DiffViewer, LineKind, Stretch},
    primitives::{Icon, IconName, Tooltip},
    theme::{ActiveTheme, ControlSize, Elevation, IconSize, Mode, Radius, TextSize, Theme},
};
use gpui::{
    div, prelude::*, px, size, uniform_list, App, Bounds, ClipboardItem, Context, Entity,
    ExternalPaths, FontWeight, PathPromptOptions, SharedString, Subscription, TitlebarOptions,
    Window, WindowBounds, WindowOptions,
};
use jsondiff::{
    compare_paths_with_base, compare_with_base, export_markdown, format_json, load_file_from_str,
    matches_filter, validate_json, Comparison, MAX_FILES, MIN_FILES,
};
use std::ops::Range;
use std::path::PathBuf;

const ROW_H: f32 = 34.0;
const PATH_W: f32 = 320.0;

/// Modos de visualización integrados.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewMode {
    Table,
    GitDiff,
    Editor,
}

pub struct DiffView {
    paths: Vec<PathBuf>,
    comparison: Option<Comparison>,
    error: Option<SharedString>,
    view_mode: ViewMode,

    // Estado para vista Tabla
    only_changed: bool,
    search_query: String,
    search_input: Entity<TextInput>,
    _search_sub: Subscription,
    visible: Vec<usize>,

    // Estado para vista Diff Git
    git_target_idx: usize,
    git_layout: DiffLayout,
    git_open_stretches: Vec<usize>,

    // Estado para vista Editor
    editor_file_idx: usize,
    editor: Option<Entity<CodeEditor>>,

    // Notificaciones / Toast
    status_toast: Option<SharedString>,
}

/// Calcula marcas en el gutter (líneas añadidas) para el editor comparando contra el archivo base.
fn compute_git_marks(base: &str, current: &str) -> Vec<(usize, GitMark)> {
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
            view_mode: ViewMode::Table,
            only_changed: true,
            search_query: String::new(),
            search_input,
            _search_sub: search_sub,
            visible: Vec::new(),
            git_target_idx: 1,
            git_layout: DiffLayout::Unified,
            git_open_stretches: Vec::new(),
            editor_file_idx: 0,
            editor: None,
            status_toast: None,
        };
        v.reload();
        v
    }

    fn reload(&mut self) {
        self.comparison = None;
        self.error = None;
        self.visible.clear();
        self.editor = None;
        self.git_open_stretches.clear();

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
                if self.git_target_idx >= self.paths.len() {
                    self.git_target_idx = 1;
                }
                if self.editor_file_idx >= self.paths.len() {
                    self.editor_file_idx = 0;
                }
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
                .timer(std::time::Duration::from_millis(2400))
                .await;
            this.update(cx, |this, cx| {
                this.status_toast = None;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Inicializa o actualiza el editor para el archivo seleccionado.
    fn init_editor_if_needed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(c) = &self.comparison else {
            self.editor = None;
            return;
        };
        if self.editor_file_idx >= c.files.len() {
            self.editor_file_idx = 0;
        }
        let current_file = &c.files[self.editor_file_idx];
        let content = current_file.content.clone();
        let base_idx = c.base_index;
        let base_content = c
            .files
            .get(base_idx)
            .map(|f| f.content.clone())
            .unwrap_or_default();
        let is_not_base = self.editor_file_idx != base_idx;

        let editor = cx.new(|cx| {
            let mut ed = CodeEditor::new(content.clone(), window, cx)
                .language("json")
                .line_numbers(LineNumbers::Absolute)
                .rainbow_brackets();
            if is_not_base && !base_content.is_empty() {
                let marks = compute_git_marks(&base_content, &content);
                ed.set_git_marks(marks, cx);
            }
            ed
        });
        self.editor = Some(editor);
    }

    /// Aplica el formateo de 2 espacios al JSON del editor.
    fn format_editor_json(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = &self.editor else { return };
        let text = editor.read(cx).text().to_string();
        match format_json(&text) {
            Ok(formatted) => {
                editor.update(cx, |ed, cx| {
                    ed.set_text(formatted, cx);
                });
                self.show_toast("JSON formateado correctamente (2 espacios)", cx);
            }
            Err(e) => {
                self.show_toast(format!("Error al formatear: {e}"), cx);
            }
        }
    }

    /// Valida la sintaxis JSON del texto actual del editor.
    fn validate_editor_json(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = &self.editor else { return };
        let text = editor.read(cx).text().to_string();
        match validate_json(&text) {
            Ok(()) => {
                self.show_toast("Sintaxis JSON válida ✓", cx);
            }
            Err(e) => {
                self.show_toast(format!("Error sintáctico: {e}"), cx);
            }
        }
    }

    /// Aplica el texto del editor a la comparación en memoria y refresca todas las vistas.
    fn apply_editor_changes(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.editor.clone() else {
            return;
        };
        let text = editor.read(cx).text().to_string();
        let Some(c) = &self.comparison else { return };
        if self.editor_file_idx >= c.files.len() {
            return;
        }

        let file_path = c.files[self.editor_file_idx].path.clone();
        match load_file_from_str(&file_path, &text) {
            Ok(loaded) => {
                let mut new_files = c.files.clone();
                new_files[self.editor_file_idx] = loaded;
                let base_idx = c.base_index;
                match compare_with_base(new_files, base_idx) {
                    Ok(new_comp) => {
                        self.comparison = Some(new_comp);
                        self.refilter();
                        if self.editor_file_idx != base_idx {
                            let base_content =
                                &self.comparison.as_ref().unwrap().files[base_idx].content;
                            let marks = compute_git_marks(base_content, &text);
                            editor.update(cx, |ed, cx| {
                                ed.set_git_marks(marks, cx);
                            });
                        }
                        self.show_toast("Cambios aplicados al diff exitosamente", cx);
                        cx.notify();
                    }
                    Err(e) => self.show_toast(format!("Error al comparar: {e}"), cx),
                }
            }
            Err(e) => self.show_toast(format!("JSON inválido: {e}"), cx),
        }
    }

    /// Guarda el contenido actual del editor directamente en el archivo en disco.
    fn save_editor_to_disk(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.editor.clone() else {
            return;
        };
        let text = editor.read(cx).text().to_string();
        let Some(c) = &self.comparison else { return };
        if self.editor_file_idx >= c.files.len() {
            return;
        }

        let current_file = &c.files[self.editor_file_idx];
        let path = current_file.path.clone();
        let name = current_file.name.clone();

        if let Err(e) = std::fs::write(&path, &text) {
            self.show_toast(format!("No se pudo guardar: {e}"), cx);
            return;
        }
        self.apply_editor_changes(cx);
        self.show_toast(format!("Guardado exitosamente en {}", name), cx);
    }
}

impl gpui::Render for DiffView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Si estamos en modo editor y el editor no existe, inicializarlo
        if self.view_mode == ViewMode::Editor && self.editor.is_none() {
            self.init_editor_if_needed(window, cx);
        }

        let theme = cx.theme();
        let colors = &theme.colors;
        let view = cx.entity().clone();

        // ------------------------------------------------ Toolbar superior principal
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
            // Lado izquierdo: Abrir archivos y selector de vistas
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
                    .when(self.comparison.is_some(), |bar| {
                        let view = view.clone();
                        bar.child(
                            SegmentedControl::new(
                                "view_mode",
                                match self.view_mode {
                                    ViewMode::Table => "table",
                                    ViewMode::GitDiff => "git",
                                    ViewMode::Editor => "editor",
                                },
                            )
                            .size(ControlSize::Sm)
                            .segment("table", "Tabla", Some(IconName::Table))
                            .segment("git", "Diff Git", Some(IconName::GitBranch))
                            .segment("editor", "Editor", Some(IconName::Code))
                            .on_change(move |value, _, cx| {
                                let mode = match value.as_ref() {
                                    "git" => ViewMode::GitDiff,
                                    "editor" => ViewMode::Editor,
                                    _ => ViewMode::Table,
                                };
                                view.update(cx, |this, cx| {
                                    this.view_mode = mode;
                                    this.editor = None;
                                    cx.notify();
                                });
                            }),
                        )
                    }),
            )
            // Centro: controles dependientes del modo actual
            .child(div().flex().flex_row().items_center().gap_3().when(
                self.view_mode == ViewMode::Table && self.comparison.is_some(),
                |bar| {
                    let view = view.clone();
                    bar.child(
                        SegmentedControl::new(
                            "filter_mode",
                            if self.only_changed { "changed" } else { "all" },
                        )
                        .size(ControlSize::Sm)
                        .segment("changed", "Solo cambios", Some(IconName::Filter))
                        .segment("all", "Todos", Some(IconName::ListChecks))
                        .on_change(move |value, _, cx| {
                            let is_changed = value.as_ref() == "changed";
                            view.update(cx, |this, cx| {
                                this.only_changed = is_changed;
                                this.refilter();
                                cx.notify();
                            });
                        }),
                    )
                    .child(
                        div().w(px(240.0)).child(
                            Input::new(&self.search_input)
                                .size(ControlSize::Sm)
                                .prefix(
                                    Icon::new(IconName::Search)
                                        .size(IconSize::Sm)
                                        .color(colors.fg_subtle),
                                )
                                .clearable(),
                        ),
                    )
                },
            ))
            // Lado derecho: Notificaciones, badges de estado, exportación y tema
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .when_some(self.status_toast.clone(), |el, toast| {
                        el.child(Badge::new(toast).tone(Tone::Success).dot())
                    })
                    .when(
                        self.view_mode == ViewMode::Table && self.comparison.is_some(),
                        |el| {
                            let c = self.comparison.as_ref().unwrap();
                            let diff_count = c.changed_count();
                            let total = c.rows.len();
                            let view = view.clone();
                            el.child(
                                Badge::new(format!("{diff_count} de {total} con diferencias"))
                                    .tone(if diff_count > 0 {
                                        Tone::Warning
                                    } else {
                                        Tone::Success
                                    })
                                    .dot(),
                            )
                            .child(
                                Button::new("export_md", "Copiar Markdown")
                                    .size(ControlSize::Sm)
                                    .variant(ButtonVariant::Outline)
                                    .icon(IconName::Copy)
                                    .on_click(move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            if let Some(c) = &this.comparison {
                                                let md = export_markdown(c, this.only_changed);
                                                cx.write_to_clipboard(ClipboardItem::new_string(
                                                    md,
                                                ));
                                                this.show_toast(
                                                    "Reporte Markdown copiado al portapapeles",
                                                    cx,
                                                );
                                            }
                                        });
                                    }),
                            )
                        },
                    )
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

        // ------------------------------------------------ Cuerpo según estado y vista
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
            match self.view_mode {
                // ========================================== VISTA 1: TABLA COMPARATIVA
                ViewMode::Table => {
                    let base_ix = c.base_index;

                    // Encabezado de columnas
                    let mut header = div()
                        .flex()
                        .flex_row()
                        .w_full()
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
                                                            format!(
                                                                "Archivo [{}] ahora es la base",
                                                                i + 1
                                                            ),
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
                    let visible_indices = self.visible.clone();
                    let list_indices = visible_indices.clone();
                    let rows_data = c.rows.clone();
                    let files_count = c.files.len();
                    let list_view = view.clone();

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
                                        .h(px(ROW_H))
                                        .items_center()
                                        .border_b_1()
                                        .border_color(colors.border.opacity(0.5))
                                        .bg(if is_diff {
                                            colors.warning.opacity(0.06)
                                        } else {
                                            colors.surface
                                        })
                                        .hover(|s| s.bg(colors.hover));

                                    // Columna de ruta
                                    let path_str = row.path.clone();
                                    let copy_path = path_str.clone();
                                    let path_cell = div()
                                        .id(("path_cell", row_idx))
                                        .w(px(PATH_W))
                                        .px_3()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap_2()
                                        .cursor_pointer()
                                        .tooltip(Tooltip::text("Clic para copiar ruta"))
                                        .on_click({
                                            let view = list_view.clone();
                                            move |_, _, cx| {
                                                cx.write_to_clipboard(ClipboardItem::new_string(
                                                    copy_path.clone(),
                                                ));
                                                view.update(cx, |this, cx| {
                                                    this.show_toast(
                                                        format!("Ruta copiada: {}", copy_path),
                                                        cx,
                                                    );
                                                });
                                            }
                                        })
                                        .child(div().w(px(8.0)).h(px(8.0)).rounded_full().bg(
                                            if is_diff {
                                                colors.warning
                                            } else {
                                                colors.success
                                            },
                                        ))
                                        .child(
                                            div()
                                                .flex_1()
                                                .overflow_hidden()
                                                .whitespace_nowrap()
                                                .text_ellipsis()
                                                .font_weight(if is_diff {
                                                    FontWeight::BOLD
                                                } else {
                                                    FontWeight::NORMAL
                                                })
                                                .text_color(if is_diff {
                                                    colors.fg
                                                } else {
                                                    colors.fg_muted
                                                })
                                                .child(path_str),
                                        );

                                    row_el = row_el.child(path_cell);

                                    // Columnas de valores
                                    for file_col in 0..files_count {
                                        let cell = row.cells.get(file_col).and_then(|c| c.as_ref());
                                        let cell_el = if let Some(c) = cell {
                                            let val_str = c.value.clone();
                                            let copy_val = val_str.clone();
                                            let line_info = format!("L{}", c.line);

                                            div()
                                                .id(("val_cell", row_idx * 10 + file_col))
                                                .flex_1()
                                                .min_w_0()
                                                .px_3()
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .gap_2()
                                                .cursor_pointer()
                                                .tooltip(Tooltip::text("Clic para copiar valor"))
                                                .on_click({
                                                    let view = list_view.clone();
                                                    move |_, _, cx| {
                                                        cx.write_to_clipboard(
                                                            ClipboardItem::new_string(
                                                                copy_val.clone(),
                                                            ),
                                                        );
                                                        view.update(cx, |this, cx| {
                                                            this.show_toast(
                                                                "Valor copiado al portapapeles",
                                                                cx,
                                                            );
                                                        });
                                                    }
                                                })
                                                .child(
                                                    div()
                                                        .text_size(theme.text_size(TextSize::Xs))
                                                        .text_color(colors.fg_subtle)
                                                        .child(line_info),
                                                )
                                                .child(
                                                    div()
                                                        .flex_1()
                                                        .overflow_hidden()
                                                        .whitespace_nowrap()
                                                        .text_ellipsis()
                                                        .text_color(if is_diff {
                                                            colors.fg
                                                        } else {
                                                            colors.fg_muted
                                                        })
                                                        .child(val_str),
                                                )
                                        } else {
                                            div()
                                                .id(("val_cell_empty", row_idx * 10 + file_col))
                                                .flex_1()
                                                .min_w_0()
                                                .px_3()
                                                .text_color(colors.fg_subtle)
                                                .child("∅")
                                        };

                                        row_el = row_el.child(cell_el);
                                    }

                                    row_el.into_any_element()
                                })
                                .collect()
                        },
                    )
                    .size_full();

                    let body_content = if visible_indices.is_empty() {
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
                                            .child(if self.only_changed {
                                                if c.changed_count() == 0 {
                                                    "¡Los archivos son idénticos! No hay diferencias."
                                                } else {
                                                    "No hay diferencias que coincidan con la búsqueda."
                                                }
                                            } else {
                                                "No se encontraron campos que coincidan con la búsqueda."
                                            }),
                                    )
                                    .when(self.only_changed, |el| {
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

                // ========================================== VISTA 2: DIFF ESTILO GIT
                ViewMode::GitDiff => {
                    let base_idx = c.base_index;
                    let target_idx =
                        if self.git_target_idx < c.files.len() && self.git_target_idx != base_idx {
                            self.git_target_idx
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
                                    Badge::new(format!(
                                        "Base: [{}] {}",
                                        base_idx + 1,
                                        base_file.name
                                    ))
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
                    .layout(self.git_layout)
                    .open(self.git_open_stretches.clone())
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

                // ========================================== VISTA 3: EDITOR DE CÓDIGO
                ViewMode::Editor => {
                    let base_idx = c.base_index;
                    let current_file_idx = self.editor_file_idx.min(c.files.len() - 1);
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
                                    Button::new("fmt_json", "Formatear")
                                        .size(ControlSize::Sm)
                                        .variant(ButtonVariant::Ghost)
                                        .icon(IconName::Sparkles)
                                        .on_click({
                                            let view = view.clone();
                                            move |_, _, cx| {
                                                view.update(cx, |this, cx| {
                                                    this.format_editor_json(cx)
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
                                                    this.validate_editor_json(cx)
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
                                                    this.apply_editor_changes(cx)
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
                                                    this.save_editor_to_disk(cx)
                                                });
                                            }
                                        }),
                                ),
                        );

                    let editor_element = if let Some(editor) = &self.editor {
                        div().flex_1().min_h_0().size_full().child(editor.clone())
                    } else {
                        div().flex_1().min_h_0().size_full()
                    };

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
            }
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
                                .child("Comparador y Editor de JSON"),
                        )
                        .child(
                            div()
                                .text_size(theme.text_size(TextSize::Sm))
                                .text_color(colors.fg_muted)
                                .text_center()
                                .child(
                                    "Arrastra de 2 a 6 archivos JSON a esta ventana o utiliza el botón para seleccionarlos del disco.\n\
                                     Incluye vista matricial, comparador de cambios estilo Git y editor de código en vivo.",
                                ),
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
                        title: Some("JSON Diff & Editor".into()),
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
