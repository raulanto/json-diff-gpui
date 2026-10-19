//! Operaciones del editor en vivo, selectores JSONPath, densidad visual y notificaciones toast.

use crate::gui::components::editor_view::compute_git_marks;
use crate::gui::types::Density;
use crate::gui::DiffView;
use ely_gpui_component::editor::{find_all, CodeEditor, LineNumbers};
use ely_gpui_component::theme::ActiveTheme;
use gpui::{prelude::*, Context, Focusable, SharedString, Window};
use jsondiff::{
    compare_with_base, format_json, generate_schema_from_str, load_file_from_str, validate_json,
};
use std::time::Duration;

impl DiffView {
    /// Muestra un mensaje flotante (toast) con auto-ocultamiento.
    pub(crate) fn show_toast(&mut self, msg: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.status_toast = Some(msg.into());
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(2400))
                .await;
            this.update(cx, |this, cx| {
                self_status_toast_clear(this, cx);
            })
            .ok();
        })
        .detach();
    }

    /// Aplica una consulta de selector JSONPath (ej. "$.data").
    pub(crate) fn apply_selector(&mut self, text: &str, is_submit: bool, cx: &mut Context<Self>) {
        let trimmed = text.trim();
        let query = if trimmed.is_empty() { "$" } else { trimmed };

        if let Some(c) = &mut self.comparison {
            match c.set_selector(query) {
                Ok(_) => {
                    self.refilter();
                    if is_submit {
                        self.show_toast(
                            if query == "$" {
                                "Selector restaurado a la raíz ($)".to_string()
                            } else {
                                format!("Selector aplicado: {}", query)
                            },
                            cx,
                        );
                    }
                    cx.notify();
                }
                Err(err) => {
                    if is_submit {
                        self.show_toast(format!("Error en selector: {}", err), cx);
                    }
                }
            }
        }
    }

    /// Restaura el selector a la raíz global ($).
    pub(crate) fn reset_selector(&mut self, cx: &mut Context<Self>) {
        self.selector_input.update(cx, |input, cx| {
            input.set_text("$", cx);
        });
        self.apply_selector("$", true, cx);
    }

    /// Inicializa la instancia del CodeEditor si aún no ha sido creada para la pestaña activa.
    pub(crate) fn init_editor_if_needed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
                .rainbow_brackets()
                .minimap()
                .sticky_scroll();
            if is_not_base && !base_content.is_empty() {
                let marks = compute_git_marks(&base_content, &content);
                ed.set_git_marks(marks, cx);
            }
            ed
        });

        self.editor = Some(editor);
        self.update_schema_editor(window, cx);
    }

    /// Formatea el contenido del editor con 2 espacios e identación normalizada.
    pub(crate) fn format_editor_json(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = &self.editor else { return };
        let text = editor.read(cx).text().to_string();
        match format_json(&text) {
            Ok(formatted) => {
                editor.update(cx, |ed, cx| {
                    ed.set_text(formatted, cx);
                });
                self.sync_schema_if_present(cx);
                self.show_toast("JSON formateado correctamente (2 espacios)", cx);
            }
            Err(e) => {
                self.show_toast(format!("Error al formatear: {e}"), cx);
            }
        }
    }

    /// Valida la sintaxis del JSON en el editor sin aplicar cambios.
    pub(crate) fn validate_editor_json(&mut self, cx: &mut Context<Self>) {
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

    /// Aplica en caliente los cambios del editor a la comparación en memoria.
    pub(crate) fn apply_editor_changes(&mut self, cx: &mut Context<Self>) {
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
                let old_mode = c.diff_mode;
                let old_selector = c.selector.clone();
                match compare_with_base(new_files, base_idx) {
                    Ok(mut new_comp) => {
                        new_comp.set_diff_mode(old_mode);
                        let _ = new_comp.set_selector(&old_selector);
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
                        self.sync_schema_if_present(cx);
                        self.show_toast("Cambios aplicados al diff exitosamente", cx);
                        cx.notify();
                    }
                    Err(e) => self.show_toast(format!("Error al comparar: {e}"), cx),
                }
            }
            Err(e) => self.show_toast(format!("JSON inválido: {e}"), cx),
        }
    }

    /// Guarda el contenido actual del editor directamente en el disco.
    pub(crate) fn save_editor_to_disk(&mut self, cx: &mut Context<Self>) {
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
        self.show_toast(format!("Guardado: {name}"), cx);
    }

    /// Alterna la densidad de filas entre Compacta y Cómoda.
    pub fn toggle_density(&mut self, cx: &mut Context<Self>) {
        self.density = match self.density {
            Density::Compact => Density::Comfortable,
            Density::Comfortable => Density::Compact,
        };
        self.show_toast(
            match self.density {
                Density::Compact => "Densidad: Compacta (~40% más filas)",
                Density::Comfortable => "Densidad: Cómoda (espaciado generoso)",
            },
            cx,
        );
        cx.notify();
    }

    /// Abre y enfoca el widget de búsqueda en el editor.
    pub(crate) fn open_find_widget(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_find_widget = true;
        let handle = self.find_input.read(cx).focus_handle(cx);
        window.focus(&handle, cx);
        self.update_find_matches(cx);
        cx.notify();
    }

    /// Alterna la visibilidad del widget de búsqueda en el editor.
    pub(crate) fn toggle_find_widget(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.show_find_widget {
            self.close_find_widget(Some(window), cx);
        } else {
            self.open_find_widget(window, cx);
        }
    }

    /// Actualiza y resalta las coincidencias de búsqueda en el editor activo.
    pub(crate) fn update_find_matches(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = &self.editor else { return };
        let text = editor.read(cx).text().to_string();
        let query = self.find_input.read(cx).text().to_string();

        if query.is_empty() {
            self.find_matches.clear();
            self.find_current_index = None;
            editor.update(cx, |ed, cx| ed.set_backgrounds(Vec::new(), cx));
            cx.notify();
            return;
        }

        match find_all(&text, &query, self.find_options) {
            Ok(matches) => {
                let total = matches.len();
                self.find_matches = matches;
                if total > 0 {
                    if self.find_current_index.is_none()
                        || self.find_current_index.unwrap() >= total
                    {
                        self.find_current_index = Some(0);
                    }
                } else {
                    self.find_current_index = None;
                }

                let accent = cx.theme().colors.accent;
                let backgrounds: Vec<(std::ops::Range<usize>, gpui::Hsla)> = self
                    .find_matches
                    .iter()
                    .enumerate()
                    .map(|(i, range)| {
                        let is_current = self.find_current_index == Some(i);
                        let color = if is_current {
                            accent.opacity(0.65)
                        } else {
                            accent.opacity(0.25)
                        };
                        (range.clone(), color)
                    })
                    .collect();

                editor.update(cx, |ed, cx| ed.set_backgrounds(backgrounds, cx));
            }
            Err(_) => {
                self.find_matches.clear();
                self.find_current_index = None;
                editor.update(cx, |ed, cx| ed.set_backgrounds(Vec::new(), cx));
            }
        }
        cx.notify();
    }

    /// Avanza o retrocede entre las coincidencias de búsqueda.
    pub(crate) fn step_find_match(&mut self, forward: bool, cx: &mut Context<Self>) {
        let total = self.find_matches.len();
        if total == 0 {
            return;
        }

        let curr = self.find_current_index.unwrap_or(0);
        let next_idx = if forward {
            (curr + 1) % total
        } else {
            (curr + total - 1) % total
        };
        self.find_current_index = Some(next_idx);

        let Some(editor) = &self.editor else { return };
        let accent = cx.theme().colors.accent;
        let backgrounds: Vec<(std::ops::Range<usize>, gpui::Hsla)> = self
            .find_matches
            .iter()
            .enumerate()
            .map(|(i, range)| {
                let is_current = self.find_current_index == Some(i);
                let color = if is_current {
                    accent.opacity(0.65)
                } else {
                    accent.opacity(0.25)
                };
                (range.clone(), color)
            })
            .collect();

        editor.update(cx, |ed, cx| ed.set_backgrounds(backgrounds, cx));
        cx.notify();
    }

    /// Reemplaza la coincidencia actual o todas las coincidencias.
    pub(crate) fn replace_find_match(&mut self, all: bool, cx: &mut Context<Self>) {
        let Some(editor) = self.editor.clone() else {
            return;
        };
        let text = editor.read(cx).text().to_string();
        let query = self.find_input.read(cx).text().to_string();
        let replacement = self.replace_input.read(cx).text().to_string();

        if query.is_empty() || self.find_matches.is_empty() {
            return;
        }

        if all {
            let count = self.find_matches.len();
            let mut new_text = text;
            let mut rev_matches = self.find_matches.clone();
            rev_matches.sort_by(|a, b| b.start.cmp(&a.start));
            for range in rev_matches {
                if range.end <= new_text.len()
                    && new_text.is_char_boundary(range.start)
                    && new_text.is_char_boundary(range.end)
                {
                    new_text.replace_range(range, &replacement);
                }
            }
            editor.update(cx, |ed, cx| ed.set_text(new_text, cx));
            self.update_find_matches(cx);
            self.show_toast(format!("Reemplazadas {count} ocurrencias"), cx);
        } else if let Some(curr_idx) = self.find_current_index {
            if curr_idx < self.find_matches.len() {
                let range = self.find_matches[curr_idx].clone();
                let mut new_text = text;
                if range.end <= new_text.len()
                    && new_text.is_char_boundary(range.start)
                    && new_text.is_char_boundary(range.end)
                {
                    new_text.replace_range(range, &replacement);
                    editor.update(cx, |ed, cx| ed.set_text(new_text, cx));
                    self.update_find_matches(cx);
                    self.show_toast("Ocurrencia reemplazada", cx);
                }
            }
        }
    }

    /// Cierra el widget de búsqueda y limpia los resaltados en el editor, devolviendo opcionalmente el foco.
    pub(crate) fn close_find_widget(
        &mut self,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        self.show_find_widget = false;
        self.find_matches.clear();
        self.find_current_index = None;
        if let Some(editor) = &self.editor {
            editor.update(cx, |ed, cx| ed.set_backgrounds(Vec::new(), cx));
            if let Some(w) = window {
                let handle = editor.read(cx).focus_handle(cx);
                w.focus(&handle, cx);
            }
        }
        cx.notify();
    }

    /// Regenera e inicializa o actualiza el CodeEditor del panel lateral de Schema para el archivo JSON activo.
    pub(crate) fn update_schema_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = if let Some(editor) = &self.editor {
            editor.read(cx).text().to_string()
        } else if let Some(c) = &self.comparison {
            if self.editor_file_idx < c.files.len() {
                c.files[self.editor_file_idx].content.clone()
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        if text.trim().is_empty() {
            self.schema_editor = None;
            self.generated_schema_text.clear();
            return;
        }

        let title = self
            .comparison
            .as_ref()
            .and_then(|c| c.files.get(self.editor_file_idx))
            .map(|f| f.name.as_str())
            .unwrap_or("DocumentSchema");

        match generate_schema_from_str(&text, Some(title)) {
            Ok(schema_str) => {
                if let Some(schema_ed) = &self.schema_editor {
                    schema_ed.update(cx, |ed, cx| {
                        ed.set_text(schema_str.clone(), cx);
                    });
                } else {
                    let ed_text = schema_str.clone();
                    let schema_editor = cx.new(|cx| {
                        CodeEditor::new(ed_text, window, cx)
                            .language("json")
                            .line_numbers(LineNumbers::Absolute)
                            .read_only()
                            .rainbow_brackets()
                            .minimap()
                    });
                    self.schema_editor = Some(schema_editor);
                }
                self.generated_schema_text = schema_str;
            }
            Err(e) => {
                self.generated_schema_text = format!("// Error al generar schema:\n// {e}");
                if let Some(schema_ed) = &self.schema_editor {
                    schema_ed.update(cx, |ed, cx| {
                        ed.set_text(self.generated_schema_text.clone(), cx);
                    });
                } else {
                    let ed_text = self.generated_schema_text.clone();
                    let schema_editor = cx.new(|cx| {
                        CodeEditor::new(ed_text, window, cx)
                            .language("json")
                            .line_numbers(LineNumbers::Absolute)
                            .read_only()
                    });
                    self.schema_editor = Some(schema_editor);
                }
            }
        }
    }

    /// Sincroniza el contenido del editor de schema si ya está instanciado.
    pub(crate) fn sync_schema_if_present(&mut self, cx: &mut Context<Self>) {
        if let Some(schema_ed) = &self.schema_editor {
            let text = if let Some(editor) = &self.editor {
                editor.read(cx).text().to_string()
            } else {
                return;
            };
            let title = self
                .comparison
                .as_ref()
                .and_then(|c| c.files.get(self.editor_file_idx))
                .map(|f| f.name.as_str())
                .unwrap_or("DocumentSchema");
            if let Ok(schema_str) = generate_schema_from_str(&text, Some(title)) {
                schema_ed.update(cx, |ed, cx| {
                    ed.set_text(schema_str.clone(), cx);
                });
                self.generated_schema_text = schema_str;
            }
        }
    }

    /// Alterna la visibilidad del panel lateral de JSON Schema en la vista del editor.
    pub(crate) fn toggle_schema_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_schema_panel = !self.show_schema_panel;
        if self.show_schema_panel && self.schema_editor.is_none() {
            self.update_schema_editor(window, cx);
        }
        self.show_toast(
            if self.show_schema_panel {
                "Panel de JSON Schema visible"
            } else {
                "Panel de JSON Schema oculto"
            },
            cx,
        );
        cx.notify();
    }

    /// Abre el diálogo de previsualización de JSON Schema generado para el archivo activo.
    pub(crate) fn open_schema_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = if let Some(editor) = &self.editor {
            editor.read(cx).text().to_string()
        } else if let Some(c) = &self.comparison {
            if self.editor_file_idx < c.files.len() {
                c.files[self.editor_file_idx].content.clone()
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        if text.trim().is_empty() {
            self.show_toast("No hay contenido JSON para generar el schema", cx);
            return;
        }

        let title = self
            .comparison
            .as_ref()
            .and_then(|c| c.files.get(self.editor_file_idx))
            .map(|f| f.name.as_str())
            .unwrap_or("DocumentSchema");

        match generate_schema_from_str(&text, Some(title)) {
            Ok(schema_str) => {
                let ed_text = schema_str.clone();
                let schema_editor = cx.new(|cx| {
                    CodeEditor::new(ed_text, window, cx)
                        .language("json")
                        .line_numbers(LineNumbers::Absolute)
                        .read_only()
                        .rainbow_brackets()
                });
                self.schema_editor = Some(schema_editor);
                self.generated_schema_text = schema_str;
                self.show_schema_dialog = true;
                cx.notify();
            }
            Err(e) => {
                self.show_toast(format!("Error al generar schema: {e}"), cx);
            }
        }
    }

    /// Copia el JSON Schema generado al portapapeles.
    pub(crate) fn copy_schema_to_clipboard(&mut self, cx: &mut Context<Self>) {
        if !self.generated_schema_text.is_empty() {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                self.generated_schema_text.clone(),
            ));
            self.show_toast("JSON Schema copiado al portapapeles ✓", cx);
        }
    }

    /// Exporta el JSON Schema generado a un archivo en disco.
    pub(crate) fn export_schema_to_disk(&mut self, cx: &mut Context<Self>) {
        if self.generated_schema_text.is_empty() {
            return;
        }
        let file_name = self
            .comparison
            .as_ref()
            .and_then(|c| c.files.get(self.editor_file_idx))
            .map(|f| format!("{}.schema.json", f.name))
            .unwrap_or_else(|| "schema.json".to_string());

        let default_path = std::path::PathBuf::from(file_name);
        let schema_content = self.generated_schema_text.clone();
        let rx = cx.prompt_for_new_path(&default_path, None);
        cx.spawn(async move |this, cx| {
            let res = rx.await;
            if let Ok(Ok(Some(path))) = res {
                if let Err(e) = std::fs::write(&path, &schema_content) {
                    this.update(cx, |this, cx| {
                        this.show_toast(format!("Error al guardar schema: {e}"), cx);
                    })
                    .ok();
                } else {
                    this.update(cx, |this, cx| {
                        this.show_toast(format!("Schema guardado en: {}", path.display()), cx);
                    })
                    .ok();
                }
            }
        })
        .detach();
    }
}

fn self_status_toast_clear(this: &mut DiffView, cx: &mut Context<DiffView>) {
    this.status_toast = None;
    cx.notify();
}
