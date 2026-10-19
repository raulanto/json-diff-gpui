//! Núcleo de comparación: independiente de la interfaz.
//!
//! Cada archivo JSON se normaliza (indentación de 2 espacios, mismo orden de
//! claves) y se aplana a una lista de hojas `ruta -> (línea, valor)`.
//! Después se unen las rutas de todos los archivos y se marca como cambiada
//! cualquier ruta cuyo valor difiera entre archivos (o falte en alguno).

extern crate self as jsondiff;

pub mod core;

// Re-exportamos toda la API pública de `core` en la raíz del crate
// para mantener 100% de compatibilidad hacia atrás con el código existente.
pub use core::*;

#[cfg(feature = "gui")]
pub mod gui;
