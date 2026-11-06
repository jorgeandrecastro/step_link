//! Topologie B-Rep (Boundary Representation).
//!
//! La hiérarchie est : [`Solid`] → [`Shell`] → [`Face`] → [`Edge`] → [`Vertex`].
//! Cette première version est volontairement simple : les faces sont planes
//! et délimitées par une boucle externe d'arêtes orientées.

/// Point (ou vecteur) 3D en double précision.
pub type Point3 = [f64; 3];

/// Sommet de la topologie : un point de l'espace.
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

/// Arête orientée reliant deux sommets (segment de droite pour l'instant).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Edge {
    /// Sommet de départ.
    pub start: Vertex,
    /// Sommet d'arrivée.
    pub end: Vertex,
}

impl Edge {
    /// Crée une arête orientée de `start` vers `end`.
    pub fn new(start: Vertex, end: Vertex) -> Self {
        Self { start, end }
    }
}

/// Face plane délimitée par une boucle fermée d'arêtes consécutives.
#[derive(Debug, Clone, PartialEq)]
pub struct Face {
    /// Boucle externe : chaque arête commence là où la précédente se termine.
    pub outer_loop: Vec<Edge>,
    /// Normale sortante de la face (unitaire).
    pub normal: Point3,
}

impl Face {
    /// Crée une face à partir de sa boucle externe et de sa normale.
    pub fn new(outer_loop: Vec<Edge>, normal: Point3) -> Self {
        Self { outer_loop, normal }
    }

    /// Construit une face polygonale à partir de sommets ordonnés (fermeture implicite).
    pub fn from_polygon(vertices: &[Vertex], normal: Point3) -> Self {
        let outer_loop = vertices
            .iter()
            .enumerate()
            .map(|(i, v)| Edge::new(*v, vertices[(i + 1) % vertices.len()]))
            .collect();
        Self { outer_loop, normal }
    }

    /// Sommets de la boucle externe, dans l'ordre.
    pub fn vertices(&self) -> impl Iterator<Item = &Vertex> {
        self.outer_loop.iter().map(|e| &e.start)
    }
}

/// Coque : ensemble connexe de faces (surface fermée ou ouverte).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Shell {
    /// Faces composant la coque.
    pub faces: Vec<Face>,
}

/// Solide : un ensemble de coques (la première est la coque externe).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Solid {
    /// Coques délimitant le solide.
    pub shells: Vec<Shell>,
}

impl Solid {
    /// Construit un parallélépipède rectangle centré sur l'origine.
    ///
    /// Utile pour les tests et comme géométrie de repli.
    pub fn cuboid(width: f64, height: f64, depth: f64) -> Self {
        let (x, y, z) = (width / 2.0, height / 2.0, depth / 2.0);
        let v = |sx: f64, sy: f64, sz: f64| Vertex::new(sx * x, sy * y, sz * z);

        let faces = vec![
            // +Z
            Face::from_polygon(
                &[v(-1., -1., 1.), v(1., -1., 1.), v(1., 1., 1.), v(-1., 1., 1.)],
                [0., 0., 1.],
            ),
            // -Z
            Face::from_polygon(
                &[v(-1., -1., -1.), v(-1., 1., -1.), v(1., 1., -1.), v(1., -1., -1.)],
                [0., 0., -1.],
            ),
            // +X
            Face::from_polygon(
                &[v(1., -1., -1.), v(1., 1., -1.), v(1., 1., 1.), v(1., -1., 1.)],
                [1., 0., 0.],
            ),
            // -X
            Face::from_polygon(
                &[v(-1., -1., -1.), v(-1., -1., 1.), v(-1., 1., 1.), v(-1., 1., -1.)],
                [-1., 0., 0.],
            ),
            // +Y
            Face::from_polygon(
                &[v(-1., 1., -1.), v(-1., 1., 1.), v(1., 1., 1.), v(1., 1., -1.)],
                [0., 1., 0.],
            ),
            // -Y
            Face::from_polygon(
                &[v(-1., -1., -1.), v(1., -1., -1.), v(1., -1., 1.), v(-1., -1., 1.)],
                [0., -1., 0.],
            ),
        ];

        Self {
            shells: vec![Shell { faces }],
        }
    }

    /// Nombre total de faces du solide.
    pub fn face_count(&self) -> usize {
        self.shells.iter().map(|s| s.faces.len()).sum()
    }
}
