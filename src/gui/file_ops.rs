//! Operaciones de archivos: recarga, filtrado, adición, diálogos del sistema y reordenamiento.

use crate::gui::DiffView;
use gpui::{Context, PathPromptOptions};
use jsondiff::{compare_paths_with_base, matches_filter_normalized, MAX_FILES, MIN_FILES};
use std::path::PathBuf;
use std::sync::Arc;

impl DiffView {
    /// Recarga los archivos del disco y recalcula la comparación.
    pub(crate) fn reload(&mut self) {
        self.error = None;

        if self.paths.is_empty() {
            self.comparison = None;
            self.visible.clear();
            self.sync_render_cache();
            return;
        }

        if self.paths.len() < MIN_FILES {
            self.error = Some(
                format!(
                    "Se necesitan al menos {MIN_FILES} archivos para comparar (hay {}). Arrastra más archivos o usa «Abrir archivos…».",
                    self.paths.len()
                )
                .into(),
            );
            self.comparison = None;
            self.visible.clear();
            self.sync_render_cache();
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
            self.sync_render_cache();
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
                self.sync_render_cache();
            }
        }
    }

    /// Sincroniza la caché compartida `Arc` para el render virtualizado de alta velocidad.
    #[inline]
    pub(crate) fn sync_render_cache(&mut self) {
        if let Some(c) = &self.comparison {
            self.cached_rows = Arc::from(c.rows.as_slice());
        } else {
            self.cached_rows = Arc::from([]);
        }
        self.cached_visible = Arc::from(self.visible.as_slice());
    }

    /// Recalcula los índices de filas visibles según el filtro de cambios y la búsqueda (zero-allocation).
    pub(crate) fn refilter(&mut self) {
        if let Some(c) = &self.comparison {
            let q_norm = self.search_query.trim().to_lowercase();
            let has_query = !q_norm.is_empty();
            let only_changed = self.only_changed;

            self.visible = c
                .rows
                .iter()
                .enumerate()
                .filter(|(_, r)| {
                    if only_changed && !r.changed {
                        return false;
                    }
                    if has_query && !matches_filter_normalized(r, &q_norm) {
                        return false;
                    }
                    true
                })
                .map(|(i, _)| i)
                .collect();
        } else {
            self.visible.clear();
        }
        self.sync_render_cache();
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
        self.sync_render_cache();
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
}
