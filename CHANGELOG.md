# Changelog

## 0.6.0

### Added
- `tessellate_with_unit_scale(solid, unit_scale)`: tessellation with a 0.01 mm tolerance regardless of the file's length unit (`DEFAULT_TOLERANCE_METERS`).
- Interior points ("angular rails") on cylinders and cones, with correct 3D evaluation of their positions.
- Diagnostics on standard error: ignored faces or contours, *ear clipping* fallbacks (including unbridged holes), and faces whose winding disagrees with their normals.
- `inspect` example prints `unit_scale`; `load_step` example enables a wireframe overlay (requires `POLYGON_MODE_LINE` and `IMMEDIATES` GPU support).
- Tests: cylinder and cone interior points, identical segment counts for the same cylinder in cm and mm, merged wound loops.

### Changed
- `tessellate(solid)` now uses a tolerance relative to the solid's bounding-box diagonal (`diagonal * 5e-4`) instead of a fixed value in model units; `DEFAULT_TOLERANCE` remains the fallback for degenerate bounding boxes.
- The Bevy loader and the `inspect` example use `tessellate_with_unit_scale`, fixing coarse (octagonal) holes in files not expressed in millimeters.
- Minimum tolerance clamp lowered to `1e-12`.

### Fixed
- Drilled cylinder, cone, and sphere zones bounded by two full circles without a seam edge are merged into a single band (artificial seam), with the second circle aligned to the first to avoid inverted triangles.
- A degenerate outer contour no longer lets an inner contour be mistaken for the outer one: the face is skipped and reported.

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