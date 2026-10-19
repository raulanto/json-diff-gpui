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
}
