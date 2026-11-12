//! Discrétisation des arêtes (droites, cercles, ellipses, B-splines) et des boucles.

use super::spline::sample_bspline;
use crate::brep::{Curve, Edge, EdgeId, Frame, Loop, Point3, Solid, dot, sub};
use std::collections::HashMap;
use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, TAU};

/// Points déjà calculés de chaque arête, partagés entre faces voisines.
pub(super) type EdgeCache = HashMap<EdgeId, Vec<Point3>>;

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

/// Points d'une boucle, sans répéter le point de jonction entre deux arêtes.
pub(super) fn loop_polyline(
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

#[cfg(test)]
mod tests {
    use super::super::dist;
    use super::*;

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
}