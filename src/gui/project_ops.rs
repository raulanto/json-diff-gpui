//! Operaciones de proyectos, espacio de trabajo (Workspace), pestañas y persistencia atómica.

use crate::gui::DiffView;
use gpui::{Context, PathPromptOptions, Window};
use jsondiff::{Project, Workspace};
use std::path::PathBuf;

impl DiffView {
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
            if self.view_mode == crate::gui::types::ViewMode::Home {
                self.view_mode = crate::gui::types::ViewMode::Table;
                cx.notify();
            }
            return;
        }
        self.sync_current_project_state();
        if self.workspace.select_tab(id) {
            self.load_active_project(window, cx);
            if self.view_mode == crate::gui::types::ViewMode::Home {
                self.view_mode = crate::gui::types::ViewMode::Table;
            }
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
        self.view_mode = crate::gui::types::ViewMode::Table;
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

    /// Reordena una pestaña de proyecto arrastrada.
    pub(crate) fn reorder_project_tab(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        if self.workspace.reorder_tabs(from, to) {
            let _ = self.workspace.save_to_disk();
            cx.notify();
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

    /// Guarda el proyecto activo actual como un bundle con todos sus archivos JSON en disco.
    pub(crate) fn save_current_project_bundle(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_current_project_state();
        let projects_dir = self.workspace.projects_dir();

        let files_content: Vec<(String, String)> = if let Some(c) = &self.comparison {
            c.files
                .iter()
                .map(|f| (f.name.clone(), f.content.clone()))
                .collect()
        } else {
            self.paths
                .iter()
                .filter_map(|p| {
                    let name = p
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("archivo.json")
                        .to_string();
                    std::fs::read_to_string(p)
                        .ok()
                        .map(|content| (name, content))
                })
                .collect()
        };

        if files_content.is_empty() {
            self.show_toast(
                "El proyecto no tiene archivos JSON para guardar en disco",
                cx,
            );
            return;
        }

        let Some(active) = self.workspace.active_project_mut() else {
            return;
        };

        match active.save_bundle(&projects_dir, &files_content) {
            Ok(saved_path) => {
                let name = active.name.clone();
                let count = files_content.len();
                let _ = self.workspace.save_to_disk();
                self.refresh_discovered_bundles();
                self.show_toast(
                    format!(
                        "Proyecto '{}' guardado con {count} archivos JSON en: {}",
                        name,
                        saved_path.display()
                    ),
                    cx,
                );
                cx.notify();
            }
            Err(e) => {
                self.show_toast(format!("Error al guardar bundle del proyecto: {e}"), cx);
            }
        }
    }

    /// Permite al usuario seleccionar una nueva carpeta raíz para almacenar los proyectos.
    pub(crate) fn choose_projects_dir(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Selecciona la carpeta raíz donde guardar tus proyectos JSON".into()),
        });

        cx.spawn(async move |this, cx| {
            let res = rx.await;
            if let Ok(Ok(Some(paths))) = res {
                if let Some(first) = paths.first() {
                    let chosen = first.clone();
                    this.update(cx, |this, cx| {
                        this.workspace.set_projects_dir(Some(chosen.clone()));
                        let _ = this.workspace.save_to_disk();
                        this.refresh_discovered_bundles();
                        this.show_toast(
                            format!("Ubicación de proyectos actualizada: {}", chosen.display()),
                            cx,
                        );
                        cx.notify();
                    })
                    .ok();
                    return;
                }
            }

            #[cfg(target_os = "linux")]
            {
                if let Ok(out) = std::process::Command::new("zenity")
                    .args([
                        "--file-selection",
                        "--directory",
                        "--title=Selecciona la carpeta raíz de proyectos",
                    ])
                    .output()
                {
                    if out.status.success() {
                        let stdout = String::from_utf8_lossy(&out.stdout);
                        let trimmed = stdout.trim();
                        if !trimmed.is_empty() {
                            let chosen = PathBuf::from(trimmed);
                            this.update(cx, |this, cx| {
                                this.workspace.set_projects_dir(Some(chosen.clone()));
                                let _ = this.workspace.save_to_disk();
                                this.refresh_discovered_bundles();
                                this.show_toast(
                                    format!(
                                        "Ubicación de proyectos actualizada: {}",
                                        chosen.display()
                                    ),
                                    cx,
                                );
                                cx.notify();
                            })
                            .ok();
                        }
                    }
                }
            }
        })
        .detach();
    }

    /// Restablece la carpeta de proyectos a la ubicación por defecto del sistema.
    pub(crate) fn reset_projects_dir(&mut self, cx: &mut Context<Self>) {
        self.workspace.set_projects_dir(None);
        let _ = self.workspace.save_to_disk();
        self.refresh_discovered_bundles();
        let default_dir = self.workspace.projects_dir();
        self.show_toast(
            format!(
                "Carpeta de proyectos restablecida a la predeterminada: {}",
                default_dir.display()
            ),
            cx,
        );
        cx.notify();
    }

    /// Abre un proyecto desde un bundle por su nombre de proyecto o carpeta.
    pub(crate) fn open_project_bundle_by_name(
        &mut self,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let bundle_info = self
            .discovered_bundles
            .iter()
            .find(|b| {
                b.name == name || b.folder_path.file_name().and_then(|n| n.to_str()) == Some(name)
            })
            .cloned();

        let Some(bundle) = bundle_info else {
            self.show_toast(format!("No se encontró el proyecto '{name}'"), cx);
            return;
        };

        match jsondiff::Project::load_bundle(&bundle.folder_path) {
            Ok(proj) => {
                let proj_name = proj.name.clone();
                let file_count = proj.paths.len();
                self.sync_current_project_state();
                self.workspace.add_project(proj);
                self.load_active_project(window, cx);
                self.view_mode = crate::gui::types::ViewMode::Table;
                self.show_toast(
                    format!("Proyecto '{proj_name}' abierto con {file_count} archivos JSON"),
                    cx,
                );
                cx.notify();
            }
            Err(e) => {
                self.show_toast(format!("Error al abrir proyecto: {e}"), cx);
            }
        }
    }

    /// Abre un proyecto a partir de una ruta seleccionada en el FileTree (ej. "Proyecto 1/v1.json").
    pub(crate) fn open_from_file_tree_path(
        &mut self,
        path_str: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let parts: Vec<&str> = path_str.split('/').collect();
        let project_name = parts.first().copied().unwrap_or(path_str);
        self.open_project_bundle_by_name(project_name, window, cx);
    }
}
