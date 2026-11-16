//! # step_link
//!
//! Pure Rust B-Rep geometry engine, STEP (ISO 10303-21) parser, and tessellator.
//!
//! This crate has **no dependencies** on Bevy or any rendering engine:
//! it can be used in any context (CLI tools, viewers, game engines).
//!
//! Typical pipeline:
//!
//! 1. [`parser::parse_step`] reads a STEP file and produces a [`brep::Solid`].
//! 2. [`mesh::tessellate`] converts the solid into [`mesh::MeshData`] (triangles).
//! 3. The host application consumes the result (e.g. `bevy_step_link`).
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
mod builder;
