//! Tessellation : conversion d'un [`Solid`] B-Rep en maillage triangulaire.

use crate::brep::{Face, Solid, Surface};

/// Maillage triangulaire indépendant de tout moteur de rendu.
///
/// Le *flat shading* est utilisé : les sommets sont dupliqués par face
/// afin que chaque face possède sa propre normale.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MeshData {
    /// Positions des sommets.
    pub positions: Vec<[f32; 3]>,
    /// Normales par sommet (même longueur que `positions`).
    pub normals: Vec<[f32; 3]>,
    /// Indices de triangles (3 par triangle).
    pub indices: Vec<u32>,
}

impl MeshData {
    /// Nombre de triangles du maillage.
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
}

/// Tessellise un solide complet.
pub fn tessellate(solid: &Solid) -> MeshData {
    let mut mesh = MeshData::default();
    for face in solid.faces() {
        tessellate_face(solid, face, &mut mesh);
    }
    mesh
}


/// Tessellise une face plane par triangulation en éventail de sa boucle externe.
///
/// Les faces non planes et les boucles internes sont ignorées pour l'instant
/// (étape 4 de la feuille de route).
fn tessellate_face(solid: &Solid, face: &Face, mesh: &mut MeshData) {
    let Surface::Plane { frame } = &face.surface else {
        return;
    };
    let points = solid.loop_points(&face.outer_loop);
    if points.len() < 3 {
        return;
    }
    let sign = if face.same_sense { 1.0 } else { -1.0 };
    let normal = frame.z.map(|c| (c * sign) as f32);
    let base = mesh.positions.len() as u32;
    for p in &points {
        mesh.positions.push(p.map(|c| c as f32));
        mesh.normals.push(normal);
    }
    for i in 1..(points.len() as u32 - 1) {
        mesh.indices.extend_from_slice(&[base, base + i, base + i + 1]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cuboid_has_twelve_triangles() {
        let mesh = tessellate(&Solid::cuboid(2.0, 2.0, 2.0));
        assert_eq!(mesh.triangle_count(), 12);
        assert_eq!(mesh.positions.len(), 24);
        assert_eq!(mesh.normals.len(), 24);
    }
}
