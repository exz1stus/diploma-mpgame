use super::{
    presentation::{self as paint, ACCENT, BORDER, CREAM, INK, MUTED},
    state::{DragStack, InventoryClient, ItemFilter},
};
use bevy::prelude::Vec2;
use bevy_egui::egui;
use shared::inventory::{
    Destination, InventoryAction, InventoryConfig, InventoryView, SlotRef,
    definitions::ConsumeEffect,
};

pub(super) fn matches_filter(
    item: &str,
    config: &InventoryConfig,
    client: &InventoryClient,
) -> bool {
    let definition = config.items.get(item);
    let consumable = definition.is_some_and(|item| item.consume.is_some());
    let category = match client.filter {
        ItemFilter::All => true,
        ItemFilter::Materials => super::expedition::category(item, consumable) == "Material",
        ItemFilter::Equipment => super::expedition::category(item, consumable) == "Equipment",
        ItemFilter::Consumables => consumable,
    };
    let query = client.search.trim().to_lowercase();
    category
        && (query.is_empty()
            || item.to_lowercase().contains(&query)
            || definition.is_some_and(|item| item.display_name.to_lowercase().contains(&query)))
}

pub(super) fn draw_grid(
    ui: &mut egui::Ui,
    view: &InventoryView,
    config: &InventoryConfig,
    client: &mut InventoryClient,
) {
    let configured_columns = config
        .ui
        .layouts
        .iter()
        .find(|layout| layout.profile == view.inventory.profile)
        .map_or(5, |layout| usize::from(layout.columns));
    let size = config.ui.slot_size_px.max(44.0);
    let gap = config.ui.slot_gap_px;
    let fitting_columns = ((ui.available_width() + gap) / (size + gap))
        .floor()
        .max(1.0) as usize;
    let columns = configured_columns.min(fitting_columns);
    let mut matches = 0;

    egui::Grid::new(("inventory_grid", view.id))
        .spacing(egui::vec2(gap, gap))
        .show(ui, |ui| {
            for (index, slot) in view.inventory.slots.iter().enumerate() {
                let source = SlotRef {
                    inventory: view.id,
                    slot: index as u16,
                    expected_revision: slot.revision,
                };

                let selected = client.selected.is_some_and(|selected| {
                    selected.inventory == source.inventory && selected.slot == source.slot
                });
                let matching = slot
                    .stack
                    .as_ref()
                    .is_none_or(|stack| matches_filter(&stack.item, config, client));
                matches += usize::from(slot.stack.is_some() && matching);
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click_and_drag());
                if view.inventory.cargo_reserved(index) {
                    ui.painter().rect_filled(rect, 7, INK);
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "Cargo",
                        egui::FontId::monospace(10.0),
                        MUTED,
                    );
                    if (index + 1) % columns == 0 {
                        ui.end_row();
                    }
                    continue;
                }
                let dragging = response.dnd_hover_payload::<DragStack>();
                let valid_target = dragging.as_ref().is_some_and(|drag| {
                    let item = client
                        .state
                        .views
                        .iter()
                        .find(|view| view.id == drag.source.inventory)
                        .and_then(|view| view.inventory.slots.get(usize::from(drag.source.slot)))
                        .filter(|slot| slot.revision == drag.source.expected_revision)
                        .and_then(|slot| slot.stack.as_ref());
                    item.is_some_and(|item| {
                        slot.stack
                            .as_ref()
                            .is_none_or(|stack| stack.item == item.item)
                            && config.items.get(&item.item).is_some_and(|definition| {
                                definition.max_stack.saturating_sub(
                                    slot.stack.as_ref().map_or(0, |stack| stack.quantity),
                                ) >= drag.quantity
                            })
                    }) && (drag.source.inventory != source.inventory
                        || drag.source.slot != source.slot)
                });
                let outline = if valid_target {
                    ACCENT
                } else if dragging.is_some() {
                    paint::color(config.ui.busy_rgba)
                } else if selected {
                    paint::color(config.ui.selected_rgba)
                } else if client.pending.is_some() || !client.actions.is_empty() {
                    paint::color(config.ui.pending_rgba)
                } else if response.hovered() {
                    MUTED
                } else {
                    BORDER
                };
                let fill = if selected {
                    outline.gamma_multiply(0.18)
                } else {
                    paint::color(config.ui.empty_rgba)
                };
                ui.painter().rect(
                    rect,
                    7,
                    fill,
                    egui::Stroke::new(if selected || valid_target { 2.0 } else { 1.0 }, outline),
                    egui::StrokeKind::Inside,
                );
                ui.painter().text(
                    rect.left_top() + egui::vec2(5.0, 4.0),
                    egui::Align2::LEFT_TOP,
                    (index + 1).to_string(),
                    egui::FontId::monospace(9.0),
                    MUTED.gamma_multiply(0.5),
                );

                if let Some(stack) = &slot.stack {
                    let definition = config.items.get(&stack.item);
                    let tint = definition.map_or(CREAM, |item| paint::color(item.fallback_rgba));
                    let tint = if matching {
                        tint
                    } else {
                        tint.gamma_multiply(0.2)
                    };
                    paint::item_glyph(
                        ui.painter(),
                        rect.shrink(9.0).translate(egui::vec2(0.0, -3.0)),
                        &stack.item,
                        tint,
                    );
                    let badge = egui::Rect::from_min_max(
                        rect.right_bottom() - egui::vec2(29.0, 18.0),
                        rect.right_bottom() - egui::vec2(3.0, 3.0),
                    );
                    ui.painter().rect_filled(badge, 3, INK);
                    ui.painter().text(
                        badge.center(),
                        egui::Align2::CENTER_CENTER,
                        stack.quantity.to_string(),
                        egui::FontId::monospace(11.0),
                        if matching {
                            CREAM
                        } else {
                            MUTED.gamma_multiply(0.3)
                        },
                    );
                    let limit = stack.quantity.min(config.rules.max_quantity_per_command);
                    let quantity = if selected {
                        client.quantity.clamp(1, limit)
                    } else {
                        limit
                    };
                    if matching {
                        response.dnd_set_drag_payload(DragStack { source, quantity });
                    }
                    response.clone().on_hover_ui(|ui| {
                        ui.strong(
                            definition
                                .map_or(stack.item.as_str(), |item| item.display_name.as_str()),
                        );
                        ui.label(format!(
                            "{} / {} per stack",
                            stack.quantity,
                            definition.map_or(stack.quantity, |item| item.max_stack)
                        ));
                        if let Some(ConsumeEffect::Heal { amount }) =
                            definition.and_then(|item| item.consume.as_ref())
                        {
                            ui.colored_label(paint::GREEN, format!("Restores {amount} health"));
                        }
                        ui.small("Drag to move · Shift-click to transfer");
                    });

                    if matching && response.clicked() {
                        client.selected = Some(source);
                        if !selected {
                            client.quantity = limit;
                        }

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
                                    quantity: limit,
                                });
                            }
                        }
                    }

                    let owned = Some(view.id) == client.state.player;
                    let can_consume = owned
                        && definition.is_some_and(|item| item.consume.is_some())
                        && client
                            .state
                            .health
                            .is_some_and(|health| health < config.rules.max_health);
                    if matching && response.double_clicked() && can_consume {
                        client.enqueue(InventoryAction::ConsumeItem { source });
                    }
                    if matching {
                        response.context_menu(|ui| {
                            ui.strong(
                                definition
                                    .map_or(stack.item.as_str(), |item| item.display_name.as_str()),
                            );
                            ui.separator();
                            if ui.button("Select stack").clicked() {
                                client.selected = Some(source);
                                client.quantity = limit;
                                ui.close();
                            }
                            if ui.button("Select half").clicked() {
                                client.selected = Some(source);
                                client.quantity = stack.quantity.div_ceil(2).min(limit);
                                ui.close();
                            }
                            let target = if owned {
                                client.state.open_container
                            } else {
                                client.state.player
                            };
                            if ui
                                .add_enabled(
                                    target.is_some(),
                                    egui::Button::new(if owned {
                                        "Store in container"
                                    } else {
                                        "Take stack"
                                    }),
                                )
                                .clicked()
                            {
                                if let Some(target) = target {
                                    client.enqueue(InventoryAction::TransferContainerItem {
                                        source,
                                        destination: Destination::Auto(target),
                                        quantity: limit,
                                    });
                                }
                                ui.close();
                            }
                            if ui
                                .add_enabled(can_consume, egui::Button::new("Use one"))
                                .clicked()
                            {
                                client.enqueue(InventoryAction::ConsumeItem { source });
                                ui.close();
                            }

                            if ui
                                .add_enabled(owned, egui::Button::new("Drop one"))
                                .clicked()
                            {
                                client.enqueue(InventoryAction::DropItem {
                                    source,
                                    quantity: 1,
                                    direction: Vec2::Y,
                                });
                                ui.close();
                            }

                            if ui
                                .add_enabled(owned, egui::Button::new("Drop stack"))
                                .clicked()
                            {
                                client.enqueue(InventoryAction::DropItem {
                                    source,
                                    quantity: limit,
                                    direction: Vec2::Y,
                                });
                                ui.close();
                            }
                        });
                    }
                }

                if let Some(drag) = response.dnd_release_payload::<DragStack>()
                    && valid_target
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
    if matches == 0 && (!client.search.trim().is_empty() || client.filter != ItemFilter::All) {
        ui.colored_label(
            MUTED,
            "No matching items. Clear the search or change the filter.",
        );
    } else if view.inventory.slots.iter().all(|slot| slot.stack.is_none()) {
        ui.colored_label(MUTED, "Nothing here yet.");
    }
}
