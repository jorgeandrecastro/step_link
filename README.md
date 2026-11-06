# step_link

Moteur géométrique **B-Rep** et parser **STEP (ISO 10303-21)** en pur Rust, pensé pour être utilisé notamment avec [Bevy](https://bevyengine.org).

## Vision

Importer des pièces CAO (`.step` / `.stp`) directement dans un moteur de jeu ou un viewer, sans passer par une conversion externe en STL/OBJ, et en conservant la topologie B-Rep (solides, coques, faces, arêtes, sommets) pour des usages futurs : mesure, sélection de faces, retessellation à la volée.

Le format natif de Fusion 360 (`.f3d`) n'est pas lisible : dans Fusion, exporter via *Fichier → Exporter → STEP*.

Le projet est découpé en deux crates :

| Crate | Rôle |
|---|---|
| [`step_link`](crates/step_link) | Cœur pur Rust : topologie B-Rep, parser STEP, tessellation. **Aucune dépendance Bevy.** |
| [`bevy_step_link`](crates/bevy_step_link) | `StepLinkPlugin` + `AssetLoader` Bevy : `.step` / `.stp` → `Mesh`. |

## État d'avancement

- [x] Structures B-Rep minimales (`Solid`, `Shell`, `Face`, `Edge`, `Vertex`)
- [x] Tessellation de faces planes convexes (éventail)
- [x] `AssetLoader` Bevy + plugin
- [x] Parser Part 21 : en-tête, entités, paramètres typés, entités complexes, commentaires
- [ ] Reconstruction de la topologie à partir des entités (pour l'instant aucun solide n'est produit)

> Tant que la reconstruction n'est pas faite, le loader affiche un pavé de remplacement quel que soit le fichier STEP.

## Roadmap

### 1. Parser Part 21 générique ✅
Lexer et graphe `#id → entité` (`parse_part21`). Paramètres : références, entiers, réels, chaînes, énumérations, listes, valeurs typées, `$` et `*`. Entités complexes `#5 = (A() B())`.

- [ ] Décoder les séquences d'échappement des chaînes (`\X\`, `\X2\`)
- [ ] Test sur de vrais exports (Fusion 360, FreeCAD, SolidWorks)
- [ ] Fuzzing du lexer (`cargo fuzz`), aucune panique sur fichier malformé

### 2. Modèle B-Rep complet
- [ ] `Face { surface, loops, same_sense }` avec boucle externe et boucles internes (trous)
- [ ] `Surface` : plan, cylindre, cône, sphère, tore, B-spline/NURBS
- [ ] Arêtes portant leur courbe (droite, cercle, B-spline) et partagées entre faces par index

### 3. Entités STEP vers topologie
- [ ] Chaîne `MANIFOLD_SOLID_BREP → CLOSED_SHELL → ADVANCED_FACE → FACE_BOUND → EDGE_LOOP → ORIENTED_EDGE → EDGE_CURVE → VERTEX_POINT → CARTESIAN_POINT`
- [ ] Surfaces et repères : `PLANE`, `CYLINDRICAL_SURFACE`, `AXIS2_PLACEMENT_3D`
- [ ] `BREP_WITH_VOIDS`
- [ ] Premier jalon : un cube exporté de Fusion 360 devient un `Solid` correct

### 4. Tessellation
- [ ] Plans : *ear clipping* avec trous (à la place de l'éventail)
- [ ] Cylindres, cônes, sphères, tores : grille paramétrique limitée par les boucles
- [ ] NURBS : échantillonnage de la surface
- [ ] Discrétisation des arêtes partagée entre faces voisines (pas de fissures)
- [ ] Tolérance (flèche maximale) configurable

### 5. Intégration Bevy
- [ ] Supprimer le pavé de remplacement du loader
- [ ] Conversion d'unités et d'axes (mm / Z-up vers m / Y-up)
- [ ] Plusieurs solides et assemblages (`NEXT_ASSEMBLY_USAGE_OCCURRENCE`, transformations)
- [ ] Réglages du loader (tolérance, échelle) via `type Settings`
- [ ] Couleurs (`STYLED_ITEM`)

### 6. Robustesse et publication
- [ ] Corpus de pièces de complexité croissante : cube, cylindre percé, pièce avec congés


## Installation

```toml
[dependencies]
bevy = "0.20"
bevy_step_link = "0.1"
```

Pour utiliser uniquement le cœur, sans Bevy :

```toml
[dependencies]
step_link = "0.1"
```

## Exemple (Bevy)

```rust
use bevy::prelude::*;
use bevy_step_link::StepLinkPlugin;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, StepLinkPlugin))
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands, assets: Res<AssetServer>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let mesh: Handle<Mesh> = assets.load("models/part.step");
    commands.spawn((Mesh3d(mesh), MeshMaterial3d(materials.add(Color::WHITE))));
    commands.spawn((Camera3d::default(), Transform::from_xyz(-3.0, 3.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y)));
}
```

Exemple complet : `cargo run -p bevy_step_link --example load_step`.

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

## Développement

```sh
cargo check --workspace
cargo test --workspace
```

## Licence

Distribué sous double licence, à votre choix :

- [Apache License, Version 2.0](LICENSE-APACHE)
- [MIT license](LICENSE-MIT)