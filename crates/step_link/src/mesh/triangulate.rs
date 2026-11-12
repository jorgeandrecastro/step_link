//! Triangulation 2D.
//!
//! - [`triangulate_refined`] : Delaunay contraint (`spade`) avec points intérieurs optionnels.
//! - [`triangulate`] : *ear clipping* avec trous raccordés par des ponts, utilisé comme repli
//!   quand les contours sont pathologiques (auto-intersections après dépliage UV, par exemple).

use spade::Triangulation as _;
use spade::{ConstrainedDelaunayTriangulation, HasPosition, Point2};

const EPS: f64 = 1e-12;

/// Nombre maximal de points intérieurs par face (le pas est agrandi au besoin).
const MAX_INTERIOR_POINTS: usize = 6_000;

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

/// Triangule un polygone avec trous par *ear clipping*.
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

// ---------------------------------------------------------------------------------------------
// Delaunay contraint avec points intérieurs
// ---------------------------------------------------------------------------------------------

/// Sommet `spade` : `source` est l'indice dans la liste des contours mise à plat
/// (`usize::MAX` pour un point intérieur).
struct Vertex {
    position: Point2<f64>,
    source: usize,
}

impl HasPosition for Vertex {
    type Scalar = f64;

    fn position(&self) -> Point2<f64> {
        self.position
    }
}

/// Résultat de la triangulation.
///
/// `source[i]` vaut l'indice du point `i` dans la liste `outer` puis `holes` (mise à plat),
/// ou `usize::MAX` pour un point intérieur ajouté. Les triangles sont dans le sens trigonométrique.
pub(super) struct Triangulation {
    pub points: Vec<[f64; 2]>,
    pub source: Vec<usize>,
    pub triangles: Vec<[usize; 3]>,
}

/// Repli : *ear clipping*, sans point intérieur.
fn fallback(outer: &[[f64; 2]], holes: &[Vec<[f64; 2]>]) -> Triangulation {
    let points: Vec<[f64; 2]> = outer.iter().chain(holes.iter().flatten()).copied().collect();
    Triangulation {
        source: (0..points.len()).collect(),
        triangles: triangulate(outer, holes),
        points,
    }
}

/// Test point-dans-polygone (règle pair/impair sur l'ensemble des contours).
fn inside_rings(rings: &[&[[f64; 2]]], p: [f64; 2]) -> bool {
    let mut inside = false;
    for ring in rings {
        let mut j = ring.len().saturating_sub(1);
        for i in 0..ring.len() {
            let (a, b) = (ring[i], ring[j]);
            if (a[1] > p[1]) != (b[1] > p[1])
                && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
            {
                inside = !inside;
            }
            j = i;
        }
    }
    inside
}

fn dist_to_segment(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len2 = dx * dx + dy * dy;
    let t = if len2 < 1e-18 {
        0.0
    } else {
        (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2).clamp(0.0, 1.0)
    };
    (p[0] - a[0] - t * dx).hypot(p[1] - a[1] - t * dy)
}

fn far_from_ring(ring: &[[f64; 2]], p: [f64; 2], margin: f64) -> bool {
    (0..ring.len()).all(|i| dist_to_segment(p, ring[i], ring[(i + 1) % ring.len()]) >= margin)
}

/// Grille décalée (nid d'abeille) couvrant la boîte englobante du contour externe.
fn grid_candidates(outer: &[[f64; 2]], step: [f64; 2]) -> Vec<[f64; 2]> {
    let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
    for p in outer {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let (w, h) = (hi[0] - lo[0], hi[1] - lo[1]);
    if !(w.is_finite() && h.is_finite()) || w <= 0.0 || h <= 0.0 {
        return Vec::new();
    }
    let (mut su, mut sv) = (step[0].max(1e-9), step[1].max(1e-9));
    while (w / su + 1.0) * (h / sv + 1.0) > MAX_INTERIOR_POINTS as f64 {
        su *= 1.4;
        sv *= 1.4;
    }
    let mut out = Vec::new();
    let mut j = 0usize;
    loop {
        let v = lo[1] + (j as f64 + 0.5) * sv;
        if v >= hi[1] {
            break;
        }
        let shift = if j % 2 == 1 { 0.5 * su } else { 0.0 };
        let mut i = 0usize;
        loop {
            let u = lo[0] + (i as f64 + 0.5) * su + shift;
            if u >= hi[0] {
                break;
            }
            out.push([u, v]);
            i += 1;
        }
        j += 1;
    }
    out
}

fn build_cdt(rings: &[&[[f64; 2]]], interior: &[[f64; 2]]) -> Option<Triangulation> {
    let mut cdt = ConstrainedDelaunayTriangulation::<Vertex>::new();

    // 1. Tous les sommets des contours d'abord, puis les contraintes.
    let mut ring_handles = Vec::with_capacity(rings.len());
    let mut next = 0usize;
    for ring in rings {
        let mut handles = Vec::with_capacity(ring.len());
        for p in ring.iter() {
            let vertex = Vertex {
                position: Point2::new(p[0], p[1]),
                source: next,
            };
            next += 1;
            handles.push(cdt.insert(vertex).ok()?);
        }
        ring_handles.push(handles);
    }
    for handles in &ring_handles {
        for i in 0..handles.len() {
            let (a, b) = (handles[i], handles[(i + 1) % handles.len()]);
            if a == b {
                continue; // points consécutifs confondus
            }
            if !cdt.can_add_constraint(a, b) {
                return None; // contours qui se croisent : repli
            }
            cdt.add_constraint(a, b);
        }
    }

    // 2. Points intérieurs. Un point confondu avec un sommet existant est sans conséquence.
    for p in interior {
        let _ = cdt.insert(Vertex {
            position: Point2::new(p[0], p[1]),
            source: usize::MAX,
        });
    }

    let mut points = vec![[0.0; 2]; cdt.num_vertices()];
    let mut source = vec![usize::MAX; cdt.num_vertices()];
    for v in cdt.vertices() {
        let i = v.fix().index();
        let p = v.position();
        points[i] = [p.x, p.y];
        source[i] = v.data().source;
    }

    // La triangulation couvre l'enveloppe convexe : on garde les triangles situés dans la face
    // (règle pair/impair, ce qui exclut aussi les trous). Les contours étant des arêtes
    // contraintes, un triangle est entièrement dedans ou entièrement dehors.
    let mut triangles = Vec::new();
    for face in cdt.inner_faces() {
        let [a, b, c] = face.vertices().map(|v| v.fix().index());
        let centroid = [
            (points[a][0] + points[b][0] + points[c][0]) / 3.0,
            (points[a][1] + points[b][1] + points[c][1]) / 3.0,
        ];
        if !inside_rings(rings, centroid) {
            continue;
        }
        // Orientation trigonométrique garantie, triangles plats écartés.
        let orientation = cross2(points[a], points[b], points[c]);
        if orientation > 0.0 {
            triangles.push([a, b, c]);
        } else if orientation < 0.0 {
            triangles.push([a, c, b]);
        }
    }
    if triangles.is_empty() {
        return None;
    }
    Some(Triangulation {
        points,
        source,
        triangles,
    })
}

/// Triangulation de Delaunay contrainte par les contours, avec points intérieurs optionnels.
///
/// `step` : pas de la grille intérieure, dans le même espace 2D que les contours
/// (`None` : aucun point intérieur). En cas d'échec (contours qui se croisent, valeurs non
/// finies), retombe sur l'*ear clipping* sans point intérieur.
pub(super) fn triangulate_refined(
    outer: &[[f64; 2]],
    holes: &[Vec<[f64; 2]>],
    step: Option<[f64; 2]>,
) -> Triangulation {
    let mut rings: Vec<&[[f64; 2]]> = vec![outer];
    rings.extend(holes.iter().map(Vec::as_slice));

    let interior: Vec<[f64; 2]> = match step {
        Some(step) => {
            let margin = 0.35 * step[0].min(step[1]);
            grid_candidates(outer, step)
                .into_iter()
                .filter(|&p| {
                    inside_rings(&rings, p) && rings.iter().all(|r| far_from_ring(r, p, margin))
                })
                .collect()
        }
        None => Vec::new(),
    };

    build_cdt(&rings, &interior).unwrap_or_else(|| fallback(outer, holes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn total_area(t: &Triangulation) -> f64 {
        t.triangles
            .iter()
            .map(|tr| cross2(t.points[tr[0]], t.points[tr[1]], t.points[tr[2]]) / 2.0)
            .sum()
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
    fn refined_without_interior_points_covers_square() {
        let outer = [[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]];
        let t = triangulate_refined(&outer, &[], None);
        assert_eq!(t.points.len(), 4);
        assert_eq!(t.triangles.len(), 2);
        assert!((total_area(&t) - 16.0).abs() < 1e-9);
    }

    #[test]
    fn refined_adds_interior_points_and_keeps_area() {
        let outer = [[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]];
        let t = triangulate_refined(&outer, &[], Some([1.0, 1.0]));
        assert!(t.points.len() > 4);
        assert!((total_area(&t) - 16.0).abs() < 1e-9);
        for tr in &t.triangles {
            assert!(cross2(t.points[tr[0]], t.points[tr[1]], t.points[tr[2]]) > 0.0);
        }
    }

    #[test]
    fn refined_respects_holes() {
        let outer = [[0.0, 0.0], [8.0, 0.0], [8.0, 8.0], [0.0, 8.0]];
        let hole = vec![[3.0, 3.0], [3.0, 5.0], [5.0, 5.0], [5.0, 3.0]];
        let t = triangulate_refined(&outer, &[hole], Some([1.0, 1.0]));
        assert!(t.points.len() > 8);
        assert!((total_area(&t) - 60.0).abs() < 1e-9);
    }

    #[test]
    fn self_intersecting_outline_falls_back() {
        // Nœud papillon : spade refuse la contrainte, on retombe sur l'ear clipping sans panique.
        let outer = [[0.0, 0.0], [2.0, 2.0], [2.0, 0.0], [0.0, 2.0]];
        let t = triangulate_refined(&outer, &[], Some([0.5, 0.5]));
        assert_eq!(t.points.len(), t.source.len());
    }
}