//! Tessellation : conversion d'un [`Solid`] B-Rep en maillage triangulaire.

use crate::brep::{Face, Solid};

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
    for face in solid.shells.iter().flat_map(|s| s.faces.iter()) {
        tessellate_face(face, &mut mesh);
    }
    mesh
}

/// Tessellise une face plane par triangulation en éventail.
///
/// Correct pour les polygones convexes ; les contours concaves et les surfaces
/// courbes nécessiteront un algorithme dédié (ex. *ear clipping*).
fn tessellate_face(face: &Face, mesh: &mut MeshData) {
    let verts: Vec<_> = face.vertices().collect();
    if verts.len() < 3 {
        return;
    }
    let base = mesh.positions.len() as u32;
    let normal = face.normal.map(|c| c as f32);
    for v in &verts {
        mesh.positions.push(v.position.map(|c| c as f32));
        mesh.normals.push(normal);
    }
    for i in 1..(verts.len() as u32 - 1) {
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
