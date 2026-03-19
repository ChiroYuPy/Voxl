//! Système de modèles de blocs inspiré de Minecraft
//! Permet de définir des blocs avec plusieurs éléments (cuboids) et des textures par face

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use serde::{Deserialize, Serialize};
use crate::voxel::face::VoxelFace;

/// Bounds d'un élément (cuboid) en coordonnées locales (0-16 comme Minecraft)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ElementBounds {
    /// Point minimum (x, y, z) en coordonnées locales [0-16]
    pub from: [f32; 3],
    /// Point maximum (x, y, z) en coordonnées locales [0-16]
    pub to: [f32; 3],
}

impl ElementBounds {
    /// Retourne les vertices pour une face spécifique de ce cuboid
    /// Utilise VoxelFace pour obtenir l'ordre des coins, puis transforme selon les bounds
    pub fn get_face_vertices(&self, face: VoxelFace) -> [[f32; 3]; 6] {
        let quad = face.quad_vertices();

        // Transformer chaque coin du cube unité vers les bounds de l'élément
        let transform = |v: [f32; 3]| -> [f32; 3] {
            let x = self.from[0] + v[0] * (self.to[0] - self.from[0]);
            let y = self.from[1] + v[1] * (self.to[1] - self.from[1]);
            let z = self.from[2] + v[2] * (self.to[2] - self.from[2]);
            [x / 16.0, y / 16.0, z / 16.0]
        };

        let c0 = transform(quad[0]);
        let c1 = transform(quad[1]);
        let c2 = transform(quad[2]);
        let c3 = transform(quad[3]);

        // Deux triangles: (0,1,2) et (0,2,3)
        [c0, c1, c2, c0, c2, c3]
    }

    /// Retourne les UV pour une face spécifique de ce cuboid
    /// Utilise les positions 3D des coins pour calculer les UV selon l'orientation de la face
    pub fn get_face_uvs(&self, face: VoxelFace) -> [[f32; 2]; 6] {
        let quad = face.quad_vertices();

        // Pour chaque coin, extraire les coordonnées UV appropriées selon la face
        let get_uv = |v: [f32; 3]| -> [f32; 2] {
            let (u_coord, v_coord, u_idx, v_idx) = match face {
                VoxelFace::Top | VoxelFace::Bottom => (v[0], v[2], 0, 2),     // x, z
                VoxelFace::North | VoxelFace::South => (v[0], v[1], 0, 1),     // x, y
                VoxelFace::East | VoxelFace::West => (v[2], v[1], 2, 1),       // z, y
            };

            let u = self.from[u_idx] + u_coord * (self.to[u_idx] - self.from[u_idx]);
            let v = self.from[v_idx] + v_coord * (self.to[v_idx] - self.from[v_idx]);

            [u / 16.0, v / 16.0]
        };

        let uv0 = get_uv(quad[0]);
        let uv1 = get_uv(quad[1]);
        let uv2 = get_uv(quad[2]);
        let uv3 = get_uv(quad[3]);

        // Deux triangles: (0,1,2) et (0,2,3)
        [uv0, uv1, uv2, uv0, uv2, uv3]
    }
}

/// Définition d'une face d'un élément (avant résolution des textures)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementFace {
    /// Nom de la texture (référence vers BlockModel.textures, ex: "#texture" ou "stone")
    pub texture: String,
    /// Si true, la face n'est pas générée (optimisation)
    #[serde(default = "default_element_face_enabled")]
    pub enabled: bool,
    /// Face du voxel utilisée pour le culling (détermine quel voisin vérifier)
    #[serde(default)]
    pub cull_face: Option<VoxelFace>,
}

fn default_element_face_enabled() -> bool { true }

/// Face d'élément avec texture ID résolu (utilisé après chargement)
#[derive(Debug, Clone)]
pub struct ResolvedElementFace {
    /// ID de la texture dans l'atlas
    pub texture_id: usize,
    /// Si true, la face est activée
    pub enabled: bool,
    /// Face du voxel utilisée pour le culling
    pub cull_face: Option<VoxelFace>,
}

impl ElementFace {
    /// Crée une face activée avec une texture
    pub fn new(texture: impl Into<String>) -> Self {
        Self {
            texture: texture.into(),
            enabled: true,
            cull_face: None,
        }
    }

    /// Crée une face désactivée
    pub fn disabled() -> Self {
        Self {
            texture: String::new(),
            enabled: false,
            cull_face: None,
        }
    }

    /// Définit la face de culling
    pub fn with_cull_face(mut self, face: VoxelFace) -> Self {
        self.cull_face = Some(face);
        self
    }
}

/// Définition d'un élément (cuboid) dans un modèle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelElement {
    /// Bounds du cuboid [0-16]
    #[serde(flatten)]
    pub bounds: ElementBounds,
    /// Faces de l'élément (liste de (face, face_def))
    #[serde(default)]
    pub faces: Vec<(VoxelFace, ElementFace)>,
}

/// Élément résolu avec IDs de texture (utilisé pour le rendu)
#[derive(Debug, Clone)]
pub struct ResolvedModelElement {
    /// Bounds du cuboid [0-16]
    pub bounds: ElementBounds,
    /// Faces de l'élément (clé: VoxelFace)
    pub faces: HashMap<VoxelFace, ResolvedElementFace>,
}

impl ModelElement {
    /// Retourne true si une face est activée pour cet élément
    pub fn has_face(&self, face: VoxelFace) -> bool {
        self.faces.iter()
            .find(|(f, _)| *f == face)
            .map(|(_, ef)| ef.enabled)
            .unwrap_or(false)
    }

    /// Retourne la définition d'une face si elle existe
    pub fn get_face(&self, face: VoxelFace) -> Option<&ElementFace> {
        self.faces.iter()
            .find(|(f, _)| *f == face)
            .map(|(_, ef)| ef)
            .filter(|ef| ef.enabled)
    }
}

/// Définition complète d'un modèle de bloc
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockModel {
    /// Nom unique du modèle (filename sans extension)
    pub name: String,

    /// Liste des textures référencées (noms des fichiers texture sans extension)
    /// Peut inclure des références comme "#texture" pour pointer vers d'autres textures du modèle
    #[serde(default)]
    pub textures: Vec<(String, String)>,

    /// Liste des éléments (cuboids) composant le modèle
    #[serde(default)]
    pub elements: Vec<ModelElement>,

    /// Type de rendu du modèle (utilisé pour le culling)
    #[serde(default)]
    pub render_type: String,

    /// Est-ce que le bloc est solide (collision)
    #[serde(default = "default_collidable")]
    pub collidable: bool,
}

/// Modèle résolu avec IDs de texture (utilisé pour le rendu)
#[derive(Debug, Clone)]
pub struct ResolvedBlockModel {
    /// Nom du modèle
    pub name: String,
    /// Liste des éléments avec textures résolues
    pub elements: Vec<ResolvedModelElement>,
    /// Type de rendu du modèle
    pub render_type: String,
    /// Est-ce que le bloc est solide
    pub collidable: bool,
}

fn default_collidable() -> bool { true }

impl BlockModel {
    /// Crée un modèle cube plein simple (toutes les faces avec la même texture)
    pub fn cube(name: impl Into<String>, texture: impl Into<String>) -> Self {
        let texture = texture.into();
        let name = name.into();

        let faces: Vec<(VoxelFace, ElementFace)> = VoxelFace::ALL
            .iter()
            .map(|&face| (face, ElementFace::new("#texture".to_string())))
            .collect();

        Self {
            name,
            textures: vec![("texture".to_string(), texture)],
            elements: vec![ModelElement {
                bounds: ElementBounds { from: [0.0, 0.0, 0.0], to: [16.0, 16.0, 16.0] },
                faces,
            }],
            render_type: "opaque".to_string(),
            collidable: true,
        }
    }

    /// Résout une référence de texture (ex: "#texture" -> "stone")
    pub fn resolve_texture(&self, reference: &str) -> Option<String> {
        if reference.starts_with('#') {
            let key = &reference[1..];
            self.textures.iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
        } else {
            Some(reference.to_string())
        }
    }

    /// Retourne la liste de toutes les textures utilisées par ce modèle
    pub fn get_used_textures(&self) -> Vec<String> {
        let mut textures = Vec::new();

        for element in &self.elements {
            for (_, face_def) in &element.faces {
                if face_def.enabled {
                    if let Some(tex) = self.resolve_texture(&face_def.texture) {
                        if !textures.contains(&tex) {
                            textures.push(tex);
                        }
                    }
                }
            }
        }

        textures
    }

    /// Résout le modèle en utilisant une map texture_name -> texture_id
    /// Retourne un ResolvedBlockModel avec les IDs de texture
    pub fn resolve(&self, texture_map: &HashMap<String, usize>) -> ResolvedBlockModel {
        let elements = self.elements.iter().map(|element| {
            let mut faces = HashMap::new();
            for (face, face_def) in &element.faces {
                if face_def.enabled {
                    // Résoudre le nom de texture vers un ID
                    let texture_name = self.resolve_texture(&face_def.texture)
                        .unwrap_or_else(|| "stone".to_string());
                    let texture_id = *texture_map.get(&texture_name).unwrap_or(&0);
                    faces.insert(*face, ResolvedElementFace {
                        texture_id,
                        enabled: true,
                        cull_face: face_def.cull_face,
                    });
                }
            }
            ResolvedModelElement {
                bounds: element.bounds,
                faces,
            }
        }).collect();

        ResolvedBlockModel {
            name: self.name.clone(),
            elements,
            render_type: self.render_type.clone(),
            collidable: self.collidable,
        }
    }

    /// Retourne true si le modèle est vide (pas d'éléments)
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }
}

/// Loader pour les modèles de blocs
pub struct ModelLoader {
    models: HashMap<String, BlockModel>,
}

impl ModelLoader {
    /// Crée un nouveau loader
    pub fn new() -> Self {
        Self {
            models: HashMap::new(),
        }
    }

    /// Charge tous les modèles depuis le dossier models/
    pub fn load_from_folder(&mut self) -> Result<Vec<String>, String> {
        let models_dir = Path::new("models");

        if !models_dir.exists() {
            // Créer le dossier models s'il n'existe pas
            fs::create_dir_all(models_dir)
                .map_err(|e| format!("Failed to create models folder: {}", e))?;
            println!("Created models/ folder");
            return Ok(Vec::new());
        }

        let mut entries: Vec<_> = fs::read_dir(models_dir)
            .map_err(|e| format!("Failed to read models folder: {}", e))?
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry.path().extension().map_or(false, |ext| ext == "ron")
            })
            .collect();

        entries.sort_by_key(|entry| entry.file_name());

        let mut all_textures = Vec::new();

        for entry in entries {
            let path = entry.path();
            let file_name = path.file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| format!("Invalid filename: {:?}", path))?;

            let content = fs::read_to_string(&path)
                .map_err(|e| format!("Failed to read {:?}: {}", path, e))?;

            let mut model: BlockModel = ron::from_str(&content)
                .map_err(|e| format!("Failed to parse {:?}: {}", path, e))?;

            model.name = file_name.to_string();

            // Collecter les textures utilisées
            let textures = model.get_used_textures();
            for tex in &textures {
                if !all_textures.contains(tex) {
                    all_textures.push(tex.clone());
                }
            }

            self.models.insert(model.name.clone(), model);
            println!("Loaded model '{}' (textures: {})", file_name, textures.join(", "));
        }

        Ok(all_textures)
    }

    /// Retourne un modèle par son nom
    pub fn get(&self, name: &str) -> Option<&BlockModel> {
        self.models.get(name)
    }

    /// Retourne tous les modèles chargés
    pub fn all_models(&self) -> &HashMap<String, BlockModel> {
        &self.models
    }
}

impl Default for ModelLoader {
    fn default() -> Self {
        Self::new()
    }
}
