//! Parser STEP (ISO 10303-21, « Part 21 »).
//!
//! Lexer et graphe d'entités : le fichier est lu en un [`StepFile`] (en-tête +
//! entités indexées par `#id`) avec des paramètres typés ([`Param`]).
//! [`parse_step`] reconstruit ensuite les solides B-Rep à partir de ce graphe.
//!
//! Limites connues : les séquences d'échappement des chaînes (`\X\`, `\X2\`, …)
//! ne sont pas décodées, et seules les sections `HEADER` et `DATA` sont lues.

use crate::brep::Solid;
use crate::builder::build_solids;
use std::collections::BTreeMap;
use std::fmt;

/// Identifiant d'entité STEP (le `42` de `#42`).
pub type EntityId = u64;

/// Erreurs possibles lors de la lecture d'un fichier STEP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// Le fichier ne commence pas par `ISO-10303-21;`.
    MissingHeader,
    /// Aucune section `DATA;` n'a été trouvée.
    MissingDataSection,
    /// Le fichier se termine sans `END-ISO-10303-21;`.
    MissingFooter,
    /// Erreur de syntaxe à une ligne donnée.
    Syntax {
        /// Numéro de ligne (à partir de 1).
        line: usize,
        /// Description de l'erreur.
        message: String,
    },
    /// Entité STEP invalide ou non prise en charge.
    Invalid {
        /// Identifiant de l'entité en cause.
        entity: EntityId,
        /// Description du problème.
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

/// Paramètre d'une entité STEP.
#[derive(Debug, Clone, PartialEq)]
pub enum Param {
    /// Référence vers une autre entité (`#12`).
    Ref(EntityId),
    /// Entier (`3`).
    Int(i64),
    /// Réel (`1.5`, `1.E-3`).
    Real(f64),
    /// Chaîne (`'texte'`), apostrophes doublées déjà résolues.
    Str(String),
    /// Énumération ou booléen, sans les points, en majuscules (`.T.` → `T`).
    Enum(String),
    /// Donnée binaire (`"0A1F"`), contenu hexadécimal brut.
    Binary(String),
    /// Liste de paramètres (`(1.,2.,3.)`).
    List(Vec<Param>),
    /// Valeur typée (`LENGTH_MEASURE(10.)`).
    Typed(String, Vec<Param>),
    /// Valeur non renseignée (`$`).
    Omitted,
    /// Valeur dérivée (`*`).
    Derived,
}

impl Param {
    /// Retourne l'identifiant référencé s'il s'agit d'un [`Param::Ref`].
    pub fn as_id(&self) -> Option<EntityId> {
        match self {
            Self::Ref(id) => Some(*id),
            _ => None,
        }
    }

    /// Retourne la valeur numérique d'un [`Param::Real`] ou [`Param::Int`].
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Real(v) => Some(*v),
            Self::Int(v) => Some(*v as f64),
            _ => None,
        }
    }

    /// Retourne la chaîne d'un [`Param::Str`].
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s),
            _ => None,
        }
    }

    /// Retourne les éléments d'un [`Param::List`].
    pub fn as_list(&self) -> Option<&[Param]> {
        match self {
            Self::List(items) => Some(items),
            _ => None,
        }
    }
}

/// Entité simple : un nom de type et ses paramètres (`CARTESIAN_POINT('',(0.,0.,0.))`).
#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    /// Nom du type, en majuscules.
    pub name: String,
    /// Paramètres, dans l'ordre.
    pub params: Vec<Param>,
}

/// Entité du fichier : un enregistrement simple, ou plusieurs pour une entité complexe.
#[derive(Debug, Clone, PartialEq)]
pub struct Entity {
    /// Enregistrements composant l'entité (jamais vide).
    pub records: Vec<Record>,
}

impl Entity {
    /// Nom du premier enregistrement.
    pub fn name(&self) -> &str {
        self.records.first().map_or("", |r| r.name.as_str())
    }

    /// Vrai si l'entité est complexe (`#5 = (A() B())`).
    pub fn is_complex(&self) -> bool {
        self.records.len() > 1
    }

    /// Cherche l'enregistrement portant ce nom (en majuscules).
    pub fn record(&self, name: &str) -> Option<&Record> {
        self.records.iter().find(|r| r.name == name)
    }
}

/// Contenu d'un fichier STEP après lecture.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StepFile {
    /// Enregistrements de la section `HEADER`.
    pub header: Vec<Record>,
    /// Entités de la section `DATA`, indexées par identifiant.
    pub entities: BTreeMap<EntityId, Entity>,
}

impl StepFile {
    /// Retourne l'entité portant cet identifiant.
    pub fn get(&self, id: EntityId) -> Option<&Entity> {
        self.entities.get(&id)
    }
}

/// Résultat de l'analyse d'un fichier STEP.
#[derive(Debug, Clone, PartialEq)]
pub struct StepModel {
    /// Solides reconstruits, un par `MANIFOLD_SOLID_BREP`.
    pub solids: Vec<Solid>,
    /// Nombre d'entités de la section `DATA`.
    pub entity_count: usize,
    /// Mètres par unité de longueur du fichier (`0.001` pour un fichier en millimètres, `1.0` si inconnu).
    pub unit_scale: f64,
}

/// Analyse un fichier STEP et reconstruit ses solides.
///
/// # Erreurs
///
/// Retourne une [`ParseError`] si le fichier est invalide.
pub fn parse_step(input: &str) -> Result<StepModel, ParseError> {
    let file = parse_part21(input)?;
    let solids = build_solids(&file)?;
    Ok(StepModel {
        solids,
        entity_count: file.entities.len(),
        unit_scale: length_unit_scale(&file),
    })
}

/// Échelle de l'unité de longueur SI déclarée dans le fichier ; `1.0` sinon (unités non SI non gérées).
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

/// Lit la structure Part 21 : en-tête et graphe d'entités, sans interpréter la géométrie.
///
/// # Erreurs
///
/// Retourne une [`ParseError`] si la syntaxe est invalide.
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

    /// Saute les espaces et les commentaires `/* ... */`.
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

    /// Lit un mot-clé en majuscules (`CARTESIAN_POINT`, `END-ISO-10303-21`, …).
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

    /// Consomme le mot-clé attendu, sinon ne bouge pas.
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