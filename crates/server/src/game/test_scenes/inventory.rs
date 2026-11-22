use avian2d::physics_transform::Position;
use bevy::prelude::*;
use shared::{
    components::NetworkPhysicsState,
    inventory::{Chest, GroundItem, ItemStack},
};

use crate::{game::inventory::InventoryService, net::entity::NetworkCommandsExt};

/// Present only in the inventory scene. Normal worlds keep full-health spawns.
#[derive(Resource, Default)]
pub(crate) struct InventoryDemo {
    spawned: bool,
}

pub(super) fn spawn(
    mut commands: Commands,
    service: Res<InventoryService>,
    mut demo: ResMut<InventoryDemo>,
) {
    if demo.spawned {
        return;
    }

    let scene = service
        .config
        .load_demo_scene()
        .expect("invalid inventory demo scene");
    let chest_count = scene.chests.len();
    let ground_count = scene.ground_items.len();

    for fixture in scene.chests {
        let mut inventory = service.config.inventory(&fixture.profile);
        for stack in fixture.contents {
            inventory.slots[usize::from(stack.slot)].stack = Some(ItemStack {
                item: stack.item,
                quantity: stack.quantity,
            });
        }
        let position = Vec2::from(fixture.position);
        commands.network_spawn((
            Chest,
            inventory,
            Position(position),
            NetworkPhysicsState {
                position,
                ..default()
            },
            Transform::from_xyz(position.x, position.y, 0.0),
        ));
    }

    for fixture in scene.ground_items {
        let position = Vec2::from(fixture.position);
        commands.network_spawn((
            GroundItem {
                revision: 0,
                stack: ItemStack {
                    item: fixture.item,
                    quantity: fixture.quantity,
                },
            },
            Position(position),
            NetworkPhysicsState {
                position,
                ..default()
            },
            Transform::from_xyz(position.x, position.y, 0.0),
        ));
    }

    demo.spawned = true;
    info!("Inventory demo: spawned {chest_count} chests and {ground_count} ground stacks");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::{entity, messages::MessageBuffer, server::TickCounter};
    use shared::{
        inventory::{Inventory, InventoryConfig},
        replication::NetworkEntityMap,
        role::NetworkRole,
        snapshots::NetworkIdentity,
    };

    #[test]
    fn inventory_fixture_spawns_once_with_network_identities() {
        let config = InventoryConfig::load().unwrap();
        let fixture = config.load_demo_scene().unwrap();
        let chest_count = fixture.chests.len();
        let ground_count = fixture.ground_items.len();
        let expected_contents: u32 = fixture
            .chests
            .iter()
            .flat_map(|chest| &chest.contents)
            .map(|stack| stack.quantity)
            .sum();

        let mut app = shared::app_init(NetworkRole::Server);
        app.init_resource::<NetworkEntityMap>();
        app.init_resource::<MessageBuffer>();
        app.init_resource::<InventoryDemo>();
        app.insert_resource(TickCounter(0));
        app.insert_resource(InventoryService::new(config));
        entity::plugin(&mut app);
        app.add_systems(Startup, spawn);

        // Explicitly repeat startup to exercise the scene's one-time guard.
        app.world_mut().run_schedule(Startup);
        app.world_mut().run_schedule(Startup);

        let mut chests = app.world_mut().query_filtered::<&Inventory, With<Chest>>();
        assert_eq!(chests.iter(app.world()).count(), chest_count);
        let actual_contents: u32 = chests
            .iter(app.world())
            .flat_map(|inventory| &inventory.slots)
            .filter_map(|slot| slot.stack.as_ref())
            .map(|stack| stack.quantity)
            .sum();
        assert_eq!(actual_contents, expected_contents);

        let mut ground = app.world_mut().query::<&GroundItem>();
        assert_eq!(ground.iter(app.world()).count(), ground_count);
        let mut identities = app.world_mut().query::<(Entity, &NetworkIdentity)>();
        assert_eq!(
            identities.iter(app.world()).count(),
            chest_count + ground_count
        );
        for (entity, identity) in identities.iter(app.world()) {
            assert_eq!(
                app.world()
                    .resource::<NetworkEntityMap>()
                    .get_local_entity(identity.id),
                Some(entity)
            );
        }
    }
}
