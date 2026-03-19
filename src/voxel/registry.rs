use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::RwLock;
use serde::{Deserialize, Serialize};
use serde::de::Visitor;
use std::fmt;

use crate::voxel::face::VoxelFace;

/// Type de rendu d'un bloc - détermine comment les faces adjacentes sont traitées
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum RenderType {
    /// Bloc complètement opaque (cache les faces adjacentes)
    Opaque,
    /// Bloc transparent mais solide (verre, glace) - ne cache pas les faces adjacentes
    Transparent,
    /// Bloc avec découpe alpha (feuilles, herbes) - ne cache pas les faces adjacentes
    Cutout,
    /// Bloc translucide avec blending (eau, verre teinté) - ne cache pas les faces adjacentes
    Translucent,
    /// Bloc qui n'est jamais rendu (air, triggers)
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
    /// Retourne true si le bloc cache les faces adjacentes (culling)
    pub fn culls_adjacent_faces(&self) -> bool {
        matches!(self, Self::Opaque)
    }

    /// Retourne true si le bloc est visible
    pub fn is_visible(&self) -> bool {
        !matches!(self, Self::Invisible)
    }
}

/// Identifiant global d'un voxel (0 = air/vide)
pub type GlobalVoxelId = usize;

/// Identifiant de voxel sous forme de chaîne (ex: "grass", "dirt")
pub type VoxelStringId = String;

/// Configuration d'un block depuis un fichier TOML
#[derive(Debug, Deserialize, Clone)]
pub struct BlockConfig {
    /// Nom affichable du block
    pub name: String,
    /// Nom du fichier de texture (sans extension, dans assets/textures/)
    pub texture: String,
    /// Texture pour les côtés (optionnel, utilise "texture" par défaut)
    #[serde(default)]
    pub texture_side: Option<String>,
    /// Texture pour le dessous (optionnel, utilise "texture" par défaut)
    #[serde(default)]
    pub texture_bottom: Option<String>,
    /// Type de rendu du bloc (opaque, transparent, cutout, translucent, invisible)
    #[serde(default)]
    pub render_type: RenderType,
    /// Est-ce que le bloc est solide (collision)
    #[serde(default = "default_collidable")]
    pub collidable: bool,
}

fn default_collidable() -> bool { true }

/// Coordonnées UV pour une texture dans l'atlas
#[derive(Clone, Copy, Debug)]
pub struct TextureUV {
    pub u_min: f32,
    pub v_min: f32,
    pub u_max: f32,
    pub v_max: f32,
    /// Taille de la texture dans l'atlas (normalisée 0-1)
    pub size_in_atlas: f32,
}

impl TextureUV {
    pub fn new(u_min: f32, v_min: f32, u_max: f32, v_max: f32, size_in_atlas: f32) -> Self {
        Self { u_min, v_min, u_max, v_max, size_in_atlas }
    }

    /// Retourne les coordonnées UV pour le vertex shader (offset + scale)
    pub fn to_uv_offset(&self) -> (f32, f32) {
        (self.u_min, self.v_min)
    }
}

/// Définition d'un type de voxel
#[derive(Clone, Debug)]
pub struct VoxelDefinition {
    /// Identifiant global unique
    pub global_id: GlobalVoxelId,
    /// Identifiant sous forme de chaîne
    pub string_id: VoxelStringId,
    /// Nom affichable
    pub name: String,
    /// Coordonnées UV pour la texture du dessus
    pub uv_top: TextureUV,
    /// Coordonnées UV pour la texture des côtés
    pub uv_side: TextureUV,
    /// Coordonnées UV pour la texture du dessous
    pub uv_bottom: TextureUV,
    /// Type de rendu du bloc
    pub render_type: RenderType,
    /// Est-ce que le bloc est solide (collision)
    pub collidable: bool,
}

impl VoxelDefinition {
    /// Retourne les coordonnées UV dans l'atlas pour une face donnée
    pub fn get_uv_for_face(&self, face: &VoxelFace) -> (f32, f32) {
        match face {
            VoxelFace::Top => self.uv_top.to_uv_offset(),
            VoxelFace::Bottom => self.uv_bottom.to_uv_offset(),
            _ => self.uv_side.to_uv_offset(), // North, South, East, West
        }
    }

    /// Retourne les coordonnées UV dans l'atlas 2x2 (ancienne méthode pour compatibilité)
    pub fn atlas_uv_offset(&self) -> (f32, f32) {
        self.uv_side.to_uv_offset()
    }
}

/// Registry des types de voxels
pub struct VoxelRegistry {
    /// Definitions indexées par global_id (0 est toujours vide/air)
    definitions: Vec<VoxelDefinition>,
    /// Map string_id -> global_id
    string_to_id: HashMap<String, GlobalVoxelId>,
}

impl VoxelRegistry {
    /// Crée un nouveau registry vide (seul l'air existe avec id 0)
    pub fn new() -> Self {
        let mut registry = Self {
            definitions: Vec::new(),
            string_to_id: HashMap::new(),
        };
        // L'air (vide) est toujours à l'index 0
        let uv_air = TextureUV::new(0.0, 0.0, 0.0, 0.0, 1.0);
        registry.register(VoxelDefinition {
            global_id: 0,
            string_id: "air".to_string(),
            name: "Air".to_string(),
            uv_top: uv_air,
            uv_side: uv_air,
            uv_bottom: uv_air,
            render_type: RenderType::Invisible,
            collidable: false,
        });
        registry
    }

    /// Charge les définitions de blocks depuis le dossier blocks/
    /// Retourne la liste des textures à charger pour générer l'atlas
    pub fn load_from_folder(&mut self) -> Result<Vec<String>, String> {
        let blocks_dir = Path::new("blocks");

        if !blocks_dir.exists() {
            return Err("Blocks folder not found".to_string());
        }

        // Lire tous les fichiers .toml du dossier blocks
        let mut entries: Vec<_> = fs::read_dir(blocks_dir)
            .map_err(|e| format!("Failed to read blocks folder: {}", e))?
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry.path().extension().map_or(false, |ext| ext == "toml")
            })
            .collect();

        // Trier par ordre alphabétique (l'ordre de chargement détermine l'ID)
        entries.sort_by_key(|entry| entry.file_name());

        let mut texture_list = Vec::new();

        for entry in entries {
            let path = entry.path();
            let file_name = path.file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| format!("Invalid filename: {:?}", path))?;

            let string_id = file_name.to_string();

            // Skip air - c'est déjà défini
            if string_id == "air" {
                continue;
            }

            // Lire et parser le fichier TOML
            let content = fs::read_to_string(&path)
                .map_err(|e| format!("Failed to read {:?}: {}", path, e))?;

            let config: BlockConfig = toml::from_str(&content)
                .map_err(|e| format!("Failed to parse {:?}: {}", path, e))?;

            // Ajouter les textures à la liste (pour l'atlas)
            texture_list.push(config.texture.clone());
            if let Some(ref side) = config.texture_side {
                texture_list.push(side.clone());
            }
            if let Some(ref bottom) = config.texture_bottom {
                texture_list.push(bottom.clone());
            }

            println!("Loaded block '{}' (texture: {})", string_id, config.texture);
        }

        Ok(texture_list)
    }

    /// Enregistre les blocks avec leurs coordonnées UV calculées depuis l'atlas
    pub fn register_with_uvs(
        &mut self,
        blocks_data: Vec<(String, BlockConfig, TextureUV, TextureUV, TextureUV)>,
        texture_size_in_atlas: f32,
    ) {
        for (string_id, config, uv_top, uv_side, uv_bottom) in blocks_data {
            // Mettre à jour la taille dans l'atlas pour chaque texture
            let uv_top = TextureUV::new(uv_top.u_min, uv_top.v_min, uv_top.u_max, uv_top.v_max, texture_size_in_atlas);
            let uv_side = TextureUV::new(uv_side.u_min, uv_side.v_min, uv_side.u_max, uv_side.v_max, texture_size_in_atlas);
            let uv_bottom = TextureUV::new(uv_bottom.u_min, uv_bottom.v_min, uv_bottom.u_max, uv_bottom.v_max, texture_size_in_atlas);

            self.register(VoxelDefinition {
                global_id: self.definitions.len(),
                string_id: string_id.clone(),
                name: config.name,
                uv_top,
                uv_side,
                uv_bottom,
                render_type: config.render_type,
                collidable: config.collidable,
            });
            println!("Registered block '{}' (ID: {})", string_id, self.definitions.len() - 1);
        }
    }

    /// Enregistre une nouvelle définition de voxel
    fn register(&mut self, def: VoxelDefinition) -> GlobalVoxelId {
        let global_id = def.global_id;
        self.string_to_id.insert(def.string_id.clone(), global_id);
        self.definitions.push(def);
        global_id
    }

    /// Enregistre un nouveau type de voxel avec un string_id unique
    pub fn register_voxel(&mut self, string_id: &str, name: &str, texture_index: u32) -> GlobalVoxelId {
        // Pour l'atlas statique (fallback), on suppose une grille 2x2
        let size_in_atlas = 0.5;
        let uv = TextureUV::new(
            (texture_index % 2) as f32 * 0.5,
            (texture_index / 2) as f32 * 0.5,
            (texture_index % 2) as f32 * 0.5 + 0.5,
            (texture_index / 2) as f32 * 0.5 + 0.5,
            size_in_atlas,
        );

        let global_id = self.definitions.len();
        self.register(VoxelDefinition {
            global_id,
            string_id: string_id.to_string(),
            name: name.to_string(),
            uv_top: uv,
            uv_side: uv,
            uv_bottom: uv,
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

    /// Enregistre les blocks avec leurs UVs
    pub fn register_with_uvs(&self, blocks_data: Vec<(String, BlockConfig, TextureUV, TextureUV, TextureUV)>, texture_size_in_atlas: f32) {
        self.inner.write().unwrap().register_with_uvs(blocks_data, texture_size_in_atlas);
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
