# Changelog

## 0.4.0

### Ajouté
- Triangulation de Delaunay contrainte via `spade` : les contours sont des arêtes contraintes, les trous sont exclus.
- Points intérieurs (grille en nid d'abeille dans l'espace UV) sur les sphères, tores et surfaces B-spline/NURBS, avec un pas dérivé de la tolérance.
- Normales analytiques aux points intérieurs (formules exactes pour sphère et tore, dérivées partielles pour les B-splines).
- Tests : Delaunay avec et sans points intérieurs, trous, repli sur contour auto-intersecté, aller-retour `(u, v)` ↔ point pour sphère et tore.

### Modifié
- L'*ear clipping* n'est plus que le repli utilisé quand `spade` refuse les contours.
- `SplineChart::eval` borne `(u, v)` au domaine de la surface.

### Dépendances
- `spade 2`.

## 0.3.0
- Version initiale publiée sur crates.io.