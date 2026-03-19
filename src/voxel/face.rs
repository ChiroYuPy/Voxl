use glam::IVec3;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TriangleDiagonal {
    Primary,
    Secondary,
}

impl Default for TriangleDiagonal {
    fn default() -> Self {
        Self::Primary
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VoxelFace { Top, Bottom, North, South, East, West, }

impl VoxelFace {
    pub const ALL: [VoxelFace; 6] = [Self::Top, Self::Bottom, Self::North, Self::South, Self::East, Self::West];

    pub fn normal(self) -> IVec3 {
        match self {
            Self::Top    => IVec3::new(0, 1, 0),
            Self::Bottom => IVec3::new(0, -1, 0),
            Self::North  => IVec3::new(0, 0, 1),
            Self::South  => IVec3::new(0, 0, -1),
            Self::East   => IVec3::new(1, 0, 0),
            Self::West   => IVec3::new(-1, 0, 0),
        }
    }

    pub fn normal_f32(self) -> [f32; 3] {
        let n = self.normal();
        [n.x as f32, n.y as f32, n.z as f32]
    }

    pub fn opposite(self) -> VoxelFace {
        match self {
            Self::Top    => VoxelFace::Bottom,
            Self::Bottom => VoxelFace::Top,
            Self::North  => VoxelFace::South,
            Self::South  => VoxelFace::North,
            Self::East   => VoxelFace::West,
            Self::West   => VoxelFace::East,
        }
    }

    pub fn quad_vertices(self) -> [[f32; 3]; 4] {
        match self {
            // Face UP (y+): vue du dessus
            // coin0=(0,1,1), coin1=(1,1,1), coin2=(1,1,0), coin3=(0,1,0)
            Self::Top => [
                [0.0, 1.0, 0.0],  // coin0
                [1.0, 1.0, 0.0],  // coin1
                [1.0, 1.0, 1.0],  // coin2
                [0.0, 1.0, 1.0],  // coin3
            ],
            // Face DOWN (y-): vue du dessous
            Self::Bottom => [
                [0.0, 0.0, 1.0],  // coin0
                [1.0, 0.0, 1.0],  // coin1
                [1.0, 0.0, 0.0],  // coin2
                [0.0, 0.0, 0.0],  // coin3
            ],
            // Face NORTH (z+)
            Self::North => [
                [1.0, 0.0, 1.0],  // coin0
                [0.0, 0.0, 1.0],  // coin1
                [0.0, 1.0, 1.0],  // coin2
                [1.0, 1.0, 1.0],  // coin3
            ],
            // Face SOUTH (z-)
            Self::South => [
                [0.0, 0.0, 0.0],  // coin0
                [1.0, 0.0, 0.0],  // coin1
                [1.0, 1.0, 0.0],  // coin2
                [0.0, 1.0, 0.0],  // coin3
            ],
            // Face EAST (x+)
            Self::East => [
                [1.0, 0.0, 0.0],  // coin0
                [1.0, 0.0, 1.0],  // coin1
                [1.0, 1.0, 1.0],  // coin2
                [1.0, 1.0, 0.0],  // coin3
            ],
            // Face WEST (x-)
            Self::West => [
                [0.0, 0.0, 1.0],  // coin0
                [0.0, 0.0, 0.0],  // coin1
                [0.0, 1.0, 0.0],  // coin2
                [0.0, 1.0, 1.0],  // coin3
            ],
        }
    }

    /// Retourne les 4 UV du quad dans l'ordre standard:
    /// coin0=(1,1), coin1=(0,1), coin2=(0,0), coin3=(1,0)
    pub fn quad_uvs(self) -> [[f32; 2]; 4] {
        // Mêmes UV pour toutes les faces
        [
            [1.0, 1.0],  // coin0
            [0.0, 1.0],  // coin1
            [0.0, 0.0],  // coin2
            [1.0, 0.0],  // coin3
        ]
    }

    /// Retourne les 6 vertices pour former 2 triangles, avec choix de la diagonale
    ///
    /// # Diagonale Primary (coin0 → coin2)
    /// Triangle 1: coin0, coin1, coin2
    /// Triangle 2: coin0, coin2, coin3
    ///
    /// # Diagonale Secondary (coin1 → coin3)
    /// Triangle 1: coin0, coin1, coin3
    /// Triangle 2: coin1, coin2, coin3
    pub fn triangles(self, diagonal: TriangleDiagonal) -> [[f32; 3]; 6] {
        let quad = self.quad_vertices();
        match diagonal {
            TriangleDiagonal::Primary => {
                // Triangles: (0,1,2) et (0,2,3)
                [
                    quad[0], quad[1], quad[2],  // Triangle 1
                    quad[0], quad[2], quad[3],  // Triangle 2
                ]
            }
            TriangleDiagonal::Secondary => {
                // Triangles: (0,1,3) et (1,2,3)
                [
                    quad[0], quad[1], quad[3],  // Triangle 1
                    quad[1], quad[2], quad[3],  // Triangle 2
                ]
            }
        }
    }

    /// Retourne les 6 UV correspondants aux triangles, avec choix de la diagonale
    pub fn triangle_uvs(self, diagonal: TriangleDiagonal) -> [[f32; 2]; 6] {
        let quad_uvs = self.quad_uvs();
        match diagonal {
            TriangleDiagonal::Primary => {
                // Triangles: (0,1,2) et (0,2,3)
                [
                    quad_uvs[0], quad_uvs[1], quad_uvs[2],  // Triangle 1
                    quad_uvs[0], quad_uvs[2], quad_uvs[3],  // Triangle 2
                ]
            }
            TriangleDiagonal::Secondary => {
                // Triangles: (0,1,3) et (1,2,3)
                [
                    quad_uvs[0], quad_uvs[1], quad_uvs[3],  // Triangle 1
                    quad_uvs[1], quad_uvs[2], quad_uvs[3],  // Triangle 2
                ]
            }
        }
    }

    /// Méthode legacy pour compatibilité - utilise la diagonale primaire
    pub fn vertices(self) -> [[f32; 3]; 6] {
        self.triangles(TriangleDiagonal::Primary)
    }

    /// UV legacy pour compatibilité (correspond à l'ancien ordre)
    pub const BASE_UVS: [[f32; 2]; 6] = [[1.0, 1.0], [0.0, 1.0], [0.0, 0.0], [1.0, 1.0], [0.0, 0.0], [1.0, 0.0]];
}
