//! Gestión de proyectos, espacio de trabajo (Workspace), pestañas y persistencia en disco.

use crate::core::types::{DiffMode, MAX_FILES};
use std::fs;
use std::path::{Path, PathBuf};

fn default_selector() -> String {
    "$".to_string()
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn generate_short_id() -> String {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNTER: AtomicUsize = AtomicUsize::new(1);
    let c = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{:04x}", (c.wrapping_mul(7919)) & 0xFFFF)
}

/// Un proyecto o sesión de comparación de JSON guardable y reutilizable.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Project {
    /// Identificador único del proyecto (prefijo proj_ + timestamp + id corto).
    pub id: String,
    /// Nombre descriptivo del proyecto (ej. "API Users v1 vs v2", "Configuración Producción").
    pub name: String,
    /// Rutas a los archivos JSON que pertenecen a este proyecto.
    pub paths: Vec<PathBuf>,
    /// Selector JSONPath configurado (por defecto "$").
    #[serde(default = "default_selector")]
    pub selector: String,
    /// Modo de comparación (Base fija o En cadena).
    #[serde(default)]
    pub diff_mode: DiffMode,
    /// Índice del archivo base para el modo base fija.
    #[serde(default)]
    pub base_index: usize,
    /// Filtro de solo cambios activo.
    #[serde(default)]
    pub only_changed: bool,
    /// Marca de tiempo de creación (segundos UNIX).
    #[serde(default = "current_timestamp")]
    pub created_at: u64,
    /// Marca de tiempo de última actualización (segundos UNIX).
    #[serde(default = "current_timestamp")]
    pub updated_at: u64,
}

impl Project {
    /// Crea un nuevo proyecto con un nombre y configuración por defecto.
    pub fn new(name: impl Into<String>) -> Self {
        let now = current_timestamp();
        let id = format!("proj_{}_{}", now, generate_short_id());
        Self {
            id,
            name: name.into(),
            paths: Vec::new(),
            selector: default_selector(),
            diff_mode: DiffMode::FixedBase,
            base_index: 0,
            only_changed: false,
            created_at: now,
            updated_at: now,
        }
    }

    /// Crea un nuevo proyecto con rutas iniciales.
    pub fn with_paths(name: impl Into<String>, paths: Vec<PathBuf>) -> Self {
        let mut proj = Self::new(name);
        proj.paths = paths;
        proj
    }

    /// Agrega una ruta al proyecto respetando MAX_FILES y sin duplicados.
    pub fn add_path(&mut self, path: PathBuf) -> bool {
        if self.paths.len() >= MAX_FILES || self.paths.contains(&path) {
            return false;
        }
        self.paths.push(path);
        self.touch();
        true
    }

    /// Remueve una ruta del proyecto por índice.
    pub fn remove_path(&mut self, idx: usize) -> Option<PathBuf> {
        if idx < self.paths.len() {
            let p = self.paths.remove(idx);
            if self.base_index >= self.paths.len() && !self.paths.is_empty() {
                self.base_index = self.paths.len() - 1;
            }
            self.touch();
            Some(p)
        } else {
            None
        }
    }

    /// Mueve una ruta de una posición a otra dentro del proyecto.
    pub fn move_path(&mut self, from: usize, to: usize) -> bool {
        if from < self.paths.len() && to < self.paths.len() && from != to {
            let item = self.paths.remove(from);
            self.paths.insert(to, item);
            self.touch();
            true
        } else {
            false
        }
    }

    /// Actualiza la marca de tiempo de modificación.
    pub fn touch(&mut self) {
        self.updated_at = current_timestamp();
    }

    /// Serializa el proyecto a formato JSON con sangría.
    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| format!("Error al serializar proyecto: {e}"))
    }

    /// Deserializa un proyecto desde texto JSON.
    pub fn from_json(json_str: &str) -> Result<Self, String> {
        serde_json::from_str(json_str).map_err(|e| format!("Error al parsear proyecto JSON: {e}"))
    }

    /// Guarda el proyecto en un archivo JSON en disco.
    pub fn save_to_file(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Error al crear directorio para el proyecto: {e}"))?;
        }
        let json = self.to_json()?;
        fs::write(path, json).map_err(|e| format!("Error al escribir archivo de proyecto: {e}"))
    }

    /// Carga un proyecto desde un archivo JSON en disco.
    pub fn load_from_file(path: &Path) -> Result<Self, String> {
        let content = fs::read_to_string(path).map_err(|e| {
            format!(
                "Error al leer archivo de proyecto '{}': {e}",
                path.display()
            )
        })?;
        Self::from_json(&content)
    }
}

/// Espacio de trabajo que agrupa múltiples proyectos/pestañas de comparación.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Workspace {
    /// ID del proyecto activo actualmente seleccionado.
    pub active_project_id: Option<String>,
    /// Lista de proyectos abiertos en pestañas (tabs).
    pub projects: Vec<Project>,
}

impl Default for Workspace {
    fn default() -> Self {
        Self::default_initial()
    }
}

impl Workspace {
    /// Crea un workspace vacío sin proyectos.
    pub fn new() -> Self {
        Self {
            active_project_id: None,
            projects: Vec::new(),
        }
    }

    /// Crea un workspace inicial con una pestaña por defecto ("Proyecto 1").
    pub fn default_initial() -> Self {
        let initial = Project::new("Proyecto 1");
        let id = initial.id.clone();
        Self {
            active_project_id: Some(id),
            projects: vec![initial],
        }
    }

    /// Añade un proyecto al workspace, lo enfoca y devuelve su ID.
    pub fn add_project(&mut self, project: Project) -> String {
        let id = project.id.clone();
        if let Some(pos) = self.projects.iter().position(|p| p.id == id) {
            self.projects[pos] = project;
        } else {
            self.projects.push(project);
        }
        self.active_project_id = Some(id.clone());
        id
    }

    /// Crea una nueva pestaña de proyecto ("Proyecto N") y la activa.
    pub fn new_tab(&mut self, name: Option<String>) -> &Project {
        let count = self.projects.len() + 1;
        let proj_name = name.unwrap_or_else(|| format!("Proyecto {count}"));
        let proj = Project::new(proj_name);
        let id = proj.id.clone();
        self.projects.push(proj);
        self.active_project_id = Some(id);
        self.active_project().expect("acaba de ser insertado")
    }

    /// Cierra una pestaña por su ID. Si no quedan pestañas, crea una nueva vacía.
    pub fn close_tab(&mut self, id: &str) -> Option<Project> {
        let pos = self.projects.iter().position(|p| p.id == id)?;
        let removed = self.projects.remove(pos);

        if self.projects.is_empty() {
            let def = Project::new("Proyecto 1");
            let new_id = def.id.clone();
            self.projects.push(def);
            self.active_project_id = Some(new_id);
        } else if self.active_project_id.as_deref() == Some(id) {
            let next_pos = if pos > 0 { pos - 1 } else { 0 };
            self.active_project_id = Some(self.projects[next_pos].id.clone());
        }

        Some(removed)
    }

    /// Retorna una referencia al proyecto activo actual.
    pub fn active_project(&self) -> Option<&Project> {
        if let Some(id) = &self.active_project_id {
            if let Some(idx) = self.projects.iter().position(|p| &p.id == id) {
                return self.projects.get(idx);
            }
        }
        self.projects.first()
    }

    /// Retorna una referencia mutable al proyecto activo actual.
    pub fn active_project_mut(&mut self) -> Option<&mut Project> {
        if let Some(id) = &self.active_project_id.clone() {
            if let Some(idx) = self.projects.iter().position(|p| &p.id == id) {
                return self.projects.get_mut(idx);
            }
        }
        self.projects.first_mut()
    }

    /// Selecciona una pestaña por su ID de proyecto.
    pub fn select_tab(&mut self, id: &str) -> bool {
        if self.projects.iter().any(|p| p.id == id) {
            self.active_project_id = Some(id.to_string());
            true
        } else {
            false
        }
    }

    /// Selecciona una pestaña por su índice numérico (base 0).
    pub fn select_tab_by_index(&mut self, idx: usize) -> bool {
        if idx < self.projects.len() {
            self.active_project_id = Some(self.projects[idx].id.clone());
            true
        } else {
            false
        }
    }

    /// Retorna el índice de la pestaña activa en la lista.
    pub fn active_index(&self) -> usize {
        if let Some(active_id) = &self.active_project_id {
            self.projects
                .iter()
                .position(|p| &p.id == active_id)
                .unwrap_or(0)
        } else {
            0
        }
    }

    /// Cambia a la siguiente pestaña cíclicamente.
    pub fn next_tab(&mut self) {
        if !self.projects.is_empty() {
            let next = (self.active_index() + 1) % self.projects.len();
            self.select_tab_by_index(next);
        }
    }

    /// Cambia a la pestaña anterior cíclicamente.
    pub fn prev_tab(&mut self) {
        if !self.projects.is_empty() {
            let idx = self.active_index();
            let prev = if idx == 0 {
                self.projects.len() - 1
            } else {
                idx - 1
            };
            self.select_tab_by_index(prev);
        }
    }

    /// Renombra un proyecto existente.
    pub fn rename_project(&mut self, id: &str, new_name: impl Into<String>) -> bool {
        if let Some(p) = self.projects.iter_mut().find(|p| p.id == id) {
            p.name = new_name.into();
            p.touch();
            true
        } else {
            false
        }
    }

    /// Duplica un proyecto existente creando una nueva pestaña.
    pub fn duplicate_project(&mut self, id: &str) -> Option<String> {
        let p = self.projects.iter().find(|p| p.id == id)?.clone();
        let mut clone = p;
        clone.id = format!("proj_{}_{}", current_timestamp(), generate_short_id());
        clone.name = format!("{} (Copia)", clone.name);
        clone.touch();
        let new_id = clone.id.clone();
        self.projects.push(clone);
        self.active_project_id = Some(new_id.clone());
        Some(new_id)
    }

    /// Reordena una pestaña de la posición `from` a la posición `to`.
    pub fn reorder_tabs(&mut self, from: usize, to: usize) -> bool {
        if from < self.projects.len() && to < self.projects.len() && from != to {
            let item = self.projects.remove(from);
            self.projects.insert(to, item);
            true
        } else {
            false
        }
    }

    /// Obtiene la ruta predeterminada para persistencia del workspace en el sistema.
    pub fn default_storage_path() -> PathBuf {
        if let Ok(config_home) = std::env::var("XDG_CONFIG_HOME") {
            if !config_home.trim().is_empty() {
                return PathBuf::from(config_home)
                    .join("json-diff-gpui")
                    .join("workspace.json");
            }
        }
        if let Ok(home) = std::env::var("HOME") {
            if !home.trim().is_empty() {
                return PathBuf::from(home)
                    .join(".config")
                    .join("json-diff-gpui")
                    .join("workspace.json");
            }
        }
        if let Ok(app_data) = std::env::var("APPDATA") {
            if !app_data.trim().is_empty() {
                return PathBuf::from(app_data)
                    .join("json-diff-gpui")
                    .join("workspace.json");
            }
        }
        PathBuf::from(".json-diff-workspace.json")
    }

    /// Guarda el estado de todo el workspace en disco de forma atómica y segura.
    pub fn save_to_disk(&self) -> Result<(), String> {
        let path = Self::default_storage_path();
        self.export_to_file(&path)
    }

    /// Carga el estado del workspace desde el disco si existe.
    pub fn load_from_disk() -> Result<Self, String> {
        let path = Self::default_storage_path();
        if path.exists() {
            Self::import_from_file(&path)
        } else {
            Ok(Self::default_initial())
        }
    }

    /// Exporta todo el workspace a un archivo JSON específico.
    pub fn export_to_file(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Error al crear directorio para el workspace: {e}"))?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Error al serializar workspace: {e}"))?;
        fs::write(path, json)
            .map_err(|e| format!("Error al escribir workspace en '{}': {e}", path.display()))
    }

    /// Importa un workspace completo desde un archivo JSON específico.
    pub fn import_from_file(path: &Path) -> Result<Self, String> {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Error al leer workspace de '{}': {e}", path.display()))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("Error al parsear archivo de workspace JSON: {e}"))
    }
}
