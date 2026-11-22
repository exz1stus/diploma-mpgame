//! Responsive inventory workspace, item inspection, and confirmed action feedback.

#[cfg(test)]
#[path = "ui_tests.rs"]
mod tests;

use super::{
    presentation::{self as paint, ACCENT, CREAM, GREEN, INK, MUTED},
    slots::draw_grid,
    state::{InventoryClient, ItemFilter},
};
use crate::ui::menu::MenuState;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use shared::{
    components::{NetworkPhysicsState, Player},
    inventory::{
        Destination, InventoryAction, InventoryConfig, InventoryView, definitions::ConsumeEffect,
    },
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
    ground: Query<(&shared::inventory::GroundItem, &NetworkPhysicsState)>,
    mut inventory: ResMut<InventoryClient>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    if !menu.in_game() {
        egui::DragAndDrop::clear_payload(ctx);
        return Ok(());
    }

    let origin = players
        .iter()
        .find(|(identity, _)| Some(identity.id) == inventory.state.player)
        .map(|(_, state)| state.position);
    inventory.beacon_signal = ground
        .iter()
        .find(|(item, _)| item.stack.item == shared::inventory::survival::BEACON)
        .map(|(_, state)| state.position);
    if !inventory.visible {
        egui::DragAndDrop::clear_payload(ctx);
        if let Some(pos) = origin {
            super::expedition::hud(ctx, &config, &inventory, pos);
        }
        return Ok(());
    }
    inventory.position = origin.unwrap_or_default();
    let mut nearby: Vec<_> = players
        .iter()
        .filter(|(identity, state)| {
            Some(identity.id) != inventory.state.player
                && origin.is_some_and(|origin| {
                    origin.distance_squared(state.position) <= config.rules.give_range.powi(2)
                })
        })
        .map(|(identity, _)| identity.id)
        .collect();
    nearby.sort_unstable();
    if inventory.recipient.is_some_and(|id| !nearby.contains(&id)) {
        inventory.recipient = None;
    }

    draw_inventory(ctx, &config, &nearby, &mut inventory);
    Ok(())
}

fn draw_inventory(
    ctx: &egui::Context,
    config: &InventoryConfig,
    nearby: &[u64],
    client: &mut InventoryClient,
) {
    let mut close_requested = false;
    let mut viewport_ui = egui::Ui::new(
        ctx.clone(),
        egui::Id::new("inventory_viewport"),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(INK).inner_margin(20.0))
        .show(&mut viewport_ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("inventory_workspace_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    paint::style(ui);
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("FIELD INVENTORY")
                                .size(24.0)
                                .strong()
                                .color(CREAM),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .button("Close")
                                .on_hover_text("Close inventory [Esc]")
                                .clicked()
                            {
                                close_requested = true;
                            }
                        });
                    });
                    if client.state.expedition.is_some() {
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut client.mission_tab, false, "Backpack");
                            ui.selectable_value(&mut client.mission_tab, true, "Expedition");
                        });
                        if client.mission_tab {
                            super::expedition::console(ui, config, client);
                            return;
                        }
                    }
                    ui.colored_label(
                        MUTED,
                        "Pack for the journey. Select an item to inspect or move it.",
                    );
                    ui.add_space(6.0);

                    ui.horizontal_wrapped(|ui| {
                        if let Some(health) = client.state.health {
                            ui.add(
                                egui::ProgressBar::new(
                                    f32::from(health) / f32::from(config.rules.max_health),
                                )
                                .desired_width(180.0)
                                .fill(GREEN)
                                .text(format!("Health  {health} / {}", config.rules.max_health)),
                            );
                        }
                        let busy = client.pending.is_some() || !client.actions.is_empty();
                        if client.epoch == 0 {
                            ui.spinner();
                            ui.colored_label(MUTED, "Loading inventory…");
                        } else if busy {
                            ui.spinner();
                            ui.colored_label(
                                paint::color(config.ui.pending_rgba),
                                "Confirming action…",
                            );
                        } else {
                            ui.colored_label(GREEN, "Ready");
                        }
                        if ui
                            .add_enabled(!busy && client.epoch != 0, egui::Button::new("Refresh"))
                            .clicked()
                        {
                            client.enqueue(InventoryAction::Refresh);
                        }
                    });
                    ui.separator();

                    ui.horizontal_wrapped(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut client.search)
                                .id_salt("inventory_search")
                                .hint_text("Search items…")
                                .desired_width(220.0)
                                .char_limit(80),
                        );
                        if !client.search.is_empty() && ui.small_button("Clear").clicked() {
                            client.search.clear();
                        }
                        for (filter, label) in [
                            (ItemFilter::All, "All items"),
                            (ItemFilter::Materials, "Materials"),
                            (ItemFilter::Consumables, "Consumables"),
                            (ItemFilter::Equipment, "Equipment"),
                        ] {
                            ui.selectable_value(&mut client.filter, filter, label);
                        }
                    });

                    let views = client.state.views.clone();
                    let ready =
                        client.epoch != 0 && client.pending.is_none() && client.actions.is_empty();
                    if !ready {
                        egui::DragAndDrop::clear_payload(ctx);
                    }
                    // Search never changes the authoritative slot IDs.
                    ui.add_enabled_ui(ready, |ui| {
                        if ui.available_width() >= 860.0 {
                            let left = ui.available_width() - 290.0;
                            ui.horizontal_top(|ui| {
                                ui.allocate_ui_with_layout(
                                    egui::vec2(left, 0.0),
                                    egui::Layout::top_down(egui::Align::Min),
                                    |ui| {
                                        draw_storage(ui, &views, config, client);
                                    },
                                );
                                ui.allocate_ui_with_layout(
                                    egui::vec2(280.0, 0.0),
                                    egui::Layout::top_down(egui::Align::Min),
                                    |ui| {
                                        draw_inspector(ui, config, nearby, client);
                                    },
                                );
                            });
                        } else {
                            draw_storage(ui, &views, config, client);
                            draw_inspector(ui, config, nearby, client);
                        }
                    });

                    if let Some(error) = client.error.clone() {
                        ui.horizontal_wrapped(|ui| {
                            ui.colored_label(egui::Color32::LIGHT_RED, error);
                            if ui.small_button("Dismiss").clicked() {
                                client.error = None;
                            }
                        });
                    } else if let Some(notice) = &client.notice {
                        ui.colored_label(GREEN, notice);
                    }
                    ui.separator();
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            egui::RichText::new(format!(
                                "[{}] Inventory   [{}] Interact   [{}] Drop",
                                key_label(&config.ui.bindings.toggle_inventory),
                                key_label(&config.ui.bindings.interact_or_collect),
                                key_label(&config.ui.bindings.drop_selected)
                            ))
                            .small()
                            .color(MUTED),
                        );
                        ui.label(
                            egui::RichText::new(
                                "Shift-click: transfer   ·   Right-click: actions   ·   Esc: close",
                            )
                            .small()
                            .color(MUTED),
                        );
                    });
                });
        });

    if close_requested {
        client.visible = false;
        // Closing must survive an outstanding operation, just like the keyboard close path.
        client.actions.push_back(InventoryAction::CloseContainer);
        egui::DragAndDrop::clear_payload(ctx);
    }
}

fn key_label(binding: &str) -> &str {
    binding.strip_prefix("Key").unwrap_or(binding)
}

fn draw_storage(
    ui: &mut egui::Ui,
    views: &[InventoryView],
    config: &InventoryConfig,
    client: &mut InventoryClient,
) {
    if views.is_empty() {
        paint::card().show(ui, |ui| {
            ui.heading("Your backpack");
            ui.colored_label(MUTED, "Waiting for your inventory from the server…");
        });
        return;
    }
    if views.len() > 1 && ui.available_width() >= 520.0 {
        ui.columns(2, |columns| {
            for (index, view) in views.iter().enumerate() {
                draw_container(&mut columns[index % 2], view, config, client);
            }
        });
    } else {
        for view in views {
            draw_container(ui, view, config, client);
        }
    }
    if client.state.open_container.is_none() {
        paint::card().show(ui, |ui| {
            ui.set_width((ui.available_width() - 2.0).max(0.0));
            ui.colored_label(ACCENT, "NEARBY STORAGE");
            ui.colored_label(
                MUTED,
                format!(
                    "Approach a container and press [{}] to open it alongside your backpack.",
                    key_label(&config.ui.bindings.interact_or_collect)
                ),
            );
        });
    }
}

fn draw_container(
    ui: &mut egui::Ui,
    view: &InventoryView,
    config: &InventoryConfig,
    client: &mut InventoryClient,
) {
    paint::card().show(ui, |ui| {
        ui.set_width((ui.available_width() - 2.0).max(0.0));
        let owned = Some(view.id) == client.state.player;
        ui.colored_label(ACCENT, if owned { "CARRIED" } else { "CONTAINER" });
        let name = config
            .profiles
            .get(&view.inventory.profile)
            .map_or("Inventory", |profile| profile.display_name.as_str());
        ui.label(
            egui::RichText::new(if owned { "Your backpack" } else { name })
                .size(18.0)
                .strong(),
        );
        let used = view.inventory.used_slots();
        let capacity = view.inventory.slots.len();
        ui.add(
            egui::ProgressBar::new(used as f32 / capacity.max(1) as f32)
                .desired_width(ui.available_width())
                .fill(if used == capacity { ACCENT } else { GREEN })
                .text(format!("{used} / {capacity} slots")),
        );
        ui.add_space(4.0);
        draw_grid(ui, view, config, client);
        if !owned && ui.small_button("Close container").clicked() {
            client.enqueue(InventoryAction::CloseContainer);
        }
    });
}

fn draw_inspector(
    ui: &mut egui::Ui,
    config: &InventoryConfig,
    nearby: &[u64],
    client: &mut InventoryClient,
) {
    paint::card().show(ui, |ui| {
        ui.set_width((ui.available_width() - 2.0).max(0.0));
        ui.colored_label(ACCENT, "ITEM DETAILS");
        let Some((source, stack)) = client.selected_stack().map(|(source, stack)| (source, stack.clone())) else {
            ui.add_space(28.0);
            ui.label(egui::RichText::new("Select an item").size(18.0));
            ui.colored_label(MUTED, "Select a stack to see its details and choose how much to move.");
            ui.add_space(18.0);
            ui.colored_label(MUTED, "Drag onto an empty or matching slot to move items. Select a smaller amount to split a stack.");
            return;
        };
        let definition = config.items.get(&stack.item);
        let tint = definition.map_or(CREAM, |item| paint::color(item.fallback_rgba));
        let consumable = definition.is_some_and(|item| item.consume.is_some());
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(56.0, 56.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 10, tint.gamma_multiply(0.12));
            paint::item_glyph(ui.painter(), rect.shrink(6.0), &stack.item, tint);
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(definition.map_or(stack.item.as_str(), |item| item.display_name.as_str())).size(20.0).strong());
                ui.colored_label(MUTED, super::expedition::category(&stack.item, consumable));
            });
        });
        ui.label(format!("In stack: {}  /  {}", stack.quantity, definition.map_or(stack.quantity, |item| item.max_stack)));
        if let Some(ConsumeEffect::Heal { amount }) = definition.and_then(|item| item.consume.as_ref()) {
            ui.colored_label(GREEN, format!("Restores {amount} health. Consumes one."));
        }
        ui.separator();
        let limit = stack.quantity.min(config.rules.max_quantity_per_command);
        client.quantity = client.quantity.clamp(1, limit);
        ui.label("Amount to move");
        ui.add(egui::Slider::new(&mut client.quantity, 1..=limit).integer());
        ui.horizontal(|ui| {
            if ui.button("1").clicked() { client.quantity = 1; }
            if ui.button("Half").clicked() { client.quantity = stack.quantity.div_ceil(2).min(limit); }
            if ui.button("Max").clicked() { client.quantity = limit; }
        });
        let quantity = client.quantity;
        let owned = Some(source.inventory) == client.state.player;
        let target = if owned { client.state.open_container } else { client.state.player };
        if let Some(target) = target {
            let label = if owned { format!("Store {quantity} in container") } else { format!("Take {quantity} into backpack") };
            if ui.add_sized([ui.available_width(), 34.0], egui::Button::new(label).fill(ACCENT.gamma_multiply(0.25))).clicked() {
                client.enqueue(InventoryAction::TransferContainerItem { source, destination: Destination::Auto(target), quantity });
            }
        }
        if owned && client.state.expedition.is_some() { super::expedition::equipment(ui,config,client,source,&stack); }
        if owned {
            ui.horizontal(|ui| {
            if consumable {
                let injured = client.state.health.is_some_and(|health| health < config.rules.max_health);
                if ui.add_enabled(injured, egui::Button::new("Use one")).on_hover_text(if injured { "Consume one item" } else { "Your health is full" }).clicked() {
                    client.enqueue(InventoryAction::ConsumeItem { source });
                }
            }
            if ui.button(format!("Drop {quantity}")).clicked() {
                client.enqueue(InventoryAction::DropItem { source, quantity, direction: Vec2::Y });
            }
            });
            ui.separator();
            ui.label("Give to a nearby player");
            ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("inventory_recipient")
                .selected_text(client.recipient.map_or_else(|| "Choose player".to_owned(), |id| format!("Player {id}")))
                .width((ui.available_width() - 108.0).max(80.0))
                .show_ui(ui, |ui| {
                    for id in nearby { ui.selectable_value(&mut client.recipient, Some(*id), format!("Player {id}")); }
                });
            if ui.add_enabled(client.recipient.is_some_and(|id| nearby.contains(&id)), egui::Button::new(format!("Give {quantity}"))).clicked()
                && let Some(recipient) = client.recipient {
                client.enqueue(InventoryAction::TransferContainerItem { source, destination: Destination::Auto(recipient), quantity });
            }
            });
            if nearby.is_empty() { ui.colored_label(MUTED, "No players within giving range."); }
        }
    });
}
