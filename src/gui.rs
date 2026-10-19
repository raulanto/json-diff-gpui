//! Interfaz gráfica principal (GPUI + Ely Components).
//!
//! Arquitectura modular dividida en componentes reutilizables, tipos descriptivos
//! y despacho desacoplado de comandos y atajos de teclado.

pub mod components;
pub mod shortcuts;
pub mod types;

pub use types::{Density, SettingsTab, ViewMode};

use ely_gpui_component::{
    buttons::{Button, ButtonVariant},
    editor::{CodeEditor, LineNumbers},
    forms::{InputEvent, TextInput},
    git::DiffLayout,
    primitives::IconName,
    theme::{ActiveTheme, ControlSize, Mode, Radius, TextSize, Theme},
};
use gpui::{
    div, prelude::*, px, size, App, Bounds, Context, Entity, ExternalPaths, FocusHandle,
    PathPromptOptions, SharedString, Subscription, TitlebarOptions, Window, WindowBounds,
    WindowOptions,
};
use jsondiff::{
    compare_paths_with_base, compare_with_base, format_json, load_file_from_str, matches_filter,
    validate_json, Comparison, DiffMode, Project, Workspace, MAX_FILES, MIN_FILES,
};
use std::path::PathBuf;

/// Estado principal de la aplicación gráfica y orquestador de vistas.
pub struct DiffView {
    pub(crate) paths: Vec<PathBuf>,
    pub(crate) comparison: Option<Comparison>,
    pub(crate) error: Option<SharedString>,
    pub(crate) view_mode: ViewMode,

    // Densidad visual (Compacta o Cómoda)
    pub(crate) density: Density,

    // Estado para vista Tabla
    pub(crate) only_changed: bool,
    pub(crate) search_query: String,
    pub(crate) search_input: Entity<TextInput>,
    pub(crate) _search_sub: Subscription,
    pub(crate) visible: Vec<usize>,

    // Selector JSONPath (ej. "$.data", "$")
    pub(crate) selector_input: Entity<TextInput>,
    pub(crate) _selector_sub: Subscription,

    // Estado para vista Diff Git
    pub(crate) git_target_idx: usize,
    pub(crate) git_layout: DiffLayout,
    pub(crate) git_open_stretches: Vec<usize>,

    // Estado para vista Editor
    pub(crate) editor_file_idx: usize,
    pub(crate) editor: Option<Entity<CodeEditor>>,

    // Paleta de comandos (Command Palette)
    pub(crate) show_command_palette: bool,

    // Configuración de la aplicación
    pub(crate) show_settings: bool,
    pub(crate) settings_tab: SettingsTab,
    pub(crate) settings_shortcuts_search: Entity<TextInput>,
    pub(crate) _settings_sub: Subscription,

    // Espacio de trabajo multi-proyecto y pestañas
    pub(crate) workspace: Workspace,
    pub(crate) project_rename_input: Entity<TextInput>,
    pub(crate) _project_rename_sub: Subscription,
    pub(crate) renaming_project_id: Option<String>,

    // Foco global para captura de atajos
    pub(crate) focus_handle: FocusHandle,

    // Notificaciones / Toast
    pub(crate) status_toast: Option<SharedString>,
}

impl DiffView {
    /// Inicializa la vista con la lista de rutas dadas.
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
            |_, _, event: &InputEvent, cx| {
                if let InputEvent::Changed = event {
                    cx.notify();
                }
            },
        );

        let project_rename_input =
            cx.new(|cx| TextInput::new(window, cx).placeholder("Nombre del proyecto…"));

        let project_rename_sub =
            cx.subscribe(&project_rename_input, |_, _, event: &InputEvent, cx| {
                if let InputEvent::Changed = event {
                    cx.notify();
                }
            });

        let mut workspace =
            Workspace::load_from_disk().unwrap_or_else(|_| Workspace::default_initial());

        let initial_paths = if !paths.is_empty() {
            if let Some(proj) = workspace.active_project_mut() {
                proj.paths = paths.clone();
            }
            paths
        } else if let Some(proj) = workspace.active_project() {
            proj.paths.clone()
        } else {
            Vec::new()
        };

        let initial_only_changed = workspace
            .active_project()
            .map(|p| p.only_changed)
            .unwrap_or(true);
        let initial_selector = workspace
            .active_project()
            .map(|p| p.selector.clone())
            .unwrap_or_else(|| "$".to_string());
        let initial_diff_mode = workspace
            .active_project()
            .map(|p| p.diff_mode)
            .unwrap_or(DiffMode::FixedBase);
        let initial_base_index = workspace
            .active_project()
            .map(|p| p.base_index)
            .unwrap_or(0);

        if initial_selector != "$" {
            selector_input.update(cx, |input, cx| {
                input.set_text(&initial_selector, cx);
            });
        }

        let mut this = Self {
            paths: initial_paths,
            comparison: None,
            error: None,
            view_mode: ViewMode::Table,
            density: Density::Compact,
            only_changed: initial_only_changed,
            search_query: String::new(),
            search_input,
            _search_sub: search_sub,
            visible: Vec::new(),
            selector_input,
            _selector_sub: selector_sub,
            git_target_idx: 1,
            git_layout: DiffLayout::Split,
            git_open_stretches: Vec::new(),
            editor_file_idx: 0,
            editor: None,
            show_command_palette: false,
            show_settings: false,
            settings_tab: SettingsTab::General,
            settings_shortcuts_search,
            _settings_sub: settings_sub,
            workspace,
            project_rename_input,
            _project_rename_sub: project_rename_sub,
            renaming_project_id: None,
            focus_handle: cx.focus_handle(),
            status_toast: None,
        };

        this.reload();
        if let Some(c) = &mut this.comparison {
            c.set_diff_mode(initial_diff_mode);
            let _ = c.set_selector(&initial_selector);
            let _ = c.set_base_index(initial_base_index);
            this.refilter();
        }
        this
    }

    /// Recarga los archivos del disco y recalcula la comparación.
    pub(crate) fn reload(&mut self) {
        self.error = None;

        if self.paths.is_empty() {
            self.comparison = None;
            self.visible.clear();
            return;
        }

        if self.paths.len() < MIN_FILES {
            self.error = Some(
                format!("Se necesitan al menos {MIN_FILES} archivos para comparar (hay {}). Arrastra más archivos o usa «Abrir archivos…».", self.paths.len()).into(),
            );
            self.comparison = None;
            self.visible.clear();
            return;
        }

        if self.paths.len() > MAX_FILES {
            self.error = Some(
                format!(
                    "Máximo {MAX_FILES} archivos soportados (hay {}).",
                    self.paths.len()
                )
                .into(),
            );
            self.comparison = None;
            self.visible.clear();
            return;
        }

        let base_idx = self.comparison.as_ref().map(|c| c.base_index).unwrap_or(0);
        let diff_mode = self
            .comparison
            .as_ref()
            .map(|c| c.diff_mode)
            .unwrap_or_default();
        let current_selector = self
            .comparison
            .as_ref()
            .map(|c| c.selector.clone())
            .unwrap_or_else(|| "$".to_string());

        match compare_paths_with_base(&self.paths, base_idx) {
            Ok(mut c) => {
                c.set_diff_mode(diff_mode);
                let _ = c.set_selector(&current_selector);
                self.comparison = Some(c);
                self.refilter();
            }
            Err(e) => {
                self.error = Some(e.into());
                self.comparison = None;
                self.visible.clear();
            }
        }
    }

    /// Recalcula los índices de filas visibles según el filtro de cambios y la búsqueda.
    pub(crate) fn refilter(&mut self) {
        if let Some(c) = &self.comparison {
            self.visible = c
                .rows
                .iter()
                .enumerate()
                .filter(|(_, r)| {
                    if self.only_changed && !r.changed {
                        return false;
                    }
                    if !self.search_query.is_empty() && !matches_filter(r, &self.search_query) {
                        return false;
                    }
                    true
                })
                .map(|(i, _)| i)
                .collect();
        } else {
            self.visible.clear();
        }
    }

    /// Actualiza la lista de archivos abiertos y recarga la comparación.
    pub(crate) fn set_paths(&mut self, mut paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        paths.truncate(MAX_FILES);
        self.paths = paths;
        self.reload();
        self.sync_current_project_state();
        cx.notify();
    }

    /// Agrega archivos adicionales a la lista existente sin duplicados y respetando el límite máximo.
    pub(crate) fn add_paths(&mut self, new_paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        let mut added = 0;
        let prev_len = self.paths.len();
        for p in new_paths {
            if !self.paths.contains(&p) {
                if self.paths.len() < MAX_FILES {
                    self.paths.push(p);
                    added += 1;
                } else {
                    self.show_toast(
                        format!("Límite alcanzado: máximo {MAX_FILES} archivos."),
                        cx,
                    );
                    break;
                }
            }
        }
        if added > 0 {
            self.reload();
            self.sync_current_project_state();
            self.show_toast(
                format!(
                    "{added} archivo(s) agregado(s). Total: {} de {MAX_FILES}",
                    self.paths.len()
                ),
                cx,
            );
            cx.notify();
        } else if prev_len > 0 {
            self.show_toast("Los archivos seleccionados ya estaban cargados", cx);
        }
    }

    /// Abre el diálogo nativo para seleccionar y REEMPLAZAR los archivos cargados.
    pub(crate) fn pick_files(&mut self, cx: &mut Context<Self>) {
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

    /// Abre el diálogo nativo para seleccionar y AGREGAR archivos a la comparación actual.
    pub(crate) fn pick_more_files(&mut self, cx: &mut Context<Self>) {
        let available = MAX_FILES.saturating_sub(self.paths.len());
        if available == 0 {
            self.show_toast(
                format!("Ya se alcanzó el límite máximo de {MAX_FILES} archivos"),
                cx,
            );
            return;
        }

        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some(
                format!("Selecciona hasta {available} archivo(s) JSON para agregar").into(),
            ),
        });
        cx.spawn(async move |this, cx| {
            let res = rx.await;
            if let Ok(Ok(Some(paths))) = res {
                if !paths.is_empty() {
                    this.update(cx, |this, cx| this.add_paths(paths, cx)).ok();
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
                        "--title=Selecciona hasta {available} archivo(s) JSON para agregar"
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
                            this.update(cx, |this, cx| this.add_paths(paths, cx)).ok();
                        }
                    }
                }
            }
        })
        .detach();
    }

    /// Mueve un archivo de una columna a otra en la comparación interactiva.
    pub(crate) fn move_file(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        if let Some(c) = &mut self.comparison {
            if c.move_file(from, to).is_ok() {
                if from < self.paths.len() && to < self.paths.len() {
                    let item = self.paths.remove(from);
                    self.paths.insert(to, item);
                }
                self.refilter();
                self.sync_current_project_state();
                self.show_toast(format!("Archivo movido a la posición [{}]", to + 1), cx);
                cx.notify();
            }
        }
    }

    /// Limpia todos los archivos cargados y regresa a la pantalla de bienvenida.
    pub(crate) fn clear_files(&mut self, cx: &mut Context<Self>) {
        self.paths.clear();
        self.comparison = None;
        self.visible.clear();
        self.error = None;
        self.editor = None;
        self.search_query.clear();
        self.search_input.update(cx, |input, cx| {
            input.set_text("", cx);
        });
        self.sync_current_project_state();
        self.show_toast("Archivos limpiados", cx);
        cx.notify();
    }

    /// Quita un archivo específico de la lista de comparación.
    pub(crate) fn remove_file(&mut self, idx: usize, cx: &mut Context<Self>) {
        if idx < self.paths.len() {
            let removed = self.paths.remove(idx);
            let name = removed
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| format!("archivo {}", idx + 1));
            self.reload();
            self.sync_current_project_state();
            self.show_toast(format!("Descartado: {name}"), cx);
            cx.notify();
        }
    }

    /// Sincroniza el estado visual actual con el proyecto activo en el Workspace.
    pub(crate) fn sync_current_project_state(&mut self) {
        if let Some(proj) = self.workspace.active_project_mut() {
            proj.paths = self.paths.clone();
            proj.only_changed = self.only_changed;
            proj.selector = self
                .comparison
                .as_ref()
                .map(|c| c.selector.clone())
                .unwrap_or_else(|| "$".to_string());
            if let Some(c) = &self.comparison {
                proj.diff_mode = c.diff_mode;
                proj.base_index = c.base_index;
            }
            proj.touch();
        }
        let _ = self.workspace.save_to_disk();
    }

    /// Carga el proyecto activo del Workspace en la vista.
    pub(crate) fn load_active_project(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(proj) = self.workspace.active_project() {
            self.paths = proj.paths.clone();
            self.only_changed = proj.only_changed;
            let sel = proj.selector.clone();
            let mode = proj.diff_mode;
            let base = proj.base_index;

            self.selector_input.update(cx, |input, cx| {
                input.set_text(if sel == "$" { "" } else { &sel }, cx);
            });

            self.editor = None;
            self.reload();

            if let Some(c) = &mut self.comparison {
                c.set_diff_mode(mode);
                let _ = c.set_selector(&sel);
                let _ = c.set_base_index(base);
                self.refilter();
            }
        }
        cx.notify();
    }

    /// Cambia a otra pestaña de proyecto por su ID.
    pub(crate) fn switch_project_tab(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace.active_project_id.as_deref() == Some(id) {
            return;
        }
        self.sync_current_project_state();
        if self.workspace.select_tab(id) {
            self.load_active_project(window, cx);
            if let Some(proj) = self.workspace.active_project() {
                self.show_toast(format!("Pestaña: {}", proj.name), cx);
            }
        }
    }

    /// Crea una nueva pestaña de proyecto y la activa.
    pub(crate) fn new_project_tab(
        &mut self,
        name: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_current_project_state();
        let proj_name = {
            let p = self.workspace.new_tab(name);
            p.name.clone()
        };
        self.load_active_project(window, cx);
        self.show_toast(format!("Nuevo proyecto: {proj_name}"), cx);
    }

    /// Cierra una pestaña de proyecto.
    pub(crate) fn close_project_tab(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_current_project_state();
        if let Some(removed) = self.workspace.close_tab(id) {
            self.load_active_project(window, cx);
            self.show_toast(format!("Pestaña cerrada: {}", removed.name), cx);
        }
    }

    /// Duplica el proyecto activo actual creando una copia en nueva pestaña.
    pub(crate) fn duplicate_current_project(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_current_project_state();
        if let Some(active_id) = self.workspace.active_project_id.clone() {
            if self.workspace.duplicate_project(&active_id).is_some() {
                self.load_active_project(window, cx);
                self.show_toast("Proyecto duplicado en nueva pestaña", cx);
            }
        }
    }

    /// Abre el diálogo para renombrar un proyecto.
    pub(crate) fn open_rename_dialog(
        &mut self,
        proj_id: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(proj) = self.workspace.projects.iter().find(|p| p.id == proj_id) {
            let name = proj.name.clone();
            self.project_rename_input.update(cx, |input, cx| {
                input.set_text(&name, cx);
            });
            self.renaming_project_id = Some(proj_id.to_string());
            cx.notify();
        }
    }

    /// Confirma el nuevo nombre del proyecto que se está renombrando.
    pub(crate) fn confirm_rename_project(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.renaming_project_id.take() {
            let new_name = self.project_rename_input.read(cx).text().trim().to_string();
            if !new_name.is_empty() {
                self.workspace.rename_project(&id, new_name.clone());
                let _ = self.workspace.save_to_disk();
                self.show_toast(format!("Proyecto renombrado a: {new_name}"), cx);
                cx.notify();
            }
        }
    }

    /// Guarda el proyecto activo en un archivo JSON en disco (Exportar proyecto).
    pub(crate) fn save_project_to_file(&mut self, cx: &mut Context<Self>) {
        self.sync_current_project_state();
        let active = match self.workspace.active_project() {
            Some(p) => p.clone(),
            None => return,
        };
        let suggested = format!("{}.jsondiff.json", active.name.replace(' ', "_"));
        let default_path = PathBuf::from(&suggested);
        let rx = cx.prompt_for_new_path(&default_path, None);
        cx.spawn(async move |this, cx| {
            let res = rx.await;
            if let Ok(Ok(Some(path))) = res {
                if let Err(e) = active.save_to_file(&path) {
                    this.update(cx, |this, cx| {
                        this.show_toast(format!("Error al guardar proyecto: {e}"), cx);
                    })
                    .ok();
                } else {
                    this.update(cx, |this, cx| {
                        this.show_toast(format!("Proyecto guardado en: {}", path.display()), cx);
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    /// Carga un proyecto desde un archivo JSON (Importar proyecto).
    pub(crate) fn load_project_from_file(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Selecciona un archivo de proyecto JSON (*.json)".into()),
        });
        cx.spawn(async move |this, cx| {
            let res = rx.await;
            if let Ok(Ok(Some(paths))) = res {
                if let Some(first) = paths.first() {
                    match Project::load_from_file(first) {
                        Ok(proj) => {
                            this.update(cx, |this, cx| {
                                let name = proj.name.clone();
                                this.workspace.add_project(proj);
                                this.paths = this
                                    .workspace
                                    .active_project()
                                    .map(|p| p.paths.clone())
                                    .unwrap_or_default();
                                this.reload();
                                this.show_toast(format!("Proyecto importado: {name}"), cx);
                                cx.notify();
                            })
                            .ok();
                        }
                        Err(e) => {
                            this.update(cx, |this, cx| {
                                this.show_toast(format!("Error al importar proyecto: {e}"), cx);
                            })
                            .ok();
                        }
                    }
                }
            }
        })
        .detach();
    }

    /// Exporta todo el workspace a un archivo JSON de respaldo.
    pub(crate) fn export_workspace_dialog(&mut self, cx: &mut Context<Self>) {
        self.sync_current_project_state();
        let ws = self.workspace.clone();
        let default_path = PathBuf::from("workspace.jsondiff.json");
        let rx = cx.prompt_for_new_path(&default_path, None);
        cx.spawn(async move |this, cx| {
            let res = rx.await;
            if let Ok(Ok(Some(path))) = res {
                if let Err(e) = ws.export_to_file(&path) {
                    this.update(cx, |this, cx| {
                        this.show_toast(format!("Error al exportar workspace: {e}"), cx);
                    })
                    .ok();
                } else {
                    this.update(cx, |this, cx| {
                        this.show_toast("Espacio de trabajo exportado con éxito", cx);
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    /// Importa todo el workspace desde un archivo JSON.
    pub(crate) fn import_workspace_dialog(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Selecciona un archivo de espacio de trabajo (*.json)".into()),
        });
        cx.spawn(async move |this, cx| {
            let res = rx.await;
            if let Ok(Ok(Some(paths))) = res {
                if let Some(first) = paths.first() {
                    match Workspace::import_from_file(first) {
                        Ok(ws) => {
                            this.update(cx, |this, cx| {
                                this.workspace = ws;
                                this.paths = this
                                    .workspace
                                    .active_project()
                                    .map(|p| p.paths.clone())
                                    .unwrap_or_default();
                                this.reload();
                                this.show_toast("Espacio de trabajo importado con éxito", cx);
                                cx.notify();
                            })
                            .ok();
                        }
                        Err(e) => {
                            this.update(cx, |this, cx| {
                                this.show_toast(format!("Error al importar workspace: {e}"), cx);
                            })
                            .ok();
                        }
                    }
                }
            }
        })
        .detach();
    }

    /// Muestra un mensaje flotante (toast) con auto-ocultamiento.
    pub(crate) fn show_toast(&mut self, msg: impl Into<SharedString>, cx: &mut Context<Self>) {
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
                let marks = components::editor_view::compute_git_marks(&base_content, &content);
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
                            let marks =
                                components::editor_view::compute_git_marks(base_content, &text);
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
    }
}

/// Punto de entrada principal para lanzar la ventana de la aplicación GPUI.
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
