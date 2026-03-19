use image::RgbaImage;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use wgpu;

/// Informations sur une texture dans l'atlas
#[derive(Debug, Clone)]
pub struct AtlasTexture {
    pub name: String,
    pub uv: (f32, f32, f32, f32), // u_min, v_min, u_max, v_max
}

/// Génère un atlas de texture dynamique à partir d'une liste de noms de textures
/// Retourne (texture, texture_view, texture_uvs, texture_size_in_atlas)
pub fn generate_texture_atlas(
    texture_names: &[String],
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<(wgpu::Texture, wgpu::TextureView, HashMap<String, (f32, f32, f32, f32)>, f32), String> {
    // 1. Dédupliquer les noms de textures
    let mut unique_textures: Vec<&String> = texture_names.iter().collect::<std::collections::HashSet<_>>()
        .iter().map(|s| *s).collect();
    unique_textures.sort();

    if unique_textures.is_empty() {
        return Err("No textures to load".to_string());
    }

    // 2. Charger toutes les textures individuelles
    let mut loaded_images: Vec<(String, RgbaImage)> = Vec::new();
    let texture_size = 16; // Taille standard des textures de blocks

    for texture_name in &unique_textures {
        let path = format!("assets/textures/{}.png", texture_name);
        if !Path::new(&path).exists() {
            eprintln!("Warning: Texture not found: {}", path);
            // Créer une texture magenta par défaut
            let mut img = RgbaImage::new(texture_size, texture_size);
            for pixel in img.pixels_mut() {
                *pixel = image::Rgba([255, 0, 255, 255]);
            }
            loaded_images.push((texture_name.to_string(), img));
            continue;
        }

        let img = image::open(&path)
            .map_err(|e| format!("Failed to load texture {}: {}", path, e))?
            .to_rgba8();

        // Redimensionner à 16x16 si nécessaire
        let resized = if img.dimensions() != (texture_size, texture_size) {
            image::imageops::resize(&img, texture_size, texture_size, image::imageops::FilterType::Nearest)
        } else {
            img
        };

        loaded_images.push((texture_name.to_string(), resized));
    }

    // 3. Calculer la taille de l'atlas (grille carrée)
    let count = loaded_images.len();
    let grid_size = (count as f32).sqrt().ceil() as u32;
    let atlas_size = grid_size * texture_size;

    // Créer l'image de l'atlas
    let mut atlas_image = RgbaImage::new(atlas_size, atlas_size);

    // Placer chaque texture dans l'atlas
    let mut texture_uvs: HashMap<String, (f32, f32, f32, f32)> = HashMap::new();

    for (index, (name, img)) in loaded_images.iter().enumerate() {
        let x = (index as u32 % grid_size) * texture_size;
        let y = (index as u32 / grid_size) * texture_size;

        // Copier la texture dans l'atlas
        for dy in 0..texture_size {
            for dx in 0..texture_size {
                let pixel = img.get_pixel(dx, dy);
                atlas_image.put_pixel(x + dx, y + dy, *pixel);
            }
        }

        // Calculer les coordonnées UV normalisées
        let u_min = x as f32 / atlas_size as f32;
        let v_min = y as f32 / atlas_size as f32;
        let u_max = (x + texture_size) as f32 / atlas_size as f32;
        let v_max = (y + texture_size) as f32 / atlas_size as f32;

        texture_uvs.insert(name.clone(), (u_min, v_min, u_max, v_max));
    }

    // 4. Créer la texture wgpu
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Generated Texture Atlas"),
        size: wgpu::Extent3d {
            width: atlas_size,
            height: atlas_size,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    // Upload de l'atlas vers le GPU
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &atlas_image,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(atlas_size * 4),
            rows_per_image: Some(atlas_size),
        },
        wgpu::Extent3d {
            width: atlas_size,
            height: atlas_size,
            depth_or_array_layers: 1,
        },
    );

    println!("Generated texture atlas: {}x{}, {} textures (grid: {}x{}, tex_size: {:.3})",
        atlas_size, atlas_size, texture_uvs.len(), grid_size, grid_size, 1.0 / grid_size as f32);

    // Sauvegarder l'atlas en PNG pour debug
    if let Err(e) = atlas_image.save("atlas_debug.png") {
        eprintln!("Warning: Failed to save atlas debug image: {}", e);
    } else {
        println!("Saved atlas debug image to atlas_debug.png");
    }

    // Retourner aussi la taille d'une texture dans l'atlas (normalisée 0-1)
    let texture_size_in_atlas = 1.0 / grid_size as f32;

    Ok((texture, texture_view, texture_uvs, texture_size_in_atlas))
}

/// Charge les définitions de blocks et leurs configs
pub fn load_block_configs(
) -> Result<Vec<(String, crate::voxel::BlockConfig)>, String> {
    let blocks_dir = Path::new("blocks");

    if !blocks_dir.exists() {
        return Err("Blocks folder not found".to_string());
    }

    let mut entries: Vec<_> = fs::read_dir(blocks_dir)
        .map_err(|e| format!("Failed to read blocks folder: {}", e))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry.path().extension().map_or(false, |ext| ext == "toml")
        })
        .collect();

    entries.sort_by_key(|entry| entry.file_name());

    let mut blocks = Vec::new();

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

        let config: crate::voxel::BlockConfig = toml::from_str(&content)
            .map_err(|e| format!("Failed to parse {:?}: {}", path, e))?;

        blocks.push((string_id, config));
    }

    Ok(blocks)
}
