//! # bevy_step_link
//!
//! Intégration [Bevy](https://bevyengine.org) de [`step_link`] : charge les fichiers
//! `.step` / `.stp` comme des [`Mesh`](bevy::render::mesh::Mesh) via l'`AssetServer`.
//!
//! ```no_run
//! use bevy::prelude::*;
//! use bevy_step_link::StepLinkPlugin;
//!
//! App::new()
//!     .add_plugins((DefaultPlugins, StepLinkPlugin))
//!     .run();
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod loader;

pub use loader::{StepLoader, StepLoaderError};

use bevy::app::{App, Plugin};
use bevy::asset::AssetApp;

/// Plugin Bevy enregistrant le [`StepLoader`] pour les extensions `step` et `stp`.
///
/// Après l'ajout du plugin, `asset_server.load::<Mesh>("piece.step")` fonctionne.
#[derive(Debug, Default, Clone, Copy)]
pub struct StepLinkPlugin;

impl Plugin for StepLinkPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset_loader::<StepLoader>();
    }
}
