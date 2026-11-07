//! Tessellation : conversion d'un [`Solid`] B-Rep en maillage triangulaire.
//!
//! Les arêtes sont discrétisées une fois et partagées entre faces voisines.
//! Chaque face est ensuite projetée dans l'espace de sa surface et triangulée par
//! *ear clipping*. Plans, cylindres, cônes, tores et surfaces B-spline sont pris en
//! charge ; les sphères sont ignorées pour l'instant.

use crate::brep::{
    BSplineCurve, BSplineSurface, Curve, Edge, EdgeId, Face, Frame, Loop, Point3, Solid, Surface,
    cross, dot, normalize, sub,
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

fn dist(a: Point3, b: Point3) -> f64 {
    let d = sub(a, b);
    dot(d, d).sqrt()
}

fn scaled(v: Point3, k: f64) -> Point3 {
    [v[0] * k, v[1] * k, v[2] * k]
}

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

/// Courbe B-spline complète (rationnelle si des poids sont fournis), orientée pour partir de `from`.
fn sample_bspline(spline: &BSplineCurve, from: Point3, to: Point3) -> Vec<Point3> {
    let degree = spline.degree;
    let count = spline.control_points.len();
    if count <= degree || spline.knots.len() != count + degree + 1 {
        return vec![from, to];
    }

    // Courbe rationnelle : on interpole les points pondérés et les poids séparément.
    let (control, weight_track): (Vec<Point3>, Option<Vec<Point3>>) = match &spline.weights {
        Some(w) if w.len() == count => (
            spline
                .control_points
                .iter()
                .zip(w)
                .map(|(p, &w)| [p[0] * w, p[1] * w, p[2] * w])
                .collect(),
            Some(w.iter().map(|&w| [w, 0.0, 0.0]).collect()),
        ),
        _ => (spline.control_points.clone(), None),
    };

    let (t0, t1) = (spline.knots[degree], spline.knots[count]);
    let segments = (count * 4).max(8);
    let mut points: Vec<Point3> = (0..=segments)
        .map(|i| {
            let t = t0 + (t1 - t0) * i as f64 / segments as f64;
            let p = de_boor(degree, &spline.knots, &control, t);
            match &weight_track {
                Some(track) => {
                    let w = de_boor(degree, &spline.knots, track, t)[0];
                    if w.abs() > 1e-12 {
                        [p[0] / w, p[1] / w, p[2] / w]
                    } else {
                        p
                    }
                }
                None => p,
            }
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
// Surface B-spline : évaluation et projection d'un point
// ---------------------------------------------------------------------------

struct SplineChart {
    degree_u: usize,
    degree_v: usize,
    knots_u: Vec<f64>,
    knots_v: Vec<f64>,
    /// Points de contrôle (déjà multipliés par leur poids si la surface est rationnelle).
    control: Vec<Vec<Point3>>,
    /// Poids sous la forme `[w, 0, 0]`, pour réutiliser l'évaluation des points.
    weights: Option<Vec<Vec<Point3>>>,
    /// `[[u_min, u_max], [v_min, v_max]]`.
    domain: [[f64; 2]; 2],
    /// Échantillons `((u, v), point)` pour la recherche grossière du point le plus proche.
    grid: Vec<([f64; 2], Point3)>,
    /// Longueur 3D approximative par unité de paramètre, pour des coordonnées 2D proches du métrique.
    scale: [f64; 2],
}

impl SplineChart {
    fn new(s: &BSplineSurface) -> Option<Self> {
        let nu = s.control_points.len();
        let nv = s.control_points.first()?.len();
        if nu <= s.degree_u || nv <= s.degree_v || s.control_points.iter().any(|r| r.len() != nv) {
            return None;
        }
        if s.knots_u.len() != nu + s.degree_u + 1 || s.knots_v.len() != nv + s.degree_v + 1 {
            return None;
        }

        let rational = s
            .weights
            .as_ref()
            .filter(|w| w.len() == nu && w.iter().all(|r| r.len() == nv));
        let control: Vec<Vec<Point3>> = match rational {
            Some(w) => s
                .control_points
                .iter()
                .zip(w)
                .map(|(row, weights)| {
                    row.iter()
                        .zip(weights)
                        .map(|(p, &w)| [p[0] * w, p[1] * w, p[2] * w])
                        .collect()
                })
                .collect(),
            None => s.control_points.clone(),
        };
        let weights = rational.map(|w| {
            w.iter()
                .map(|row| row.iter().map(|&w| [w, 0.0, 0.0]).collect())
                .collect()
        });

        let domain = [
            [s.knots_u[s.degree_u], s.knots_u[nu]],
            [s.knots_v[s.degree_v], s.knots_v[nv]],
        ];
        if domain[0][1] <= domain[0][0] || domain[1][1] <= domain[1][0] {
            return None;
        }

        let mut chart = Self {
            degree_u: s.degree_u,
            degree_v: s.degree_v,
            knots_u: s.knots_u.clone(),
            knots_v: s.knots_v.clone(),
            control,
            weights,
            domain,
            grid: Vec::new(),
            scale: [1.0, 1.0],
        };

        let gu = (nu * 3).clamp(8, 40);
        let gv = (nv * 3).clamp(8, 40);
        let [[u0, u1], [v0, v1]] = domain;
        for i in 0..=gu {
            for j in 0..=gv {
                let u = u0 + (u1 - u0) * i as f64 / gu as f64;
                let v = v0 + (v1 - v0) * j as f64 / gv as f64;
                let point = chart.eval(u, v);
                chart.grid.push(([u, v], point));
            }
        }

        let stride = gv + 1;
        let row_length: f64 = (0..gu)
            .map(|i| {
                dist(
                    chart.grid[i * stride + gv / 2].1,
                    chart.grid[(i + 1) * stride + gv / 2].1,
                )
            })
            .sum();
        let column_length: f64 = (0..gv)
            .map(|j| {
                dist(
                    chart.grid[(gu / 2) * stride + j].1,
                    chart.grid[(gu / 2) * stride + j + 1].1,
                )
            })
            .sum();
        let nonzero = |x: f64| if x > 1e-12 { x } else { 1.0 };
        chart.scale = [
            nonzero(row_length / (u1 - u0)),
            nonzero(column_length / (v1 - v0)),
        ];
        Some(chart)
    }

    fn blend(&self, grid: &[Vec<Point3>], u: f64, v: f64) -> Point3 {
        let rows: Vec<Point3> = grid
            .iter()
            .map(|row| de_boor(self.degree_v, &self.knots_v, row, v))
            .collect();
        de_boor(self.degree_u, &self.knots_u, &rows, u)
    }

    fn eval(&self, u: f64, v: f64) -> Point3 {
        let p = self.blend(&self.control, u, v);
        match &self.weights {
            Some(weights) => {
                let w = self.blend(weights, u, v)[0];
                if w.abs() > 1e-12 {
                    scaled(p, 1.0 / w)
                } else {
                    p
                }
            }
            None => p,
        }
    }

    /// Dérivées partielles par différences finies centrées (décentrées au bord du domaine).
    fn partials(&self, u: f64, v: f64) -> (Point3, Point3) {
        let [[u0, u1], [v0, v1]] = self.domain;
        let (hu, hv) = ((u1 - u0) * 1e-4, (v1 - v0) * 1e-4);
        let (ua, ub) = ((u - hu).max(u0), (u + hu).min(u1));
        let (va, vb) = ((v - hv).max(v0), (v + hv).min(v1));
        let su = scaled(sub(self.eval(ub, v), self.eval(ua, v)), 1.0 / (ub - ua));
        let sv = scaled(sub(self.eval(u, vb), self.eval(u, va)), 1.0 / (vb - va));
        (su, sv)
    }

    /// Paramètres `(u, v)` du point de la surface le plus proche de `p`.
    fn invert(&self, p: Point3) -> [f64; 2] {
        let [[u0, u1], [v0, v1]] = self.domain;
        let mut best = self.grid[0];
        for cell in &self.grid {
            if dist(cell.1, p) < dist(best.1, p) {
                best = *cell;
            }
        }
        let [mut u, mut v] = best.0;
        for _ in 0..12 {
            let residual = sub(p, self.eval(u, v));
            let (su, sv) = self.partials(u, v);
            let (a, b, c) = (dot(su, su), dot(su, sv), dot(sv, sv));
            let det = a * c - b * b;
            if det.abs() < 1e-18 {
                break;
            }
            let (ru, rv) = (dot(su, residual), dot(sv, residual));
            let du = (c * ru - b * rv) / det;
            let dv = (a * rv - b * ru) / det;
            u = (u + du).clamp(u0, u1);
            v = (v + dv).clamp(v0, v1);
            if du.abs() < 1e-12 * (u1 - u0) && dv.abs() < 1e-12 * (v1 - v0) {
                break;
            }
        }
        [u, v]
    }

    fn normal(&self, p: Point3) -> Point3 {
        let [u, v] = self.invert(p);
        let (su, sv) = self.partials(u, v);
        normalize(cross(su, sv))
    }
}

// ---------------------------------------------------------------------------
// Projection d'une face dans l'espace de sa surface
// ---------------------------------------------------------------------------

enum Chart {
    Plane(Frame),
    Cylinder(Frame),
    Cone(Frame, f64),
    /// Repère, rayon majeur, rayon mineur.
    Torus(Frame, f64, f64),
    BSpline(Box<SplineChart>),
}

fn radial_direction(frame: &Frame, p: Point3) -> Point3 {
    let d = sub(p, frame.origin);
    let h = dot(d, frame.z);
    normalize(std::array::from_fn(|k| d[k] - h * frame.z[k]))
}

impl Chart {
    fn from_surface(surface: &Surface) -> Option<Self> {
        match surface {
            Surface::Plane { frame } => Some(Self::Plane(*frame)),
            Surface::Cylinder { frame, .. } => Some(Self::Cylinder(*frame)),
            Surface::Cone {
                frame, semi_angle, ..
            } => Some(Self::Cone(*frame, *semi_angle)),
            Surface::Torus {
                frame,
                major_radius,
                minor_radius,
            } => Some(Self::Torus(*frame, *major_radius, *minor_radius)),
            Surface::BSpline(s) => SplineChart::new(s).map(|c| Self::BSpline(Box::new(c))),
            Surface::Sphere { .. } => None,
        }
    }

    /// Axes `(u, v)` périodiques (angles) dont les coordonnées doivent être rendues continues.
    fn periodic(&self) -> [bool; 2] {
        match self {
            Self::Plane(_) | Self::BSpline(_) => [false, false],
            Self::Cylinder(_) | Self::Cone(..) => [true, false],
            Self::Torus(..) => [true, true],
        }
    }

    /// Coordonnées dans l'espace de la surface (angles pour les axes périodiques).
    fn uv(&self, p: Point3) -> [f64; 2] {
        match self {
            Self::Plane(f) => {
                let d = sub(p, f.origin);
                [dot(d, f.x), dot(d, f.y())]
            }
            Self::Cylinder(f) | Self::Cone(f, _) => {
                let d = sub(p, f.origin);
                [dot(d, f.y()).atan2(dot(d, f.x)), dot(d, f.z)]
            }
            Self::Torus(f, major, _) => {
                let d = sub(p, f.origin);
                let h = dot(d, f.z);
                let r = (dot(d, d) - h * h).max(0.0).sqrt();
                [dot(d, f.y()).atan2(dot(d, f.x)), h.atan2(r - *major)]
            }
            Self::BSpline(s) => s.invert(p),
        }
    }

    /// Normale de la surface en `p` (avant prise en compte de `same_sense`).
    fn normal(&self, p: Point3) -> Point3 {
        match self {
            Self::Plane(f) => f.z,
            Self::Cylinder(f) => radial_direction(f, p),
            Self::Cone(f, angle) => {
                let radial = radial_direction(f, p);
                let (s, c) = angle.sin_cos();
                std::array::from_fn(|k| c * radial[k] - s * f.z[k])
            }
            Self::Torus(f, major, _) => {
                let d = sub(p, f.origin);
                let radial = radial_direction(f, p);
                normalize(std::array::from_fn(|k| d[k] - *major * radial[k]))
            }
            Self::BSpline(s) => s.normal(p),
        }
    }

    /// Facteurs appliqués à `(u, v)` pour obtenir des coordonnées 2D proches du métrique.
    fn scale(&self, loops: &[Vec<Point3>]) -> [f64; 2] {
        match self {
            Self::Plane(_) => [1.0, 1.0],
            Self::Cylinder(f) | Self::Cone(f, _) => [mean_radius(f, loops), 1.0],
            Self::Torus(_, major, minor) => [major.max(1e-9), minor.max(1e-9)],
            Self::BSpline(s) => s.scale,
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

/// Rend les angles continus le long d'une boucle, pour l'axe `axis`.
fn unwrap_axis(uv: &mut [[f64; 2]], axis: usize) {
    for i in 1..uv.len() {
        let previous = uv[i - 1][axis];
        uv[i][axis] += TAU * ((previous - uv[i][axis]) / TAU).round();
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
        assert!(dist(first, last) < 1e-9);
        for p in &points {
            assert!((dot(*p, *p).sqrt() - 10.0).abs() < 1e-9);
        }
    }

    #[test]
    fn inverts_bspline_surface_point() {
        // Surface plane S(u, v) = (4u, 2v, 0).
        let surface = BSplineSurface {
            degree_u: 1,
            degree_v: 1,
            control_points: vec![
                vec![[0.0, 0.0, 0.0], [0.0, 2.0, 0.0]],
                vec![[4.0, 0.0, 0.0], [4.0, 2.0, 0.0]],
            ],
            weights: None,
            knots_u: vec![0.0, 0.0, 1.0, 1.0],
            knots_v: vec![0.0, 0.0, 1.0, 1.0],
        };
        let chart = SplineChart::new(&surface).unwrap();
        let [u, v] = chart.invert([1.0, 1.5, 0.0]);
        assert!((u - 0.25).abs() < 1e-6);
        assert!((v - 0.75).abs() < 1e-6);
        let normal = chart.normal([1.0, 1.5, 0.0]);
        assert!(dist(normal, [0.0, 0.0, 1.0]) < 1e-6);
    }

    #[test]
    fn torus_normal_points_outward() {
        let frame = Frame::from_normal([0.0; 3], [0.0, 0.0, 1.0]);
        let chart = Chart::Torus(frame, 10.0, 2.0);
        let outer_equator: Point3 = std::array::from_fn(|k| frame.x[k] * 12.0);
        assert!(dist(chart.normal(outer_equator), frame.x) < 1e-9);
        let [u, v] = chart.uv(outer_equator);
        assert!(u.is_finite());
        assert!(v.abs() < 1e-9);
    }
}