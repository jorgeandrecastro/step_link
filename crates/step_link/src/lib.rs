//! # step_link
//!
//! Moteur géométrique B-Rep, parser STEP (ISO 10303-21) et tessellation en pur Rust.
//!
//! Cette crate n'a **aucune dépendance** vers Bevy ni vers un moteur de rendu :
//! elle peut être utilisée dans n'importe quel contexte (outils CLI, viewers, moteurs de jeu).
//!
//! Pipeline typique :
//!
//! 1. [`parser::parse_step`] lit un fichier STEP et produit un [`brep::Solid`] ;
//! 2. [`mesh::tessellate`] convertit le solide en [`mesh::MeshData`] (triangles) ;
//! 3. le résultat est consommé par l'application hôte (ex. `bevy_step_link`).
//!
//! ```
//! use step_link::{brep::Solid, mesh::tessellate};
//!
//! let cube = Solid::cuboid(1.0, 1.0, 1.0);
//! let mesh = tessellate(&cube);
//! assert_eq!(mesh.indices.len(), 36); // 6 faces × 2 triangles × 3 indices
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod brep;
pub mod mesh;
pub mod parser;
