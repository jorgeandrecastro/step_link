use bevy::input::mouse::{
    AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit,
};
use bevy::prelude::*;

use bevy_step_link::StepLinkPlugin;

// ============================================================
// CONFIGURATION
// ============================================================

const MODEL_PATH: &str = "models/UMI-HH-01_Handle.step";

// Taille cible de la plus grande dimension de l'objet (unités Bevy).
const TARGET_SIZE: f32 = 2.0;

// Correction d'orientation. STEP est souvent Z-up.
// Essaie: Quat::IDENTITY, ou from_rotation_x(-FRAC_PI_2), ou from_rotation_x(FRAC_PI_2)
fn model_rotation() -> Quat {
    Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)
}

const HOME_RADIUS: f32 = TARGET_SIZE * 2.5;
const MIN_RADIUS: f32 = 0.3;
const MAX_RADIUS: f32 = 40.0;

const ORBIT_SENSITIVITY: f32 = 0.006;
const ZOOM_SENSITIVITY: f32 = 0.1;

// ============================================================
// MAIN
// ============================================================

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, StepLinkPlugin))
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (fit_model, orbit_camera, update_camera_transform).chain(),
        )
        .run();
}

// ============================================================
// COMPONENTS
// ============================================================

#[derive(Component)]
struct OrbitCamera {
    focus: Vec3,
    radius: f32,
    yaw: f32,
    pitch: f32,
    home_radius: f32,
}

/// Modèle à centrer/mettre à l'échelle dès que le mesh est chargé.
#[derive(Component)]
struct StepModel;

/// Marqueur: le modèle a déjà été ajusté.
#[derive(Component)]
struct Fitted;

// ============================================================
// SCENE SETUP
// ============================================================

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mesh: Handle<Mesh> = asset_server.load(MODEL_PATH);

    let material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.78, 0.82, 0.88),
        metallic: 0.25,
        perceptual_roughness: 0.45,
        double_sided: true, // winding parfois incohérent sur les STEP
        cull_mode: None,
        ..default()
    });

    // Transform définitif calculé dans fit_model() une fois le mesh chargé.
    commands.spawn((
        Mesh3d(mesh),
        MeshMaterial3d(material),
        Transform::default(),
        StepModel,
    ));

    commands.spawn((
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            near: 0.01,
            far: 500.0,
            ..default()
        }),
        Transform::default(),
        OrbitCamera {
            focus: Vec3::ZERO,
            radius: HOME_RADIUS,
            yaw: 0.6,
            pitch: 0.35,
            home_radius: HOME_RADIUS,
        },
    ));

    commands.spawn((
        DirectionalLight {
            illuminance: 12_000.0,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.8, -0.5, 0.0)),
    ));

    commands.insert_resource(GlobalAmbientLight {
        brightness: 250.0,
        ..default()
    });
}

// ============================================================
// AUTO CENTER + AUTO SCALE
// ============================================================

fn fit_model(
    mut commands: Commands,
    meshes: Res<Assets<Mesh>>,
    mut models: Query<(Entity, &Mesh3d, &mut Transform), (With<StepModel>, Without<Fitted>)>,
) {
    for (entity, mesh3d, mut transform) in &mut models {
        let Some(mesh) = meshes.get(&mesh3d.0) else {
            continue; // pas encore chargé
        };
        let Some(positions) = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|a| a.as_float3())
        else {
            continue;
        };
        if positions.is_empty() {
            continue;
        }

        let rotation = model_rotation();

        // Bounding box APRÈS rotation de correction.
        let mut min = Vec3::splat(f32::MAX);
        let mut max = Vec3::splat(f32::MIN);
        for p in positions {
            let v = rotation * Vec3::from(*p);
            min = min.min(v);
            max = max.max(v);
        }

        let center = (min + max) * 0.5;
        let max_dim = (max - min).max_element().max(f32::EPSILON);
        let scale = TARGET_SIZE / max_dim;

        // world = scale * (rotation * v) + translation
        // On veut que le centre de la bbox soit à l'origine.
        transform.rotation = rotation;
        transform.scale = Vec3::splat(scale);
        transform.translation = -center * scale;

        info!(
            "Modèle ajusté: taille={:?}, centre={:?}, scale={}",
            max - min,
            center,
            scale
        );

        commands.entity(entity).insert(Fitted);
    }
}

// ============================================================
// CAMERA INPUT
// ============================================================

fn orbit_camera(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    mut query: Query<&mut OrbitCamera>,
) {
    for mut orbit in &mut query {
        if keys.just_pressed(KeyCode::KeyR) {
            orbit.radius = orbit.home_radius;
            orbit.yaw = 0.6;
            orbit.pitch = 0.35;
            orbit.focus = Vec3::ZERO;
        }

        // Rotation: glisser vers la droite fait tourner l'objet vers la droite.
        if buttons.pressed(MouseButton::Left) {
            orbit.yaw -= motion.delta.x * ORBIT_SENSITIVITY;
            orbit.pitch += motion.delta.y * ORBIT_SENSITIVITY;

            let limit = std::f32::consts::FRAC_PI_2 - 0.02;
            orbit.pitch = orbit.pitch.clamp(-limit, limit);
        }

        // Zoom: normalisé selon molette (Line) ou trackpad (Pixel).
        let raw = scroll.delta.y;
        if raw.abs() > f32::EPSILON {
            let delta = match scroll.unit {
                MouseScrollUnit::Line => raw,
                MouseScrollUnit::Pixel => raw / 40.0,
            }
            .clamp(-3.0, 3.0);

            orbit.radius *= (-delta * ZOOM_SENSITIVITY).exp();
            orbit.radius = orbit.radius.clamp(MIN_RADIUS, MAX_RADIUS);
        }
    }
}

// ============================================================
// CAMERA TRANSFORM
// ============================================================

fn update_camera_transform(mut query: Query<(&OrbitCamera, &mut Transform)>) {
    for (orbit, mut transform) in &mut query {
        let rotation = Quat::from_euler(EulerRot::YXZ, orbit.yaw, -orbit.pitch, 0.0);
        transform.rotation = rotation;
        transform.translation = orbit.focus + rotation * Vec3::Z * orbit.radius;
    }
}