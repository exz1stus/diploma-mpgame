use avian2d::{
    physics_transform::Position,
    spatial_query::{SpatialQuery, SpatialQueryFilter, SpatialQuerySystems},
};
use bevy::{
    app::{App, FixedPostUpdate},
    math::Dir2,
    prelude::IntoScheduleConfigs,
};
use bevy_ecs::{
    entity::Entity,
    message::MessageReader,
    query::With,
    system::{Commands, Query, Res},
};
use shared::{
    commands::{CommandRequest, NetworkCommandAppExt, Shoot},
    components::{Ownership, Player},
    replication::ReplicationSet,
};

use crate::networking::client::ReplicationClients;

pub fn configure(app: &mut App) {
    app.add_command::<Shoot>();
    app.add_systems(
        FixedPostUpdate,
        handle_shoot
            .after(SpatialQuerySystems)
            .before(ReplicationSet::Collect),
    );
}

fn handle_shoot(
    mut requests: MessageReader<CommandRequest<Shoot>>,
    clients: Res<ReplicationClients>,
    players: Query<(Entity, &Ownership), With<Player>>,
    spatial_query: SpatialQuery,
    positions: Query<&Position>,
    mut commands: Commands,
) {
    const SHOOTING_RANGE: f32 = 500.0;
    for request in requests.read() {
        let Some(client_state) = clients.clients.get(&request.context.client_id) else {
            continue;
        };

        let Some((player, _)) = client_state.owned_entities.iter().find_map(|&entity| {
            players
                .get(entity)
                .ok()
                .filter(|(_, ownership)| ownership.client_id == request.context.client_id)
        }) else {
            continue;
        };

        let Ok(direction) = Dir2::new(request.command.direction) else {
            continue;
        };
        let Ok(origin) = positions.get(player) else {
            continue;
        };

        let filter = SpatialQueryFilter::from_excluded_entities([player]);
        let Some(hit) = spatial_query.cast_ray(origin.0, direction, SHOOTING_RANGE, true, &filter)
        else {
            continue;
        };

        if players.get(hit.entity).is_ok() {
            commands.entity(hit.entity).try_despawn();
        }
    }
}
