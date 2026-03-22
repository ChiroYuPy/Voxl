use std::collections::HashMap;
use std::fs;
use std::sync::RwLock;
use serde::{Deserialize, Serialize};
use serde::de::Visitor;
use std::fmt;

use tracing::info;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum RenderType {
    Opaque,
    Transparent,
    Cutout,
    Translucent,
    Invisible,
}

impl Default for RenderType {
    fn default() -> Self {
        Self::Opaque
    }
}

impl<'de> Deserialize<'de> for RenderType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct RenderTypeVisitor;

        impl<'de> Visitor<'de> for RenderTypeVisitor {
            type Value = RenderType;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a string representing render type: opaque, transparent, cutout, translucent, or invisible")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value.to_lowercase().as_str() {
                    "opaque" => Ok(RenderType::Opaque),
                    "transparent" => Ok(RenderType::Transparent),
                    "cutout" => Ok(RenderType::Cutout),
                    "translucent" => Ok(RenderType::Translucent),
                    "invisible" => Ok(RenderType::Invisible),
                    other => Err(E::custom(format!("unknown render type: {}", other))),
                }
            }
        }

        deserializer.deserialize_str(RenderTypeVisitor)
    }
}

impl RenderType {
    pub fn culls_adjacent_faces(&self) -> bool {
        matches!(self, Self::Opaque)
    }

    pub fn is_visible(&self) -> bool {
        !matches!(self, Self::Invisible)
    }
}

pub type GlobalVoxelId = usize;

pub type VoxelStringId = String;

#[derive(Debug, Deserialize, Clone)]
pub struct BlockConfig {
    pub name: String,

    #[serde(default)]
    pub model: Option<String>,

    #[serde(default)]
    pub texture: Option<String>,

    #[serde(default)]
    pub render_type: RenderType,

    #[serde(default = "default_collidable")]
    pub collidable: bool,
}

fn default_collidable() -> bool { true }

impl BlockConfig {
    pub fn uses_model(&self) -> bool {
        self.model.is_some()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TextureUV {
    pub u_min: f32,
    pub v_min: f32,
    pub u_max: f32,
    pub v_max: f32,
    pub size_in_atlas: f32,
}

impl TextureUV {
    pub fn new(u_min: f32, v_min: f32, u_max: f32, v_max: f32, size_in_atlas: f32) -> Self {
        Self { u_min, v_min, u_max, v_max, size_in_atlas }
    }

    pub fn to_uv_offset(&self) -> (f32, f32) {
        (self.u_min, self.v_min)
    }
}

#[derive(Clone, Debug)]
pub struct VoxelDefinition {
    pub global_id: GlobalVoxelId,
    pub string_id: VoxelStringId,
    pub name: String,
    pub model_name: Option<String>,
    pub render_type: RenderType,
    pub collidable: bool,
}

impl VoxelDefinition {
    pub fn uses_model(&self) -> bool {
        self.model_name.is_some()
    }
}

pub struct VoxelRegistry {
    definitions: Vec<VoxelDefinition>,
    string_to_id: HashMap<String, GlobalVoxelId>,
    model_loader: Option<crate::voxel::model::ModelLoader>,
    texture_uvs: HashMap<String, (usize, TextureUV)>,
    resolved_models: HashMap<String, crate::voxel::model::ResolvedBlockModel>,
}

impl VoxelRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            definitions: Vec::new(),
            string_to_id: HashMap::new(),
            model_loader: None,
            texture_uvs: HashMap::new(),
            resolved_models: HashMap::new(),
        };
        registry.register(VoxelDefinition {
            global_id: 0,
            string_id: "air".to_string(),
            name: "Air".to_string(),
            model_name: None,
            render_type: RenderType::Invisible,
            collidable: false,
        });
        registry
    }

    pub fn load_models(&mut self) -> Result<Vec<String>, String> {
        let mut loader = crate::voxel::model::ModelLoader::new();
        let textures = loader.load_from_folder()?;
        self.model_loader = Some(loader);
        Ok(textures)
    }

    pub fn register_texture_uvs(&mut self, texture_uvs: HashMap<String, (usize, TextureUV)>) {
        self.texture_uvs = texture_uvs;
    }

    pub fn resolve_models(&mut self) {
        if self.model_loader.is_none() {
            return;
        }

        let texture_map: HashMap<String, usize> = self.texture_uvs
            .iter()
            .map(|(name, (id, _uv))| (name.clone(), *id))
            .collect();

        let loader = self.model_loader.as_ref().unwrap();
        for (name, model) in loader.all_models() {
            let resolved = model.resolve(&texture_map);
            self.resolved_models.insert(name.clone(), resolved);
        }
    }

    pub fn get_resolved_model(&self, name: &str) -> Option<&crate::voxel::model::ResolvedBlockModel> {
        self.resolved_models.get(name)
    }

    pub fn get_texture_uv(&self, name: &str) -> Option<TextureUV> {
        self.texture_uvs.get(name).map(|(_, uv)| *uv)
    }

    pub fn get_texture_uv_by_id(&self, texture_id: usize) -> TextureUV {
        for (_name, (id, uv)) in &self.texture_uvs {
            if *id == texture_id {
                return *uv;
            }
        }
        TextureUV::new(0.0, 0.0, 1.0, 1.0, 1.0)
    }

    pub fn get_model(&self, name: &str) -> Option<&crate::voxel::model::BlockModel> {
        self.model_loader.as_ref()?.get(name)
    }

    pub fn has_models(&self) -> bool {
        self.model_loader.is_some()
    }

    pub fn load_from_folder(&mut self) -> Result<Vec<String>, String> {
        let blocks_dir = crate::paths::assets_dir().join("blocks");

        tracing::info!("Loading blocks from: {:?}", blocks_dir);

        if !blocks_dir.exists() {
            return Err(format!("Blocks folder not found: {:?}", blocks_dir));
        }

        let mut entries: Vec<_> = fs::read_dir(blocks_dir)
            .map_err(|e| format!("Failed to read blocks folder: {}", e))?
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry.path().extension().map_or(false, |ext| ext == "ron")
            })
            .collect();

        entries.sort_by_key(|entry| entry.file_name());

        let mut texture_list = Vec::new();
        let mut loaded_count = 0;
        let mut unknown_models = Vec::new();

        for entry in entries {
            let path = entry.path();
            let file_name = path.file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| format!("Invalid filename: {:?}", path))?;

            let string_id = file_name.to_string();

            if string_id == "air" {
                continue;
            }

            let content = fs::read_to_string(&path)
                .map_err(|e| format!("Failed to read {:?}: {}", path, e))?;

            let config: BlockConfig = ron::from_str(&content)
                .map_err(|e| format!("Failed to parse {:?}: {}", path, e))?;

            // Register the block from config
            self.register_from_config(&string_id, &config);

            if config.uses_model() {
                if let Some(model_name) = &config.model {
                    if let Some(model) = self.get_model(model_name) {
                        for tex in model.get_used_textures() {
                            if !texture_list.contains(&tex) {
                                texture_list.push(tex);
                            }
                        }
                        loaded_count += 1;
                    } else {
                        unknown_models.push((string_id, model_name.clone()));
                    }
                }
            }
        }

        info!("Loaded {} block definitions (unique textures: {})", loaded_count, texture_list.len());

        if !unknown_models.is_empty() {
            tracing::warn!("Warning: {} blocks reference unknown models: {}",
                unknown_models.len(),
                unknown_models.iter()
                    .map(|(block, model)| format!("'{}' -> '{}'", block, model))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }

        Ok(texture_list)
    }

    /// Registers a new voxel definition
    fn register(&mut self, def: VoxelDefinition) -> GlobalVoxelId {
        let global_id = def.global_id;
        self.string_to_id.insert(def.string_id.clone(), global_id);
        self.definitions.push(def);
        global_id
    }

    /// Enregistre un nouveau type de voxel avec un string_id unique
    pub fn register_voxel(&mut self, string_id: &str, name: &str, texture_index: u32) -> GlobalVoxelId {
        // Pour l'atlas statique (fallback), on suppose une grille 2x2
        let _size_in_atlas = 0.5;
        let _uv = TextureUV::new(
            (texture_index % 2) as f32 * 0.5,
            (texture_index / 2) as f32 * 0.5,
            (texture_index % 2) as f32 * 0.5 + 0.5,
            (texture_index / 2) as f32 * 0.5 + 0.5,
            _size_in_atlas,
        );

        let global_id = self.definitions.len();
        self.register(VoxelDefinition {
            global_id,
            string_id: string_id.to_string(),
            name: name.to_string(),
            model_name: None,
            render_type: RenderType::Opaque,
            collidable: true,
        })
    }

    /// Retourne la définition à partir du global_id
    pub fn get(&self, global_id: GlobalVoxelId) -> Option<&VoxelDefinition> {
        self.definitions.get(global_id)
    }

    /// Retourne le global_id à partir du string_id
    pub fn get_id_by_string(&self, string_id: &str) -> Option<GlobalVoxelId> {
        self.string_to_id.get(string_id).copied()
    }

    /// Retourne la définition à partir du string_id
    pub fn get_by_string(&self, string_id: &str) -> Option<&VoxelDefinition> {
        let id = self.get_id_by_string(string_id)?;
        self.get(id)
    }

    /// Retourne true si le global_id représente de l'air (vide)
    pub fn is_air(&self, global_id: GlobalVoxelId) -> bool {
        global_id == 0
    }

    /// Retourne true si le block est solide (propriété physique)
    pub fn is_solid(&self, global_id: GlobalVoxelId) -> bool {
        self.get(global_id).map_or(false, |d| d.collidable)
    }

    /// Retourne true si le block est opaque (propriété graphique - pour culling)
    pub fn is_opaque(&self, global_id: GlobalVoxelId) -> bool {
        self.get(global_id).map_or(false, |d| d.render_type.culls_adjacent_faces())
    }

    /// Retourne le type de rendu du block
    pub fn get_render_type(&self, global_id: GlobalVoxelId) -> RenderType {
        self.get(global_id).map_or(RenderType::Invisible, |d| d.render_type)
    }

    /// Retourne true si le block est solide (collision)
    pub fn is_collidable(&self, global_id: GlobalVoxelId) -> bool {
        self.get(global_id).map_or(false, |d| d.collidable)
    }

    /// Retourne le nombre de types de voxels enregistrés
    pub fn len(&self) -> usize {
        self.definitions.len()
    }

    /// Retourne true si le registry est vide (sauf l'air)
    pub fn is_empty(&self) -> bool {
        self.definitions.len() <= 1
    }

    /// Registers a voxel from a BlockConfig
    pub fn register_from_config(&mut self, string_id: &str, config: &BlockConfig) -> GlobalVoxelId {
        let global_id = self.definitions.len();
        self.register(VoxelDefinition {
            global_id,
            string_id: string_id.to_string(),
            name: config.name.clone(),
            model_name: config.model.clone(),
            render_type: config.render_type,
            collidable: config.collidable,
        })
    }
}

impl Default for VoxelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Registry partagé entre les threads (pour accès depuis le monde, le mesh, etc.)
#[derive(Clone)]
pub struct SharedVoxelRegistry {
    inner: std::sync::Arc<RwLock<VoxelRegistry>>,
}

impl SharedVoxelRegistry {
    pub fn new() -> Self {
        Self {
            inner: std::sync::Arc::new(RwLock::new(VoxelRegistry::new())),
        }
    }

    /// Charge les définitions de blocks depuis le dossier blocks/
    /// Retourne la liste des textures à charger
    pub fn load_from_folder(&self) -> Result<Vec<String>, String> {
        self.inner.write().unwrap().load_from_folder()
    }

    /// Charge les modèles depuis le dossier models/
    /// Retourne la liste de toutes les textures utilisées par les modèles
    pub fn load_models(&self) -> Result<Vec<String>, String> {
        self.inner.write().unwrap().load_models()
    }

    pub fn register_voxel(&self, string_id: &str, name: &str, texture_index: u32) -> GlobalVoxelId {
        self.inner.write().unwrap().register_voxel(string_id, name, texture_index)
    }

    pub fn get(&self, global_id: GlobalVoxelId) -> Option<VoxelDefinition> {
        self.inner.read().unwrap().get(global_id).cloned()
    }

    pub fn get_id_by_string(&self, string_id: &str) -> Option<GlobalVoxelId> {
        self.inner.read().unwrap().get_id_by_string(string_id)
    }

    pub fn get_by_string(&self, string_id: &str) -> Option<VoxelDefinition> {
        self.inner.read().unwrap().get_by_string(string_id).cloned()
    }

    pub fn is_air(&self, global_id: GlobalVoxelId) -> bool {
        self.inner.read().unwrap().is_air(global_id)
    }

    pub fn is_solid(&self, global_id: GlobalVoxelId) -> bool {
        self.inner.read().unwrap().is_solid(global_id)
    }

    pub fn is_opaque(&self, global_id: GlobalVoxelId) -> bool {
        self.inner.read().unwrap().is_opaque(global_id)
    }

    pub fn get_render_type(&self, global_id: GlobalVoxelId) -> RenderType {
        self.inner.read().unwrap().get_render_type(global_id)
    }

    pub fn is_collidable(&self, global_id: GlobalVoxelId) -> bool {
        self.inner.read().unwrap().is_collidable(global_id)
    }

    pub fn len(&self) -> usize {
        self.inner.read().unwrap().len()
    }

    /// Enregistre les UVs des textures depuis l'atlas
    pub fn register_texture_uvs(&self, texture_uvs: HashMap<String, (usize, TextureUV)>) {
        self.inner.write().unwrap().register_texture_uvs(texture_uvs);
    }

    /// Résout tous les modèles avec les IDs de texture
    pub fn resolve_models(&self) {
        self.inner.write().unwrap().resolve_models();
    }

    /// Retourne le modèle résolu avec le nom donné
    pub fn get_resolved_model(&self, name: &str) -> Option<crate::voxel::model::ResolvedBlockModel> {
        self.inner.read().unwrap().get_resolved_model(name).cloned()
    }

    /// Retourne les UV d'une texture par son nom
    pub fn get_texture_uv(&self, name: &str) -> Option<TextureUV> {
        self.inner.read().unwrap().get_texture_uv(name)
    }

    /// Retourne les UV d'une texture par son ID
    pub fn get_texture_uv_by_id(&self, texture_id: usize) -> TextureUV {
        self.inner.read().unwrap().get_texture_uv_by_id(texture_id)
    }

    /// Retourne le modèle avec le nom donné, s'il existe
    pub fn get_model(&self, name: &str) -> Option<crate::voxel::model::BlockModel> {
        self.inner.read().unwrap().get_model(name).cloned()
    }

    /// Retourne true si le système de modèles est chargé
    pub fn has_models(&self) -> bool {
        self.inner.read().unwrap().has_models()
    }

    /// Registers a voxel from a BlockConfig
    pub fn register_from_config(&self, string_id: &str, config: &BlockConfig) -> GlobalVoxelId {
        self.inner.write().unwrap().register_from_config(string_id, config)
    }
}

impl Default for SharedVoxelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Initialise le registry en chargeant les blocks depuis le dossier blocks/
pub fn initialize_registry(_registry: &SharedVoxelRegistry) {
    // La registration se fait maintenant depuis state.rs après génération de l'atlas
}
