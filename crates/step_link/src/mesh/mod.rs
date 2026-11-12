//! Tessellation : conversion d'un [`Solid`] B-Rep en maillage triangulaire.
//!
//! Les arêtes sont discrétisées une fois et partagées entre faces voisines.
//! Chaque face est ensuite projetée dans l'espace de sa surface et triangulée par
//! *ear clipping*. Plans, cylindres, cônes, sphères, tores et surfaces B-spline sont pris en charge.
//!
//! Organisation du module :
//! - `edges` : discrétisation des arêtes et des boucles
//! - `spline` : évaluation de Boor, courbes et surfaces B-spline
//! - `chart` : projection d'une surface dans son espace paramétrique
//! - `triangulate` : triangulation 2D avec trous

mod chart;
mod edges;
mod spline;
mod triangulate;

use crate::brep::{Face, Point3, Solid, sub, dot};
use chart::{Chart, unwrap_axis};
use edges::{EdgeCache, loop_polyline};
use std::f64::consts::TAU;
use triangulate::{area, triangulate};

/// Flèche maximale par défaut, dans l'unité du modèle (millimètres pour la plupart des exports CAO).
pub const DEFAULT_TOLERANCE: f64 = 0.05;

/// Maillage triangulaire indépendant de tout moteur de rendu.
///
/// Les sommets sont dupliqués par face afin que chaque face possède ses propres normales.
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

/// Tessellise un solide complet avec la tolérance par défaut.
pub fn tessellate(solid: &Solid) -> MeshData {
    tessellate_with_tolerance(solid, DEFAULT_TOLERANCE)
}

/// Tessellise un solide ; `tolerance` est la flèche maximale tolérée sur les courbes.
pub fn tessellate_with_tolerance(solid: &Solid, tolerance: f64) -> MeshData {
    let tolerance = tolerance.max(1e-6);
    let mut mesh = MeshData::default();
    let mut cache = EdgeCache::new();
    for face in solid.faces() {
        tessellate_face(solid, face, &mut cache, tolerance, &mut mesh);
    }
    mesh
}

pub(crate) fn dist(a: Point3, b: Point3) -> f64 {
    let d = sub(a, b);
    dot(d, d).sqrt()
}

pub(crate) fn scaled(v: Point3, k: f64) -> Point3 {
    [v[0] * k, v[1] * k, v[2] * k]
}

fn tessellate_face(
    solid: &Solid,
    face: &Face,
    cache: &mut EdgeCache,
    tolerance: f64,
    mesh: &mut MeshData,
) {
    let Some(chart) = Chart::from_surface(&face.surface) else {
        return;
    };
    let mut loops3d: Vec<Vec<Point3>> = std::iter::once(&face.outer_loop)
        .chain(face.inner_loops.iter())
        .map(|l| loop_polyline(solid, cache, l, tolerance))
        .filter(|points| points.len() >= 3)
        .collect();
    if loops3d.is_empty() {
        return;
    }

    let periodic = chart.periodic();
    let mut loops2d: Vec<Vec<[f64; 2]>> = loops3d
        .iter()
        .map(|points| {
            let mut uv: Vec<[f64; 2]> = points.iter().map(|&p| chart.uv(p)).collect();
            for axis in 0..2 {
                if periodic[axis] {
                    unwrap_axis(&mut uv, axis);
                }
            }
            uv
        })
        .collect();

    // Le plus grand contour est le contour externe.
    let outer_index = (0..loops2d.len())
        .max_by(|&a, &b| area(&loops2d[a]).abs().total_cmp(&area(&loops2d[b]).abs()))
        .unwrap_or(0);
    loops2d.swap(0, outer_index);
    loops3d.swap(0, outer_index);

    // Les trous sont décalés d'un nombre entier de tours pour rester près du contour externe.
    for axis in 0..2 {
        if !periodic[axis] {
            continue;
        }
        let mean = |l: &[[f64; 2]]| l.iter().map(|p| p[axis]).sum::<f64>() / l.len() as f64;
        let reference = mean(&loops2d[0]);
        for hole in loops2d.iter_mut().skip(1) {
            let shift = TAU * ((reference - mean(hole)) / TAU).round();
            for p in hole.iter_mut() {
                p[axis] += shift;
            }
        }
    }

    let scale = chart.scale(&loops3d);
    for p in loops2d.iter_mut().flatten() {
        p[0] *= scale[0];
        p[1] *= scale[1];
    }

    let triangles = triangulate(&loops2d[0], &loops2d[1..]);

    let sign = if face.same_sense { 1.0 } else { -1.0 };
    let base = mesh.positions.len() as u32;
    for &p in loops3d.iter().flatten() {
        mesh.positions.push(p.map(|c| c as f32));
        mesh.normals.push(chart.normal(p).map(|c| (c * sign) as f32));
    }
    for [a, b, c] in triangles {
        let (b, c) = if face.same_sense { (b, c) } else { (c, b) };
        mesh.indices
            .extend_from_slice(&[base + a as u32, base + b as u32, base + c as u32]);
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