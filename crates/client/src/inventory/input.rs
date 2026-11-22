//! Inventory keyboard controls and selecting the nearest chest or ground pickup.

use super::state::{DragStack, InventoryClient};
use crate::{
    net::{network::reset_disconnected_client, state::ClientState},
    schedule::ClientSet,
    ui::menu::{MenuState, toggle_menu},
};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use shared::{
    components::{NetworkPhysicsState, Player},
    inventory::{Chest, GroundItem, InventoryAction, InventoryConfig},
    snapshots::NetworkIdentity,
};

pub(super) fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        controls
            .in_set(ClientSet::Session)
            .after(reset_disconnected_client)
            .before(toggle_menu),
    );
}

fn key(binding: &str) -> Option<KeyCode> {
    let letter = binding.strip_prefix("Key")?.as_bytes();

    if letter.len() != 1 || !letter[0].is_ascii_uppercase() {
        return None;
    }

    const KEYS: [KeyCode; 26] = [
        KeyCode::KeyA,
        KeyCode::KeyB,
        KeyCode::KeyC,
        KeyCode::KeyD,
        KeyCode::KeyE,
        KeyCode::KeyF,
        KeyCode::KeyG,
        KeyCode::KeyH,
        KeyCode::KeyI,
        KeyCode::KeyJ,
        KeyCode::KeyK,
        KeyCode::KeyL,
        KeyCode::KeyM,
        KeyCode::KeyN,
        KeyCode::KeyO,
        KeyCode::KeyP,
        KeyCode::KeyQ,
        KeyCode::KeyR,
        KeyCode::KeyS,
        KeyCode::KeyT,
        KeyCode::KeyU,
        KeyCode::KeyV,
        KeyCode::KeyW,
        KeyCode::KeyX,
        KeyCode::KeyY,
        KeyCode::KeyZ,
    ];

    Some(KEYS[usize::from(letter[0] - b'A')])
}

fn controls(
    mut contexts: EguiContexts,
    keys: Res<ButtonInput<KeyCode>>,
    capture: Res<bevy_egui::input::EguiWantsInput>,
    menu: Res<MenuState>,
    config: Res<InventoryConfig>,
    connection: Res<ClientState>,
    players: Query<(&NetworkIdentity, &NetworkPhysicsState), With<Player>>,
    chests: Query<(&NetworkIdentity, &NetworkPhysicsState), With<Chest>>,
    ground: Query<(&NetworkIdentity, &NetworkPhysicsState, &GroundItem)>,
    mut inventory: ResMut<InventoryClient>,
) {
    inventory.escape_consumed = false;

    if !menu.in_game() || !connection.initiated {
        return;
    }

    if keys.just_pressed(KeyCode::Escape) && inventory.visible {
        inventory.escape_consumed = true;
        if let Ok(ctx) = contexts.ctx_mut()
            && egui::DragAndDrop::has_payload_of_type::<DragStack>(ctx)
        {
            egui::DragAndDrop::clear_payload(ctx);
            return;
        }
        inventory.visible = false;

        // Close is safe to queue behind an outstanding mutation.
        inventory.actions.push_back(InventoryAction::CloseContainer);
        return;
    }

    if capture.wants_any_keyboard_input() {
        return;
    }

    if key(&config.ui.bindings.toggle_inventory).is_some_and(|key| keys.just_pressed(key)) {
        inventory.visible = !inventory.visible;

        if !inventory.visible {
            inventory.actions.push_back(InventoryAction::CloseContainer);
        }
    }

    if key(&config.ui.bindings.drop_selected).is_some_and(|key| keys.just_pressed(key))
        && inventory.visible
        && let Some((source, quantity)) = inventory.selected_quantity()
        && Some(source.inventory) == inventory.state.player
    {
        inventory.enqueue(InventoryAction::DropItem {
            source,
            quantity,
            direction: Vec2::Y,
        });
    }

    if !key(&config.ui.bindings.interact_or_collect).is_some_and(|key| keys.just_pressed(key)) {
        return;
    }

    let Some(player_id) = inventory.state.player else {
        return;
    };

    let Some(origin) = players
        .iter()
        .find(|(identity, _)| identity.id == player_id)
        .map(|(_, state)| state.position)
    else {
        return;
    };

    // Choose the nearest eligible interaction.
    let chest = chests
        .iter()
        .filter_map(|(identity, state)| {
            let distance = origin.distance_squared(state.position);

            (distance <= config.rules.container_range.powi(2)).then_some((distance, identity.id))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0));

    let pickup = ground
        .iter()
        .filter_map(|(identity, state, item)| {
            let distance = origin.distance_squared(state.position);

            (distance <= config.rules.pickup_range.powi(2)).then_some((
                distance,
                identity.id,
                item.revision,
                item.stack.quantity,
            ))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0));

    match (chest, pickup) {
        (Some(chest), Some(pickup)) if pickup.0 < chest.0 => {
            inventory.enqueue(InventoryAction::CollectItem {
                ground: pickup.1,
                expected_revision: pickup.2,
                quantity: pickup.3,
            });
        }

        (Some(chest), _) => {
            inventory.mission_tab = false;
            inventory.visible = true;
            inventory.enqueue(InventoryAction::OpenContainer { container: chest.1 });
        }

        (None, Some(pickup)) => {
            inventory.enqueue(InventoryAction::CollectItem {
                ground: pickup.1,
                expected_revision: pickup.2,
                quantity: pickup.3,
            });
        }

        _ => {
            if let Some(view) = &inventory.state.expedition {
                if let Some(site) = super::expedition::near_site(view, origin) {
                    let id = site.id;
                    inventory.enqueue(InventoryAction::Expedition(
                        shared::inventory::survival::ExpeditionAction::WorkSite(id),
                    ));
                } else if origin.length() <= shared::inventory::survival::SHIP_RADIUS {
                    inventory.visible = true;
                    inventory.mission_tab = true;
                }
            }
        }
    }
}
