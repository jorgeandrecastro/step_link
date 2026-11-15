//! [`AssetLoader`] Bevy pour les fichiers STEP.
//!
//! Le maillage est converti en mètres et en axes Bevy (Y vers le haut).
//! Les couleurs du fichier deviennent des couleurs de sommets.

use bevy::asset::{AssetLoader, LoadContext, RenderAssetUsages, io::Reader};
use bevy::reflect::TypePath;
use bevy::render::mesh::{Indices, Mesh, PrimitiveTopology};
use std::fmt;
use step_link::mesh::{DEFAULT_COLOR, MeshData, tessellate};
use step_link::parser::{ParseError, parse_step};

/// Erreurs de chargement d'un fichier STEP.
#[derive(Debug)]
pub enum StepLoaderError {
    /// Erreur d'entrée/sortie lors de la lecture du fichier.
    Io(std::io::Error),
    /// Le fichier n'est pas de l'UTF-8 valide.
    Utf8(std::string::FromUtf8Error),
    /// Le contenu STEP est invalide.
    Parse(ParseError),
    /// Le fichier ne contient aucun solide.
    NoSolid,
}

impl fmt::Display for StepLoaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "erreur d'E/S : {e}"),
            Self::Utf8(e) => write!(f, "fichier STEP non UTF-8 : {e}"),
            Self::Parse(e) => write!(f, "fichier STEP invalide : {e}"),
            Self::NoSolid => write!(f, "aucun solide dans le fichier STEP"),
        }
    }
}

impl std::error::Error for StepLoaderError {}

impl From<std::io::Error> for StepLoaderError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<std::string::FromUtf8Error> for StepLoaderError {
    fn from(e: std::string::FromUtf8Error) -> Self {
        Self::Utf8(e)
    }
}

impl From<ParseError> for StepLoaderError {
    fn from(e: ParseError) -> Self {
        Self::Parse(e)
    }
}

/// Chargeur d'assets transformant un fichier `.step` / `.stp` en [`Mesh`].
#[derive(Debug, Default, Clone, Copy, TypePath)]
pub struct StepLoader;

impl AssetLoader for StepLoader {
    type Asset = Mesh;
    type Settings = ();
    type Error = StepLoaderError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &Self::Settings,
        _load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let text = String::from_utf8(bytes)?;

        let model = parse_step(&text)?;
        if model.skipped_faces > 0 {
            bevy::log::warn!(
                "{} face(s) ignorée(s) : surface non prise en charge",
                model.skipped_faces
            );
        }
        if model.solids.is_empty() {
            return Err(StepLoaderError::NoSolid);
        }

        // Unités du fichier vers mètres, puis Z vers le haut (STEP) vers Y vers le haut (Bevy).
        let scale = model.unit_scale as f32;
        let parts: Vec<MeshData> = model.solids.iter().map(tessellate).collect();
        let colored = parts.iter().any(|p| !p.colors.is_empty());
        let mut data = MeshData::default();
        for part in &parts {
            let offset = data.positions.len() as u32;
            data.positions.extend(
                part.positions
                    .iter()
                    .map(|&[x, y, z]| [x * scale, z * scale, -y * scale]),
            );
            data.normals
                .extend(part.normals.iter().map(|&[x, y, z]| [x, z, -y]));
            data.indices.extend(part.indices.iter().map(|i| i + offset));
            if colored {
                if part.colors.is_empty() {
                    data.colors
                        .extend(std::iter::repeat_n(DEFAULT_COLOR, part.positions.len()));
                } else {
                    data.colors.extend_from_slice(&part.colors);
                }
            }
        }
        Ok(to_bevy_mesh(data))
    }

    fn extensions(&self) -> &[&str] {
        &["step", "stp"]
    }
}

/// Convertit un [`MeshData`] en [`Mesh`] Bevy ; les couleurs sRGB deviennent des couleurs de sommets linéaires.
fn to_bevy_mesh(data: MeshData) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, data.positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, data.normals)
    .with_inserted_indices(Indices::U32(data.indices));

    if !data.colors.is_empty() {
        let colors: Vec<[f32; 4]> = data
            .colors
            .iter()
            .map(|&[r, g, b]| [srgb_to_linear(r), srgb_to_linear(g), srgb_to_linear(b), 1.0])
            .collect();
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    }
    mesh
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}