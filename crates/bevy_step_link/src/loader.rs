//! [`AssetLoader`] Bevy pour les fichiers STEP.

use bevy::asset::{AssetLoader, LoadContext, RenderAssetUsages, io::Reader};
use bevy::reflect::TypePath;
use bevy::render::mesh::{Indices, Mesh, PrimitiveTopology};
use std::fmt;
use step_link::brep::Solid;
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
}

impl fmt::Display for StepLoaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "erreur d'E/S : {e}"),
            Self::Utf8(e) => write!(f, "fichier STEP non UTF-8 : {e}"),
            Self::Parse(e) => write!(f, "fichier STEP invalide : {e}"),
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

        let mut model = parse_step(&text)?;
        // Repli temporaire tant que le parser ne construit pas de solides.
        if model.solids.is_empty() {
            model.solids.push(Solid::cuboid(2.0, 1.0, 1.0));
        }

        let mut data = MeshData::default();
        for solid in &model.solids {
            let part = tessellate(solid);
            let offset = data.positions.len() as u32;
            data.positions.extend(part.positions);
            data.normals.extend(part.normals);
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