//! Conversion du graphe d'entités STEP en [`Solid`] B-Rep.

use crate::brep::{
    BSplineCurve, Curve, Edge, EdgeId, Face, Frame, Loop, OrientedEdge, Point3, Shell, Solid,
    Surface, Vertex, VertexId,
};
use crate::parser::{EntityId, Param, ParseError, Record, StepFile};
use std::collections::HashMap;

fn invalid(entity: EntityId, message: impl Into<String>) -> ParseError {
    ParseError::Invalid {
        entity,
        message: message.into(),
    }
}

/// Accès typé aux paramètres d'un enregistrement.
struct Args<'a> {
    id: EntityId,
    record: &'a Record,
}

impl<'a> Args<'a> {
    fn invalid(&self, message: &str) -> ParseError {
        invalid(self.id, message)
    }

    fn param(&self, index: usize) -> Result<&'a Param, ParseError> {
        self.record
            .params
            .get(index)
            .ok_or_else(|| self.invalid("paramètre manquant"))
    }

    fn reference(&self, index: usize) -> Result<EntityId, ParseError> {
        self.param(index)?
            .as_id()
            .ok_or_else(|| self.invalid("référence attendue"))
    }

    fn real(&self, index: usize) -> Result<f64, ParseError> {
        self.param(index)?
            .as_f64()
            .ok_or_else(|| self.invalid("nombre attendu"))
    }

    fn flag(&self, index: usize) -> Result<bool, ParseError> {
        match self.param(index)? {
            Param::Enum(s) if s == "T" => Ok(true),
            Param::Enum(s) if s == "F" => Ok(false),
            _ => Err(self.invalid("booléen attendu")),
        }
    }

    fn unsigned(&self, param: &Param) -> Result<usize, ParseError> {
        match param {
            Param::Int(v) if *v >= 0 => Ok(*v as usize),
            _ => Err(self.invalid("entier positif attendu")),
        }
    }

    fn list(&self, index: usize) -> Result<&'a [Param], ParseError> {
        self.param(index)?
            .as_list()
            .ok_or_else(|| self.invalid("liste attendue"))
    }

    fn references(&self, index: usize) -> Result<Vec<EntityId>, ParseError> {
        self.list(index)?
            .iter()
            .map(|p| p.as_id().ok_or_else(|| self.invalid("référence attendue")))
            .collect()
    }

    fn reals(&self, index: usize) -> Result<Vec<f64>, ParseError> {
        self.list(index)?
            .iter()
            .map(|p| p.as_f64().ok_or_else(|| self.invalid("nombre attendu")))
            .collect()
    }
}

fn vec3(args: &Args, index: usize) -> Result<Point3, ParseError> {
    match args.reals(index)?.as_slice() {
        [x, y, z] => Ok([*x, *y, *z]),
        _ => Err(args.invalid("3 coordonnées attendues")),
    }
}

struct Builder<'a> {
    file: &'a StepFile,
    vertices: Vec<Vertex>,
    edges: Vec<Edge>,
    vertex_ids: HashMap<EntityId, VertexId>,
    edge_ids: HashMap<EntityId, EdgeId>,
}

impl<'a> Builder<'a> {
    fn new(file: &'a StepFile) -> Self {
        Self {
            file,
            vertices: Vec::new(),
            edges: Vec::new(),
            vertex_ids: HashMap::new(),
            edge_ids: HashMap::new(),
        }
    }

    fn args(&self, id: EntityId, name: &str) -> Result<Args<'a>, ParseError> {
        let file: &'a StepFile = self.file;
        let entity = file
            .get(id)
            .ok_or_else(|| invalid(id, "entité introuvable"))?;
        let record = entity.record(name).ok_or_else(|| {
            invalid(id, format!("`{name}` attendu, trouvé `{}`", entity.name()))
        })?;
        Ok(Args { id, record })
    }

    fn point(&self, id: EntityId) -> Result<Point3, ParseError> {
        let args = self.args(id, "CARTESIAN_POINT")?;
        vec3(&args, 1)
    }

    fn direction(&self, id: EntityId) -> Result<Point3, ParseError> {
        let args = self.args(id, "DIRECTION")?;
        vec3(&args, 1)
    }

    fn placement(&self, id: EntityId) -> Result<Frame, ParseError> {
        let args = self.args(id, "AXIS2_PLACEMENT_3D")?;
        let origin = self.point(args.reference(1)?)?;
        let z = match args.param(2)? {
            Param::Ref(r) => self.direction(*r)?,
            _ => [0.0, 0.0, 1.0],
        };
        Ok(match args.param(3)? {
            Param::Ref(r) => Frame::new(origin, z, self.direction(*r)?),
            _ => Frame::from_normal(origin, z),
        })
    }

    fn curve(&self, id: EntityId) -> Result<Curve, ParseError> {
        let file: &'a StepFile = self.file;
        let name = file
            .get(id)
            .ok_or_else(|| invalid(id, "entité introuvable"))?
            .name();
        match name {
            "LINE" => Ok(Curve::Line),
            "CIRCLE" => {
                let args = self.args(id, name)?;
                Ok(Curve::Circle {
                    frame: self.placement(args.reference(1)?)?,
                    radius: args.real(2)?,
                })
            }
            "ELLIPSE" => {
                let args = self.args(id, name)?;
                Ok(Curve::Ellipse {
                    frame: self.placement(args.reference(1)?)?,
                    semi_major: args.real(2)?,
                    semi_minor: args.real(3)?,
                })
            }
            "B_SPLINE_CURVE_WITH_KNOTS" => {
                let args = self.args(id, name)?;
                let degree = args.unsigned(args.param(1)?)?;
                let control_points = args
                    .references(2)?
                    .into_iter()
                    .map(|p| self.point(p))
                    .collect::<Result<Vec<_>, _>>()?;
                let multiplicities = args
                    .list(6)?
                    .iter()
                    .map(|p| args.unsigned(p))
                    .collect::<Result<Vec<_>, _>>()?;
                let knots: Vec<f64> = args
                    .reals(7)?
                    .into_iter()
                    .zip(multiplicities)
                    .flat_map(|(knot, count)| std::iter::repeat_n(knot, count))
                    .collect();
                Ok(Curve::BSpline(BSplineCurve {
                    degree,
                    control_points,
                    weights: None,
                    knots,
                }))
            }
            other => Err(invalid(id, format!("courbe `{other}` non prise en charge"))),
        }
    }

    fn surface(&self, id: EntityId) -> Result<Surface, ParseError> {
        let file: &'a StepFile = self.file;
        let name = file
            .get(id)
            .ok_or_else(|| invalid(id, "entité introuvable"))?
            .name();
        match name {
            "PLANE" => {
                let args = self.args(id, name)?;
                Ok(Surface::Plane {
                    frame: self.placement(args.reference(1)?)?,
                })
            }
            "CYLINDRICAL_SURFACE" => {
                let args = self.args(id, name)?;
                Ok(Surface::Cylinder {
                    frame: self.placement(args.reference(1)?)?,
                    radius: args.real(2)?,
                })
            }
            "CONICAL_SURFACE" => {
                let args = self.args(id, name)?;
                Ok(Surface::Cone {
                    frame: self.placement(args.reference(1)?)?,
                    radius: args.real(2)?,
                    semi_angle: args.real(3)?,
                })
            }
            "SPHERICAL_SURFACE" => {
                let args = self.args(id, name)?;
                Ok(Surface::Sphere {
                    frame: self.placement(args.reference(1)?)?,
                    radius: args.real(2)?,
                })
            }
            "TOROIDAL_SURFACE" => {
                let args = self.args(id, name)?;
                Ok(Surface::Torus {
                    frame: self.placement(args.reference(1)?)?,
                    major_radius: args.real(2)?,
                    minor_radius: args.real(3)?,
                })
            }
            other => Err(invalid(id, format!("surface `{other}` non prise en charge"))),
        }
    }

    fn vertex(&mut self, id: EntityId) -> Result<VertexId, ParseError> {
        if let Some(&known) = self.vertex_ids.get(&id) {
            return Ok(known);
        }
        let args = self.args(id, "VERTEX_POINT")?;
        let position = self.point(args.reference(1)?)?;
        self.vertices.push(Vertex { position });
        let vertex = self.vertices.len() - 1;
        self.vertex_ids.insert(id, vertex);
        Ok(vertex)
    }

    fn edge(&mut self, id: EntityId) -> Result<EdgeId, ParseError> {
        if let Some(&known) = self.edge_ids.get(&id) {
            return Ok(known);
        }
        let args = self.args(id, "EDGE_CURVE")?;
        let start = self.vertex(args.reference(1)?)?;
        let end = self.vertex(args.reference(2)?)?;
        let curve = self.curve(args.reference(3)?)?;
        let same_sense = args.flag(4)?;
        self.edges.push(Edge {
            start,
            end,
            curve,
            same_sense,
        });
        let edge = self.edges.len() - 1;
        self.edge_ids.insert(id, edge);
        Ok(edge)
    }

    fn edge_loop(&mut self, id: EntityId, reversed: bool) -> Result<Loop, ParseError> {
        let args = self.args(id, "EDGE_LOOP")?;
        let mut edges = Vec::new();
        for oriented_id in args.references(1)? {
            let oriented = self.args(oriented_id, "ORIENTED_EDGE")?;
            let edge = self.edge(oriented.reference(3)?)?;
            let forward = oriented.flag(4)?;
            edges.push(OrientedEdge { edge, forward });
        }
        if reversed {
            edges.reverse();
            for oriented in &mut edges {
                oriented.forward = !oriented.forward;
            }
        }
        Ok(Loop { edges })
    }

    fn face(&mut self, id: EntityId) -> Result<Face, ParseError> {
        let file: &'a StepFile = self.file;
        let args = self.args(id, "ADVANCED_FACE")?;
        let surface = self.surface(args.reference(2)?)?;
        let same_sense = args.flag(3)?;

        let mut bounds = Vec::new();
        for bound_id in args.references(1)? {
            let declared_outer = file
                .get(bound_id)
                .ok_or_else(|| invalid(bound_id, "entité introuvable"))?
                .name()
                == "FACE_OUTER_BOUND";
            let name = if declared_outer {
                "FACE_OUTER_BOUND"
            } else {
                "FACE_BOUND"
            };
            let bound = self.args(bound_id, name)?;
            let face_loop = self.edge_loop(bound.reference(1)?, !bound.flag(2)?)?;
            bounds.push((declared_outer, face_loop));
        }
        if bounds.is_empty() {
            return Err(args.invalid("face sans contour"));
        }

        // Sans FACE_OUTER_BOUND déclaré, le premier contour est supposé externe.
        let outer_index = bounds.iter().position(|(outer, _)| *outer).unwrap_or(0);
        let (_, outer_loop) = bounds.remove(outer_index);
        let inner_loops = bounds.into_iter().map(|(_, l)| l).collect();
        Ok(Face {
            surface,
            outer_loop,
            inner_loops,
            same_sense,
        })
    }

    fn shell(&mut self, id: EntityId) -> Result<Shell, ParseError> {
        let args = self.args(id, "CLOSED_SHELL")?;
        let mut faces = Vec::new();
        for face_id in args.references(1)? {
            faces.push(self.face(face_id)?);
        }
        Ok(Shell { faces })
    }
}

/// Reconstruit un [`Solid`] par `MANIFOLD_SOLID_BREP` présent dans le fichier.
pub(crate) fn build_solids(file: &StepFile) -> Result<Vec<Solid>, ParseError> {
    let mut solids = Vec::new();
    for (&id, entity) in &file.entities {
        if entity.name() != "MANIFOLD_SOLID_BREP" {
            continue;
        }
        let mut builder = Builder::new(file);
        let args = builder.args(id, "MANIFOLD_SOLID_BREP")?;
        let shell = builder.shell(args.reference(1)?)?;
        solids.push(Solid {
            vertices: builder.vertices,
            edges: builder.edges,
            shells: vec![shell],
        });
    }
    Ok(solids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_step;

    const TRIANGLE: &str = r##"#1=CARTESIAN_POINT('',(0.,0.,0.));
#2=CARTESIAN_POINT('',(1.,0.,0.));
#3=CARTESIAN_POINT('',(0.,1.,0.));
#4=VERTEX_POINT('',#1);
#5=VERTEX_POINT('',#2);
#6=VERTEX_POINT('',#3);
#7=DIRECTION('',(1.,0.,0.));
#8=VECTOR('',#7,1.);
#9=LINE('',#1,#8);
#10=EDGE_CURVE('',#4,#5,#9,.T.);
#11=EDGE_CURVE('',#5,#6,#9,.T.);
#12=EDGE_CURVE('',#6,#4,#9,.T.);
#13=ORIENTED_EDGE('',*,*,#10,.T.);
#14=ORIENTED_EDGE('',*,*,#11,.T.);
#15=ORIENTED_EDGE('',*,*,#12,.F.);
#16=EDGE_LOOP('',(#13,#14,#15));
#17=FACE_BOUND('',#16,.T.);
#18=DIRECTION('',(0.,0.,1.));
#19=AXIS2_PLACEMENT_3D('',#1,#18,$);
#20=PLANE('',#19);
#21=ADVANCED_FACE('',(#17),#20,.T.);
#22=CLOSED_SHELL('',(#21));
#23=MANIFOLD_SOLID_BREP('',#22);
"##;

    fn file(body: &str) -> String {
        format!("ISO-10303-21;\nHEADER;\nENDSEC;\nDATA;\n{body}ENDSEC;\nEND-ISO-10303-21;\n")
    }

    #[test]
    fn builds_triangle_face() {
        let model = parse_step(&file(TRIANGLE)).unwrap();
        assert_eq!(model.solids.len(), 1);
        let solid = &model.solids[0];
        assert_eq!(solid.vertices.len(), 3);
        assert_eq!(solid.edges.len(), 3);
        assert_eq!(solid.face_count(), 1);

        let face = solid.faces().next().unwrap();
        assert!(face.same_sense);
        assert!(face.inner_loops.is_empty());
        assert!(!face.outer_loop.edges[2].forward);
        assert_eq!(solid.loop_points(&face.outer_loop).len(), 3);
        assert!(matches!(&face.surface, Surface::Plane { frame } if frame.z == [0.0, 0.0, 1.0]));
    }

    #[test]
    fn rejects_unsupported_surface() {
        let body = TRIANGLE.replace("PLANE('',#19)", "B_SPLINE_SURFACE_WITH_KNOTS('',#19)");
        assert!(matches!(
            parse_step(&file(&body)),
            Err(ParseError::Invalid { entity: 20, .. })
        ));
    }
}