use crate::game::commands::PredictedVisuals;
use crate::networking::prediction::{Prediction, PredictionSettings};
use crate::networking::{
    interpolation::{InterpolationSample, sample_at_tick},
    state::{ClientState, Owned, RemoteWorld},
};
use avian2d::physics_transform::Position as PhysicsPosition;
use bevy::prelude::*;
use shared::{
    components::{NetworkPhysicsState, Player},
    replication::{ReplicatedComponent, ReplicationRegistry},
    snapshots::NetworkIdentity,
};

#[derive(Component, Default)]
pub struct InterpolationDebug {
    pub display_tick: f64,
    pub sample: Option<InterpolationSample>,
}

#[derive(Resource)]
pub struct CircleAssets {
    mesh: Handle<Mesh>,
}

#[derive(Component)]
pub struct PlayerVisual;

#[derive(Component, Default)]
pub struct LocalVisualPosition {
    pub position: Option<Vec2>,
}
pub fn setup_circle_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    commands.insert_resource(CircleAssets {
        mesh: meshes.add(Circle::new(16.0)),
    });
}

pub fn add_circle_visuals(
    mut commands: Commands,
    new_entities: Query<(Entity, &NetworkIdentity), Added<Player>>,
    assets: Res<CircleAssets>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    for (player, identity) in &new_entities {
        let hue = ((identity.id % 360) * 137 % 360) as f32;
        let material = materials.add(ColorMaterial::from_color(Color::hsl(hue, 0.8, 0.6)));

        commands.entity(player).insert((
            InterpolationDebug::default(),
            LocalVisualPosition::default(),
        ));

        commands.spawn((
            PlayerVisual,
            ChildOf(player),
            Mesh2d(assets.mesh.clone()),
            MeshMaterial2d(material),
            Transform::from_xyz(0.0, 0.0, 0.1),
        ));
    }
}

pub fn render_player_visuals(
    remote_world: Res<RemoteWorld>,
    mut client_state: ResMut<ClientState>,
    registry: Res<ReplicationRegistry>,
    prediction_settings: Res<PredictionSettings>,
    time: Res<Time<Real>>,
    collision_proxies: Res<
        crate::networking::physics::RemoteCollisionProxies,
    >,
    proxy_positions: Query<&PhysicsPosition>,
    mut players: Query<
        (
            &NetworkIdentity,
            &NetworkPhysicsState,
            Option<&Owned>,
            Option<&PhysicsPosition>,
            &Transform,
            &mut InterpolationDebug,
            &mut LocalVisualPosition,
        ),
        Without<PlayerVisual>,
    >,
    mut visuals: Query<
        (&ChildOf, &mut Transform),
        With<PlayerVisual>,
    >,

    predicted_visuals: Res<PredictedVisuals>,
) {
    if !client_state.initiated {
        return;
    }

    let display_tick =
        client_state.clock.advance(time.elapsed_secs_f64());

    let interpolate = registry
        .components
        .get(&NetworkPhysicsState::NETWORK_ID)
        .is_some_and(|component| component.interpolated);

    for (child_of, mut visual_transform) in &mut visuals {
        let Ok((
            identity,
            authoritative,
            owned,
            physics,
            root_transform,
            mut debug,
            mut local_visual,
        )) = players.get_mut(child_of.parent())
        else {
            continue;
        };

        debug.display_tick = display_tick;
        debug.sample = None;

        visual_transform.scale =
            if predicted_visuals.is_hidden(identity.id) {
                Vec3::ZERO
            } else {
                Vec3::ONE
            };

        let root_position =
            root_transform.translation.truncate();

        let proxy_position = collision_proxies
            .entities
            .get(&identity.id)
            .and_then(|entity| proxy_positions.get(*entity).ok())
            .map(|position| position.0);

        let visual_position = if owned.is_some() {
            let position = if prediction_settings.enabled {
                physics.map_or(
                    root_position,
                    |position| position.0,
                )
            } else {
                authoritative.position
            };

            local_visual.position = Some(position);
            position
        } else if let Some(position) = proxy_position {
            position
        } else if interpolate {
            match remote_world
                .positions
                .get(&identity.id)
                .and_then(|samples| {
                    sample_at_tick(samples, display_tick)
                })
            {
                Some(sample) => {
                    debug.sample = Some(sample);
                    sample.position
                }
                None => authoritative.position,
            }
        } else {
            authoritative.position
        };

        let local_offset = visual_position - root_position;

        visual_transform.translation.x = local_offset.x;
        visual_transform.translation.y = local_offset.y;
    }
}

pub fn current_visual_position(world: &World, player: Entity) -> Vec2 {
    let entity = world.entity(player);

    if let Some(position) = entity
        .get::<LocalVisualPosition>()
        .and_then(|visual| visual.position)
    {
        return position;
    }

    let physics_position = entity
        .get::<PhysicsPosition>()
        .expect("Predicted player requires position")
        .0;

    let visual_offset = entity
        .get::<Prediction>()
        .expect("Predicted player requires prediction")
        .visual_offset;

    physics_position + visual_offset
}
