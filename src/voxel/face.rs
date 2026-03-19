use glam::IVec3;
use serde::{Deserialize, Serialize, Deserializer, de::Visitor};
use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
pub enum TriangleDiagonal {
    Primary,
    Secondary,
}

impl Default for TriangleDiagonal {
    fn default() -> Self {
        Self::Primary
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Hash)]
pub enum VoxelFace { Top, Bottom, North, South, East, West, }

impl<'de> Deserialize<'de> for VoxelFace {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct VoxelFaceVisitor;

        impl<'de> Visitor<'de> for VoxelFaceVisitor {
            type Value = VoxelFace;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("one of: top, bottom, north, south, east, west")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value.to_lowercase().as_str() {
                    "top"    => Ok(VoxelFace::Top),
                    "bottom" => Ok(VoxelFace::Bottom),
                    "north"  => Ok(VoxelFace::North),
                    "south"  => Ok(VoxelFace::South),
                    "east"   => Ok(VoxelFace::East),
                    "west"   => Ok(VoxelFace::West),
                    other    => Err(E::custom(format!("unknown face: {}", other))),
                }
            }
        }

        deserializer.deserialize_str(VoxelFaceVisitor)
    }
}

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
            Self::Top    => [[0.0, 1.0, 0.0], [1.0, 1.0, 0.0], [1.0, 1.0, 1.0], [0.0, 1.0, 1.0]],
            Self::Bottom => [[0.0, 0.0, 1.0], [1.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 0.0, 0.0]],
            Self::North  => [[1.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 1.0, 1.0], [1.0, 1.0, 1.0]],
            Self::South  => [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]],
            Self::East   => [[1.0, 0.0, 0.0], [1.0, 0.0, 1.0], [1.0, 1.0, 1.0], [1.0, 1.0, 0.0]],
            Self::West   => [[0.0, 0.0, 1.0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 1.0, 1.0]],
        }
    }

    pub fn quad_uvs(self) -> [[f32; 2]; 4] {
        [[1.0, 1.0], [0.0, 1.0], [0.0, 0.0], [1.0, 0.0]]
    }

    pub fn triangles(self, diagonal: TriangleDiagonal) -> [[f32; 3]; 6] {
        let quad = self.quad_vertices();
        match diagonal {
            TriangleDiagonal::Primary => {
                [
                    quad[0], quad[1], quad[2],
                    quad[0], quad[2], quad[3],
                ]
            }
            TriangleDiagonal::Secondary => {
                [
                    quad[0], quad[1], quad[3],
                    quad[1], quad[2], quad[3],
                ]
            }
        }
    }

    pub fn triangle_uvs(self, diagonal: TriangleDiagonal) -> [[f32; 2]; 6] {
        let quad_uvs = self.quad_uvs();
        match diagonal {
            TriangleDiagonal::Primary => {
                [
                    quad_uvs[0], quad_uvs[1], quad_uvs[2],
                    quad_uvs[0], quad_uvs[2], quad_uvs[3],
                ]
            }
            TriangleDiagonal::Secondary => {
                [
                    quad_uvs[0], quad_uvs[1], quad_uvs[3],
                    quad_uvs[1], quad_uvs[2], quad_uvs[3],
                ]
            }
        }
    }
}
