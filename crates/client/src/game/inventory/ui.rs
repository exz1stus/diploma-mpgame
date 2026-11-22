//! Inventory windows, amount selection, giving, consuming, and pending/error messages.

use super::{slots::draw_grid, state::InventoryClient};
use crate::game::menu::MenuState;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use shared::{
    components::{NetworkPhysicsState, Player},
    inventory::{Destination, InventoryAction, InventoryConfig},
    snapshots::NetworkIdentity,
};

pub(super) fn plugin(app: &mut App) {
    app.add_systems(EguiPrimaryContextPass, inventory_ui);
}

fn inventory_ui(
    mut contexts: EguiContexts,
    menu: Res<MenuState>,
    config: Res<InventoryConfig>,
    players: Query<(&NetworkIdentity, &NetworkPhysicsState), With<Player>>,
    mut inventory: ResMut<InventoryClient>,
) -> Result {
    let ctx = contexts.ctx_mut()?;

    if !menu.in_game() || !inventory.visible {
        egui::DragAndDrop::clear_payload(ctx);
        return Ok(());
    }

    let views = inventory.state.views.clone();
    let busy = inventory.pending.is_some() || !inventory.actions.is_empty();

    let origin = players
        .iter()
        .find(|(identity, _)| Some(identity.id) == inventory.state.player)
        .map(|(_, state)| state.position);

    let nearby: Vec<_> = players
        .iter()
        .filter(|(identity, state)| {
            Some(identity.id) != inventory.state.player
                && origin.is_some_and(|origin| {
                    origin.distance_squared(state.position) <= config.rules.give_range.powi(2)
                })
        })
        .map(|(identity, _)| identity.id)
        .collect();

    egui::Window::new("Inventory").show(ctx, |ui| {
        if let Some(health) = inventory.state.health {
            ui.label(format!("Health: {health}"));
        }

        if busy {
            ui.label("Waiting for server…");
        }

        if let Some(error) = &inventory.error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }

        ui.add_enabled_ui(!busy, |ui| {
            ui.horizontal_top(|ui| {
                for view in &views {
                    ui.vertical(|ui| {
                        draw_grid(ui, view, &config, &mut inventory);
                    });
                }
            });

            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Amount:");
                ui.add(
                    egui::DragValue::new(&mut inventory.quantity)
                        .range(1..=config.rules.max_quantity_per_command),
                );
            });

            egui::ComboBox::from_id_salt("inventory_recipient")
                .selected_text(
                    inventory
                        .recipient
                        .map_or_else(|| "Choose player".to_owned(), |id| format!("Player {id}")),
                )
                .show_ui(ui, |ui| {
                    for id in &nearby {
                        ui.selectable_value(
                            &mut inventory.recipient,
                            Some(*id),
                            format!("Player {id}"),
                        );
                    }
                });

            if let Some((source, quantity)) = inventory.selected_quantity()
                && Some(source.inventory) == inventory.state.player
            {
                ui.horizontal(|ui| {
                    if ui.button("Drop").clicked() {
                        inventory.enqueue(InventoryAction::DropItem {
                            source,
                            quantity,
                            direction: Vec2::Y,
                        });
                    }

                    if ui.button("Consume one").clicked() {
                        inventory.enqueue(InventoryAction::ConsumeItem { source });
                    }

                    if ui.button("Give").clicked()
                        && let Some(recipient) = inventory.recipient
                    {
                        inventory.enqueue(InventoryAction::TransferContainerItem {
                            source,
                            destination: Destination::Auto(recipient),
                            quantity,
                        });
                    }
                });
            }

            if ui.button("Refresh").clicked() {
                inventory.enqueue(InventoryAction::Refresh);
            }
        });
    });

    Ok(())
}
