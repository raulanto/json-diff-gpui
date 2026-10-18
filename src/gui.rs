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
    navigation::{Command, CommandPalette},
    overlays::Dialog,
    primitives::{Icon, IconName, Tooltip},
    settings::{KeyboardShortcutsList, SettingsRow, SettingsSection, Shortcut},
    theme::{ActiveTheme, ControlSize, Elevation, IconSize, Mode, Radius, TextSize, Theme},
    typography::KbdCombo,
};
use gpui::{
    div, prelude::*, px, size, uniform_list, App, Bounds, ClipboardItem, Context, Entity,
    ExternalPaths, FocusHandle, FontWeight, KeyDownEvent, PathPromptOptions, SharedString,
    Subscription, TitlebarOptions, Window, WindowBounds, WindowOptions,
};
use jsondiff::{
    compare_paths_with_base, compare_with_base, detect_json_type, export_markdown, format_json,
    is_type_change, load_file_from_str, matches_filter, validate_json, Comparison, DiffMode,
    JsonType, MAX_FILES, MIN_FILES,
};
use std::ops::Range;
use std::path::PathBuf;

const ROW_H: f32 = 38.0;
const PATH_W: f32 = 320.0;

/// Modos de visualización integrados.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewMode {
    Table,
    GitDiff,
    Editor,
}

/// Pestañas del diálogo de Configuración.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsTab {
    General,
    Shortcuts,
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

    // Selector JSONPath (ej. "$.data", "$")
    selector_input: Entity<TextInput>,
    _selector_sub: Subscription,

    // Estado para vista Diff Git
    git_target_idx: usize,
    git_layout: DiffLayout,
    git_open_stretches: Vec<usize>,

    // Estado para vista Editor
    editor_file_idx: usize,
    editor: Option<Entity<CodeEditor>>,

    // Paleta de comandos (Command Palette)
    show_command_palette: bool,

    // Configuración de la aplicación
    show_settings: bool,
    settings_tab: SettingsTab,
    settings_shortcuts_search: Entity<TextInput>,
    _settings_sub: Subscription,

    // Foco global para captura de atajos
    focus_handle: FocusHandle,

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

        let selector_input =
            cx.new(|cx| TextInput::new(window, cx).placeholder("Selector (ej. $.data, $)"));

        let selector_sub =
            cx.subscribe(
                &selector_input,
                |this, entity, event: &InputEvent, cx| match event {
                    InputEvent::Changed => {
                        let text = entity.read(cx).text().to_string();
                        this.apply_selector(&text, false, cx);
                    }
                    InputEvent::Submit => {
                        let text = entity.read(cx).text().to_string();
                        this.apply_selector(&text, true, cx);
                    }
                    _ => {}
                },
            );

        let settings_shortcuts_search =
            cx.new(|cx| TextInput::new(window, cx).placeholder("Buscar atajos de teclado…"));

        let settings_sub = cx.subscribe(
            &settings_shortcuts_search,
            |_this, _entity, event: &InputEvent, cx| {
                if let InputEvent::Changed = event {
                    cx.notify();
                }
            },
        );

        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle, cx);

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
            selector_input,
            _selector_sub: selector_sub,
            git_target_idx: 1,
            git_layout: DiffLayout::Unified,
            git_open_stretches: Vec::new(),
            editor_file_idx: 0,
            editor: None,
            show_command_palette: false,
            show_settings: false,
            settings_tab: SettingsTab::General,
            settings_shortcuts_search,
            _settings_sub: settings_sub,
            focus_handle,
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
            prompt: Some(format!("Elige de {MIN_FILES} a {MAX_FILES} archivos JSON").into()),
        });
        cx.spawn(async move |this, cx| {
            let res = rx.await;
            if let Ok(Ok(Some(paths))) = res {
                if !paths.is_empty() {
                    this.update(cx, |this, cx| this.set_paths(paths, cx)).ok();
                    return;
                }
            }

            // Fallback con zenity en Linux si xdg-desktop-portal no está disponible o falla
            #[cfg(target_os = "linux")]
            {
                let output = std::process::Command::new("zenity")
                    .arg("--file-selection")
                    .arg("--multiple")
                    .arg("--separator=|")
                    .arg("--file-filter=Archivos JSON (*.json) | *.json")
                    .arg(format!(
                        "--title=Elige de {MIN_FILES} a {MAX_FILES} archivos JSON"
                    ))
                    .output();

                if let Ok(out) = output {
                    if out.status.success() {
                        let stdout = String::from_utf8_lossy(&out.stdout);
                        let paths: Vec<PathBuf> = stdout
                            .trim()
                            .split('|')
                            .map(|s| PathBuf::from(s.trim()))
                            .filter(|p| !p.as_os_str().is_empty())
                            .collect();
                        if !paths.is_empty() {
                            this.update(cx, |this, cx| this.set_paths(paths, cx)).ok();
                        }
                    }
                }
            }
        })
        .detach();
    }

    fn move_file(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        if from == to || from >= self.paths.len() || to >= self.paths.len() {
            return;
        }
        let file_name = self.paths[from]
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| format!("Archivo {}", from + 1));

        let p = self.paths.remove(from);
        self.paths.insert(to, p);

        if let Some(c) = &mut self.comparison {
            if c.move_file(from, to).is_ok() {
                self.refilter();
                self.show_toast(
                    format!("'{}' movido a la posición [{}]", file_name, to + 1),
                    cx,
                );
                cx.notify();
            }
        }
    }

    /// Limpia todos los archivos JSON cargados y regresa a la pantalla inicial.
    fn clear_files(&mut self, cx: &mut Context<Self>) {
        self.paths.clear();
        self.search_query.clear();
        self.search_input.update(cx, |input, cx| {
            input.set_text("", cx);
        });
        self.selector_input.update(cx, |input, cx| {
            input.set_text("$", cx);
        });
        self.reload();
        self.show_toast("Archivos limpiados", cx);
        cx.notify();
    }

    /// Quita un archivo específico de la comparación (si hay más del mínimo requerido).
    fn remove_file(&mut self, idx: usize, cx: &mut Context<Self>) {
        if idx >= self.paths.len() || self.paths.len() <= MIN_FILES {
            return;
        }
        let file_name = self.paths[idx]
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| format!("Archivo {}", idx + 1));
        self.paths.remove(idx);
        self.reload();
        self.show_toast(format!("'{}' quitado de la comparación", file_name), cx);
        cx.notify();
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

    /// Aplica un selector JSON (ej. "$.data", "$.items[*].id", "$") reconstruyendo las filas.
    fn apply_selector(&mut self, text: &str, is_submit: bool, cx: &mut Context<Self>) {
        let selector_str = if text.trim().is_empty() {
            "$"
        } else {
            text.trim()
        };
        let result = if let Some(c) = &mut self.comparison {
            match c.set_selector(selector_str) {
                Ok(()) => Ok((c.rows.len(), selector_str.to_string())),
                Err(e) => Err(e),
            }
        } else {
            return;
        };

        match result {
            Ok((total, sel_name)) => {
                self.refilter();
                if is_submit || sel_name != "$" {
                    let msg = if sel_name == "$" {
                        "Estructura completa del JSON ($)".to_string()
                    } else {
                        format!("Estructura enfocada: '{}' ({} campos)", sel_name, total)
                    };
                    self.show_toast(msg, cx);
                }
                cx.notify();
            }
            Err(e) => {
                if is_submit {
                    self.show_toast(format!("Error en selector: {e}"), cx);
                }
            }
        }
    }

    /// Restaura el selector a la raíz ("$") mostrando todo el documento.
    fn reset_selector(&mut self, cx: &mut Context<Self>) {
        self.selector_input.update(cx, |input, cx| {
            input.set_text("$", cx);
        });
        self.apply_selector("$", true, cx);
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

    /// Lista exhaustiva de atajos de teclado registrados en la aplicación.
    pub fn shortcuts_list() -> Vec<Shortcut> {
        vec![
            Shortcut {
                group: "General & Navegación".into(),
                name: "Abrir paleta de comandos".into(),
                keys: "secondary-k".into(),
            },
            Shortcut {
                group: "General & Navegación".into(),
                name: "Abrir Configuración de la aplicación".into(),
                keys: "secondary-,".into(),
            },
            Shortcut {
                group: "General & Navegación".into(),
                name: "Abrir archivos JSON del disco".into(),
                keys: "secondary-o".into(),
            },
            Shortcut {
                group: "General & Navegación".into(),
                name: "Cerrar paleta de comandos / diálogos".into(),
                keys: "escape".into(),
            },
            Shortcut {
                group: "Vistas".into(),
                name: "Ver: Tabla matricial".into(),
                keys: "secondary-1".into(),
            },
            Shortcut {
                group: "Vistas".into(),
                name: "Ver: Diff estilo Git".into(),
                keys: "secondary-2".into(),
            },
            Shortcut {
                group: "Vistas".into(),
                name: "Ver: Editor en vivo".into(),
                keys: "secondary-3".into(),
            },
            Shortcut {
                group: "Comparación y Filtros".into(),
                name: "Alternar modo: Base fija / En cadena".into(),
                keys: "secondary-m".into(),
            },
            Shortcut {
                group: "Comparación y Filtros".into(),
                name: "Alternar filtro: Solo cambios / Todos".into(),
                keys: "secondary-shift-c".into(),
            },
            Shortcut {
                group: "Comparación y Filtros".into(),
                name: "Copiar reporte Markdown".into(),
                keys: "secondary-shift-m".into(),
            },
            Shortcut {
                group: "Comparación y Filtros".into(),
                name: "Limpiar todos los archivos cargados".into(),
                keys: "secondary-shift-l".into(),
            },
            Shortcut {
                group: "Editor".into(),
                name: "Formatear JSON (2 espacios)".into(),
                keys: "secondary-shift-i".into(),
            },
            Shortcut {
                group: "Editor".into(),
                name: "Guardar archivo editado en disco".into(),
                keys: "secondary-s".into(),
            },
            Shortcut {
                group: "Personalización".into(),
                name: "Alternar tema (Claro / Oscuro)".into(),
                keys: "secondary-t".into(),
            },
        ]
    }

    /// Ejecuta la acción seleccionada desde la paleta de comandos.
    fn run_command_action(&mut self, action: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.show_command_palette = false;
        match action {
            "open_files" => self.pick_files(cx),
            "clear_files" => self.clear_files(cx),
            "view_table" => {
                self.view_mode = ViewMode::Table;
                self.editor = None;
                cx.notify();
            }
            "view_git" => {
                self.view_mode = ViewMode::GitDiff;
                self.editor = None;
                cx.notify();
            }
            "view_editor" => {
                self.view_mode = ViewMode::Editor;
                self.init_editor_if_needed(window, cx);
                cx.notify();
            }
            "toggle_diff_mode" => {
                if let Some(c) = &mut self.comparison {
                    let new_mode = match c.diff_mode {
                        DiffMode::FixedBase => DiffMode::Sequential,
                        DiffMode::Sequential => DiffMode::FixedBase,
                    };
                    c.set_diff_mode(new_mode);
                    self.refilter();
                    self.show_toast(
                        if new_mode == DiffMode::Sequential {
                            "Modo En cadena: cada archivo se compara con el anterior"
                        } else {
                            "Modo Base fija: todos se comparan con la base"
                        },
                        cx,
                    );
                    cx.notify();
                }
            }
            "toggle_filter" => {
                self.only_changed = !self.only_changed;
                self.refilter();
                self.show_toast(
                    if self.only_changed {
                        "Filtro: Mostrando solo cambios"
                    } else {
                        "Filtro: Mostrando todos los campos"
                    },
                    cx,
                );
                cx.notify();
            }
            "reset_selector" => {
                self.reset_selector(cx);
            }
            "export_markdown" => {
                if let Some(c) = &self.comparison {
                    let md = export_markdown(c, self.only_changed);
                    cx.write_to_clipboard(ClipboardItem::new_string(md));
                    self.show_toast("Reporte Markdown copiado al portapapeles", cx);
                }
            }
            "format_json" => {
                if self.view_mode != ViewMode::Editor {
                    self.view_mode = ViewMode::Editor;
                    self.init_editor_if_needed(window, cx);
                }
                self.format_editor_json(cx);
            }
            "validate_json" => {
                if self.view_mode != ViewMode::Editor {
                    self.view_mode = ViewMode::Editor;
                    self.init_editor_if_needed(window, cx);
                }
                self.validate_editor_json(cx);
            }
            "apply_changes" => {
                if self.view_mode == ViewMode::Editor {
                    self.apply_editor_changes(cx);
                }
            }
            "save_file" => {
                if self.view_mode == ViewMode::Editor {
                    self.save_editor_to_disk(cx);
                }
            }
            "open_settings" => {
                self.show_settings = true;
                cx.notify();
            }
            "toggle_theme" => {
                let next = if cx.theme().is_dark() {
                    Mode::Light
                } else {
                    Mode::Dark
                };
                Theme::set_mode(next, cx);
            }
            _ => {}
        }
    }

    /// Maneja los atajos de teclado globales. Devuelve true si el atajo fue consumido.
    fn handle_global_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let raw_key = event.keystroke.key.as_str();
        let key_lower = raw_key.to_lowercase();
        let key = key_lower.as_str();
        let m = &event.keystroke.modifiers;

        // Escape cierra cualquier modal activo
        if (key == "escape" || key == "esc") && !m.modified() {
            if self.show_command_palette {
                self.show_command_palette = false;
                cx.notify();
                return true;
            }
            if self.show_settings {
                self.show_settings = false;
                cx.notify();
                return true;
            }
            return false;
        }

        // Teclas de función directas (sin modificadores)
        if !m.modified() {
            if key == "f1" {
                self.show_command_palette = !self.show_command_palette;
                self.show_settings = false;
                cx.notify();
                return true;
            }
            if key == "f2" {
                self.show_settings = !self.show_settings;
                self.show_command_palette = false;
                cx.notify();
                return true;
            }
        }

        // Atajos primarios con Ctrl (Linux/Win) o Cmd (macOS)
        let is_ctrl = m.secondary() || m.control || m.platform;
        if is_ctrl && !m.alt {
            if !m.shift {
                match key {
                    // Paleta de comandos: Ctrl+K o Ctrl+P
                    "k" | "p" => {
                        self.show_command_palette = !self.show_command_palette;
                        self.show_settings = false;
                        cx.notify();
                        return true;
                    }
                    // Configuración: Ctrl+, (en Linux puede llegar como "," o "comma")
                    "," | "comma" | "<" => {
                        self.show_settings = !self.show_settings;
                        self.show_command_palette = false;
                        cx.notify();
                        return true;
                    }
                    // Abrir archivos: Ctrl+O
                    "o" | "open" => {
                        self.pick_files(cx);
                        return true;
                    }
                    // Vistas: Ctrl+1, Ctrl+2, Ctrl+3
                    "1" => {
                        self.view_mode = ViewMode::Table;
                        self.editor = None;
                        cx.notify();
                        return true;
                    }
                    "2" => {
                        self.view_mode = ViewMode::GitDiff;
                        self.editor = None;
                        cx.notify();
                        return true;
                    }
                    "3" => {
                        self.view_mode = ViewMode::Editor;
                        self.init_editor_if_needed(window, cx);
                        cx.notify();
                        return true;
                    }
                    // Alternar modo diff: Ctrl+M
                    "m" => {
                        if let Some(c) = &mut self.comparison {
                            let new_mode = match c.diff_mode {
                                DiffMode::FixedBase => DiffMode::Sequential,
                                DiffMode::Sequential => DiffMode::FixedBase,
                            };
                            c.set_diff_mode(new_mode);
                            self.refilter();
                            self.show_toast(
                                if new_mode == DiffMode::Sequential {
                                    "Modo En cadena: cada archivo se compara con el anterior"
                                } else {
                                    "Modo Base fija: todos se comparan con la base"
                                },
                                cx,
                            );
                            cx.notify();
                            return true;
                        }
                    }
                    // Alternar tema: Ctrl+T
                    "t" => {
                        let next = if cx.theme().is_dark() {
                            Mode::Light
                        } else {
                            Mode::Dark
                        };
                        Theme::set_mode(next, cx);
                        return true;
                    }
                    // Guardar en editor: Ctrl+S
                    "s" => {
                        if self.view_mode == ViewMode::Editor {
                            self.save_editor_to_disk(cx);
                            return true;
                        }
                    }
                    _ => {}
                }
            } else {
                // Combinaciones con Shift (Ctrl+Shift+...)
                match key {
                    // Paleta de comandos alternativa (estilo VS Code): Ctrl+Shift+P
                    "p" => {
                        self.show_command_palette = !self.show_command_palette;
                        self.show_settings = false;
                        cx.notify();
                        return true;
                    }
                    // Configuración alternativa: Ctrl+Shift+,
                    "," | "comma" | "<" => {
                        self.show_settings = !self.show_settings;
                        self.show_command_palette = false;
                        cx.notify();
                        return true;
                    }
                    // Filtrar solo cambios: Ctrl+Shift+C
                    "c" => {
                        self.only_changed = !self.only_changed;
                        self.refilter();
                        self.show_toast(
                            if self.only_changed {
                                "Filtro: Mostrando solo cambios"
                            } else {
                                "Filtro: Mostrando todos los campos"
                            },
                            cx,
                        );
                        cx.notify();
                        return true;
                    }
                    // Copiar reporte Markdown: Ctrl+Shift+M
                    "m" => {
                        if let Some(c) = &self.comparison {
                            let md = export_markdown(c, self.only_changed);
                            cx.write_to_clipboard(ClipboardItem::new_string(md));
                            self.show_toast("Reporte Markdown copiado al portapapeles", cx);
                            return true;
                        }
                    }
                    // Limpiar archivos: Ctrl+Shift+L
                    "l" => {
                        self.clear_files(cx);
                        return true;
                    }
                    // Formatear JSON: Ctrl+Shift+I o Ctrl+Shift+F
                    "i" | "f" => {
                        if self.view_mode == ViewMode::Editor {
                            self.format_editor_json(cx);
                            return true;
                        }
                    }
                    _ => {}
                }
            }
        }

        false
    }

    /// Renderiza la paleta de comandos interactiva de Ely.
    fn render_command_palette(&self, view: Entity<Self>) -> impl IntoElement {
        CommandPalette::new("global_command_palette", {
            let view = view.clone();
            move |_, cx| {
                view.update(cx, |this, cx| {
                    this.show_command_palette = false;
                    cx.notify();
                });
            }
        })
        .group(
            "Navegación y Vistas",
            vec![
                Command::new("view_table", "Ver: Tabla matricial")
                    .icon(IconName::Table)
                    .keys("secondary-1"),
                Command::new("view_git", "Ver: Diff estilo Git")
                    .icon(IconName::GitBranch)
                    .keys("secondary-2"),
                Command::new("view_editor", "Ver: Editor en vivo")
                    .icon(IconName::Code)
                    .keys("secondary-3"),
            ],
        )
        .group(
            "Archivos",
            vec![
                Command::new("open_files", "Abrir archivos JSON…")
                    .icon(IconName::FolderOpen)
                    .keys("secondary-o"),
                Command::new("clear_files", "Limpiar archivos cargados")
                    .icon(IconName::Trash2)
                    .keys("secondary-shift-l"),
                Command::new("export_markdown", "Copiar reporte Markdown")
                    .icon(IconName::Copy)
                    .keys("secondary-shift-m"),
            ],
        )
        .group(
            "Comparación y Filtros",
            vec![
                Command::new("toggle_diff_mode", "Alternar modo: Base fija / En cadena")
                    .icon(IconName::SlidersHorizontal)
                    .keys("secondary-m"),
                Command::new("toggle_filter", "Alternar filtro: Solo cambios / Todos")
                    .icon(IconName::Filter)
                    .keys("secondary-shift-c"),
                Command::new(
                    "reset_selector",
                    "Restaurar selector JSONPath a la raíz ($)",
                )
                .icon(IconName::RotateCcw),
            ],
        )
        .group(
            "Editor de Código",
            vec![
                Command::new("format_json", "Formatear JSON (2 espacios)")
                    .icon(IconName::Sparkles)
                    .keys("secondary-shift-i"),
                Command::new("validate_json", "Validar sintaxis JSON").icon(IconName::Check),
                Command::new("apply_changes", "Aplicar edición a la comparación")
                    .icon(IconName::Play),
                Command::new("save_file", "Guardar cambios en disco")
                    .icon(IconName::Save)
                    .keys("secondary-s"),
            ],
        )
        .group(
            "Configuración y Preferencias",
            vec![
                Command::new("open_settings", "Abrir Configuración de la aplicación")
                    .icon(IconName::Settings)
                    .keys("secondary-,"),
                Command::new("toggle_theme", "Alternar tema (Claro / Oscuro)")
                    .icon(IconName::Sun)
                    .keys("secondary-t"),
            ],
        )
        .on_run({
            let view = view.clone();
            move |val, window, cx| {
                view.update(cx, |this, cx| {
                    this.run_command_action(val.as_ref(), window, cx);
                });
            }
        })
    }

    /// Renderiza el modal de Configuración de la aplicación usando Dialog y componentes de Ely.
    fn render_settings_dialog(
        &self,
        view: Entity<Self>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let colors = &theme.colors;
        let shortcuts = Self::shortcuts_list();

        let tab_content = match self.settings_tab {
            SettingsTab::General => {
                let current_diff_mode = self
                    .comparison
                    .as_ref()
                    .map(|c| c.diff_mode)
                    .unwrap_or(DiffMode::FixedBase);
                let current_filter = if self.only_changed { "changed" } else { "all" };
                let current_theme = if cx.theme().is_dark() { "dark" } else { "light" };

                let view1 = view.clone();
                let view2 = view.clone();
                let view3 = view.clone();

                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(
                        SettingsSection::new("Apariencia y Visualización")
                            .description("Personaliza el tema visual y la paleta de colores de Ely.")
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
                            ),
                    )
                    .child(
                        SettingsSection::new("Motor de Comparación")
                            .description("Ajusta la estrategia de evaluación y filtros entre versiones de JSON.")
                            .row(
                                SettingsRow::new("Modo de comparación")
                                    .description("Base fija compara contra el primer archivo; En cadena compara secuencialmente.")
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
            SettingsTab::Shortcuts => {
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_size(theme.text_size(TextSize::Sm))
                            .text_color(colors.fg_muted)
                            .child("Escribe para filtrar atajos por nombre, acción o combinación de teclas:"),
                    )
                    .child(
                        div()
                            .id("settings_shortcuts_scroll")
                            .max_h(px(320.0))
                            .overflow_y_scroll()
                            .child(KeyboardShortcutsList::new(shortcuts, &self.settings_shortcuts_search)),
                    )
                    .into_any_element()
            }
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
                        match self.settings_tab {
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
        .action({
            let view = view.clone();
            move |close| {
                Button::new("btn_done_settings", "Listo")
                    .primary()
                    .size(ControlSize::Sm)
                    .on_click(move |_, window, cx| {
                        close(window, cx);
                        view.update(cx, |this, cx| {
                            this.show_settings = false;
                            cx.notify();
                        });
                    })
            }
        })
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
                    .when(!self.paths.is_empty(), |bar| {
                        bar.child(
                            Button::new("clear_files", "Limpiar")
                                .size(ControlSize::Sm)
                                .variant(ButtonVariant::Outline)
                                .icon(IconName::Trash2)
                                .on_click(cx.listener(|this, _, _, cx| this.clear_files(cx))),
                        )
                    })
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
                    let current_diff_mode = self
                        .comparison
                        .as_ref()
                        .map(|c| c.diff_mode)
                        .unwrap_or_default();
                    let current_selector = self
                        .comparison
                        .as_ref()
                        .map(|c| c.selector.as_str())
                        .unwrap_or("$");

                    bar.child(
                        SegmentedControl::new(
                            "diff_mode",
                            match current_diff_mode {
                                DiffMode::FixedBase => "fixed",
                                DiffMode::Sequential => "seq",
                            },
                        )
                        .size(ControlSize::Sm)
                        .segment("fixed", "Base fija", Some(IconName::Pin))
                        .segment("seq", "En cadena", Some(IconName::ArrowRight))
                        .on_change({
                            let view = view.clone();
                            move |value, _, cx| {
                                let new_mode = if value.as_ref() == "seq" {
                                    DiffMode::Sequential
                                } else {
                                    DiffMode::FixedBase
                                };
                                view.update(cx, |this, cx| {
                                    if let Some(c) = &mut this.comparison {
                                        c.set_diff_mode(new_mode);
                                        this.refilter();
                                        this.show_toast(
                                            if new_mode == DiffMode::Sequential {
                                                "Modo En cadena: cada archivo se compara con el anterior"
                                            } else {
                                                "Modo Base fija: todos se comparan con la base"
                                            },
                                            cx,
                                        );
                                        cx.notify();
                                    }
                                });
                            }
                        }),
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
                        div().w(px(200.0)).child(
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
                    .child(
                        div().w(px(200.0)).child(
                            Input::new(&self.selector_input)
                                .size(ControlSize::Sm)
                                .prefix(
                                    Icon::new(IconName::Braces)
                                        .size(IconSize::Sm)
                                        .color(colors.fg_subtle),
                                )
                                .clearable(),
                        ),
                    )
                    .when(current_selector != "$", |el| {
                        let view = view.clone();
                        el.child(
                            Button::new("reset_selector", "Restaurar $")
                                .size(ControlSize::Sm)
                                .variant(ButtonVariant::Ghost)
                                .icon(IconName::RotateCcw)
                                .on_click(move |_, _, cx| {
                                    view.update(cx, |this, cx| {
                                        this.reset_selector(cx);
                                    });
                                }),
                        )
                    })
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
                        IconButton::new("cmd_palette_btn", IconName::Command)
                            .size(ControlSize::Sm)
                            .tooltip("Paleta de comandos (Ctrl+K)")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_command_palette = !this.show_command_palette;
                                this.show_settings = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        IconButton::new("settings_btn", IconName::Settings)
                            .size(ControlSize::Sm)
                            .tooltip("Configuración (Ctrl+,)")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_settings = !this.show_settings;
                                this.show_command_palette = false;
                                cx.notify();
                            })),
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
                            "Modo claro (Ctrl+T)"
                        } else {
                            "Modo oscuro (Ctrl+T)"
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
                .child(
                    Button::new("btn_clear_error", "Limpiar y volver a empezar")
                        .size(ControlSize::Sm)
                        .variant(ButtonVariant::Outline)
                        .icon(IconName::RotateCcw)
                        .on_click(cx.listener(|this, _, _, cx| this.clear_files(cx))),
                )
                .into_any_element()
        } else if let Some(c) = &self.comparison {
            match self.view_mode {
                // ========================================== VISTA 1: TABLA COMPARATIVA
                ViewMode::Table => {
                    let base_ix = c.base_index;
                    let diff_mode = c.diff_mode;
                    let files_count = c.files.len();

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
                                .flex_none()
                                .px_3()
                                .flex()
                                .flex_row()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child("Campo / Ruta JSON"),
                                )
                                .when(c.selector != "$", |el| {
                                    el.child(
                                        Badge::new(c.selector.clone()).tone(Tone::Accent).dot(),
                                    )
                                }),
                        );

                    for (i, f) in c.files.iter().enumerate() {
                        let is_base = match diff_mode {
                            DiffMode::FixedBase => i == base_ix,
                            DiffMode::Sequential => i == 0,
                        };
                        let label = format!(
                            "[{}] {}{}",
                            i + 1,
                            f.name,
                            if f.lines_exact { "" } else { " *" }
                        );
                        let full_path = f.path.display().to_string();

                        let can_move_left = i > 0;
                        let can_move_right = i < files_count - 1;

                        let mut move_controls = div().flex().flex_row().items_center().gap_0p5();

                        if can_move_left {
                            let view = view.clone();
                            move_controls = move_controls.child(
                                IconButton::new(("move_left", i), IconName::ChevronLeft)
                                    .size(ControlSize::Sm)
                                    .tooltip("Mover archivo hacia la izquierda")
                                    .on_click(move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            this.move_file(i, i - 1, cx);
                                        });
                                    }),
                            );
                        }

                        if can_move_right {
                            let view = view.clone();
                            move_controls = move_controls.child(
                                IconButton::new(("move_right", i), IconName::ChevronRight)
                                    .size(ControlSize::Sm)
                                    .tooltip("Mover archivo hacia la derecha")
                                    .on_click(move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            this.move_file(i, i + 1, cx);
                                        });
                                    }),
                            );
                        }

                        if files_count > MIN_FILES {
                            let view = view.clone();
                            move_controls = move_controls.child(
                                IconButton::new(("remove_file", i), IconName::X)
                                    .size(ControlSize::Sm)
                                    .tooltip("Quitar este archivo de la comparación")
                                    .on_click(move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            this.remove_file(i, cx);
                                        });
                                    }),
                            );
                        }

                        let role_badge = match diff_mode {
                            DiffMode::FixedBase => {
                                if is_base {
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
                                }
                            }
                            DiffMode::Sequential => {
                                if i == 0 {
                                    div().child(Badge::new("Origen").tone(Tone::Accent).dot())
                                } else {
                                    div().child(Badge::new(format!("vs [{}]", i)).tone(Tone::Info))
                                }
                            }
                        };

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
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1()
                                    .child(role_badge)
                                    .child(move_controls),
                            );

                        header = header.child(col_header);
                    }

                    // Filas virtualizadas mediante uniform_list
                    let visible_indices = self.visible.clone();
                    let list_indices = visible_indices.clone();
                    let rows_data = c.rows.clone();
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
                                            colors.warning.opacity(0.04)
                                        } else {
                                            colors.surface
                                        })
                                        .hover(|s| s.bg(colors.hover.opacity(0.5)));

                                    // Columna de ruta
                                    let path_str = row.path.clone();
                                    let copy_path = path_str.clone();

                                    // Separar prefijo padre y clave hoja para mejor legibilidad
                                    let (parent_path, leaf_name) = match path_str.rfind('.') {
                                        Some(pos) => (&path_str[..=pos], &path_str[pos + 1..]),
                                        None => ("", path_str.as_str()),
                                    };

                                    let path_cell = div()
                                        .id(("path_cell", row_idx))
                                        .w(px(PATH_W))
                                        .flex_none()
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
                                        .child(
                                            div()
                                                .flex_none()
                                                .w(px(7.0))
                                                .h(px(7.0))
                                                .rounded_full()
                                                .bg(if is_diff {
                                                    colors.warning
                                                } else {
                                                    colors.success
                                                }),
                                        )
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .overflow_hidden()
                                                .whitespace_nowrap()
                                                .text_ellipsis()
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .child(
                                                    div()
                                                        .text_color(colors.fg_subtle)
                                                        .text_size(theme.text_size(TextSize::Xs))
                                                        .child(parent_path.to_string()),
                                                )
                                                .child(
                                                    div()
                                                        .font_weight(if is_diff {
                                                            FontWeight::BOLD
                                                        } else {
                                                            FontWeight::MEDIUM
                                                        })
                                                        .text_color(if is_diff {
                                                            colors.fg
                                                        } else {
                                                            colors.fg_muted
                                                        })
                                                        .child(leaf_name.to_string()),
                                                ),
                                        );

                                    row_el = row_el.child(path_cell);

                                    // Columnas de valores
                                    for file_col in 0..files_count {
                                        let (is_base, ref_cell, ref_label) = match diff_mode {
                                            DiffMode::FixedBase => (
                                                file_col == base_ix,
                                                row.cells.get(base_ix).and_then(|c| c.as_ref()),
                                                format!("[{}]", base_ix + 1),
                                            ),
                                            DiffMode::Sequential => {
                                                if file_col == 0 {
                                                    (true, None, "[1]".to_string())
                                                } else {
                                                    (
                                                        false,
                                                        row.cells
                                                            .get(file_col - 1)
                                                            .and_then(|c| c.as_ref()),
                                                        format!("[{}]", file_col),
                                                    )
                                                }
                                            }
                                        };

                                        let cell =
                                            row.cells.get(file_col).and_then(|c| c.as_ref());

                                        let cell_el = if let Some(c) = cell {
                                            let val_str = c.value.clone();
                                            let copy_val = val_str.clone();
                                            let line_info = format!("L{}", c.line);

                                            let jtype = detect_json_type(&val_str);
                                            let (differs_from_ref, type_changed, is_new_field) =
                                                if is_base {
                                                    (false, false, false)
                                                } else {
                                                    match ref_cell {
                                                        Some(b) => (
                                                            b.value != c.value,
                                                            is_type_change(&b.value, &c.value),
                                                            false,
                                                        ),
                                                        None => (true, false, true),
                                                    }
                                                };

                                            let tooltip_msg = if is_base {
                                                if diff_mode == DiffMode::Sequential {
                                                    format!(
                                                        "Origen [1] (inicio de secuencia) | L{}\nClic para copiar valor",
                                                        c.line
                                                    )
                                                } else {
                                                    format!(
                                                        "Base fija [{}] | L{}\nClic para copiar valor",
                                                        base_ix + 1,
                                                        c.line
                                                    )
                                                }
                                            } else if is_new_field {
                                                format!(
                                                    "Nuevo campo (no existía en {}) | L{}\nClic para copiar valor",
                                                    ref_label, c.line
                                                )
                                            } else if type_changed {
                                                format!(
                                                    "Cambio de tipo vs {}\nAnterior: {}\nActual: {}\nClic para copiar",
                                                    ref_label,
                                                    ref_cell.map(|b| b.value.as_str()).unwrap_or(""),
                                                    val_str
                                                )
                                            } else if differs_from_ref {
                                                format!(
                                                    "Difiere de {}\nAnterior: {}\nActual: {}\nClic para copiar",
                                                    ref_label,
                                                    ref_cell.map(|b| b.value.as_str()).unwrap_or(""),
                                                    val_str
                                                )
                                            } else {
                                                format!(
                                                    "Idéntico a {} | L{}\nClic para copiar",
                                                    ref_label, c.line
                                                )
                                            };

                                            let mut val_content = div()
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .gap_1p5()
                                                .min_w_0()
                                                .overflow_hidden();

                                            if is_new_field {
                                                val_content = val_content
                                                    .child(Badge::new("+ Nuevo").tone(Tone::Success).dot());
                                            }

                                            if type_changed {
                                                val_content = val_content
                                                    .child(Badge::new("≠ Tipo").tone(Tone::Warning));
                                            }

                                            match jtype {
                                                JsonType::Boolean => {
                                                    let tone = if val_str == "true" {
                                                        Tone::Success
                                                    } else {
                                                        Tone::Warning
                                                    };
                                                    val_content = val_content
                                                        .child(Badge::new(val_str.clone()).tone(tone));
                                                }
                                                JsonType::Null => {
                                                    val_content = val_content
                                                        .child(Badge::new("null").tone(Tone::Neutral));
                                                }
                                                JsonType::Number => {
                                                    val_content = val_content.child(
                                                        div()
                                                            .font_weight(FontWeight::MEDIUM)
                                                            .text_color(if differs_from_ref {
                                                                colors.warning
                                                            } else if is_base {
                                                                colors.accent
                                                            } else {
                                                                colors.fg
                                                            })
                                                            .child(val_str.clone()),
                                                    );
                                                }
                                                JsonType::String => {
                                                    val_content = val_content.child(
                                                        div()
                                                            .overflow_hidden()
                                                            .whitespace_nowrap()
                                                            .text_ellipsis()
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
                                                            .child(val_str.clone()),
                                                    );
                                                }
                                                JsonType::Array | JsonType::Object => {
                                                    let tag = if jtype == JsonType::Array {
                                                        "[]"
                                                    } else {
                                                        "{}"
                                                    };
                                                    val_content = val_content
                                                        .child(Badge::new(tag).tone(Tone::Info))
                                                        .child(
                                                            div()
                                                                .overflow_hidden()
                                                                .whitespace_nowrap()
                                                                .text_ellipsis()
                                                                .text_color(colors.fg_muted)
                                                                .child(val_str.clone()),
                                                        );
                                                }
                                                JsonType::Other => {
                                                    val_content = val_content.child(
                                                        div()
                                                            .overflow_hidden()
                                                            .whitespace_nowrap()
                                                            .text_ellipsis()
                                                            .text_color(if differs_from_ref {
                                                                colors.fg
                                                            } else {
                                                                colors.fg_muted
                                                            })
                                                            .child(val_str.clone()),
                                                    );
                                                }
                                            }

                                            let mut cell_box = div()
                                                .id(("val_cell", row_idx * 10 + file_col))
                                                .flex_1()
                                                .min_w_0()
                                                .h(px(ROW_H - 6.0))
                                                .mx_1()
                                                .px_2()
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .gap_2()
                                                .cursor_pointer()
                                                .rounded(theme.radius(Radius::Sm))
                                                .tooltip(Tooltip::text(tooltip_msg))
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
                                                                format!(
                                                                    "Valor copiado: {}",
                                                                    copy_val
                                                                ),
                                                                cx,
                                                            );
                                                        });
                                                    }
                                                });

                                            if differs_from_ref {
                                                cell_box = cell_box
                                                    .bg(colors.warning.opacity(0.10))
                                                    .border_1()
                                                    .border_color(colors.warning.opacity(0.35));
                                            } else if is_base {
                                                cell_box = cell_box.bg(colors.sunken.opacity(0.30));
                                            }

                                            cell_box
                                                .child(
                                                    div()
                                                        .flex_none()
                                                        .px_1p5()
                                                        .py(px(1.0))
                                                        .rounded(theme.radius(Radius::Sm))
                                                        .bg(colors.sunken)
                                                        .text_size(theme.text_size(TextSize::Xs))
                                                        .text_color(colors.fg_subtle)
                                                        .child(line_info),
                                                )
                                                .child(val_content)
                                        } else {
                                            if ref_cell.is_some() {
                                                div()
                                                    .id(("val_cell_empty", row_idx * 10 + file_col))
                                                    .flex_1()
                                                    .min_w_0()
                                                    .h(px(ROW_H - 6.0))
                                                    .mx_1()
                                                    .px_2()
                                                    .flex()
                                                    .flex_row()
                                                    .items_center()
                                                    .rounded(theme.radius(Radius::Sm))
                                                    .bg(colors.danger.opacity(0.08))
                                                    .border_1()
                                                    .border_color(colors.danger.opacity(0.25))
                                                    .tooltip(Tooltip::text(format!(
                                                        "Campo ausente en este archivo (presente en {})",
                                                        ref_label
                                                    )))
                                                    .child(
                                                        Badge::new("Ausente")
                                                            .tone(Tone::Danger)
                                                            .dot(),
                                                    )
                                            } else {
                                                div()
                                                    .id(("val_cell_empty", row_idx * 10 + file_col))
                                                    .flex_1()
                                                    .min_w_0()
                                                    .h(px(ROW_H - 6.0))
                                                    .mx_1()
                                                    .px_2()
                                                    .flex()
                                                    .flex_row()
                                                    .items_center()
                                                    .tooltip(Tooltip::text("Campo ausente"))
                                                    .child(Badge::new("∅").tone(Tone::Neutral))
                                            }
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
                                            .child(if c.rows.is_empty() {
                                                format!(
                                                    "El selector '{}' no coincidió con ningún campo en los archivos JSON.",
                                                    c.selector
                                                )
                                            } else if self.only_changed {
                                                if c.changed_count() == 0 {
                                                    "¡Los archivos son idénticos! No hay diferencias en los campos seleccionados."
                                                        .to_string()
                                                } else {
                                                    "No hay diferencias que coincidan con la búsqueda."
                                                        .to_string()
                                                }
                                            } else {
                                                "No se encontraron campos que coincidan con la búsqueda."
                                                    .to_string()
                                            }),
                                    )
                                    .when(c.selector != "$", |el| {
                                        let view = view.clone();
                                        el.child(
                                            Button::new(
                                                "btn_reset_empty",
                                                "Restaurar estructura completa ($)",
                                            )
                                            .size(ControlSize::Sm)
                                            .variant(ButtonVariant::Primary)
                                            .icon(IconName::RotateCcw)
                                            .on_click(move |_, _, cx| {
                                                view.update(cx, |this, cx| {
                                                    this.reset_selector(cx);
                                                });
                                            }),
                                        )
                                    })
                                    .when(self.only_changed && !c.rows.is_empty(), |el| {
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
                                        .child("Ajustes:")
                                        .child(KbdCombo::new("secondary-,"))
                                        .child(div().text_xs().text_color(colors.fg_subtle).child("(o F2)")),
                                ),
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
            .track_focus(&self.focus_handle)
            .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if this.handle_global_key(event, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if this.handle_global_key(event, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                this.set_paths(paths.paths().to_vec(), cx);
            }))
            .child(toolbar)
            .child(body)
            .children(if self.show_command_palette {
                Some(self.render_command_palette(view.clone()))
            } else {
                None
            })
            .children(if self.show_settings {
                Some(self.render_settings_dialog(view.clone(), cx))
            } else {
                None
            })
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
