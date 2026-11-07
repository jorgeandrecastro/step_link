//! [`AssetLoader`] Bevy pour les fichiers STEP.
//!
//! Le maillage est converti en mètres et en axes Bevy (Y vers le haut).

use bevy::asset::{AssetLoader, LoadContext, RenderAssetUsages, io::Reader};
use bevy::reflect::TypePath;
use bevy::render::mesh::{Indices, Mesh, PrimitiveTopology};
use std::fmt;
use step_link::mesh::{MeshData, tessellate};
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
        if model.solids.is_empty() {
            return Err(StepLoaderError::NoSolid);
        }

        // Unités du fichier vers mètres, puis Z vers le haut (STEP) vers Y vers le haut (Bevy).
        let scale = model.unit_scale as f32;
        let mut data = MeshData::default();
        for solid in &model.solids {
            let part = tessellate(solid);
            let offset = data.positions.len() as u32;
            data.positions.extend(
                part.positions
                    .iter()
                    .map(|&[x, y, z]| [x * scale, z * scale, -y * scale]),
            );
            data.normals
                .extend(part.normals.iter().map(|&[x, y, z]| [x, z, -y]));
            data.indices.extend(part.indices.iter().map(|i| i + offset));
        }
        Ok(to_bevy_mesh(data))
    }

    fn extensions(&self) -> &[&str] {
        &["step", "stp"]
    }
}

/// Convertit un [`MeshData`] en [`Mesh`] Bevy.
fn to_bevy_mesh(data: MeshData) -> Mesh {
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, data.positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, data.normals)
    .with_inserted_indices(Indices::U32(data.indices))
}