use super::{
    interpolation::{InterpolationSample, sample_at_tick},
    state::{ClientState, Owned, RemoteWorld},
};
use bevy::prelude::*;
use shared::{
    components::Position,
    snapshots::NetworkIdentity,
    replication::{ReplicatedComponent, ReplicationRegistry},
};

#[derive(Component, Default)]
pub struct InterpolationDebug {
    pub display_tick: f64,
    pub sample: Option<InterpolationSample>,
}

#[derive(Resource)]
pub struct CircleAssets {
    mesh: Handle<Mesh>,
    blue: Handle<ColorMaterial>,
    orange: Handle<ColorMaterial>,
}

pub fn setup_circle_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    commands.insert_resource(CircleAssets {
        mesh: meshes.add(Circle::new(16.0)),
        blue: materials.add(ColorMaterial::from_color(Color::srgb(0.2, 0.5, 1.0))),
        orange: materials.add(ColorMaterial::from_color(Color::srgb(1.0, 0.5, 0.2))),
    });
}

pub fn add_circle_visuals(
    mut commands: Commands,
    new_entities: Query<(Entity, &NetworkIdentity), Added<NetworkIdentity>>,
    assets: Res<CircleAssets>,
) {
    for (entity, identity) in &new_entities {
        let material = if identity.id == 1 {
            assets.blue.clone()
        } else {
            assets.orange.clone()
        };

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

    for (identity, authoritative, owned, mut transform, mut debug) in &mut circles {
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
        transform.translation.x = position.x;
        transform.translation.y = position.y;
    }
}
