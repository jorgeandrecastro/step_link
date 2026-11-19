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

use crate::brep::{Face, Point3, Solid, Surface, cross, dot, sub};
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
    let tolerance = tolerance.max(1e-12);
    let mut mesh = MeshData::default();
    let mut cache = EdgeCache::new();
    let colored = solid.color.is_some() || solid.faces().any(|f| f.color.is_some());
    for (face_index, face) in solid.faces().enumerate() {
        tessellate_face(
            solid, face, face_index, colored, &mut cache, tolerance, &mut mesh,
        );
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

/// Number of turns a loop makes around the periodic `u` axis (angles already unwrapped).
fn winding(uv: &[[f64; 2]]) -> i32 {
    let (first, last) = (uv[0][0], uv[uv.len() - 1][0]);
    // Closing step from the last point back to the first, wrapped to [-pi, pi].
    let mut closing = first - last;
    closing -= TAU * (closing / TAU).round();
    ((last - first + closing) / TAU).round() as i32
}

/// A full circle is a straight segment in `(u, v)` (it winds around the axis without closing).
/// A cylinder, cone or sphere zone bounded by two such circles and with no seam edge
/// (typical of drilled holes) would therefore be triangulated as garbage.
///
/// The two circles are merged into one parallelogram: A left to right, a cut, B right to left,
/// and the same cut closed on the left. In 3D both cuts are the same line (an artificial seam).
fn merge_wound_loops(loops2d: &mut Vec<Vec<[f64; 2]>>, loops3d: &mut Vec<Vec<Point3>>) {
    let wound: Vec<usize> = (0..loops2d.len())
        .filter(|&i| loops2d[i].len() >= 3 && winding(&loops2d[i]) != 0)
        .collect();
    if wound.len() != 2 {
        return;
    }
    let (ia, ib) = (wound[0], wound[1]);
    if winding(&loops2d[ia]).abs() != 1 || winding(&loops2d[ib]).abs() != 1 {
        return;
    }

    let mut a2 = loops2d[ia].clone();
    let mut a3 = loops3d[ia].clone();
    let mut b2 = loops2d[ib].clone();
    let mut b3 = loops3d[ib].clone();

    // A goes towards +u, B towards -u.
    if winding(&a2) < 0 {
        a2.reverse();
        a3.reverse();
    }
    if winding(&b2) > 0 {
        b2.reverse();
        b3.reverse();
    }

    // Shift B by whole turns so that its start is close to the end of A's turn.
    let ua = a2[0][0];
    let shift = TAU * ((ua + TAU - b2[0][0]) / TAU).round();
    for p in b2.iter_mut() {
        p[0] += shift;
    }

    let mut poly2 = a2.clone();
    let mut poly3 = a3.clone();
    poly2.push([ua + TAU, a2[0][1]]);
    poly3.push(a3[0]);

    let b0 = b2[0];
    poly2.extend(b2.iter().copied());
    poly3.extend(b3.iter().copied());
    poly2.push([b0[0] - TAU, b0[1]]);
    poly3.push(b3[0]);

    // `wound` is ascending: remove the larger index first.
    for i in [ib, ia] {
        loops2d.remove(i);
        loops3d.remove(i);
    }
    loops2d.push(poly2);
    loops3d.push(poly3);
}

fn tessellate_face(
    solid: &Solid,
    face: &Face,
    face_index: usize,
    colored: bool,
    cache: &mut EdgeCache,
    tolerance: f64,
    mesh: &mut MeshData,
) {
    let Some(chart) = Chart::from_surface(&face.surface) else {
        eprintln!("face {face_index} ignorée : projection de surface impossible");
        return;
    };
    let outer = loop_polyline(solid, cache, &face.outer_loop, tolerance);
    if outer.len() < 3 {
        eprintln!(
            "face {face_index} ignorée : contour extérieur dégénéré ({} points)",
            outer.len()
        );
        return;
    }
    let mut loops3d = vec![outer];
    for (loop_index, face_loop) in face.inner_loops.iter().enumerate() {
        let points = loop_polyline(solid, cache, face_loop, tolerance);
        if points.len() < 3 {
            eprintln!(
                "face {face_index} : contour interne {loop_index} ignoré ({} points)",
                points.len()
            );
        } else {
            loops3d.push(points);
        }
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

    // Scale computed from the real contours, before any loop is merged.
    let scale = chart.scale(&loops3d);

    // Two circles around the axis, without a seam edge: merge them into one band.
    if periodic[0] {
        merge_wound_loops(&mut loops2d, &mut loops3d);
    }

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
    let first_index = mesh.indices.len();
    for [a, b, c] in triangles {
        let (b, c) = if face.same_sense { (b, c) } else { (c, b) };
        mesh.indices
            .extend_from_slice(&[base + a as u32, base + b as u32, base + c as u32]);
    }
    if let Some(&[a, b, c]) = mesh.indices.get(first_index..first_index + 3) {
        let point = |index: u32| mesh.positions[index as usize].map(f64::from);
        let (pa, pb, pc) = (point(a), point(b), point(c));
        let normal = mesh.normals[a as usize].map(f64::from);
        if dot(cross(sub(pb, pa), sub(pc, pa)), normal) < 0.0 {
            let kind = if matches!(&face.surface, Surface::BSpline(_)) {
                "B-spline"
            } else {
                "autre surface"
            };
            eprintln!("face {face_index} ({kind}) : winding incohérent avec la normale");
        }
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

    #[test]
    fn two_wound_loops_merge_into_one_band() {
        let n = 8;
        let ring = |v: f64, dir: f64| -> Vec<[f64; 2]> {
            (0..n)
                .map(|i| [dir * TAU * i as f64 / n as f64, v])
                .collect()
        };
        let mut l2 = vec![ring(0.0, 1.0), ring(1.0, -1.0)];
        let mut l3 = vec![vec![[0.0; 3]; n], vec![[0.0; 3]; n]];
        merge_wound_loops(&mut l2, &mut l3);
        assert_eq!(l2.len(), 1);
        assert_eq!(l2[0].len(), 2 * n + 2);
        assert_eq!(l2[0].len(), l3[0].len());
    }
}