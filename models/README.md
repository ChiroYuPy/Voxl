# Système de Modèles de Blocs

Ce dossier contient les définitions de modèles pour les blocs.

## Format des Fichiers

Les modèles sont définis en format TOML. Voici la structure:

```toml
name = "cube"

# Liste des textures utilisées
# Les textures peuvent être référencées avec #texture_name
[textures]
texture = "stone"        # Texture principale
top = "grass_top"        # Texture pour le dessus
side = "grass_side"      # Texture pour les côtés
bottom = "dirt"          # Texture pour le dessous

# Éléments du modèle (cuboids)
[[elements]]
# Bounds en coordonnées [0-16] (comme Minecraft)
from = [0.0, 0.0, 0.0]  # Coin minimum (x, y, z)
to = [16.0, 16.0, 16.0]  # Coin maximum (x, y, z)

# Faces de l'élément
[elements.faces]
top = { texture = "#texture", enabled = true }
bottom = { texture = "#texture", enabled = true }
north = { texture = "#texture", enabled = true }
south = { texture = "#texture", enabled = true }
east = { texture = "#texture", enabled = true }
west = { texture = "#texture", enabled = true }

# Type de rendu et collision
render_type = "opaque"    # opaque, transparent, cutout, translucent, invisible
collidable = true         # true si le bloc est solide
```

## Éléments

### Bounds
- `from`: Point minimum du cuboid [x, y, z] en coordonnées 0-16
- `to`: Point maximum du cuboid [x, y, z] en coordonnées 0-16

### Faces
Chaque face peut avoir:
- `texture`: Nom de la texture (peut être "#key" pour référencer la liste textures)
- `enabled`: true si la face doit être générée (true par défaut)
- `cull_face`: Face du voxel utilisée pour le culling (optionnel)

## Exemples

### Cube simple
```toml
[[elements]]
from = [0.0, 0.0, 0.0]
to = [16.0, 16.0, 16.0]
```

### Slab (demi-bloc)
```toml
[[elements]]
from = [0.0, 0.0, 0.0]
to = [16.0, 8.0, 16.0]
```

### Poteau
```toml
[[elements]]
from = [6.0, 0.0, 6.0]
to = [10.0, 16.0, 10.0]
```

### Multiple éléments
```toml
[[elements]]
from = [0.0, 0.0, 0.0]
to = [16.0, 8.0, 16.0]

[[elements]]
from = [4.0, 8.0, 4.0]
to = [12.0, 16.0, 12.0]
```

## Utilisation dans les Blocs

Dans le fichier `blocks/<block>.toml`:
```toml
name = "Stone"
model = "cube"  # Utilise models/cube.toml
render_type = "opaque"
collidable = true
```

## Faces Disponibles
- `top`: Face Y+ (dessus)
- `bottom`: Face Y- (dessous)
- `north`: Face Z+ (nord)
- `south`: Face Z- (sud)
- `east`: Face X+ (est)
- `west`: Face X- (ouest)
