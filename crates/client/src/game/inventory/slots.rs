//! Slot grids, stack selection, drag-and-drop, quick transfers, and item context menus.

use super::state::{DragStack, InventoryClient};
use bevy::prelude::Vec2;
use bevy_egui::egui;
use shared::inventory::{Destination, InventoryAction, InventoryConfig, InventoryView, SlotRef};

pub(super) fn draw_grid(
    ui: &mut egui::Ui,
    view: &InventoryView,
    config: &InventoryConfig,
    client: &mut InventoryClient,
) {
    let title = config
        .profiles
        .get(&view.inventory.profile)
        .map(|profile| profile.display_name.as_str())
        .unwrap_or("Inventory");

    ui.label(title);

    let columns = config
        .ui
        .layouts
        .iter()
        .find(|layout| layout.profile == view.inventory.profile)
        .map_or(5, |layout| usize::from(layout.columns));

    egui::Grid::new(("inventory_grid", view.id))
        .spacing(egui::vec2(config.ui.slot_gap_px, config.ui.slot_gap_px))
        .show(ui, |ui| {
            for (index, slot) in view.inventory.slots.iter().enumerate() {
                let source = SlotRef {
                    inventory: view.id,
                    slot: index as u16,
                    expected_revision: slot.revision,
                };

                let text = slot.stack.as_ref().map_or_else(
                    || " ".to_owned(),
                    |stack| {
                        let name = config
                            .items
                            .get(&stack.item)
                            .map(|item| item.display_name.as_str())
                            .unwrap_or(stack.item.as_str());

                        format!("{name}\n{}", stack.quantity)
                    },
                );

                let selected = client.selected.is_some_and(|selected| {
                    selected.inventory == source.inventory && selected.slot == source.slot
                });

                let mut button = egui::Button::new(text).sense(egui::Sense::click_and_drag());

                if selected {
                    button = button.fill(egui::Color32::from_rgb(110, 85, 45));
                }

                let response =
                    ui.add_sized([config.ui.slot_size_px, config.ui.slot_size_px], button);

                if let Some(stack) = &slot.stack {
                    response.dnd_set_drag_payload(DragStack {
                        source,
                        quantity: stack.quantity,
                    });

                    if response.clicked() {
                        client.selected = Some(source);

                        if ui.input(|input| input.modifiers.shift) {
                            let target = if Some(view.id) == client.state.player {
                                client.state.open_container
                            } else {
                                client.state.player
                            };

                            if let Some(target) = target {
                                client.enqueue(InventoryAction::TransferContainerItem {
                                    source,
                                    destination: Destination::Auto(target),
                                    quantity: stack.quantity,
                                });
                            }
                        }
                    }

                    if Some(view.id) == client.state.player {
                        response.context_menu(|ui| {
                            if ui.button("Consume one").clicked() {
                                client.enqueue(InventoryAction::ConsumeItem { source });
                                ui.close();
                            }

                            if ui.button("Drop one").clicked() {
                                client.enqueue(InventoryAction::DropItem {
                                    source,
                                    quantity: 1,
                                    direction: Vec2::Y,
                                });
                                ui.close();
                            }

                            if ui.button("Drop stack").clicked() {
                                client.enqueue(InventoryAction::DropItem {
                                    source,
                                    quantity: stack.quantity,
                                    direction: Vec2::Y,
                                });
                                ui.close();
                            }
                        });
                    }
                }

                if let Some(drag) = response.dnd_release_payload::<DragStack>()
                    && (drag.source.inventory != source.inventory
                        || drag.source.slot != source.slot)
                {
                    client.enqueue(InventoryAction::TransferContainerItem {
                        source: drag.source,
                        destination: Destination::Slot(source),
                        quantity: drag.quantity,
                    });
                }

                if (index + 1) % columns == 0 {
                    ui.end_row();
                }
            }
        });
}
