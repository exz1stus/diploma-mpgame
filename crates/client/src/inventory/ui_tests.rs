use super::*;
use crate::inventory::state::Pending;
use shared::inventory::{InventoryCommand, InventoryPacket, InventoryState, ItemStack, SlotRef};

struct Harness {
    ctx: egui::Context,
    config: InventoryConfig,
    client: InventoryClient,
    size: egui::Vec2,
}

impl Harness {
    fn new() -> Self {
        let config = InventoryConfig::load().unwrap();
        let mut backpack = config.inventory("player");
        backpack.slots[0].stack = Some(ItemStack {
            charge: 0,
            item: "wood".into(),
            quantity: 12,
        });
        backpack.slots[1].stack = Some(ItemStack {
            charge: 0,
            item: "healing_herb".into(),
            quantity: 3,
        });
        let mut chest = config.inventory("wooden_chest");
        chest.slots[0].stack = Some(ItemStack {
            charge: 0,
            item: "stone".into(),
            quantity: 8,
        });
        Self {
            ctx: egui::Context::default(),
            config,
            client: InventoryClient {
                visible: true,
                epoch: 1,
                state: InventoryState {
                    expedition: None,
                    player: Some(1),
                    open_container: Some(2),
                    health: Some(50),
                    views: vec![
                        InventoryView {
                            id: 1,
                            inventory: backpack,
                        },
                        InventoryView {
                            id: 2,
                            inventory: chest,
                        },
                    ],
                },
                ..Default::default()
            },
            size: egui::vec2(1280.0, 900.0),
        }
    }

    fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        self.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, self.size)),
                events,
                ..Default::default()
            },
            |ui| draw_inventory(ui.ctx(), &self.config, &[3], &mut self.client),
        )
    }

    fn settle(&mut self) -> egui::FullOutput {
        for _ in 0..3 {
            self.frame(vec![]).drop_without_applying_deltas();
        }
        self.frame(vec![])
    }

    fn click(&mut self, pos: egui::Pos2) {
        for pressed in [true, false] {
            self.frame(vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ])
            .drop_without_applying_deltas();
        }
    }

    fn click_text(&mut self, label: &str) {
        let output = self.settle();
        let pos = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == label => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .unwrap_or_else(|| {
                panic!(
                    "Missing control: {label}. Visible text: {:?}",
                    output
                        .shapes
                        .iter()
                        .filter_map(|shape| match &shape.shape {
                            egui::Shape::Text(text) =>
                                Some((&text.galley.job.text, text.pos, shape.clip_rect)),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                )
            });
        output.drop_without_applying_deltas();
        self.click(pos);
    }

    fn slots(&mut self) -> Vec<egui::Rect> {
        let output = self.settle();
        let rects = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect)
                    if (rect.rect.width() - self.config.ui.slot_size_px).abs() < 0.1
                        && (rect.rect.height() - self.config.ui.slot_size_px).abs() < 0.1 =>
                {
                    Some(rect.rect)
                }
                _ => None,
            })
            .collect();
        output.drop_without_applying_deltas();
        rects
    }
}

#[test]
fn selecting_half_then_dropping_submits_confirmed_source_and_quantity() {
    let mut ui = Harness::new();
    let slots = ui.slots();
    assert_eq!(slots.len(), 29);
    ui.click(slots[0].center());
    assert_eq!(ui.client.quantity, 12);
    ui.click_text("Half");
    assert_eq!(ui.client.quantity, 6);
    ui.click_text("Drop 6");
    assert!(matches!(
        ui.client.actions.front(),
        Some(InventoryAction::DropItem {
            source: SlotRef {
                inventory: 1,
                slot: 0,
                expected_revision: 0
            },
            quantity: 6,
            ..
        })
    ));
    // The UI proposes a change; the confirmed stack stays untouched until a server packet.
    assert_eq!(
        ui.client.state.views[0].inventory.slots[0]
            .stack
            .as_ref()
            .unwrap()
            .quantity,
        12
    );
}

#[test]
fn search_and_categories_keep_all_slot_positions_and_block_nonmatches() {
    let mut ui = Harness::new();
    let slots = ui.slots();
    ui.client.search = "  HERB  ".into();
    ui.client.filter = ItemFilter::Consumables;
    let filtered = ui.slots();
    assert_eq!(slots, filtered);
    ui.click(filtered[0].center());
    assert!(ui.client.selected.is_none());
    ui.click(filtered[1].center());
    assert_eq!(ui.client.selected.unwrap().slot, 1);
}

#[test]
fn full_health_disables_consuming_and_pending_state_blocks_mutations() {
    let mut ui = Harness::new();
    let slots = ui.slots();
    ui.click(slots[1].center());
    ui.client.state.health = Some(ui.config.rules.max_health);
    ui.click_text("Use one");
    assert!(ui.client.actions.is_empty());
    ui.client.pending = Some(Pending {
        command: InventoryCommand {
            epoch: 1,
            counter: 1,
            action: InventoryAction::Refresh,
        },
        last_sent: 0.0,
    });
    ui.click_text("Drop 3");
    assert!(ui.client.actions.is_empty());
}

#[test]
fn selecting_amount_then_dragging_splits_into_an_empty_slot() {
    let mut ui = Harness::new();
    let slots = ui.slots();
    ui.click(slots[0].center());
    ui.client.quantity = 4;
    let start = slots[0].center();
    let end = slots[2].center();
    ui.frame(vec![
        egui::Event::PointerMoved(start),
        egui::Event::PointerButton {
            pos: start,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    ])
    .drop_without_applying_deltas();
    ui.frame(vec![egui::Event::PointerMoved(
        start + egui::vec2(16.0, 0.0),
    )])
    .drop_without_applying_deltas();
    ui.frame(vec![egui::Event::PointerMoved(end)])
        .drop_without_applying_deltas();
    ui.frame(vec![egui::Event::PointerButton {
        pos: end,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    }])
    .drop_without_applying_deltas();
    assert!(matches!(
        ui.client.actions.front(),
        Some(InventoryAction::TransferContainerItem {
            source: SlotRef {
                inventory: 1,
                slot: 0,
                ..
            },
            destination: Destination::Slot(SlotRef {
                inventory: 1,
                slot: 2,
                ..
            }),
            quantity: 4,
        })
    ));
}

#[test]
fn server_revision_changes_clear_selection_and_old_packets_cannot_restore_it() {
    let mut ui = Harness::new();
    ui.client.selected = Some(SlotRef {
        inventory: 1,
        slot: 0,
        expected_revision: 0,
    });
    let previous = ui.client.state.clone();
    let mut changed = previous.clone();
    changed.views[0].inventory.slots[0].revision = 1;
    ui.client.receive(InventoryPacket {
        epoch: 1,
        serial: 2,
        state: changed,
        receipt: None,
    });
    assert!(ui.client.selected.is_none());
    assert!(ui.client.selected_quantity().is_none());
    ui.client.receive(InventoryPacket {
        epoch: 1,
        serial: 1,
        state: previous,
        receipt: None,
    });
    assert_eq!(ui.client.state.views[0].inventory.slots[0].revision, 1);
}

#[test]
fn narrow_layout_keeps_all_slots_within_the_viewport() {
    let mut ui = Harness::new();
    ui.size = egui::vec2(640.0, 900.0);
    let slots = ui.slots();
    assert_eq!(slots.len(), 29);
    for slot in slots {
        assert!(
            slot.left() >= 0.0 && slot.right() <= ui.size.x,
            "Slot outside viewport: {slot:?}"
        );
    }
}

#[test]
fn game_resolution_exposes_consuming_and_giving_without_scrolling() {
    let mut ui = Harness::new();
    ui.size = egui::vec2(1280.0, 720.0);
    let slots = ui.slots();
    ui.click(slots[1].center());
    ui.click_text("Give 3");
    assert!(ui.client.actions.is_empty());
    ui.click_text("Use one");
    assert!(matches!(
        ui.client.actions.front(),
        Some(InventoryAction::ConsumeItem {
            source: SlotRef {
                inventory: 1,
                slot: 1,
                ..
            }
        })
    ));
}

#[test]
fn closing_during_a_pending_operation_queues_container_cleanup() {
    let mut ui = Harness::new();
    ui.client.pending = Some(Pending {
        command: InventoryCommand {
            epoch: 1,
            counter: 1,
            action: InventoryAction::Refresh,
        },
        last_sent: 0.0,
    });
    ui.click_text("Close");
    assert!(!ui.client.visible);
    assert!(ui.client.pending.is_some());
    assert!(matches!(
        ui.client.actions.front(),
        Some(InventoryAction::CloseContainer)
    ));
}

#[test]
fn fullscreen_expedition_opens_working_ship_controls() {
    use shared::inventory::survival::*;
    let mut h = Harness::new();
    h.client.state.expedition = Some(Box::new(ExpeditionView {
        catalog: h.config.expedition.signature,
        gear: LifeSupport::new(&h.config.expedition, 18.0),
        solar: 100,
        seconds_to_phase: 240,
        fuel: 0,
        battery: 100,
        powered: true,
        scan: 0,
        fragments: 1,
        scan_limit: 1,
        sos: false,
        t3_built: false,
        final_suit_built: false,
        polar_scan: false,
        contacts: vec![SiteContact {
            id: 0,
            position: [1400, 700],
            remaining: 1,
        }],
    }));
    h.click_text("Expedition");
    h.click_text("Engineering wreck · 1");
    assert_eq!(h.client.waypoint, Some(0));
    h.click_text("Scan region · 10 power");
    assert!(matches!(
        h.client.actions.pop_front(),
        Some(InventoryAction::Expedition(ExpeditionAction::Scan))
    ));
    h.client.position = Vec2::new(1400., 700.);
    h.click_text("Scan region · 10 power");
    assert!(h.client.actions.is_empty());
    h.click_text("Work nearby site");
    assert!(matches!(
        h.client.actions.pop_front(),
        Some(InventoryAction::Expedition(ExpeditionAction::WorkSite(0)))
    ));
}
