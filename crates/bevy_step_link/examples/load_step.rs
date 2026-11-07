//! Exemple minimal : charge une pièce STEP et l'affiche avec Bevy.
//!
//! Lancer avec : `cargo run -p bevy_step_link --example load_step`
//! (le fichier est cherché dans `crates/bevy_step_link/assets/models/part.step`).
//!
//! Contrôles : clic gauche + glisser pour tourner, molette pour zoomer.

use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;
use bevy_step_link::StepLinkPlugin;

/// Agrandissement d'affichage : le loader livre des mètres (pièce d'environ 4 cm).
const SCALE: f32 = 50.0;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, StepLinkPlugin))
        .add_systems(Startup, setup)
        .add_systems(Update, orbit_camera)
        .run();
}

/// État de la caméra orbitale autour d'un point.
#[derive(Component)]
struct OrbitCamera {
    focus: Vec3,
    radius: f32,
    yaw: f32,
    pitch: f32,
}

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Le .step est chargé directement comme un Mesh, déjà en mètres et en Y-up.
    let mesh: Handle<Mesh> = asset_server.load("models/part.step");

    commands.spawn((
        Mesh3d(mesh),
        MeshMaterial3d(materials.add(Color::srgb(0.8, 0.7, 0.6))),
        Transform::from_scale(Vec3::splat(SCALE)),
    ));

    // Centre de la pièce : boîte englobante STEP (-20,-20,0)..(20,30,35) mm,
    // soit (0, 5, 17.5) mm ; après le loader (mètres, Y-up) : (0, 17.5, -5) mm.
    let focus = Vec3::new(0.0, 0.0175, -0.005) * SCALE;

    // Caméra orbitale.
    commands.spawn((
        Camera3d::default(),
        Transform::default(), // recalculé par `orbit_camera`
        OrbitCamera {
            focus,
            radius: 6.0,
            yaw: 0.5,
            pitch: 0.4,
        },
    ));

    // Lumière ponctuelle.
    commands.spawn((
        PointLight {
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0),
    ));

    // Lumière ambiante globale pour que les faces à l'ombre ne soient pas noires.
    commands.insert_resource(GlobalAmbientLight {
        brightness: 300.0,
        ..default()
    });
}

/// Clic gauche + glisser : tourne autour de la pièce. Molette : zoom.
fn orbit_camera(
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    mut query: Query<(&mut OrbitCamera, &mut Transform)>,
) {
    for (mut orbit, mut transform) in &mut query {
        if buttons.pressed(MouseButton::Left) {
            orbit.yaw -= motion.delta.x * 0.005;
            orbit.pitch += motion.delta.y * 0.005;
            // Évite de passer par-dessus les pôles.
            let limit = std::f32::consts::FRAC_PI_2 - 0.01;
            orbit.pitch = orbit.pitch.clamp(-limit, limit);
        }

        if scroll.delta.y != 0.0 {
            orbit.radius = (orbit.radius * (1.0 - scroll.delta.y * 0.1)).clamp(0.5, 50.0);
        }

        let rotation = Quat::from_euler(EulerRot::YXZ, orbit.yaw, -orbit.pitch, 0.0);
        transform.rotation = rotation;
        transform.translation = orbit.focus + rotation * Vec3::Z * orbit.radius;
    }
}