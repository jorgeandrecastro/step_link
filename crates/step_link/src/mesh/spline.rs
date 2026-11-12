//! Évaluation des B-splines : algorithme de de Boor, courbes, surfaces et projection de points.

use super::{dist, scaled};
use crate::brep::{BSplineCurve, BSplineSurface, Point3, cross, dot, normalize, sub};

pub(super) fn de_boor(degree: usize, knots: &[f64], control: &[Point3], t: f64) -> Point3 {
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

/// Courbe B-spline complète (rationnelle si des poids sont fournis), orientée pour partir de `from`.
pub(super) fn sample_bspline(spline: &BSplineCurve, from: Point3, to: Point3) -> Vec<Point3> {
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
    // Pas de let-chain ici : elles exigent Rust 1.88, le MSRV du projet est 1.85.
    if let (Some(&first), Some(&last)) = (points.first(), points.last()) {
        if distance(first) > distance(last) {
            points.reverse();
        }
    }
    points
}

/// Surface B-spline préparée pour l'évaluation et l'inversion de points.
pub(super) struct SplineChart {
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
    /// Cellules de la grille d'échantillonnage `[gu, gv]`.
    cells: [usize; 2],
    /// Longueur 3D approximative par unité de paramètre, pour des coordonnées 2D proches du métrique.
    pub(super) scale: [f64; 2],
}

impl SplineChart {
    pub(super) fn new(s: &BSplineSurface) -> Option<Self> {
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
            cells: [0, 0],
            scale: [1.0, 1.0],
        };

        let gu = (nu * 3).clamp(8, 40);
        let gv = (nv * 3).clamp(8, 40);
        chart.cells = [gu, gv];
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

    pub(super) fn eval(&self, u: f64, v: f64) -> Point3 {
        let u = u.clamp(self.domain[0][0], self.domain[0][1]);
        let v = v.clamp(self.domain[1][0], self.domain[1][1]);
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

    /// Pas `(du, dv)` tel que la flèche d'une corde de cette longueur reste sous `tolerance`.
    ///
    /// Estimé par la déviation du point central de cordes à 2 cellules sur la grille
    /// d'échantillonnage : la flèche croît comme le carré de la longueur de la corde.
    pub(super) fn param_steps(&self, tolerance: f64) -> [f64; 2] {
        let [gu, gv] = self.cells;
        let stride = gv + 1;
        let at = |i: usize, j: usize| self.grid[i * stride + j].1;
        let deviation = |a: Point3, b: Point3, c: Point3| {
            let mid = [
                (a[0] + c[0]) / 2.0,
                (a[1] + c[1]) / 2.0,
                (a[2] + c[2]) / 2.0,
            ];
            dist(b, mid)
        };
        let (mut dev_u, mut dev_v) = (0.0f64, 0.0f64);
        for i in 1..gu {
            for j in 0..=gv {
                dev_u = dev_u.max(deviation(at(i - 1, j), at(i, j), at(i + 1, j)));
            }
        }
        for i in 0..=gu {
            for j in 1..gv {
                dev_v = dev_v.max(deviation(at(i, j - 1), at(i, j), at(i, j + 1)));
            }
        }
        let [[u0, u1], [v0, v1]] = self.domain;
        let steps = |dev: f64, cells: usize, span: f64| {
            let cells = cells.max(1) as f64;
            let m = if dev > 1e-12 {
                (2.0 * (tolerance / dev).sqrt()).clamp(1.0, cells)
            } else {
                cells
            };
            span * m / cells
        };
        [steps(dev_u, gu, u1 - u0), steps(dev_v, gv, v1 - v0)]
    }

    /// Paramètres `(u, v)` du point de la surface le plus proche de `p`.
    pub(super) fn invert(&self, p: Point3) -> [f64; 2] {
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

    pub(super) fn normal(&self, p: Point3) -> Point3 {
        let [u, v] = self.invert(p);
        self.normal_uv(u, v)
    }

    /// Normale en `(u, v)`, sans passer par l'inversion d'un point.
    pub(super) fn normal_uv(&self, u: f64, v: f64) -> Point3 {
        let (su, sv) = self.partials(u, v);
        normalize(cross(su, sv))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_surface() -> BSplineSurface {
        // Surface plane S(u, v) = (4u, 2v, 0).
        BSplineSurface {
            degree_u: 1,
            degree_v: 1,
            control_points: vec![
                vec![[0.0, 0.0, 0.0], [0.0, 2.0, 0.0]],
                vec![[4.0, 0.0, 0.0], [4.0, 2.0, 0.0]],
            ],
            weights: None,
            knots_u: vec![0.0, 0.0, 1.0, 1.0],
            knots_v: vec![0.0, 0.0, 1.0, 1.0],
        }
    }

    #[test]
    fn inverts_bspline_surface_point() {
        let chart = SplineChart::new(&flat_surface()).unwrap();
        let [u, v] = chart.invert([1.0, 1.5, 0.0]);
        assert!((u - 0.25).abs() < 1e-6);
        assert!((v - 0.75).abs() < 1e-6);
        let normal = chart.normal([1.0, 1.5, 0.0]);
        assert!(dist(normal, [0.0, 0.0, 1.0]) < 1e-6);
    }

    #[test]
    fn flat_surface_needs_coarse_steps() {
        let chart = SplineChart::new(&flat_surface()).unwrap();
        let [du, dv] = chart.param_steps(0.05);
        // Une surface plane n'a aucune flèche : le pas couvre tout le domaine.
        assert!((du - 1.0).abs() < 1e-9 && (dv - 1.0).abs() < 1e-9);
        let normal = chart.normal_uv(0.5, 0.5);
        assert!(dist(normal, [0.0, 0.0, 1.0]) < 1e-6);
    }
}