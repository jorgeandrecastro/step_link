//! Minimal example: load a STEP part and display it with Bevy.
//!
//! Run with: `cargo run -p bevy_step_link --example load_step`
//! (the file is expected at `crates/bevy_step_link/assets/models/part.step`).
//!
//! Controls: left-click and drag to orbit; scroll to zoom.

use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;
use bevy_step_link::StepLinkPlugin;

/// Display scale: the loader outputs meters (the part is about 4 cm across).
const SCALE: f32 = 50.0;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, StepLinkPlugin))
        .add_systems(Startup, setup)
        .add_systems(Update, orbit_camera)
        .run();
}

/// State for the orbit camera around a point.
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
    // The .step file is loaded directly as a Mesh, already in meters and Y-up.
    // File colors are stored in the mesh, so the material remains white.
    let mesh: Handle<Mesh> = asset_server.load("models/part.step");

    commands.spawn((
        Mesh3d(mesh),
        MeshMaterial3d(materials.add(Color::WHITE)),
        Transform::from_scale(Vec3::splat(SCALE)),
    ));

    // Part center: STEP bounding box (-20,-20,0)..(20,30,35) mm,
    // i.e. (0, 5, 17.5) mm; after the loader (meters, Y-up): (0, 17.5, -5) mm.
    let focus = Vec3::new(0.0, 0.0175, -0.005) * SCALE;

    // Orbit camera.
    commands.spawn((
        Camera3d::default(),
        Transform::default(), // recalculated by `orbit_camera`
        OrbitCamera {
            focus,
            radius: 6.0,
            yaw: 0.5,
            pitch: 0.4,
        },
    ));

    // Point light.
    commands.spawn((
        PointLight {
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0),
    ));

    // Global ambient light keeps shadowed faces from appearing black.
    commands.insert_resource(GlobalAmbientLight {
        brightness: 300.0,
        ..default()
    });
}

/// Left-click and drag to orbit around the part. Scroll to zoom.
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
            // Prevent the camera from passing over the poles.
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