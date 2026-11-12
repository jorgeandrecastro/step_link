//! Triangulation 2D par *ear clipping*, avec trous raccordés par des ponts.

const EPS: f64 = 1e-12;

fn cross2(o: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

fn same(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9
}

/// Aire signée d'un polygone (positive dans le sens trigonométrique).
pub(super) fn area(poly: &[[f64; 2]]) -> f64 {
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
pub(super) fn triangulate(outer: &[[f64; 2]], holes: &[Vec<[f64; 2]>]) -> Vec<[usize; 3]> {
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
}