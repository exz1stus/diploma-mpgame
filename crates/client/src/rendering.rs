use crate::prediction::Prediction;

use super::{
    interpolation::{InterpolationSample, sample_at_tick},
    state::{ClientState, Owned, RemoteWorld},
};
use bevy::prelude::*;
use postcard::fixint::le;
use shared::{
    components::Position,
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
    new_entities: Query<(Entity, &NetworkIdentity), Added<NetworkIdentity>>,
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
    time: Res<Time<Real>>,
    mut circles: Query<(
        &NetworkIdentity,
        &Position,
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
        .get(&Position::NETWORK_ID)
        .is_some_and(|component| component.interpolated);

    for (identity, authoritative, owned, prediction, mut transform, mut debug) in &mut circles {
        debug.display_tick = display_tick;
        debug.sample = None;
        let mut position = Vec2::new(authoritative.x, authoritative.y);
        if interpolate && owned.is_none() {
            if let Some(sampled) = remote_world
                .positions
                .get(&identity.id)
                .and_then(|samples| sample_at_tick(samples, display_tick))
            {
                position = sampled.position;
                debug.sample = Some(sampled);
            }
        }
        if owned.is_some() {
            if let Some(prediction) = prediction {
                position = Vec2::new(prediction.state.x, prediction.state.y)
            }
        }
        transform.translation.x = position.x;
        transform.translation.y = position.y;
    }
}
