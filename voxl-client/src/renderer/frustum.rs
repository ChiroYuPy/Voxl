//! Frustum culling - Simple et direct
//!
//! Extrait 6 plans depuis la matrice view-proj et teste les AABB.

use glam::{Mat4, Vec3, Vec4};

pub struct Frustum {
    planes: [Vec4; 6], // left, right, bottom, top, near, far
}

impl Frustum {
    /// Extrait les 6 plans depuis view_proj
    pub fn from_view_proj(vp: Mat4) -> Self {
        // Extrait les lignes depuis les colonnes (stockage column-major)
        let r0 = Vec4::new(vp.x_axis.x, vp.y_axis.x, vp.z_axis.x, vp.w_axis.x); // ligne 0
        let r1 = Vec4::new(vp.x_axis.y, vp.y_axis.y, vp.z_axis.y, vp.w_axis.y); // ligne 1
        let r2 = Vec4::new(vp.x_axis.z, vp.y_axis.z, vp.z_axis.z, vp.w_axis.z); // ligne 2
        let r3 = Vec4::new(vp.x_axis.w, vp.y_axis.w, vp.z_axis.w, vp.w_axis.w); // ligne 3

        // Left: row3 + row0
        let left = r3 + r0;
        // Right: row3 - row0
        let right = r3 - r0;
        // Bottom: row3 + row1
        let bottom = r3 + r1;
        // Top: row3 - row1
        let top = r3 - r1;
        // Near: row3 + row2
        let near = r3 + r2;
        // Far: row3 - row2
        let far = r3 - r2;

        Self {
            planes: [left, right, bottom, top, near, far],
        }
    }

    /// Teste si une AABB est visible (méthode p-vertex)
    pub fn intersects_aabb(&self, min: Vec3, max: Vec3) -> bool {
        for plane in &self.planes {
            // P-vertex : le coin le plus loin dans la direction du plan
            let px = if plane.x >= 0.0 { max.x } else { min.x };
            let py = if plane.y >= 0.0 { max.y } else { min.y };
            let pz = if plane.z >= 0.0 { max.z } else { min.z };

            // Distance signée : dot(normal, point) + distance
            let dist = plane.x * px + plane.y * py + plane.z * pz + plane.w;

            // Si le p-vertex est derrière le plan, l'AABB est hors du frustum
            if dist < 0.0 {
                return false;
            }
        }
        true
    }

    /// Teste si un chunk est visible
    pub fn is_chunk_visible(&self, cx: i32, cy: i32, cz: i32) -> bool {
        use voxl_common::voxel::CHUNK_SIZE;
        let min = Vec3::new(cx as f32 * CHUNK_SIZE as f32, cy as f32 * CHUNK_SIZE as f32, cz as f32 * CHUNK_SIZE as f32);
        let max = min + Vec3::splat(CHUNK_SIZE as f32);
        self.intersects_aabb(min, max)
    }
}
