//! Atajos de teclado y despacho de acciones de la paleta de comandos.

use crate::gui::types::{Density, ViewMode};
use crate::gui::DiffView;
use ely_gpui_component::{
    settings::Shortcut,
    theme::{ActiveTheme, Mode, Theme},
};
use gpui::{ClipboardItem, Context, KeyDownEvent, Window};
use jsondiff::{export_markdown, DiffMode};

/// Devuelve la lista completa y descriptiva de atajos de teclado globales.
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
            name: "Abrir y reemplazar archivos JSON".into(),
            keys: "secondary-o".into(),
        },
        Shortcut {
            group: "General & Navegación".into(),
            name: "Agregar archivos JSON a la comparación".into(),
            keys: "secondary-shift-o".into(),
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
            group: "Visualización & Densidad".into(),
            name: "Alternar densidad: Compacta / Cómoda".into(),
            keys: "secondary-d".into(),
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

/// Procesa las acciones disparadas desde la paleta de comandos interactiva.
pub fn run_command_action(
    state: &mut DiffView,
    action: &str,
    window: &mut Window,
    cx: &mut Context<DiffView>,
) {
    state.show_command_palette = false;

    match action {
        "view_table" => {
            state.view_mode = ViewMode::Table;
            state.editor = None;
            cx.notify();
        }
        "view_git" => {
            if state.comparison.is_some() {
                state.view_mode = ViewMode::GitDiff;
                state.editor = None;
                cx.notify();
            }
        }
        "view_editor" => {
            if state.comparison.is_some() {
                state.view_mode = ViewMode::Editor;
                state.init_editor_if_needed(window, cx);
                cx.notify();
            }
        }
        "open_files" => {
            state.pick_files(cx);
        }
        "add_more_files" => {
            state.pick_more_files(cx);
        }
        "clear_files" => {
            state.clear_files(cx);
        }
        "export_markdown" => {
            if let Some(c) = &state.comparison {
                let md = export_markdown(c, state.only_changed);
                cx.write_to_clipboard(ClipboardItem::new_string(md));
                state.show_toast("Reporte Markdown copiado al portapapeles", cx);
            }
        }
        "toggle_diff_mode" => {
            if let Some(c) = &mut state.comparison {
                let next = match c.diff_mode {
                    DiffMode::FixedBase => DiffMode::Sequential,
                    DiffMode::Sequential => DiffMode::FixedBase,
                };
                c.set_diff_mode(next);
                state.refilter();
                state.show_toast(
                    if next == DiffMode::Sequential {
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
            state.only_changed = !state.only_changed;
            state.refilter();
            state.show_toast(
                if state.only_changed {
                    "Filtro: Mostrando solo campos con cambios"
                } else {
                    "Filtro: Mostrando todos los campos"
                },
                cx,
            );
            cx.notify();
        }
        "toggle_density" => {
            state.toggle_density(cx);
        }
        "density_compact" => {
            state.density = Density::Compact;
            state.show_toast("Densidad: Modo compacto", cx);
            cx.notify();
        }
        "density_comfortable" => {
            state.density = Density::Comfortable;
            state.show_toast("Densidad: Modo cómodo", cx);
            cx.notify();
        }
        "reset_selector" => {
            state.reset_selector(cx);
        }
        "format_json" => {
            if state.view_mode == ViewMode::Editor {
                state.format_editor_json(cx);
            }
        }
        "validate_json" => {
            if state.view_mode == ViewMode::Editor {
                state.validate_editor_json(cx);
            }
        }
        "apply_changes" => {
            if state.view_mode == ViewMode::Editor {
                state.apply_editor_changes(cx);
            }
        }
        "save_file" => {
            if state.view_mode == ViewMode::Editor {
                state.save_editor_to_disk(cx);
            }
        }
        "open_settings" => {
            state.show_settings = true;
            state.show_command_palette = false;
            cx.notify();
        }
        "toggle_theme" => {
            let next = if cx.theme().is_dark() {
                Mode::Light
            } else {
                Mode::Dark
            };
            Theme::set_mode(next, cx);
            cx.notify();
        }
        _ => {}
    }
}

/// Manejador global de eventos de teclado (key-down) para la aplicación.
pub fn handle_global_key(
    state: &mut DiffView,
    event: &KeyDownEvent,
    window: &mut Window,
    cx: &mut Context<DiffView>,
) -> bool {
    let key = event.keystroke.key.as_str();
    let ctrl_or_cmd = event.keystroke.modifiers.control || event.keystroke.modifiers.platform;
    let shift = event.keystroke.modifiers.shift;

    if key.eq_ignore_ascii_case("escape") {
        if state.show_command_palette || state.show_settings {
            state.show_command_palette = false;
            state.show_settings = false;
            cx.notify();
            return true;
        }
    }

    if ctrl_or_cmd && (key.eq_ignore_ascii_case("k") || key.eq_ignore_ascii_case("p")) {
        state.show_command_palette = !state.show_command_palette;
        state.show_settings = false;
        cx.notify();
        return true;
    }

    if (ctrl_or_cmd && key == ",") || key.eq_ignore_ascii_case("f2") {
        state.show_settings = !state.show_settings;
        state.show_command_palette = false;
        cx.notify();
        return true;
    }

    if ctrl_or_cmd && shift && key.eq_ignore_ascii_case("o") {
        state.pick_more_files(cx);
        return true;
    }

    if ctrl_or_cmd && !shift && key.eq_ignore_ascii_case("o") {
        state.pick_files(cx);
        return true;
    }

    if ctrl_or_cmd && shift && key.eq_ignore_ascii_case("l") {
        state.clear_files(cx);
        return true;
    }

    if ctrl_or_cmd && shift && key.eq_ignore_ascii_case("c") {
        state.only_changed = !state.only_changed;
        state.refilter();
        state.show_toast(
            if state.only_changed {
                "Filtro: Mostrando solo campos con cambios"
            } else {
                "Filtro: Mostrando todos los campos"
            },
            cx,
        );
        cx.notify();
        return true;
    }

    if ctrl_or_cmd && key.eq_ignore_ascii_case("m") && !shift {
        if let Some(c) = &mut state.comparison {
            let next = match c.diff_mode {
                DiffMode::FixedBase => DiffMode::Sequential,
                DiffMode::Sequential => DiffMode::FixedBase,
            };
            c.set_diff_mode(next);
            state.refilter();
            state.show_toast(
                if next == DiffMode::Sequential {
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

    if ctrl_or_cmd && key.eq_ignore_ascii_case("d") && !shift {
        state.toggle_density(cx);
        return true;
    }

    if ctrl_or_cmd && shift && key.eq_ignore_ascii_case("m") {
        if let Some(c) = &state.comparison {
            let md = export_markdown(c, state.only_changed);
            cx.write_to_clipboard(ClipboardItem::new_string(md));
            state.show_toast("Reporte Markdown copiado al portapapeles", cx);
            return true;
        }
    }

    if ctrl_or_cmd && key == "1" {
        state.view_mode = ViewMode::Table;
        state.editor = None;
        cx.notify();
        return true;
    }

    if ctrl_or_cmd && key == "2" {
        if state.comparison.is_some() {
            state.view_mode = ViewMode::GitDiff;
            state.editor = None;
            cx.notify();
            return true;
        }
    }

    if ctrl_or_cmd && key == "3" {
        if state.comparison.is_some() {
            state.view_mode = ViewMode::Editor;
            state.init_editor_if_needed(window, cx);
            cx.notify();
            return true;
        }
    }

    if ctrl_or_cmd && key.eq_ignore_ascii_case("t") && !shift {
        let next = if cx.theme().is_dark() {
            Mode::Light
        } else {
            Mode::Dark
        };
        Theme::set_mode(next, cx);
        cx.notify();
        return true;
    }

    if state.view_mode == ViewMode::Editor {
        if ctrl_or_cmd && shift && key.eq_ignore_ascii_case("i") {
            state.format_editor_json(cx);
            return true;
        }

        if ctrl_or_cmd && key.eq_ignore_ascii_case("s") {
            state.save_editor_to_disk(cx);
            return true;
        }
    }

    false
}
