//! Projection of a face into its surface's parametric space.

use super::spline::SplineChart;
use crate::brep::{Frame, Point3, Surface, dot, normalize, sub};
use std::f64::consts::TAU;

pub(super) enum Chart {
    Plane(Frame),
    Cylinder(Frame, f64),
    Cone(Frame, f64, f64),
    /// Frame and radius.
    Sphere(Frame, f64),
    /// Frame, major radius, and minor radius.
    Torus(Frame, f64, f64),
    BSpline(Box<SplineChart>),
}

/// Distance from the `z` axis and height along that axis.
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

/// Converts a point in the local `(x, y, z)` frame to global coordinates.
fn from_local(f: &Frame, c: Point3) -> Point3 {
    let y = f.y();
    std::array::from_fn(|k| f.origin[k] + c[0] * f.x[k] + c[1] * y[k] + c[2] * f.z[k])
}

impl Chart {
    pub(super) fn from_surface(surface: &Surface) -> Option<Self> {
        match surface {
            Surface::Plane { frame } => Some(Self::Plane(*frame)),
            Surface::Cylinder { frame, radius } => Some(Self::Cylinder(*frame, *radius)),
            Surface::Cone {
                frame, radius, semi_angle
            } => Some(Self::Cone(*frame, *radius, *semi_angle)),
            Surface::Sphere { frame, radius } => Some(Self::Sphere(*frame, *radius)),
            Surface::Torus {
                frame,
                major_radius,
                minor_radius,
            } => Some(Self::Torus(*frame, *major_radius, *minor_radius)),
            Surface::BSpline(s) => SplineChart::new(s).map(|c| Self::BSpline(Box::new(c))),
        }
    }

    /// Periodic `(u, v)` axes (angles) whose coordinates must be made continuous.
    pub(super) fn periodic(&self) -> [bool; 2] {
        match self {
            Self::Plane(_) | Self::BSpline(_) => [false, false],
            Self::Cylinder(..) | Self::Cone(..) | Self::Sphere(..) => [true, false],
            Self::Torus(..) => [true, true],
        }
    }

    /// Coordinates in the surface's parameter space (angles for periodic axes).
    pub(super) fn uv(&self, p: Point3) -> [f64; 2] {
        match self {
            Self::Plane(f) => {
                let d = sub(p, f.origin);
                [dot(d, f.x), dot(d, f.y())]
            }
            Self::Cylinder(f, _) | Self::Cone(f, _, _) => {
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

    /// Surface normal at `p` (before accounting for `same_sense`).
    pub(super) fn normal(&self, p: Point3) -> Point3 {
        match self {
            Self::Plane(f) => f.z,
            Self::Cylinder(f, _) => radial_direction(f, p),
            Self::Cone(f, _, angle) => {
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

    /// Normal at a point whose `(u, v)` coordinates are already known (avoids inversion for B-splines).
    pub(super) fn normal_uv(&self, uv: [f64; 2], p: Point3) -> Point3 {
        match self {
            Self::BSpline(s) => s.normal_uv(uv[0], uv[1]),
            _ => self.normal(p),
        }
    }

    /// 3D point for unscaled `(u, v)` coordinates.
    pub(super) fn point(&self, uv: [f64; 2]) -> Point3 {
        let [u, v] = uv;
        match self {
            Self::Plane(f) => from_local(f, [u, v, 0.0]),
            Self::Cylinder(f, r) => from_local(f, [r * u.cos(), r * u.sin(), v]),
            Self::Cone(f, r, angle) => {
                let r = r + v * angle.tan();
                from_local(f, [r * u.cos(), r * u.sin(), v])
            }
            Self::Sphere(f, r) => from_local(
                f,
                [r * v.cos() * u.cos(), r * v.cos() * u.sin(), r * v.sin()],
            ),
            Self::Torus(f, major, minor) => {
                let rr = major + minor * v.cos();
                from_local(f, [rr * u.cos(), rr * u.sin(), minor * v.sin()])
            }
            Self::BSpline(s) => s.eval(u, v),
        }
    }

    /// Interior-point spacing in parameter units (radians or B-spline parameter units).
    /// `None`: for planar surfaces, the contours are sufficient.
    pub(super) fn param_steps(&self, tolerance: f64, loops: &[Vec<Point3>]) -> Option<[f64; 2]> {
        // Angular step that keeps the sagitta on a circle of radius `radius` below `tolerance`.
        let angle = |radius: f64| {
            let ratio = (tolerance / radius.max(1e-9)).min(1.0);
            (2.0 * (1.0 - ratio).acos()).clamp(0.02, 1.0)
        };
        match self {
            Self::Plane(_) => None,
            Self::Cylinder(frame, radius) | Self::Cone(frame, radius, _) => {
                let (min_v, max_v, max_radius) = loops.iter().flatten().fold(
                    (f64::INFINITY, f64::NEG_INFINITY, *radius),
                    |(lo, hi, r), &p| {
                        let (point_radius, height) = radial_and_height(frame, p);
                        (lo.min(height), hi.max(height), r.max(point_radius))
                    },
                );
                let height = max_v - min_v;
                if !height.is_finite() || height <= 0.0 {
                    return None;
                }
                Some([angle(max_radius), height / 2.0])
            }
            Self::Sphere(_, radius) => {
                let a = angle(*radius);
                Some([a, a])
            }
            Self::Torus(_, major, minor) => Some([angle(major + minor), angle(*minor)]),
            Self::BSpline(s) => Some(s.param_steps(tolerance)),
        }
    }

    /// Factors applied to `(u, v)` to obtain approximately metric 2D coordinates.
    pub(super) fn scale(&self, loops: &[Vec<Point3>]) -> [f64; 2] {
        match self {
            Self::Plane(_) => [1.0, 1.0],
            Self::Cylinder(f, _) | Self::Cone(f, _, _) => [mean_radius(f, loops), 1.0],
            Self::Sphere(f, radius) => {
                // The length of one degree of longitude is `R·cos(latitude)`.
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

/// Makes angles continuous along a loop for the `axis` axis.
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

    #[test]
    fn sphere_point_roundtrip() {
        let frame = Frame::from_normal([1.0, 2.0, 3.0], [0.0, 0.0, 1.0]);
        let chart = Chart::Sphere(frame, 5.0);
        let p = chart.point([0.3, 0.2]);
        let [u, v] = chart.uv(p);
        assert!((u - 0.3).abs() < 1e-9 && (v - 0.2).abs() < 1e-9);
    }

    #[test]
    fn torus_point_roundtrip() {
        let frame = Frame::from_normal([0.0; 3], [0.0, 0.0, 1.0]);
        let chart = Chart::Torus(frame, 10.0, 2.0);
        let p = chart.point([0.7, -0.4]);
        let [u, v] = chart.uv(p);
        assert!((u - 0.7).abs() < 1e-9 && (v + 0.4).abs() < 1e-9);
    }

    #[test]
    fn ruled_surfaces_evaluate_interior_points() {
        let frame = Frame::from_normal([0.0; 3], [0.0, 0.0, 1.0]);
        let cylinder = Chart::Cylinder(frame, 5.0);
        let cone = Chart::Cone(frame, 5.0, 0.2);
        for chart in [&cylinder, &cone] {
            let uv = [0.7, 2.0];
            let point = chart.point(uv);
            let actual = chart.uv(point);
            assert!((actual[0] - uv[0]).abs() < 1e-9);
            assert!((actual[1] - uv[1]).abs() < 1e-9);
            let expected_radius = match chart {
                Chart::Cylinder(_, radius) => *radius,
                Chart::Cone(_, radius, angle) => radius + uv[1] * angle.tan(),
                _ => unreachable!(),
            };
            assert!((radial_and_height(&frame, point).0 - expected_radius).abs() < 1e-9);
            let loops = vec![vec![chart.point([0.0, 0.0]), point]];
            assert!(chart.param_steps(0.05, &loops).is_some());
        }
    }
}