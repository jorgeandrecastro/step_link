# step_link

[![Crates.io](https://img.shields.io/crates/v/step_link.svg)](https://crates.io/crates/step_link)
[![docs.rs](https://docs.rs/step_link/badge.svg)](https://docs.rs/step_link)
[![Downloads](https://img.shields.io/crates/d/step_link.svg)](https://crates.io/crates/step_link)
[![License](https://img.shields.io/crates/l/step_link.svg)](#licence)
[![CI](https://github.com/jorgeandrecastro/step_link/actions/workflows/ci.yml/badge.svg)](https://github.com/jorgeandrecastro/step_link/actions)
[![Rust](https://img.shields.io/badge/rust-1.88%2B-orange.svg)](https://www.rust-lang.org)

Moteur géométrique **B-Rep** et parser **STEP (ISO 10303-21)** en pur Rust, pensé pour être utilisé notamment avec [Bevy](https://bevyengine.org).

![3DBenchy chargé depuis un fichier STEP et rendu dans Bevy](https://raw.githubusercontent.com/jorgeandrecastro/step_link/main/images/image2.png)

*Modèle `3DBenchy.step` rendu dans Bevy.*

## Vision

Importer des pièces CAO (`.step` / `.stp`) directement dans un moteur de jeu ou un viewer, sans passer par une conversion externe en STL/OBJ, et en conservant la topologie B-Rep (solides, coques, faces, arêtes, sommets) pour des usages futurs : mesure, sélection de faces, retessellation à la volée.

Le format natif de Fusion 360 (`.f3d`) n'est pas lisible : dans Fusion, exporter via *Fichier → Exporter → STEP*.

Le projet est découpé en deux crates :

| Crate | Rôle |
|---|---|
| [`step_link`](https://crates.io/crates/step_link) | Cœur pur Rust : topologie B-Rep, parser STEP, tessellation. **Aucune dépendance Bevy.** |
| [`bevy_step_link`](https://crates.io/crates/bevy_step_link) | `StepLinkPlugin` + `AssetLoader` Bevy : `.step` / `.stp` → `Mesh`. |

## Arborescence du projet

```text
step_link/
├── Cargo.toml
├── Cargo.lock
├── CHANGELOG.md
├── crates/
│   ├── step_link/                 # Cœur pur Rust
│   │   ├── examples/
│   │   │   └── inspect.rs
│   │   └── src/
│   │       ├── brep.rs            # Modèle topologique B-Rep
│   │       ├── builder.rs         # Reconstruction depuis les entités STEP
│   │       ├── lib.rs
│   │       ├── mesh/
│   │       │   ├── mod.rs         # Tessellation et type MeshData
│   │       │   ├── chart.rs       # Projection des surfaces dans l'espace UV
│   │       │   ├── edges.rs       # Discrétisation et cache des arêtes
│   │       │   ├── spline.rs      # Évaluation des courbes et surfaces B-spline
│   │       │   └── triangulate.rs # Delaunay contraint (spade) + ear clipping de repli
│   │       └── parser.rs          # Parser Part 21
│   └── bevy_step_link/            # Intégration Bevy
│       ├── assets/
│       │   └── models/
│       │       └── part.step
│       ├── examples/
│       │   └── load_step.rs
│       └── src/
│           ├── lib.rs
│           └── loader.rs
├── images/
│   └── image2.png
├── LICENSE-APACHE
└── LICENSE-MIT
```

## État d'avancement

- [x] Modèle B-Rep : `Solid` → `Shell` → `Face` (`Surface`, boucle externe et boucles internes) → `OrientedEdge` → `Edge` (`Curve`) → `Vertex`, arêtes partagées par index
- [x] Tessellation : plans, cylindres, cônes, sphères, tores, surfaces B-spline, contours courbes et trous (Delaunay contraint via [`spade`](https://crates.io/crates/spade), *ear clipping* en repli)
- [x] Points intérieurs et normales analytiques sur les sphères, tores et surfaces B-spline/NURBS (0.4)
- [x] `AssetLoader` Bevy + plugin
- [x] Parser Part 21 : en-tête, entités, paramètres typés, entités complexes, commentaires
- [x] Reconstruction de la topologie à partir des entités STEP (plans, cylindres, cônes, sphères, tores ; droites, cercles, ellipses, B-splines)
- [x] Validé sur un export réel (`WallHook_30mm.stp`) : affichage correct dans Bevy

> Depuis la 0.4, les faces courbes (sphères, tores, B-splines/NURBS) reçoivent des points intérieurs dont la densité suit la tolérance : elles ne sont plus facettées. Cylindres et cônes (surfaces réglées) restent triangulés par leurs contours.

## Roadmap

### 1. Parser Part 21 générique ✅
Lexer et graphe `#id → entité` (`parse_part21`). Paramètres : références, entiers, réels, chaînes, énumérations, listes, valeurs typées, `$` et `*`. Entités complexes `#5 = (A() B())`.

- [ ] Décoder les séquences d'échappement des chaînes (`\X\`, `\X2\`)
- [x] Test sur un vrai export (`WallHook_30mm.stp`)
- [ ] Tests sur d'autres exports (Fusion 360, FreeCAD, SolidWorks)
- [ ] Fuzzing du lexer (`cargo fuzz`), aucune panique sur fichier malformé

### 2. Modèle B-Rep complet ✅
`Face { surface, outer_loop, inner_loops, same_sense }`, `Surface` (plan, cylindre, cône, sphère, tore, B-spline), `Curve` (droite, cercle, ellipse, B-spline), arêtes partagées par index.

### 3. Entités STEP vers topologie ✅
Chaîne `MANIFOLD_SOLID_BREP → CLOSED_SHELL → ADVANCED_FACE → FACE_BOUND → EDGE_LOOP → ORIENTED_EDGE → EDGE_CURVE → VERTEX_POINT → CARTESIAN_POINT`, surfaces (plan, cylindre, cône, sphère, tore) et courbes (droite, cercle, ellipse, B-spline). Validé sur un export réel de 63 faces.

- [ ] `BREP_WITH_VOIDS`
- [x] Courbes B-splines rationnelles (entités complexes)
- [x] Surfaces B-spline et NURBS (lecture, rationnelles comprises)
- [ ] `SURFACE_CURVE`
- [x] Les faces à surface non prise en charge sont ignorées avec un avertissement, au lieu de faire échouer le fichier
- [ ] Contour externe : sans `FACE_OUTER_BOUND`, le premier contour est supposé externe
- [ ] Unités non SI (pouces, `CONVERSION_BASED_UNIT`)

### 4. Tessellation (en cours)
- [x] Plans : Delaunay contraint avec trous
- [x] Cylindres et cônes : triangulation dans l'espace paramétrique (angle, hauteur)
- [x] Sphères et tores
- [x] Discrétisation des arêtes partagée entre faces voisines
- [x] Tolérance (flèche maximale) configurable (`tessellate_with_tolerance`)
- [x] Surfaces B-spline/NURBS : projection des contours en (u, v)
- [x] Surfaces B-spline/NURBS, sphères et tores : points intérieurs
- [x] Subdivision intérieure des faces courbes (grille en nid d'abeille dans l'espace UV)
- [x] Repli sur l'*ear clipping* si les contours sont pathologiques
- [ ] Points intérieurs pour cylindres et cônes (rails angulaires) si des facettes restent visibles
- [ ] Soudure des sommets entre faces
- [ ] Vérifier le fond des trous coniques (artefact visible sur `WallHook_30mm`)

### 5. Intégration Bevy
- [x] Pavé de remplacement supprimé : un fichier sans solide est une erreur de chargement
- [x] Conversion d'unités et d'axes (mm / Z-up vers m / Y-up) faite par le loader
- [ ] Plusieurs solides et assemblages (`NEXT_ASSEMBLY_USAGE_OCCURRENCE`, transformations)
- [ ] Réglages du loader (tolérance, échelle) via `type Settings`
- [ ] Couleurs (`STYLED_ITEM`)

### 6. Robustesse et publication
- [ ] Corpus de pièces de complexité croissante : cube, cylindre percé, pièce avec congés
- [ ] Documentation et exemples complets
- [x] Publication sur crates.io

## Installation

```toml
[dependencies]
bevy = "0.20"
bevy_step_link = "0.4"
```

Pour utiliser uniquement le cœur, sans Bevy :

```toml
[dependencies]
step_link = "0.4"
```

## Exemple (Bevy)

Placez votre fichier dans `assets/models/part.step`.

```rust
use bevy::prelude::*;
use bevy_step_link::StepLinkPlugin;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, StepLinkPlugin))
        .add_systems(Startup, setup)
        .run();
}

fn setup(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Le .step est chargé directement comme un Mesh.
    let mesh: Handle<Mesh> = assets.load("models/part.step");

    // Le loader convertit en mètres et en Y-up : une pièce de 40 mm fait 0.04 unité.
    // On l'agrandit ici pour la voir avec la caméra ci-dessous.
    commands.spawn((
        Mesh3d(mesh),
        MeshMaterial3d(materials.add(Color::srgb(0.8, 0.7, 0.6))),
        Transform::from_scale(Vec3::splat(50.0)),
    ));

    // Sans caméra, Bevy n'affiche que le fond gris.
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(-3.0, 3.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    commands.spawn((PointLight::default(), Transform::from_xyz(4.0, 8.0, 4.0)));
}
```

Exemple complet avec caméra orbitale (clic gauche + glisser pour tourner, molette pour zoomer) :

```sh
cargo run -p bevy_step_link --example load_step
```

## Exemple (pur Rust)

```rust
use step_link::{brep::Solid, mesh::tessellate};

let mesh = tessellate(&Solid::cuboid(1.0, 1.0, 1.0));
println!("{} triangles", mesh.triangle_count());
```

Lecture d'un fichier STEP :

```rust
use step_link::parser::parse_part21;

let file = parse_part21(&std::fs::read_to_string("piece.step").unwrap()).unwrap();
println!("{} entités", file.entities.len());
```

Inspection d'un fichier (comptage des entités, statistiques du solide) :

```sh
cargo run -p step_link --example inspect -- piece.step
```

Cette commande accepte un chemin `.step` ou `.stp`. Elle affiche les entités STEP, les solides reconstruits, les faces ignorées et le nombre de triangles ; les erreurs de lecture, de parsing ou de reconstruction sont affichées dans le terminal.

Pour vérifier visuellement une nouvelle pièce avec Bevy, placez-la dans `crates/bevy_step_link/assets/models/part.step` (ou adaptez le chemin `models/part.step` dans `crates/bevy_step_link/examples/load_step.rs`), puis lancez :

```sh
cargo run -p bevy_step_link --example load_step
```

Le chargeur convertit les unités en mètres et les axes en Y-up. `cargo test --workspace` lance les tests automatisés du projet ; cette commande ne charge pas automatiquement un fichier STEP arbitraire.

## Développement

```sh
cargo check --workspace
cargo test --workspace
```

## Contribuer

Les issues et pull requests sont les bienvenues sur [GitHub](https://github.com/jorgeandrecastro/step_link/issues).
Si une pièce STEP ne se charge pas correctement, joignez le fichier (ou un extrait) à l'issue : les cas réels sont précieux pour faire avancer le parser et le tessellateur.

## Auteur

**Jorge Andre Castro** : [@jorgeandrecastro](https://github.com/jorgeandrecastro)

## Licence

Distribué sous double licence, à votre choix :

- [Apache License, Version 2.0](https://github.com/jorgeandrecastro/step_link/blob/main/LICENSE-APACHE)
- [MIT license](https://github.com/jorgeandrecastro/step_link/blob/main/LICENSE-MIT)