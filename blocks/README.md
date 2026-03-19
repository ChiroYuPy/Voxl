# Blocks Configuration

This folder contains block definitions for the voxel game. Each block is defined by a `.toml` file.

## File Naming

- The filename (without `.toml` extension) becomes the block's `string_id`
- Files are loaded in **alphabetical order** which determines the block's numeric ID
- **Exception**: `air` is always ID 0 and should not have a config file

## Block Config Format

```toml
name = "Display Name"

# Texture file name (without extension, must be in assets/textures/)
texture = "texture_name"

# Optional: Different textures for different faces
texture_side = "texture_name"    # Sides (North, South, East, West)
texture_bottom = "texture_name"  # Bottom face
# If not specified, uses the main `texture`

# Optional: Block properties (defaults shown)
solid = true
transparent = false
```

### Properties

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `name` | string | required | Human-readable name |
| `texture` | string | required | Main texture file (without .png extension) |
| `texture_side` | string | uses `texture` | Texture for side faces |
| `texture_bottom` | string | uses `texture` | Texture for bottom face |
| `solid` | boolean | `true` | Can the player walk on it? |
| `transparent` | boolean | `false` | Does light pass through? |

## Texture Files

Textures are stored in `assets/textures/` as PNG files.
- Standard size: 16x16 pixels
- Files are named after their block ID (e.g., `stone.png`, `grass_top.png`)

The texture atlas is **automatically generated** from all referenced textures at runtime.

## Example Blocks

### Stone (same texture on all faces)
```toml
name = "Stone"
texture = "stone"

solid = true
transparent = false
```

### Grass (different textures per face)
```toml
name = "Grass"
texture = "grass_top"
texture_side = "grass_side"
texture_bottom = "dirt"

solid = true
transparent = false
```

### Glass (transparent, not solid)
```toml
name = "Glass"
texture = "glass"

solid = false
transparent = true
```

## Adding a New Block

1. Create a new `.toml` file in this folder
2. Name it after your block's ID (e.g., `sand.toml`)
3. Add the required properties
4. Place your texture PNG in `assets/textures/`
5. The block will be automatically loaded on game start

## Texture Atlas

The texture atlas is automatically generated at runtime from all textures referenced in block configs. The atlas is arranged as a square grid, with each texture occupying an equal space.
