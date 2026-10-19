//! Operaciones del editor en vivo, selectores JSONPath, densidad visual y notificaciones toast.

use crate::gui::components::editor_view::compute_git_marks;
use crate::gui::types::Density;
use crate::gui::DiffView;
use ely_gpui_component::editor::{CodeEditor, LineNumbers};
use gpui::{prelude::*, Context, SharedString, Window};
use jsondiff::{compare_with_base, format_json, load_file_from_str, validate_json};
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
                .rainbow_brackets();
            if is_not_base && !base_content.is_empty() {
                let marks = compute_git_marks(&base_content, &content);
                ed.set_git_marks(marks, cx);
            }
            ed
        });

        self.editor = Some(editor);
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
}

fn self_status_toast_clear(this: &mut DiffView, cx: &mut Context<DiffView>) {
    this.status_toast = None;
    cx.notify();
}
