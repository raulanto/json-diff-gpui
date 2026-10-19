//! Definición del estado principal de la interfaz gráfica y su inicializador.

use crate::gui::types::{Density, SettingsTab, ViewMode};
use ely_gpui_component::{
    editor::{CodeEditor, FindOptions},
    forms::{InputEvent, TextInput},
    git::DiffLayout,
};
use gpui::{prelude::*, Context, Entity, FocusHandle, SharedString, Subscription, Window};
use jsondiff::{Comparison, DiffMode, Workspace};
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

    // Búsqueda y reemplazo en el Editor (FindWidget)
    pub(crate) show_find_widget: bool,
    pub(crate) find_input: Entity<TextInput>,
    pub(crate) _find_sub: Subscription,
    pub(crate) replace_input: Entity<TextInput>,
    pub(crate) _replace_sub: Subscription,
    pub(crate) find_options: FindOptions,
    pub(crate) find_matches: Vec<std::ops::Range<usize>>,
    pub(crate) find_current_index: Option<usize>,
    pub(crate) show_replace: bool,

    // Previsualización y panel de JSON Schema
    pub(crate) show_schema_panel: bool,
    pub(crate) show_schema_dialog: bool,
    pub(crate) schema_editor: Option<Entity<CodeEditor>>,
    pub(crate) generated_schema_text: String,

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

    // Caché compartida de filas para renderizado virtualizado zero-copy (Arc)
    pub(crate) cached_rows: std::sync::Arc<[jsondiff::Row]>,
    pub(crate) cached_visible: std::sync::Arc<[usize]>,

    // Epochs para debouncing de entradas de texto reactivas
    pub(crate) selector_epoch: u64,
    pub(crate) search_epoch: u64,
}

impl DiffView {
    /// Inicializa la vista principal con la lista de rutas especificadas.
    pub fn new(paths: Vec<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search_input =
            cx.new(|cx| TextInput::new(window, cx).placeholder("Buscar por campo o valor…"));

        let search_sub = cx.subscribe(&search_input, |this, entity, event: &InputEvent, cx| {
            if let InputEvent::Changed = event {
                let query = entity.read(cx).text().to_string();
                this.search_query = query;
                this.search_epoch = this.search_epoch.wrapping_add(1);
                let epoch = this.search_epoch;

                if this.search_query.is_empty() {
                    this.refilter();
                    cx.notify();
                } else {
                    cx.spawn(async move |this, cx| {
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(100))
                            .await;
                        this.update(cx, |this, cx| {
                            if this.search_epoch == epoch {
                                this.refilter();
                                cx.notify();
                            }
                        })
                        .ok();
                    })
                    .detach();
                }
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
                        this.selector_epoch = this.selector_epoch.wrapping_add(1);
                        let epoch = this.selector_epoch;
                        cx.spawn(async move |this, cx| {
                            cx.background_executor()
                                .timer(std::time::Duration::from_millis(220))
                                .await;
                            this.update(cx, |this, cx| {
                                if this.selector_epoch == epoch {
                                    this.apply_selector(&text, false, cx);
                                }
                            })
                            .ok();
                        })
                        .detach();
                    }
                    InputEvent::Submit => {
                        this.selector_epoch = this.selector_epoch.wrapping_add(1);
                        let text = entity.read(cx).text().to_string();
                        this.apply_selector(&text, true, cx);
                    }
                    _ => {}
                },
            );

        let find_input =
            cx.new(|cx| TextInput::new(window, cx).placeholder("Buscar en el editor…"));
        let find_sub = cx.subscribe(&find_input, |this, _, event: &InputEvent, cx| match event {
            InputEvent::Changed => {
                this.update_find_matches(cx);
            }
            InputEvent::Submit => {
                this.step_find_match(true, cx);
            }
            _ => {}
        });

        let replace_input = cx.new(|cx| TextInput::new(window, cx).placeholder("Reemplazar por…"));
        let replace_sub = cx.subscribe(
            &replace_input,
            |this, _, event: &InputEvent, cx| match event {
                InputEvent::Changed => {
                    cx.notify();
                }
                InputEvent::Submit => {
                    this.replace_find_match(false, cx);
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

        let project_rename_sub = cx.subscribe(
            &project_rename_input,
            |this, _, event: &InputEvent, cx| match event {
                InputEvent::Changed => {
                    cx.notify();
                }
                InputEvent::Submit => {
                    if let Some(id) = this.renaming_project_id.take() {
                        let new_name = this.project_rename_input.read(cx).text().trim().to_string();
                        if !new_name.is_empty() {
                            this.workspace.rename_project(&id, new_name.clone());
                            let _ = this.workspace.save_to_disk();
                            this.show_toast(format!("Proyecto renombrado a: {new_name}"), cx);
                            cx.notify();
                        }
                    }
                }
                _ => {}
            },
        );

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
            show_find_widget: false,
            find_input,
            _find_sub: find_sub,
            replace_input,
            _replace_sub: replace_sub,
            find_options: FindOptions::default(),
            find_matches: Vec::new(),
            find_current_index: None,
            show_replace: false,
            show_schema_panel: true,
            show_schema_dialog: false,
            schema_editor: None,
            generated_schema_text: String::new(),
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
            cached_rows: std::sync::Arc::from([]),
            cached_visible: std::sync::Arc::from([]),
            selector_epoch: 0,
            search_epoch: 0,
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
}
