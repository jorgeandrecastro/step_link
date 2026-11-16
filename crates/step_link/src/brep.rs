//! Topologie B-Rep (Boundary Representation).
//!
//! Hierarchy: [`Solid`] → [`Shell`] → [`Face`] → [`Loop`] → [`OrientedEdge`] → [`Edge`] → [`Vertex`].
//! Vertices and edges are stored once in the [`Solid`] and referenced by
//! index: an edge shared by two faces exists only once.

/// 3D point (or vector) in double precision.
pub type Point3 = [f64; 3];

/// Index of a vertex in [`Solid::vertices`].
pub type VertexId = usize;

/// Index of an edge in [`Solid::edges`].
pub type EdgeId = usize;

/// RGB color (sRGB, components between 0 and 1).
pub type Color = [f32; 3];

pub(crate) fn dot(a: Point3, b: Point3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(crate) fn cross(a: Point3, b: Point3) -> Point3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub(crate) fn sub(a: Point3, b: Point3) -> Point3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(crate) fn normalize(v: Point3) -> Point3 {
    let norm = dot(v, v).sqrt();
    if norm == 0.0 {
        v
    } else {
        [v[0] / norm, v[1] / norm, v[2] / norm]
    }
}

/// Orthonormal frame (`AXIS2_PLACEMENT_3D` in STEP).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    /// Frame origin.
    pub origin: Point3,
    /// Unit Z axis.
    pub z: Point3,
    /// Unit X axis, orthogonal to `z`.
    pub x: Point3,
}

impl Frame {
    /// Creates a frame; `z` is normalized and `x` is orthogonalized against `z`.
    ///
    /// `x` must not be collinear with `z`.
    pub fn new(origin: Point3, z: Point3, x: Point3) -> Self {
        let z = normalize(z);
        let d = dot(x, z);
        let x = normalize([x[0] - z[0] * d, x[1] - z[1] * d, x[2] - z[2] * d]);
        Self { origin, z, x }
    }

    /// Creates a frame with only the Z axis specified; X is chosen arbitrarily.
    pub fn from_normal(origin: Point3, z: Point3) -> Self {
        let z = normalize(z);
        let helper = if z[0].abs() < 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        Self {
            origin,
            z,
            x: normalize(cross(helper, z)),
        }
    }

    /// Unit Y axis (`z × x`).
    pub fn y(&self) -> Point3 {
        cross(self.z, self.x)
    }
}

/// Vertex: a point in space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vertex {
    /// Vertex position.
    pub position: Point3,
}

impl Vertex {
    /// Creates a vertex at the given position.
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self {
            position: [x, y, z],
        }
    }
}

/// Curve carried by an edge.
#[derive(Debug, Clone, PartialEq)]
pub enum Curve {
    /// Line segment between the edge's vertices.
    Line,
    /// Circle (or arc) in the frame's XY plane.
    Circle {
        /// Circle frame.
        frame: Frame,
        /// Radius.
        radius: f64,
    },
    /// Ellipse (or arc) in the frame's XY plane.
    Ellipse {
        /// Ellipse frame.
        frame: Frame,
        /// Semiaxis along X.
        semi_major: f64,
        /// Semiaxis along Y.
        semi_minor: f64,
    },
    /// B-spline curve (rational if `weights` is provided).
    BSpline(BSplineCurve),
}

/// B-spline curve.
#[derive(Debug, Clone, PartialEq)]
pub struct BSplineCurve {
    /// Degree.
    pub degree: usize,
    /// Control points.
    pub control_points: Vec<Point3>,
    /// Control-point weights (NURBS), absent for a non-rational B-spline.
    pub weights: Option<Vec<f64>>,
    /// Expanded knot vector (multiplicities already applied).
    pub knots: Vec<f64>,
}

/// Surface carried by a face.
#[derive(Debug, Clone, PartialEq)]
pub enum Surface {
    /// Plane through the frame origin with normal `z`.
    Plane {
        /// Plane frame.
        frame: Frame,
    },
    /// Cylinder with axis `z`.
    Cylinder {
        /// Cylinder frame.
        frame: Frame,
        /// Radius.
        radius: f64,
    },
    /// Cone with axis `z`.
    Cone {
        /// Cone frame.
        frame: Frame,
        /// Radius at the origin.
        radius: f64,
        /// Half-angle at the apex, in radians.
        semi_angle: f64,
    },
    /// Sphere centered at the frame origin.
    Sphere {
        /// Sphere frame.
        frame: Frame,
        /// Radius.
        radius: f64,
    },
    /// Torus with axis `z`.
    Torus {
        /// Torus frame.
        frame: Frame,
        /// Major radius.
        major_radius: f64,
        /// Minor radius.
        minor_radius: f64,
    },
    /// B-spline surface (rational if `weights` is provided).
    BSpline(BSplineSurface),
}

/// B-spline surface.
#[derive(Debug, Clone, PartialEq)]
pub struct BSplineSurface {
    /// Degree along u.
    pub degree_u: usize,
    /// Degree along v.
    pub degree_v: usize,
    /// Control-point grid, indexed by `[u][v]`.
    pub control_points: Vec<Vec<Point3>>,
    /// Grid weights (NURBS), absent for a non-rational B-spline.
    pub weights: Option<Vec<Vec<f64>>>,
    /// Expanded knot vector along u.
    pub knots_u: Vec<f64>,
    /// Expanded knot vector along v.
    pub knots_v: Vec<f64>,
}

/// Edge: a curve between two vertices, shared by adjacent faces.
#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    /// Starting vertex.
    pub start: VertexId,
    /// Ending vertex.
    pub end: VertexId,
    /// Curve carried by the edge.
    pub curve: Curve,
    /// `true` if the curve runs in the same direction as the edge (from `start` to `end`).
    pub same_sense: bool,
}

/// Use of an edge in a loop, including its traversal direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrientedEdge {
    /// Edge being used.
    pub edge: EdgeId,
    /// `true` if the edge is traversed from `start` to `end`.
    pub forward: bool,
}

/// Closed loop of consecutive oriented edges.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Loop {
    /// Edges in the loop, in traversal order.
    pub edges: Vec<OrientedEdge>,
}

/// Face: a portion of a surface bounded by an outer loop and holes.
#[derive(Debug, Clone, PartialEq)]
pub struct Face {
    /// Supporting surface.
    pub surface: Surface,
    /// Outer loop.
    pub outer_loop: Loop,
    /// Boucles internes (trous).
    pub inner_loops: Vec<Loop>,
    /// `true` if the face normal follows the surface normal, `false` if it is reversed.
    pub same_sense: bool,
    /// Color assigned to the face, taking priority over the solid's color.
    pub color: Option<Color>,
}

/// Shell: a connected set of faces.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Shell {
    /// Faces in the shell.
    pub faces: Vec<Face>,
}

/// Solid: shared geometry (vertices and edges) and the shells that use it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Solid {
    /// Vertices, referenced by [`VertexId`].
    pub vertices: Vec<Vertex>,
    /// Edges, referenced by [`EdgeId`].
    pub edges: Vec<Edge>,
    /// Shells (the first is the outer shell).
    pub shells: Vec<Shell>,
    /// Solid color, used by faces without an assigned color.
    pub color: Option<Color>,
}

impl Solid {
    /// Iterates over all faces in the solid.
    pub fn faces(&self) -> impl Iterator<Item = &Face> {
        self.shells.iter().flat_map(|s| s.faces.iter())
    }

    /// Total number of faces in the solid.
    pub fn face_count(&self) -> usize {
        self.faces().count()
    }

    /// Starting points of each oriented edge in a loop, in order.
    ///
    /// Invalid indices are ignored.
    pub fn loop_points(&self, face_loop: &Loop) -> Vec<Point3> {
        face_loop
            .edges
            .iter()
            .filter_map(|oriented| {
                let edge = self.edges.get(oriented.edge)?;
                let id = if oriented.forward { edge.start } else { edge.end };
                self.vertices.get(id).map(|v| v.position)
            })
            .collect()
    }

    /// Builds a rectangular cuboid centered at the origin.
    ///
    /// 8 vertices, 12 shared edges, 6 planar faces.
    pub fn cuboid(width: f64, height: f64, depth: f64) -> Self {
        let half = [width / 2.0, height / 2.0, depth / 2.0];
        let vertices: Vec<Vertex> = (0..8usize)
            .map(|i| {
                let sign = |bit: usize| if i & bit != 0 { 1.0 } else { -1.0 };
                Vertex::new(sign(1) * half[0], sign(2) * half[1], sign(4) * half[2])
            })
            .collect();

        let id = |sx: i8, sy: i8, sz: i8| -> VertexId {
            usize::from(sx > 0) | (usize::from(sy > 0) << 1) | (usize::from(sz > 0) << 2)
        };

        // Vertices in counterclockwise order as seen from outside, with outward normals.
        let definitions: [([[i8; 3]; 4], Point3); 6] = [
            ([[-1, -1, 1], [1, -1, 1], [1, 1, 1], [-1, 1, 1]], [0.0, 0.0, 1.0]),
            ([[-1, -1, -1], [-1, 1, -1], [1, 1, -1], [1, -1, -1]], [0.0, 0.0, -1.0]),
            ([[1, -1, -1], [1, 1, -1], [1, 1, 1], [1, -1, 1]], [1.0, 0.0, 0.0]),
            ([[-1, -1, -1], [-1, -1, 1], [-1, 1, 1], [-1, 1, -1]], [-1.0, 0.0, 0.0]),
            ([[-1, 1, -1], [-1, 1, 1], [1, 1, 1], [1, 1, -1]], [0.0, 1.0, 0.0]),
            ([[-1, -1, -1], [1, -1, -1], [1, -1, 1], [-1, -1, 1]], [0.0, -1.0, 0.0]),
        ];

        let mut edges = Vec::new();
        let mut faces = Vec::new();
        for (corners, normal) in &definitions {
            let ids: Vec<VertexId> = corners.iter().map(|c| id(c[0], c[1], c[2])).collect();
            let origin = vertices[ids[0]].position;
            faces.push(Face {
                surface: Surface::Plane {
                    frame: Frame::from_normal(origin, *normal),
                },
                outer_loop: polygon_loop(&mut edges, &ids),
                inner_loops: Vec::new(),
                same_sense: true,
                color: None,
            });
        }

        Self {
            vertices,
            edges,
            shells: vec![Shell { faces }],
            color: None,
        }
    }
}

/// Finds the edge `a → b` (or `b → a`, traversed in reverse), or creates it if absent.
fn oriented_edge(edges: &mut Vec<Edge>, a: VertexId, b: VertexId) -> OrientedEdge {
    if let Some(i) = edges.iter().position(|e| e.start == a && e.end == b) {
        return OrientedEdge {
            edge: i,
            forward: true,
        };
    }
    if let Some(i) = edges.iter().position(|e| e.start == b && e.end == a) {
        return OrientedEdge {
            edge: i,
            forward: false,
        };
    }
    edges.push(Edge {
        start: a,
        end: b,
        curve: Curve::Line,
        same_sense: true,
    });
    OrientedEdge {
        edge: edges.len() - 1,
        forward: true,
    }
}

/// Closed polygonal loop from ordered vertices.
fn polygon_loop(edges: &mut Vec<Edge>, ids: &[VertexId]) -> Loop {
    let oriented = (0..ids.len())
        .map(|i| oriented_edge(edges, ids[i], ids[(i + 1) % ids.len()]))
        .collect();
    Loop { edges: oriented }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cuboid_topology_counts() {
        let solid = Solid::cuboid(1.0, 2.0, 3.0);
        assert_eq!(solid.vertices.len(), 8);
        assert_eq!(solid.edges.len(), 12);
        assert_eq!(solid.face_count(), 6);
    }

    #[test]
    fn cuboid_edges_are_shared_with_opposite_orientation() {
        let solid = Solid::cuboid(1.0, 1.0, 1.0);
        let mut forward = vec![0; solid.edges.len()];
        let mut backward = vec![0; solid.edges.len()];
        for face in solid.faces() {
            for oriented in &face.outer_loop.edges {
                if oriented.forward {
                    forward[oriented.edge] += 1;
                } else {
                    backward[oriented.edge] += 1;
                }
            }
        }
        assert!(forward.iter().all(|&n| n == 1));
        assert!(backward.iter().all(|&n| n == 1));
    }

    #[test]
    fn frame_is_orthonormal() {
        let frame = Frame::new([0.0; 3], [0.0, 0.0, 2.0], [1.0, 0.0, 0.5]);
        assert!(dot(frame.x, frame.z).abs() < 1e-12);
        assert!((dot(frame.x, frame.x) - 1.0).abs() < 1e-12);
        assert!((dot(frame.y(), frame.y()) - 1.0).abs() < 1e-12);
    }
}