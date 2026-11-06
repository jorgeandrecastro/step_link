//! Topologie B-Rep (Boundary Representation).
//!
//! Hiérarchie : [`Solid`] → [`Shell`] → [`Face`] → [`Loop`] → [`OrientedEdge`] → [`Edge`] → [`Vertex`].
//! Les sommets et arêtes sont stockés une seule fois dans le [`Solid`] et référencés par
//! index : une arête partagée par deux faces n'existe qu'une fois.

/// Point (ou vecteur) 3D en double précision.
pub type Point3 = [f64; 3];

/// Index d'un sommet dans [`Solid::vertices`].
pub type VertexId = usize;

/// Index d'une arête dans [`Solid::edges`].
pub type EdgeId = usize;

fn dot(a: Point3, b: Point3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: Point3, b: Point3) -> Point3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize(v: Point3) -> Point3 {
    let norm = dot(v, v).sqrt();
    if norm == 0.0 {
        v
    } else {
        [v[0] / norm, v[1] / norm, v[2] / norm]
    }
}

/// Repère orthonormé (`AXIS2_PLACEMENT_3D` en STEP).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    /// Origine du repère.
    pub origin: Point3,
    /// Axe Z unitaire.
    pub z: Point3,
    /// Axe X unitaire, orthogonal à `z`.
    pub x: Point3,
}

impl Frame {
    /// Crée un repère ; `z` est normalisé et `x` est orthogonalisé par rapport à `z`.
    ///
    /// `x` ne doit pas être colinéaire à `z`.
    pub fn new(origin: Point3, z: Point3, x: Point3) -> Self {
        let z = normalize(z);
        let d = dot(x, z);
        let x = normalize([x[0] - z[0] * d, x[1] - z[1] * d, x[2] - z[2] * d]);
        Self { origin, z, x }
    }

    /// Crée un repère dont seul l'axe Z est imposé ; X est choisi arbitrairement.
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

    /// Axe Y unitaire (`z × x`).
    pub fn y(&self) -> Point3 {
        cross(self.z, self.x)
    }
}

/// Sommet : un point de l'espace.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vertex {
    /// Position du sommet.
    pub position: Point3,
}

impl Vertex {
    /// Crée un sommet à la position donnée.
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self {
            position: [x, y, z],
        }
    }
}

/// Courbe portée par une arête.
#[derive(Debug, Clone, PartialEq)]
pub enum Curve {
    /// Segment de droite entre les sommets de l'arête.
    Line,
    /// Cercle (ou arc) dans le plan XY du repère.
    Circle {
        /// Repère du cercle.
        frame: Frame,
        /// Rayon.
        radius: f64,
    },
    /// Ellipse (ou arc) dans le plan XY du repère.
    Ellipse {
        /// Repère de l'ellipse.
        frame: Frame,
        /// Demi-axe selon X.
        semi_major: f64,
        /// Demi-axe selon Y.
        semi_minor: f64,
    },
    /// Courbe B-spline (rationnelle si `weights` est renseigné).
    BSpline(BSplineCurve),
}

/// Courbe B-spline.
#[derive(Debug, Clone, PartialEq)]
pub struct BSplineCurve {
    /// Degré.
    pub degree: usize,
    /// Points de contrôle.
    pub control_points: Vec<Point3>,
    /// Poids des points de contrôle (NURBS), absents pour une B-spline non rationnelle.
    pub weights: Option<Vec<f64>>,
    /// Vecteur de nœuds développé (multiplicités déjà appliquées).
    pub knots: Vec<f64>,
}

/// Surface portée par une face.
#[derive(Debug, Clone, PartialEq)]
pub enum Surface {
    /// Plan passant par l'origine du repère, de normale `z`.
    Plane {
        /// Repère du plan.
        frame: Frame,
    },
    /// Cylindre d'axe `z`.
    Cylinder {
        /// Repère du cylindre.
        frame: Frame,
        /// Rayon.
        radius: f64,
    },
    /// Cône d'axe `z`.
    Cone {
        /// Repère du cône.
        frame: Frame,
        /// Rayon à l'origine.
        radius: f64,
        /// Demi-angle au sommet, en radians.
        semi_angle: f64,
    },
    /// Sphère centrée sur l'origine du repère.
    Sphere {
        /// Repère de la sphère.
        frame: Frame,
        /// Rayon.
        radius: f64,
    },
    /// Tore d'axe `z`.
    Torus {
        /// Repère du tore.
        frame: Frame,
        /// Rayon majeur.
        major_radius: f64,
        /// Rayon mineur.
        minor_radius: f64,
    },
    /// Surface B-spline (rationnelle si `weights` est renseigné).
    BSpline(BSplineSurface),
}

/// Surface B-spline.
#[derive(Debug, Clone, PartialEq)]
pub struct BSplineSurface {
    /// Degré selon u.
    pub degree_u: usize,
    /// Degré selon v.
    pub degree_v: usize,
    /// Grille de points de contrôle, indexée `[u][v]`.
    pub control_points: Vec<Vec<Point3>>,
    /// Poids de la grille (NURBS), absents pour une B-spline non rationnelle.
    pub weights: Option<Vec<Vec<f64>>>,
    /// Vecteur de nœuds développé selon u.
    pub knots_u: Vec<f64>,
    /// Vecteur de nœuds développé selon v.
    pub knots_v: Vec<f64>,
}

/// Arête : une courbe entre deux sommets, partagée par les faces adjacentes.
#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    /// Sommet de départ.
    pub start: VertexId,
    /// Sommet d'arrivée.
    pub end: VertexId,
    /// Courbe portée par l'arête.
    pub curve: Curve,
}

/// Utilisation d'une arête dans une boucle, avec son sens de parcours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrientedEdge {
    /// Arête utilisée.
    pub edge: EdgeId,
    /// `true` si l'arête est parcourue de `start` vers `end`.
    pub forward: bool,
}

/// Boucle fermée d'arêtes orientées, consécutives.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Loop {
    /// Arêtes de la boucle, dans l'ordre de parcours.
    pub edges: Vec<OrientedEdge>,
}

/// Face : une portion de surface délimitée par une boucle externe et des trous.
#[derive(Debug, Clone, PartialEq)]
pub struct Face {
    /// Surface support.
    pub surface: Surface,
    /// Boucle externe.
    pub outer_loop: Loop,
    /// Boucles internes (trous).
    pub inner_loops: Vec<Loop>,
    /// `true` si la normale de la face suit celle de la surface, `false` si elle est inversée.
    pub same_sense: bool,
}

/// Coque : ensemble connexe de faces.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Shell {
    /// Faces de la coque.
    pub faces: Vec<Face>,
}

/// Solide : géométrie partagée (sommets, arêtes) et coques qui l'utilisent.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Solid {
    /// Sommets, référencés par [`VertexId`].
    pub vertices: Vec<Vertex>,
    /// Arêtes, référencées par [`EdgeId`].
    pub edges: Vec<Edge>,
    /// Coques (la première est la coque externe).
    pub shells: Vec<Shell>,
}

impl Solid {
    /// Itère sur toutes les faces du solide.
    pub fn faces(&self) -> impl Iterator<Item = &Face> {
        self.shells.iter().flat_map(|s| s.faces.iter())
    }

    /// Nombre total de faces du solide.
    pub fn face_count(&self) -> usize {
        self.faces().count()
    }

    /// Points de départ de chaque arête orientée d'une boucle, dans l'ordre.
    ///
    /// Les index invalides sont ignorés.
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

    /// Construit un parallélépipède rectangle centré sur l'origine.
    ///
    /// 8 sommets, 12 arêtes partagées, 6 faces planes.
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

        // Sommets en sens anti-horaire vu de l'extérieur, et normale sortante.
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
            });
        }

        Self {
            vertices,
            edges,
            shells: vec![Shell { faces }],
        }
    }
}

/// Retrouve l'arête `a → b` (ou `b → a`, parcourue à l'envers), sinon la crée.
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
    });
    OrientedEdge {
        edge: edges.len() - 1,
        forward: true,
    }
}

/// Boucle polygonale fermée à partir de sommets ordonnés.
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