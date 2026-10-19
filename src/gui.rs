//! Interfaz gráfica principal (GPUI + Ely Components).
//!
//! Arquitectura modular desacoplada:
//! - `state`: struct `DiffView` y su constructor `new`.
//! - `file_ops`: carga, reload, agregación y selección de archivos.
//! - `project_ops`: sesiones de comparación, pestañas y persistencia de workspace.
//! - `editor_ops`: editor en vivo, validación, formateo y selectores JSONPath.
//! - `render`: orquestación de renderizado GPUI de vistas y modales.
//! - `shortcuts`: catálogo de atajos y listeners globales de teclado.
//! - `types`: enums y structs de configuración de UI.
//! - `components`: componentes visuales reutilizables.

pub mod components;
pub mod editor_ops;
pub mod file_ops;
pub mod project_ops;
pub mod render;
pub mod shortcuts;
pub mod state;
pub mod types;

pub use state::DiffView;
#[allow(unused_imports)]
pub use types::{Density, SettingsTab, ViewMode};

use ely_gpui_component::theme::{Mode, Theme};
use gpui::{prelude::*, px, size, App, Bounds, TitlebarOptions, WindowBounds, WindowOptions};
use std::path::PathBuf;

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
