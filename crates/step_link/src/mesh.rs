//! Tessellation : conversion d'un [`Solid`] B-Rep en maillage triangulaire.
//!
//! Les arêtes sont discrétisées une fois et partagées entre faces voisines.
//! Chaque face est ensuite projetée dans l'espace de sa surface et triangulée par
//! *ear clipping*. Plans, cylindres et cônes sont pris en charge ; les autres
//! surfaces sont ignorées pour l'instant.

use crate::brep::{
    BSplineCurve, Curve, Edge, EdgeId, Face, Frame, Loop, Point3, Solid, Surface, dot, normalize,
    sub,
};
use std::collections::HashMap;
use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, TAU};

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

type EdgeCache = HashMap<EdgeId, Vec<Point3>>;

// ---------------------------------------------------------------------------
// Discrétisation des arêtes
// ---------------------------------------------------------------------------

/// Points d'une arête, de `start` à `end`, extrémités comprises.
fn edge_polyline(solid: &Solid, edge: &Edge, tolerance: f64) -> Vec<Point3> {
    let (Some(a), Some(b)) = (solid.vertices.get(edge.start), solid.vertices.get(edge.end)) else {
        return Vec::new();
    };
    let (a, b) = (a.position, b.position);
    let (from, to) = if edge.same_sense { (a, b) } else { (b, a) };
    let mut points = sample_curve(&edge.curve, from, to, tolerance);
    if !edge.same_sense {
        points.reverse();
    }
    if let Some(first) = points.first_mut() {
        *first = a;
    }
    if let Some(last) = points.last_mut() {
        *last = b;
    }
    points
}

/// Échantillonne la courbe dans son sens naturel, de `from` à `to`.
fn sample_curve(curve: &Curve, from: Point3, to: Point3, tolerance: f64) -> Vec<Point3> {
    match curve {
        Curve::Line => vec![from, to],
        Curve::Circle { frame, radius } => {
            sample_conic(frame, *radius, *radius, from, to, tolerance)
        }
        Curve::Ellipse {
            frame,
            semi_major,
            semi_minor,
        } => sample_conic(frame, *semi_major, *semi_minor, from, to, tolerance),
        Curve::BSpline(spline) => sample_bspline(spline, from, to),
    }
}

/// Arc de cercle ou d'ellipse parcouru dans le sens trigonométrique ; `from == to` donne un tour complet.
fn sample_conic(
    frame: &Frame,
    a: f64,
    b: f64,
    from: Point3,
    to: Point3,
    tolerance: f64,
) -> Vec<Point3> {
    let y = frame.y();
    let angle = |p: Point3| {
        let d = sub(p, frame.origin);
        (dot(d, y) / b).atan2(dot(d, frame.x) / a)
    };
    let start = angle(from);
    let mut sweep = angle(to) - start;
    while sweep <= 1e-9 {
        sweep += TAU;
    }

    let radius = a.max(b);
    let step = if tolerance < radius {
        2.0 * (1.0 - tolerance / radius).acos()
    } else {
        FRAC_PI_2
    }
    .clamp(0.02, FRAC_PI_4);
    let segments = (sweep / step).ceil().max(1.0) as usize;

    (0..=segments)
        .map(|i| {
            let t = start + sweep * i as f64 / segments as f64;
            let (s, c) = t.sin_cos();
            std::array::from_fn(|k| frame.origin[k] + a * c * frame.x[k] + b * s * y[k])
        })
        .collect()
}

/// Courbe B-spline complète (non rationnelle), orientée pour partir de `from`.
fn sample_bspline(spline: &BSplineCurve, from: Point3, to: Point3) -> Vec<Point3> {
    let degree = spline.degree;
    let count = spline.control_points.len();
    if count <= degree || spline.knots.len() != count + degree + 1 {
        return vec![from, to];
    }
    let (t0, t1) = (spline.knots[degree], spline.knots[count]);
    let segments = (count * 4).max(8);
    let mut points: Vec<Point3> = (0..=segments)
        .map(|i| {
            let t = t0 + (t1 - t0) * i as f64 / segments as f64;
            de_boor(degree, &spline.knots, &spline.control_points, t)
        })
        .collect();
    let distance = |p: Point3| {
        let d = sub(p, from);
        dot(d, d)
    };
    if let (Some(&first), Some(&last)) = (points.first(), points.last())
        && distance(first) > distance(last)
    {
        points.reverse();
    }
    points
}

fn de_boor(degree: usize, knots: &[f64], control: &[Point3], t: f64) -> Point3 {
    let count = control.len();
    let mut span = degree;
    while span + 1 < count && t >= knots[span + 1] {
        span += 1;
    }
    let mut d: Vec<Point3> = (0..=degree).map(|j| control[j + span - degree]).collect();
    for r in 1..=degree {
        for j in (r..=degree).rev() {
            let i = j + span - degree;
            let denom = knots[i + degree - r + 1] - knots[i];
            let alpha = if denom.abs() < 1e-12 {
                0.0
            } else {
                (t - knots[i]) / denom
            };
            d[j] = std::array::from_fn(|c| (1.0 - alpha) * d[j - 1][c] + alpha * d[j][c]);
        }
    }
    d[degree]
}

/// Points d'une boucle, sans répéter le point de jonction entre deux arêtes.
fn loop_polyline(
    solid: &Solid,
    cache: &mut EdgeCache,
    face_loop: &Loop,
    tolerance: f64,
) -> Vec<Point3> {
    let mut out = Vec::new();
    for oriented in &face_loop.edges {
        let Some(edge) = solid.edges.get(oriented.edge) else {
            continue;
        };
        let points = cache
            .entry(oriented.edge)
            .or_insert_with(|| edge_polyline(solid, edge, tolerance));
        let n = points.len();
        if n < 2 {
            continue;
        }
        if oriented.forward {
            out.extend_from_slice(&points[..n - 1]);
        } else {
            out.extend(points[1..].iter().rev().copied());
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Projection d'une face dans l'espace de sa surface
// ---------------------------------------------------------------------------

enum Chart {
    Plane(Frame),
    Cylinder(Frame),
    Cone(Frame, f64),
}

impl Chart {
    fn from_surface(surface: &Surface) -> Option<Self> {
        match surface {
            Surface::Plane { frame } => Some(Self::Plane(*frame)),
            Surface::Cylinder { frame, .. } => Some(Self::Cylinder(*frame)),
            Surface::Cone {
                frame, semi_angle, ..
            } => Some(Self::Cone(*frame, *semi_angle)),
            _ => None,
        }
    }

    fn frame(&self) -> &Frame {
        match self {
            Self::Plane(f) | Self::Cylinder(f) | Self::Cone(f, _) => f,
        }
    }

    fn is_periodic(&self) -> bool {
        !matches!(self, Self::Plane(_))
    }

    /// `(x, y)` pour un plan, `(angle, hauteur)` sinon.
    fn uv(&self, p: Point3) -> [f64; 2] {
        let f = self.frame();
        let d = sub(p, f.origin);
        let y = f.y();
        match self {
            Self::Plane(_) => [dot(d, f.x), dot(d, y)],
            _ => [dot(d, y).atan2(dot(d, f.x)), dot(d, f.z)],
        }
    }

    /// Normale de la surface en `p` (avant prise en compte de `same_sense`).
    fn normal(&self, p: Point3) -> Point3 {
        let f = self.frame();
        let d = sub(p, f.origin);
        let h = dot(d, f.z);
        let radial = normalize(std::array::from_fn(|k| d[k] - h * f.z[k]));
        match self {
            Self::Plane(_) => f.z,
            Self::Cylinder(_) => radial,
            Self::Cone(_, angle) => {
                let (s, c) = angle.sin_cos();
                std::array::from_fn(|k| c * radial[k] - s * f.z[k])
            }
        }
    }
}

fn mean_radius(frame: &Frame, loops: &[Vec<Point3>]) -> f64 {
    let (sum, count) = loops.iter().flatten().fold((0.0, 0usize), |(s, n), &p| {
        let d = sub(p, frame.origin);
        let h = dot(d, frame.z);
        (s + (dot(d, d) - h * h).max(0.0).sqrt(), n + 1)
    });
    if count == 0 || sum < 1e-9 {
        1.0
    } else {
        sum / count as f64
    }
}

/// Rend les angles continus le long d'une boucle.
fn unwrap_angles(uv: &mut [[f64; 2]]) {
    for i in 1..uv.len() {
        let previous = uv[i - 1][0];
        uv[i][0] += TAU * ((previous - uv[i][0]) / TAU).round();
    }
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

    let periodic = chart.is_periodic();
    let mut loops2d: Vec<Vec<[f64; 2]>> = loops3d
        .iter()
        .map(|points| {
            let mut uv: Vec<[f64; 2]> = points.iter().map(|&p| chart.uv(p)).collect();
            if periodic {
                unwrap_angles(&mut uv);
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

    if periodic {
        let mean = |l: &[[f64; 2]]| l.iter().map(|p| p[0]).sum::<f64>() / l.len() as f64;
        let reference = mean(&loops2d[0]);
        for hole in loops2d.iter_mut().skip(1) {
            let shift = TAU * ((reference - mean(hole)) / TAU).round();
            for p in hole.iter_mut() {
                p[0] += shift;
            }
        }
        let scale = mean_radius(chart.frame(), &loops3d);
        for p in loops2d.iter_mut().flatten() {
            p[0] *= scale;
        }
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

// ---------------------------------------------------------------------------
// Triangulation 2D (ear clipping avec trous)
// ---------------------------------------------------------------------------

const EPS: f64 = 1e-12;

fn cross2(o: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

fn same(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9
}

fn area(poly: &[[f64; 2]]) -> f64 {
    let n = poly.len();
    (0..n)
        .map(|i| {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        / 2.0
}

fn segments_cross(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    let (d1, d2) = (cross2(a, b, c), cross2(a, b, d));
    let (d3, d4) = (cross2(c, d, a), cross2(c, d, b));
    ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
}

fn crosses_ring(pts: &[[f64; 2]], a: [f64; 2], b: [f64; 2], ring: &[usize]) -> bool {
    (0..ring.len()).any(|i| {
        let c = pts[ring[i]];
        let d = pts[ring[(i + 1) % ring.len()]];
        !(same(a, c) || same(a, d) || same(b, c) || same(b, d)) && segments_cross(a, b, c, d)
    })
}

/// Plus court segment entre un sommet de `ring` et un sommet de `hole` qui ne coupe aucun contour.
fn find_bridge(
    pts: &[[f64; 2]],
    ring: &[usize],
    hole: &[usize],
    pending: &[Vec<usize>],
) -> Option<(usize, usize)> {
    let mut candidates: Vec<(f64, usize, usize)> = Vec::new();
    for (ri, &r) in ring.iter().enumerate() {
        for (hi, &h) in hole.iter().enumerate() {
            let (dx, dy) = (pts[r][0] - pts[h][0], pts[r][1] - pts[h][1]);
            candidates.push((dx * dx + dy * dy, ri, hi));
        }
    }
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
    candidates
        .into_iter()
        .find(|&(_, ri, hi)| {
            let (a, b) = (pts[ring[ri]], pts[hole[hi]]);
            !crosses_ring(pts, a, b, ring)
                && !crosses_ring(pts, a, b, hole)
                && !pending.iter().any(|p| crosses_ring(pts, a, b, p))
        })
        .map(|(_, ri, hi)| (ri, hi))
}

fn is_ear(pts: &[[f64; 2]], ring: &[usize], p: usize, c: usize, n: usize) -> bool {
    let (a, b, d) = (pts[p], pts[c], pts[n]);
    if cross2(a, b, d) <= EPS {
        return false;
    }
    !ring.iter().any(|&q| {
        if q == p || q == c || q == n {
            return false;
        }
        let r = pts[q];
        if same(r, a) || same(r, b) || same(r, d) {
            return false;
        }
        cross2(a, b, r) > EPS && cross2(b, d, r) > EPS && cross2(d, a, r) > EPS
    })
}

fn clip_ears(pts: &[[f64; 2]], mut ring: Vec<usize>) -> Vec<[usize; 3]> {
    let mut triangles = Vec::new();
    while ring.len() > 3 {
        let n = ring.len();
        let triple = |i: usize| (ring[(i + n - 1) % n], ring[i], ring[(i + 1) % n]);
        let ear = (0..n).find(|&i| {
            let (p, c, nx) = triple(i);
            is_ear(pts, &ring, p, c, nx)
        });
        let index = ear.unwrap_or_else(|| {
            // Contour dégénéré : on retire le sommet le plus convexe pour terminer.
            let convexity = |i: usize| {
                let (p, c, nx) = triple(i);
                cross2(pts[p], pts[c], pts[nx])
            };
            (0..n)
                .max_by(|&a, &b| convexity(a).total_cmp(&convexity(b)))
                .unwrap_or(0)
        });
        let (p, c, nx) = triple(index);
        if cross2(pts[p], pts[c], pts[nx]) > EPS {
            triangles.push([p, c, nx]);
        }
        ring.remove(index);
    }
    if ring.len() == 3 && cross2(pts[ring[0]], pts[ring[1]], pts[ring[2]]) > EPS {
        triangles.push([ring[0], ring[1], ring[2]]);
    }
    triangles
}

/// Triangule un polygone avec trous.
///
/// Les indices retournés visent les points de `outer` suivis de ceux de chaque trou,
/// dans l'ordre donné. Les triangles sont orientés dans le sens trigonométrique.
fn triangulate(outer: &[[f64; 2]], holes: &[Vec<[f64; 2]>]) -> Vec<[usize; 3]> {
    let mut pts: Vec<[f64; 2]> = outer.to_vec();
    let mut ring: Vec<usize> = (0..outer.len()).collect();
    if area(outer) < 0.0 {
        ring.reverse();
    }

    let mut hole_rings: Vec<Vec<usize>> = Vec::new();
    for hole in holes {
        let start = pts.len();
        pts.extend_from_slice(hole);
        let mut hole_ring: Vec<usize> = (start..pts.len()).collect();
        if area(hole) > 0.0 {
            hole_ring.reverse();
        }
        hole_rings.push(hole_ring);
    }
    let max_x = |ring: &[usize]| ring.iter().map(|&i| pts[i][0]).fold(f64::MIN, f64::max);
    hole_rings.sort_by(|a, b| max_x(b).total_cmp(&max_x(a)));

    for (n, hole) in hole_rings.iter().enumerate() {
        let Some((ri, hi)) = find_bridge(&pts, &ring, hole, &hole_rings[n + 1..]) else {
            continue;
        };
        let mut merged = Vec::with_capacity(ring.len() + hole.len() + 2);
        merged.extend_from_slice(&ring[..=ri]);
        merged.extend_from_slice(&hole[hi..]);
        merged.extend_from_slice(&hole[..=hi]);
        merged.push(ring[ri]);
        merged.extend_from_slice(&ring[ri + 1..]);
        ring = merged;
    }

    clip_ears(&pts, ring)
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
    fn triangulates_square_with_hole() {
        let outer = [[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]];
        let hole = vec![[1.0, 1.0], [1.0, 3.0], [3.0, 3.0], [3.0, 1.0]];
        let triangles = triangulate(&outer, &[hole.clone()]);
        let pts: Vec<[f64; 2]> = outer.iter().copied().chain(hole).collect();
        let total: f64 = triangles
            .iter()
            .map(|t| cross2(pts[t[0]], pts[t[1]], pts[t[2]]) / 2.0)
            .sum();
        assert_eq!(triangles.len(), 8);
        assert!((total - 12.0).abs() < 1e-9);
    }

    #[test]
    fn samples_full_circle() {
        let frame = Frame::from_normal([0.0; 3], [0.0, 0.0, 1.0]);
        let start = [10.0, 0.0, 0.0];
        let points = sample_conic(&frame, 10.0, 10.0, start, start, 0.05);
        assert!(points.len() > 16);
        let (first, last) = (points[0], points[points.len() - 1]);
        let gap = sub(first, last);
        assert!(dot(gap, gap).sqrt() < 1e-9);
        for p in &points {
            assert!((dot(*p, *p).sqrt() - 10.0).abs() < 1e-9);
        }
    }
}