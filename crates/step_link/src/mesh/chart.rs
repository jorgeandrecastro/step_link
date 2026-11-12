//! Projection d'une face dans l'espace paramétrique de sa surface.

use super::spline::SplineChart;
use crate::brep::{Frame, Point3, Surface, dot, normalize, sub};
use std::f64::consts::TAU;

pub(super) enum Chart {
    Plane(Frame),
    Cylinder(Frame),
    Cone(Frame, f64),
    /// Repère, rayon.
    Sphere(Frame, f64),
    /// Repère, rayon majeur, rayon mineur.
    Torus(Frame, f64, f64),
    BSpline(Box<SplineChart>),
}

/// Distance à l'axe `z` et hauteur le long de cet axe.
fn radial_and_height(frame: &Frame, p: Point3) -> (f64, f64) {
    let d = sub(p, frame.origin);
    let h = dot(d, frame.z);
    ((dot(d, d) - h * h).max(0.0).sqrt(), h)
}

fn radial_direction(frame: &Frame, p: Point3) -> Point3 {
    let d = sub(p, frame.origin);
    let h = dot(d, frame.z);
    normalize(std::array::from_fn(|k| d[k] - h * frame.z[k]))
}

impl Chart {
    pub(super) fn from_surface(surface: &Surface) -> Option<Self> {
        match surface {
            Surface::Plane { frame } => Some(Self::Plane(*frame)),
            Surface::Cylinder { frame, .. } => Some(Self::Cylinder(*frame)),
            Surface::Cone {
                frame, semi_angle, ..
            } => Some(Self::Cone(*frame, *semi_angle)),
            Surface::Sphere { frame, radius } => Some(Self::Sphere(*frame, *radius)),
            Surface::Torus {
                frame,
                major_radius,
                minor_radius,
            } => Some(Self::Torus(*frame, *major_radius, *minor_radius)),
            Surface::BSpline(s) => SplineChart::new(s).map(|c| Self::BSpline(Box::new(c))),
        }
    }

    /// Axes `(u, v)` périodiques (angles) dont les coordonnées doivent être rendues continues.
    pub(super) fn periodic(&self) -> [bool; 2] {
        match self {
            Self::Plane(_) | Self::BSpline(_) => [false, false],
            Self::Cylinder(_) | Self::Cone(..) | Self::Sphere(..) => [true, false],
            Self::Torus(..) => [true, true],
        }
    }

    /// Coordonnées dans l'espace de la surface (angles pour les axes périodiques).
    pub(super) fn uv(&self, p: Point3) -> [f64; 2] {
        match self {
            Self::Plane(f) => {
                let d = sub(p, f.origin);
                [dot(d, f.x), dot(d, f.y())]
            }
            Self::Cylinder(f) | Self::Cone(f, _) => {
                let d = sub(p, f.origin);
                [dot(d, f.y()).atan2(dot(d, f.x)), dot(d, f.z)]
            }
            // Longitude, latitude.
            Self::Sphere(f, _) => {
                let d = sub(p, f.origin);
                let (r, h) = radial_and_height(f, p);
                [dot(d, f.y()).atan2(dot(d, f.x)), h.atan2(r)]
            }
            Self::Torus(f, major, _) => {
                let d = sub(p, f.origin);
                let (r, h) = radial_and_height(f, p);
                [dot(d, f.y()).atan2(dot(d, f.x)), h.atan2(r - *major)]
            }
            Self::BSpline(s) => s.invert(p),
        }
    }

    /// Normale de la surface en `p` (avant prise en compte de `same_sense`).
    pub(super) fn normal(&self, p: Point3) -> Point3 {
        match self {
            Self::Plane(f) => f.z,
            Self::Cylinder(f) => radial_direction(f, p),
            Self::Cone(f, angle) => {
                let radial = radial_direction(f, p);
                let (s, c) = angle.sin_cos();
                std::array::from_fn(|k| c * radial[k] - s * f.z[k])
            }
            Self::Sphere(f, _) => normalize(sub(p, f.origin)),
            Self::Torus(f, major, _) => {
                let d = sub(p, f.origin);
                let radial = radial_direction(f, p);
                normalize(std::array::from_fn(|k| d[k] - *major * radial[k]))
            }
            Self::BSpline(s) => s.normal(p),
        }
    }

    /// Facteurs appliqués à `(u, v)` pour obtenir des coordonnées 2D proches du métrique.
    pub(super) fn scale(&self, loops: &[Vec<Point3>]) -> [f64; 2] {
        match self {
            Self::Plane(_) => [1.0, 1.0],
            Self::Cylinder(f) | Self::Cone(f, _) => [mean_radius(f, loops), 1.0],
            Self::Sphere(f, radius) => {
                // La longueur d'un degré de longitude vaut `R·cos(latitude)`.
                let (sum, count) = loops.iter().flatten().fold((0.0, 0usize), |(s, n), &p| {
                    let d = sub(p, f.origin);
                    let (r, _) = radial_and_height(f, p);
                    let norm = dot(d, d).sqrt().max(1e-12);
                    (s + r / norm, n + 1)
                });
                let cos_lat = if count == 0 { 1.0 } else { sum / count as f64 };
                [radius * cos_lat.max(0.1), *radius]
            }
            Self::Torus(_, major, minor) => [major.max(1e-9), minor.max(1e-9)],
            Self::BSpline(s) => s.scale,
        }
    }
}

fn mean_radius(frame: &Frame, loops: &[Vec<Point3>]) -> f64 {
    let (sum, count) = loops
        .iter()
        .flatten()
        .fold((0.0, 0usize), |(s, n), &p| (s + radial_and_height(frame, p).0, n + 1));
    if count == 0 || sum < 1e-9 {
        1.0
    } else {
        sum / count as f64
    }
}

/// Rend les angles continus le long d'une boucle, pour l'axe `axis`.
pub(super) fn unwrap_axis(uv: &mut [[f64; 2]], axis: usize) {
    for i in 1..uv.len() {
        let previous = uv[i - 1][axis];
        uv[i][axis] += TAU * ((previous - uv[i][axis]) / TAU).round();
    }
}

#[cfg(test)]
mod tests {
    use super::super::dist;
    use super::*;

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

    #[test]
    fn sphere_uv_and_normal() {
        let frame = Frame::from_normal([0.0; 3], [0.0, 0.0, 1.0]);
        let chart = Chart::Sphere(frame, 5.0);
        let on_equator: Point3 = std::array::from_fn(|k| frame.x[k] * 5.0);
        let [lon, lat] = chart.uv(on_equator);
        assert!(lon.abs() < 1e-9 && lat.abs() < 1e-9);
        let north = [0.0, 0.0, 5.0];
        assert!((chart.uv(north)[1] - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
        assert!(dist(chart.normal(north), [0.0, 0.0, 1.0]) < 1e-9);
    }
}