//! Parser STEP (ISO 10303-21, « Part 21 »).
//!
//! Lexer and entity graph: the file is read into a [`StepFile`] (header +
//! entities indexed by `#id`) with typed parameters ([`Param`]).
//! [`parse_step`] then reconstructs B-Rep solids from this graph.
//!
//! Known limitations: string escape sequences (`\X\`, `\X2\`, …) are not decoded,
//! and only the `HEADER` and `DATA` sections are read.

use crate::brep::Solid;
use crate::builder::build_solids;
use std::collections::BTreeMap;
use std::fmt;

/// STEP entity identifier (the `42` in `#42`).
pub type EntityId = u64;

/// Errors that can occur while reading a STEP file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// The file does not start with `ISO-10303-21;`.
    MissingHeader,
    /// No `DATA;` section was found.
    MissingDataSection,
    /// The file ends without `END-ISO-10303-21;`.
    MissingFooter,
    /// A syntax error occurred on a given line.
    Syntax {
        /// Line number (starting at 1).
        line: usize,
        /// Description de l'erreur.
        message: String,
    },
    /// The STEP entity is invalid or unsupported.
    Invalid {
        /// Identifier of the entity involved.
        entity: EntityId,
        /// Description of the problem.
        message: String,
    },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingHeader => write!(f, "en-tête `ISO-10303-21;` manquant"),
            Self::MissingDataSection => write!(f, "section `DATA;` manquante"),
            Self::MissingFooter => write!(f, "pied `END-ISO-10303-21;` manquant"),
            Self::Syntax { line, message } => write!(f, "ligne {line} : {message}"),
            Self::Invalid { entity, message } => write!(f, "entité #{entity} : {message}"),
        }
    }
}

impl std::error::Error for ParseError {}

/// Parameter of a STEP entity.
#[derive(Debug, Clone, PartialEq)]
pub enum Param {
    /// Reference to another entity (`#12`).
    Ref(EntityId),
    /// Entier (`3`).
    Int(i64),
    /// Real number (`1.5`, `1.E-3`).
    Real(f64),
    /// String (`'text'`), with doubled apostrophes already resolved.
    Str(String),
    /// Enumeration or boolean, uppercase and without the periods (`.T.` → `T`).
    Enum(String),
    /// Binary data (`"0A1F"`), raw hexadecimal content.
    Binary(String),
    /// List of parameters (`(1.,2.,3.)`).
    List(Vec<Param>),
    /// Typed value (`LENGTH_MEASURE(10.)`).
    Typed(String, Vec<Param>),
    /// Unspecified value (`$`).
    Omitted,
    /// Derived value (`*`).
    Derived,
}

impl Param {
    /// Returns the referenced identifier if this is a [`Param::Ref`].
    pub fn as_id(&self) -> Option<EntityId> {
        match self {
            Self::Ref(id) => Some(*id),
            _ => None,
        }
    }

    /// Returns the numeric value of a [`Param::Real`] or [`Param::Int`].
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Real(v) => Some(*v),
            Self::Int(v) => Some(*v as f64),
            _ => None,
        }
    }

    /// Returns the string value of a [`Param::Str`].
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s),
            _ => None,
        }
    }

    /// Returns the elements of a [`Param::List`].
    pub fn as_list(&self) -> Option<&[Param]> {
        match self {
            Self::List(items) => Some(items),
            _ => None,
        }
    }
}

/// Simple entity: a type name and its parameters (`CARTESIAN_POINT('',(0.,0.,0.))`).
#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    /// Type name, in uppercase.
    pub name: String,
    /// Parameters, in order.
    pub params: Vec<Param>,
}

/// File entity: one simple record, or multiple records for a complex entity.
#[derive(Debug, Clone, PartialEq)]
pub struct Entity {
    /// Records that make up the entity (never empty).
    pub records: Vec<Record>,
}

impl Entity {
    /// Name of the first record.
    pub fn name(&self) -> &str {
        self.records.first().map_or("", |r| r.name.as_str())
    }

    /// `true` if the entity is complex (`#5 = (A() B())`).
    pub fn is_complex(&self) -> bool {
        self.records.len() > 1
    }

    /// Finds the record with this name (case-insensitive).
    pub fn record(&self, name: &str) -> Option<&Record> {
        self.records.iter().find(|r| r.name == name)
    }
}

/// Contents of a STEP file after reading.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StepFile {
    /// Records in the `HEADER` section.
    pub header: Vec<Record>,
    /// Entities in the `DATA` section, indexed by identifier.
    pub entities: BTreeMap<EntityId, Entity>,
}

impl StepFile {
    /// Returns the entity with this identifier.
    pub fn get(&self, id: EntityId) -> Option<&Entity> {
        self.entities.get(&id)
    }
}

/// Result of parsing a STEP file.
#[derive(Debug, Clone, PartialEq)]
pub struct StepModel {
    /// Reconstructed solids, one per `MANIFOLD_SOLID_BREP`.
    pub solids: Vec<Solid>,
    /// Number of entities in the `DATA` section.
    pub entity_count: usize,
    /// Meters per file length unit (`0.001` for a file in millimeters, `1.0` if unknown).
    pub unit_scale: f64,
    /// Faces skipped because their surface is unsupported.
    pub skipped_faces: usize,
}

/// Parses a STEP file and reconstructs its solids.
///
/// # Erreurs
///
/// Returns a [`ParseError`] if the file is invalid.
pub fn parse_step(input: &str) -> Result<StepModel, ParseError> {
    let file = parse_part21(input)?;
    let (solids, skipped_faces) = build_solids(&file)?;
    Ok(StepModel {
        solids,
        entity_count: file.entities.len(),
        unit_scale: length_unit_scale(&file),
        skipped_faces,
    })
}

/// Scale of the SI length unit declared in the file; `1.0` otherwise (non-SI units are unsupported).
fn length_unit_scale(file: &StepFile) -> f64 {
    file.entities
        .values()
        .find_map(|entity| {
            entity.record("LENGTH_UNIT")?;
            let si = entity.record("SI_UNIT")?;
            let prefix = match si.params.first() {
                Some(Param::Enum(p)) => p.as_str(),
                _ => "",
            };
            Some(match prefix {
                "MICRO" => 1e-6,
                "MILLI" => 1e-3,
                "CENTI" => 1e-2,
                "DECI" => 1e-1,
                "KILO" => 1e3,
                _ => 1.0,
            })
        })
        .unwrap_or(1.0)
}

/// Reads the Part 21 structure: header and entity graph, without interpreting the geometry.
///
/// # Erreurs
///
/// Returns a [`ParseError`] if the syntax is invalid.
pub fn parse_part21(input: &str) -> Result<StepFile, ParseError> {
    let mut lexer = Lexer::new(input.trim_start_matches('\u{feff}'));
    if !lexer.eat_ident("ISO-10303-21") {
        return Err(ParseError::MissingHeader);
    }
    lexer.expect(b';')?;

    let mut file = StepFile::default();
    let mut has_data = false;
    loop {
        lexer.skip_trivia()?;
        if lexer.peek().is_none() {
            return Err(ParseError::MissingFooter);
        }
        let section = lexer.ident()?;
        match section.as_str() {
            "HEADER" => {
                lexer.expect(b';')?;
                lexer.header(&mut file)?;
            }
            "DATA" => {
                lexer.skip_trivia()?;
                if lexer.peek() == Some(b'(') {
                    lexer.params()?;
                }
                lexer.expect(b';')?;
                lexer.data(&mut file)?;
                has_data = true;
            }
            "END-ISO-10303-21" => {
                lexer.expect(b';')?;
                return if has_data {
                    Ok(file)
                } else {
                    Err(ParseError::MissingDataSection)
                };
            }
            other => {
                return Err(lexer.error(&format!("section `{other}` non prise en charge")));
            }
        }
    }
}

struct Lexer<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Self { src, pos: 0 }
    }

    fn peek(&self) -> Option<u8> {
        self.src.as_bytes().get(self.pos).copied()
    }

    fn error(&self, message: &str) -> ParseError {
        let end = self.pos.min(self.src.len());
        let line = 1 + self.src.as_bytes()[..end]
            .iter()
            .filter(|&&b| b == b'\n')
            .count();
        ParseError::Syntax {
            line,
            message: message.to_string(),
        }
    }

    /// Skips whitespace and `/* ... */` comments.
    fn skip_trivia(&mut self) -> Result<(), ParseError> {
        loop {
            match self.peek() {
                Some(b' ' | b'\t' | b'\r' | b'\n') => self.pos += 1,
                Some(b'/') if self.src.as_bytes().get(self.pos + 1) == Some(&b'*') => {
                    match self.src[self.pos + 2..].find("*/") {
                        Some(i) => self.pos += i + 4,
                        None => return Err(self.error("commentaire non terminé")),
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    fn expect(&mut self, byte: u8) -> Result<(), ParseError> {
        self.skip_trivia()?;
        if self.peek() == Some(byte) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.error(&format!("`{}` attendu", byte as char)))
        }
    }

    /// Reads an uppercase keyword (`CARTESIAN_POINT`, `END-ISO-10303-21`, …).
    fn ident(&mut self) -> Result<String, ParseError> {
        self.skip_trivia()?;
        let start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'!'))
        {
            self.pos += 1;
        }
        if start == self.pos {
            return Err(self.error("mot-clé attendu"));
        }
        Ok(self.src[start..self.pos].to_ascii_uppercase())
    }

    /// Consumes the expected keyword, or leaves the input unchanged.
    fn eat_ident(&mut self, expected: &str) -> bool {
        let saved = self.pos;
        match self.ident() {
            Ok(found) if found == expected => true,
            _ => {
                self.pos = saved;
                false
            }
        }
    }

    fn header(&mut self, file: &mut StepFile) -> Result<(), ParseError> {
        loop {
            let name = self.ident()?;
            if name == "ENDSEC" {
                return self.expect(b';');
            }
            let params = self.params()?;
            self.expect(b';')?;
            file.header.push(Record { name, params });
        }
    }

    fn data(&mut self, file: &mut StepFile) -> Result<(), ParseError> {
        loop {
            self.skip_trivia()?;
            if self.peek() != Some(b'#') {
                let name = self.ident()?;
                if name == "ENDSEC" {
                    return self.expect(b';');
                }
                return Err(self.error(&format!("`{name}` inattendu dans la section DATA")));
            }

            let id = self.entity_id()?;
            self.expect(b'=')?;
            self.skip_trivia()?;
            let records = if self.peek() == Some(b'(') {
                self.pos += 1;
                let mut records = Vec::new();
                loop {
                    self.skip_trivia()?;
                    if self.peek() == Some(b')') {
                        self.pos += 1;
                        break;
                    }
                    records.push(self.record()?);
                }
                if records.is_empty() {
                    return Err(self.error("entité complexe vide"));
                }
                records
            } else {
                vec![self.record()?]
            };
            self.expect(b';')?;

            if file.entities.insert(id, Entity { records }).is_some() {
                return Err(self.error(&format!("identifiant #{id} dupliqué")));
            }
        }
    }

    fn record(&mut self) -> Result<Record, ParseError> {
        let name = self.ident()?;
        let params = self.params()?;
        Ok(Record { name, params })
    }

    fn entity_id(&mut self) -> Result<EntityId, ParseError> {
        self.expect(b'#')?;
        let start = self.pos;
        self.digits();
        self.src[start..self.pos]
            .parse()
            .map_err(|_| self.error("identifiant d'entité invalide"))
    }

    /// Lit `( p1, p2, ... )`.
    fn params(&mut self) -> Result<Vec<Param>, ParseError> {
        self.expect(b'(')?;
        let mut out = Vec::new();
        self.skip_trivia()?;
        if self.peek() == Some(b')') {
            self.pos += 1;
            return Ok(out);
        }
        loop {
            out.push(self.param()?);
            self.skip_trivia()?;
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b')') => {
                    self.pos += 1;
                    return Ok(out);
                }
                _ => return Err(self.error("`,` ou `)` attendu")),
            }
        }
    }

    fn param(&mut self) -> Result<Param, ParseError> {
        self.skip_trivia()?;
        match self.peek() {
            Some(b'#') => Ok(Param::Ref(self.entity_id()?)),
            Some(b'\'') => Ok(Param::Str(self.string()?)),
            Some(b'.') => self.enumeration(),
            Some(b'(') => Ok(Param::List(self.params()?)),
            Some(b'$') => {
                self.pos += 1;
                Ok(Param::Omitted)
            }
            Some(b'*') => {
                self.pos += 1;
                Ok(Param::Derived)
            }
            Some(b'"') => self.binary(),
            Some(b'0'..=b'9' | b'+' | b'-') => self.number(),
            Some(c) if c.is_ascii_alphabetic() || c == b'!' => {
                let name = self.ident()?;
                Ok(Param::Typed(name, self.params()?))
            }
            _ => Err(self.error("paramètre attendu")),
        }
    }

    fn string(&mut self) -> Result<String, ParseError> {
        let src = self.src;
        self.pos += 1;
        let mut out = String::new();
        loop {
            let rest = &src[self.pos..];
            let Some(i) = rest.find('\'') else {
                return Err(self.error("chaîne non terminée"));
            };
            out.push_str(&rest[..i]);
            self.pos += i + 1;
            if self.peek() == Some(b'\'') {
                out.push('\'');
                self.pos += 1;
            } else {
                return Ok(out);
            }
        }
    }

    fn enumeration(&mut self) -> Result<Param, ParseError> {
        self.pos += 1;
        let start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_ascii_alphanumeric() || c == b'_') {
            self.pos += 1;
        }
        if start == self.pos || self.peek() != Some(b'.') {
            return Err(self.error("énumération invalide"));
        }
        let name = self.src[start..self.pos].to_ascii_uppercase();
        self.pos += 1;
        Ok(Param::Enum(name))
    }

    fn binary(&mut self) -> Result<Param, ParseError> {
        let src = self.src;
        self.pos += 1;
        let rest = &src[self.pos..];
        let Some(i) = rest.find('"') else {
            return Err(self.error("donnée binaire non terminée"));
        };
        self.pos += i + 1;
        Ok(Param::Binary(rest[..i].to_string()))
    }

    fn digits(&mut self) {
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
    }

    fn number(&mut self) -> Result<Param, ParseError> {
        let start = self.pos;
        if matches!(self.peek(), Some(b'+' | b'-')) {
            self.pos += 1;
        }
        self.digits();
        let mut real = false;
        if self.peek() == Some(b'.') {
            real = true;
            self.pos += 1;
            self.digits();
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            real = true;
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            self.digits();
        }
        let text = &self.src[start..self.pos];
        if real {
            text.parse::<f64>()
                .map(Param::Real)
                .map_err(|_| self.error("réel invalide"))
        } else {
            text.parse::<i64>()
                .map(Param::Int)
                .map_err(|_| self.error("entier invalide"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r##"ISO-10303-21;
HEADER;
FILE_DESCRIPTION(('Fusion 360'),'2;1');
FILE_NAME('cube.step','2026-11-04T10:00:00',('Jorge'),(''),'',' ','');
FILE_SCHEMA(('AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }'));
ENDSEC;
DATA;
/* un commentaire */
#1 = CARTESIAN_POINT('Origine',(0.,0.,0.));
#2 = DIRECTION('',(0.,0.,1.));
#3 = AXIS2_PLACEMENT_3D('',#1,#2,$);
#4 = PLANE('',#3);
#5 = ( LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.) );
#6 = CARTESIAN_POINT('l''apostrophe',(1.E-3,-2.5,+3));
#7 = MEASURE_WITH_UNIT(LENGTH_MEASURE(10.),#5);
ENDSEC;
END-ISO-10303-21;
"##;

    #[test]
    fn reads_header_and_entities() {
        let file = parse_part21(SAMPLE).unwrap();
        assert_eq!(file.header.len(), 3);
        assert_eq!(file.entities.len(), 7);
        assert_eq!(file.get(4).unwrap().name(), "PLANE");
        assert_eq!(file.get(4).unwrap().records[0].params[1], Param::Ref(3));
    }

    #[test]
    fn reads_scalar_params() {
        let file = parse_part21(SAMPLE).unwrap();
        let params = &file.get(6).unwrap().records[0].params;
        assert_eq!(params[0].as_str(), Some("l'apostrophe"));
        let coords = params[1].as_list().unwrap();
        assert_eq!(coords[0], Param::Real(0.001));
        assert_eq!(coords[1].as_f64(), Some(-2.5));
        assert_eq!(coords[2], Param::Int(3));
        assert_eq!(file.get(3).unwrap().records[0].params[3], Param::Omitted);
    }

    #[test]
    fn reads_complex_entity() {
        let file = parse_part21(SAMPLE).unwrap();
        let unit = file.get(5).unwrap();
        assert!(unit.is_complex());
        assert_eq!(unit.records.len(), 3);
        assert_eq!(
            unit.record("SI_UNIT").unwrap().params,
            vec![Param::Enum("MILLI".into()), Param::Enum("METRE".into())]
        );
        assert_eq!(unit.record("NAMED_UNIT").unwrap().params, vec![Param::Derived]);
    }

    #[test]
    fn reads_typed_param() {
        let file = parse_part21(SAMPLE).unwrap();
        let params = &file.get(7).unwrap().records[0].params;
        assert_eq!(
            params[0],
            Param::Typed("LENGTH_MEASURE".into(), vec![Param::Real(10.0)])
        );
        assert_eq!(params[1].as_id(), Some(5));
    }

    #[test]
    fn parse_step_counts_entities() {
        assert_eq!(parse_step(SAMPLE).unwrap().entity_count, 7);
    }

    #[test]
    fn reads_length_unit_scale() {
        assert_eq!(parse_step(SAMPLE).unwrap().unit_scale, 1e-3);
    }

    #[test]
    fn rejects_missing_header() {
        assert_eq!(parse_step("DATA;"), Err(ParseError::MissingHeader));
    }

    #[test]
    fn rejects_missing_footer() {
        let input = "ISO-10303-21;\nHEADER;\nENDSEC;\nDATA;\nENDSEC;\n";
        assert_eq!(parse_step(input), Err(ParseError::MissingFooter));
    }

    #[test]
    fn rejects_missing_data() {
        let input = "ISO-10303-21;\nHEADER;\nENDSEC;\nEND-ISO-10303-21;\n";
        assert_eq!(parse_step(input), Err(ParseError::MissingDataSection));
    }

    #[test]
    fn rejects_duplicate_id() {
        let input = "ISO-10303-21;\nHEADER;\nENDSEC;\nDATA;\n#1=A();\n#1=B();\nENDSEC;\nEND-ISO-10303-21;\n";
        assert!(matches!(parse_step(input), Err(ParseError::Syntax { .. })));
    }

    #[test]
    fn rejects_unterminated_string() {
        let input = "ISO-10303-21;\nHEADER;\nENDSEC;\nDATA;\n#1=A('oups);\nENDSEC;\nEND-ISO-10303-21;\n";
        assert!(matches!(parse_step(input), Err(ParseError::Syntax { line: 5, .. })));
    }
}