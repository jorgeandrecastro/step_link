//! Tessellation: conversion of a B-Rep [`Solid`] into a triangle mesh.
//!
//! Edges are discretized once and shared between adjacent faces.
//! Each face is then projected into its surface's parameter space and triangulated using
//! constrained Delaunay (`spade`): contours are constrained edges, and curved surfaces
//! (spheres, tori, B-splines) receive interior points with a density based on the tolerance.
//! For pathological contours, the algorithm falls back to *ear clipping*.
//! Planes, cylinders, cones, spheres, tori, and B-spline surfaces are supported.
//!
//! Module organization:
//! - `edges`: discretization of edges and loops
//! - `spline`: de Boor evaluation, B-spline curves and surfaces
//! - `chart`: projection of a surface into its parameter space
//! - `triangulate`: 2D triangulation with holes and interior points

mod chart;
mod edges;
mod spline;
mod triangulate;

use crate::brep::{Face, Point3, Solid, dot, sub};
use chart::{Chart, unwrap_axis};
use edges::{EdgeCache, loop_polyline};
use std::f64::consts::TAU;
use triangulate::{Triangulation, area, triangulate_refined};

/// Default maximum sagitta, in model units (millimeters for most CAD exports).
pub const DEFAULT_TOLERANCE: f64 = 0.05;

/// Color for faces without an assigned color in a mesh where other faces are colored.
pub const DEFAULT_COLOR: [f32; 3] = [0.8, 0.8, 0.8];

/// Triangle mesh independent of any rendering engine.
///
/// Vertices are duplicated per face so that each face has its own normals.
/// Interior vertices on curved surfaces have analytical normals.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MeshData {
    /// Vertex positions.
    pub positions: Vec<[f32; 3]>,
    /// Per-vertex normals (same length as `positions`).
    pub normals: Vec<[f32; 3]>,
    /// Per-vertex RGB colors (sRGB). Empty if no faces are colored; otherwise, the same length as `positions`.
    pub colors: Vec<[f32; 3]>,
    /// Indices de triangles (3 par triangle).
    pub indices: Vec<u32>,
}

impl MeshData {
    /// Number of triangles in the mesh.
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
}

/// Tessellates an entire solid using the default tolerance.
pub fn tessellate(solid: &Solid) -> MeshData {
    tessellate_with_tolerance(solid, DEFAULT_TOLERANCE)
}

/// Tessellates a solid; `tolerance` is the maximum permitted curve sagitta.
pub fn tessellate_with_tolerance(solid: &Solid, tolerance: f64) -> MeshData {
    let tolerance = tolerance.max(1e-6);
    let mut mesh = MeshData::default();
    let mut cache = EdgeCache::new();
    let colored = solid.color.is_some() || solid.faces().any(|f| f.color.is_some());
    for face in solid.faces() {
        tessellate_face(solid, face, colored, &mut cache, tolerance, &mut mesh);
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
    colored: bool,
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

    // The largest contour is the outer contour.
    let outer_index = (0..loops2d.len())
        .max_by(|&a, &b| area(&loops2d[a]).abs().total_cmp(&area(&loops2d[b]).abs()))
        .unwrap_or(0);
    loops2d.swap(0, outer_index);
    loops3d.swap(0, outer_index);

    // Shift holes by whole revolutions to keep them near the outer contour.
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

    // Interior-point spacing, converted to scaled 2D space.
    let step = chart
        .param_steps(tolerance)
        .map(|s| [s[0] * scale[0], s[1] * scale[1]]);
    let Triangulation {
        points,
        source,
        triangles,
    } = triangulate_refined(&loops2d[0], &loops2d[1..], step);

    let sign = if face.same_sense { 1.0 } else { -1.0 };
    let color = face.color.or(solid.color).unwrap_or(DEFAULT_COLOR);
    let base = mesh.positions.len() as u32;
    let boundary: Vec<Point3> = loops3d.iter().flatten().copied().collect();
    for (q, &src) in points.iter().zip(&source) {
        let (p, normal) = match boundary.get(src) {
            // Contour point: exact position, shared with adjacent faces.
            Some(&p) => (p, chart.normal(p)),
            // Interior point: position and analytical normal from (u, v).
            None => {
                let uv = [q[0] / scale[0], q[1] / scale[1]];
                let p = chart.point(uv);
                (p, chart.normal_uv(uv, p))
            }
        };
        mesh.positions.push(p.map(|c| c as f32));
        mesh.normals.push(normal.map(|c| (c * sign) as f32));
        if colored {
            mesh.colors.push(color);
        }
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

    #[test]
    fn indices_stay_in_bounds() {
        let mesh = tessellate(&Solid::cuboid(1.0, 2.0, 3.0));
        assert!(mesh.indices.iter().all(|&i| (i as usize) < mesh.positions.len()));
    }

    #[test]
    fn colors_follow_solid_and_face() {
        let mut solid = Solid::cuboid(1.0, 1.0, 1.0);
        assert!(tessellate(&solid).colors.is_empty());

        solid.color = Some([1.0, 0.0, 0.0]);
        solid.shells[0].faces[0].color = Some([0.0, 1.0, 0.0]);
        let mesh = tessellate(&solid);
        assert_eq!(mesh.colors.len(), mesh.positions.len());
        assert_eq!(mesh.colors.iter().filter(|c| **c == [0.0, 1.0, 0.0]).count(), 4);
        assert_eq!(mesh.colors.iter().filter(|c| **c == [1.0, 0.0, 0.0]).count(), 20);
    }
}