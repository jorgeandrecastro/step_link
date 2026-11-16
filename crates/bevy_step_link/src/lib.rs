//! # bevy_step_link
//!
//! [Bevy](https://bevyengine.org) integration for [`step_link`]: load `.step` / `.stp`
//! files as [`Mesh`](bevy::render::mesh::Mesh) assets through the `AssetServer`.
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

/// Bevy plugin that registers [`StepLoader`] for the `step` and `stp` extensions.
///
/// After adding the plugin, `asset_server.load::<Mesh>("piece.step")` can be used.
#[derive(Debug, Default, Clone, Copy)]
pub struct StepLinkPlugin;

impl Plugin for StepLinkPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset_loader::<StepLoader>();
    }
}
