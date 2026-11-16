# Changelog

## 0.5.1

### Changed
- Translated project documentation (`README.md` and `CHANGELOG.md`) into English.

## 0.5.0

### Added
- Parsing of STEP colors declared via `STYLED_ITEM`, at both solid and face levels.
- Preservation of RGB colors in `MeshData` and rendering as vertex colors in Bevy.
- Face color priority over solid color; neutral tint for uncolored faces within a colored mesh.

## 0.4.1

### Added
- Constrained Delaunay triangulation via `spade`: contours act as constrained edges, and holes are excluded.
- Interior points (honeycomb grid in UV space) on spheres, tori, and B-spline/NURBS surfaces, with step size derived from tolerance.
- Analytical normals at interior points (exact formulas for spheres and tori, partial derivatives for B-splines).
- Tests: Delaunay with and without interior points, holes, fallback on self-intersecting contours, and round-trip `(u, v)` ↔ point evaluation for spheres and tori.

### Changed
- *Ear clipping* is now used solely as a fallback when `spade` rejects the contours.
- `SplineChart::eval` clamps `(u, v)` to the surface domain.

### Dependencies
- `spade 2`.