use bevy::app::App;
use bevy_ecs::{
    lifecycle::{Add, Remove},
    observer::On,
    system::{Commands, Query, ResMut},
};
use shared::components::{DisconnectPolicy, Ownership};

use crate::networking::client::{ClientLeft, ReplicationClients};

pub(crate) fn configure(app: &mut App) {
    app.add_observer(on_ownership_added);
    app.add_observer(on_ownership_removed);
    app.add_observer(on_client_left);
}

fn on_client_left(event: On<ClientLeft>, ownerships: Query<&Ownership>, mut commands: Commands) {
    for &entity in &event.state.owned_entities {
        let Ok(ownership) = ownerships.get(entity) else {
            continue; // Already despawned or disowned.
        };

        if ownership.client_id != event.client_id {
            continue;
        }

        match ownership.disconnect_policy {
            DisconnectPolicy::Despawn => {
                commands.entity(entity).try_despawn();
            }
            DisconnectPolicy::Disown => {
                commands.entity(entity).remove::<Ownership>();
            }
            DisconnectPolicy::Preserve => {
                //TODO: define preserve mechanism
            }
        }
    }
}

fn on_ownership_added(
    event: On<Add, Ownership>,
    query: Query<&Ownership>,
    mut clients: ResMut<ReplicationClients>,
) {
    let entity = event.entity;
    let Ok(ownership) = query.get(entity) else {
        return;
    };

    if let Some(client) = clients.clients.get_mut(&ownership.client_id) {
        client.owned_entities.insert(event.entity);
    }
}

fn on_ownership_removed(
    event: On<Remove, Ownership>,
    query: Query<&Ownership>,
    mut clients: ResMut<ReplicationClients>,
) {
    let entity = event.entity;
    let Ok(ownership) = query.get(entity) else {
        return;
    };

    if let Some(client) = clients.clients.get_mut(&ownership.client_id) {
        client.owned_entities.remove(&event.entity);
    }
}
