use super::{
    interpolation::{InterpolationSample, sample_at_tick},
    state::{ClientState, Owned, RemoteWorld},
};
use crate::prediction::{Prediction, PredictionSettings};
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
    for (entity, identity) in &new_entities {
        let hue = ((identity.id % 360) * 137 % 360) as f32;
        let material = materials.add(ColorMaterial::from_color(Color::hsl(hue, 0.8, 0.6)));

        commands.entity(entity).insert((
            Mesh2d(assets.mesh.clone()),
            MeshMaterial2d(material),
            InterpolationDebug::default(),
        ));
    }
}

pub fn render_remote_circles(
    remote_world: Res<RemoteWorld>,
    mut client_state: ResMut<ClientState>,
    registry: Res<ReplicationRegistry>,
    prediction_settings: Res<PredictionSettings>,
    time: Res<Time<Real>>,
    mut circles: Query<(
        &NetworkIdentity,
        &NetworkPhysicsState,
        Option<&Owned>,
        Option<&Prediction>,
        &mut Transform,
        &mut InterpolationDebug,
    )>,
) {
    if !client_state.initiated {
        return;
    }
    let display_tick = client_state.clock.advance(time.elapsed_secs_f64());
    let interpolate = registry
        .components
        .get(&NetworkPhysicsState::NETWORK_ID)
        .is_some_and(|component| component.interpolated);

    for (identity, authoritative, owned, prediction, mut transform, mut debug) in &mut circles {
        debug.display_tick = display_tick;
        debug.sample = None;
        let mut position = authoritative.position;
        if owned.is_some() && prediction_settings.enabled {
            if let Some(prediction) = prediction {
                position = prediction.state.position;
            }
        } else if owned.is_none() && interpolate {
            if let Some(sampled) = remote_world
                .positions
                .get(&identity.id)
                .and_then(|samples| sample_at_tick(samples, display_tick))
            {
                position = sampled.position;
                debug.sample = Some(sampled);
            }
        }
        transform.translation.x = position.x;
        transform.translation.y = position.y;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owned_player_uses_server_position_when_prediction_is_disabled() {
        let mut app = App::new();
        app.init_resource::<RemoteWorld>();
        app.init_resource::<ReplicationRegistry>();
        app.insert_resource(ClientState {
            initiated: true,
            ..Default::default()
        });
        app.insert_resource(PredictionSettings { enabled: false });
        app.insert_resource(Time::<Real>::default());
        app.add_systems(Update, render_remote_circles);

        let entity = app
            .world_mut()
            .spawn((
                NetworkIdentity { id: 1 },
                NetworkPhysicsState {
                    position: Vec2::new(50.0, 0.0),
                    velocity: Vec2::ZERO,
                },
                Owned,
                Prediction::new(NetworkPhysicsState {
                    position: Vec2::new(100.0, 0.0),
                    velocity: Vec2::ZERO,
                }),
                Transform::default(),
                InterpolationDebug::default(),
            ))
            .id();

        app.update();
        assert_eq!(
            app.world().get::<Transform>(entity).unwrap().translation.x,
            50.0
        );

        app.world_mut().resource_mut::<PredictionSettings>().enabled = true;
        app.update();
        assert_eq!(
            app.world().get::<Transform>(entity).unwrap().translation.x,
            100.0
        );
    }
}
