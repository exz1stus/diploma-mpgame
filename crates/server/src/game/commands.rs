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
    system::{Commands, Query, Res, ResMut},
};
use shared::{
    commands::{
        CommandError, CommandOutcome, CommandRequest, CommandResult, NetworkCommand,
        NetworkCommandAppExt, Shoot,
    },
    components::{Ownership, Player},
    replication::ReplicationSet,
    server_messages::ServerMessages,
    snapshots::{NetworkEntityId, NetworkIdentity},
};

use crate::networking::{client::ReplicationClients, messages::MessageBuffer, server::TickCounter};

pub fn plugin(app: &mut App) {
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
    identities: Query<&NetworkIdentity>,
    tick: Res<TickCounter>,
    mut outgoing: ResMut<MessageBuffer>,
    mut commands: Commands,
) {
    const SHOOTING_RANGE: f32 = 500.0;
    for request in requests.read() {
        let result = (|| -> Result<Option<NetworkEntityId>, CommandError> {
            let client_state = clients
                .clients
                .get(&request.context.client_id)
                .ok_or(CommandError::Rejected)?;

            let (player, _) = client_state
                .owned_entities
                .iter()
                .find_map(|&entity| {
                    players
                        .get(entity)
                        .ok()
                        .filter(|(_, ownership)| ownership.client_id == request.context.client_id)
                })
                .ok_or(CommandError::Rejected)?;

            let direction =
                Dir2::new(request.command.direction).map_err(|_| CommandError::Rejected)?;
            let origin = positions.get(player).map_err(|_| CommandError::Rejected)?;
            let filter = SpatialQueryFilter::from_excluded_entities([player]);

            let hit = spatial_query
                .cast_ray(origin.0, direction, SHOOTING_RANGE, true, &filter)
                .and_then(|hit| {
                    players.get(hit.entity).ok()?;
                    let identity = identities.get(hit.entity).ok()?;
                    Some((hit.entity, identity.id))
                });

            if let Some((entity, _)) = hit {
                commands.entity(entity).try_despawn();
            }

            Ok(hit.map(|(_, id)| id))
        })();

        let payload = match result {
            Ok(hit) => ServerMessages::NetworkCommandResult(CommandResult {
                id: Shoot::NETWORK_ID,
                outcome: CommandOutcome::Shoot { hit },
            }),
            Err(error) => ServerMessages::NetworkCommandError(error),
        };

        outgoing.reply_command(
            request.context.client_id,
            request.context.sequence,
            tick.0,
            payload,
        );
    }
}
