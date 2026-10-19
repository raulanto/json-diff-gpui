//! Tipos y enumeraciones para la interfaz gráfica.

/// Densidad de visualización para la tabla matricial de diferencias.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Density {
    #[default]
    Compact,
    Comfortable,
}

impl Density {
    #[inline]
    pub fn row_height(self) -> f32 {
        match self {
            Density::Compact => 26.0,
            Density::Comfortable => 36.0,
        }
    }

    #[inline]
    pub fn header_height(self) -> f32 {
        match self {
            Density::Compact => 32.0,
            Density::Comfortable => 42.0,
        }
    }

    #[inline]
    pub fn path_width(self) -> f32 {
        match self {
            Density::Compact => 260.0,
            Density::Comfortable => 320.0,
        }
    }
}

/// Modos de visualización integrados en la aplicación.
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
    Projects,
    Shortcuts,
}
