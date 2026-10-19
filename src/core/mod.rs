//! Módulo núcleo (Core) de la librería `jsondiff`.
//!
//! Contiene toda la lógica de dominio, aplanado de JSON, selectores JSONPath,
//! motores de comparación matricial, exportación de reportes y gestión de proyectos
//! sin ninguna dependencia de la interfaz gráfica.

pub mod comparison;
pub mod flatten;
pub mod loader;
pub mod project;
pub mod report;
pub mod selector;
pub mod types;

#[cfg(test)]
mod tests;

pub use comparison::*;
pub use flatten::*;
pub use loader::*;
pub use project::*;
pub use report::*;
pub use selector::*;
pub use types::*;
