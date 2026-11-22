use super::{presentation as paint, state::InventoryClient};
use bevy::math::Vec2;
use bevy_egui::egui;
use shared::inventory::{InventoryAction, InventoryConfig, ItemStack, SlotRef, survival::*};

fn location(contact: &SiteContact) -> Vec2 {
    Vec2::new(contact.position[0] as f32, contact.position[1] as f32)
}
pub(super) fn near_site(view: &ExpeditionView, origin: Vec2) -> Option<&SiteContact> {
    view.contacts
        .iter()
        .filter(|c| c.remaining > 0 && location(c).distance(origin) <= SITE_RADIUS)
        .min_by(|a, b| {
            location(a)
                .distance_squared(origin)
                .total_cmp(&location(b).distance_squared(origin))
        })
}
fn label<'a>(config: &'a InventoryConfig, id: &'a str) -> &'a str {
    config.items.get(id).map_or(id, |d| d.display_name.as_str())
}
fn duration(s: u32) -> String {
    format!("{}:{:02}", s / 60, s % 60)
}
fn ready(client: &InventoryClient) -> bool {
    client.epoch != 0 && client.pending.is_none() && client.actions.is_empty()
}
fn button(
    ui: &mut egui::Ui,
    client: &mut InventoryClient,
    enabled: bool,
    title: &str,
    action: ExpeditionAction,
) {
    if ui
        .add_enabled(enabled && ready(client), egui::Button::new(title))
        .clicked()
    {
        client.enqueue(InventoryAction::Expedition(action));
    }
}

pub(super) fn category(item: &str, consumable: bool) -> &'static str {
    if TANKS.contains(&item) || SUITS.contains(&item) {
        "Equipment"
    } else if item == BEACON {
        "Mission cargo"
    } else if consumable {
        "Consumable"
    } else {
        "Material"
    }
}
pub(super) fn equipment(
    ui: &mut egui::Ui,
    config: &InventoryConfig,
    client: &mut InventoryClient,
    source: SlotRef,
    stack: &ItemStack,
) {
    if TANKS.contains(&stack.item.as_str()) {
        ui.label(format!(
            "Oxygen   {} / {} s",
            stack.charge / OXYGEN_SECOND,
            config.expedition.capacity(&stack.item) / OXYGEN_SECOND
        ));
    } else if stack.item == SUITS[1] {
        ui.label(format!("Crystal charge   {} s", stack.charge));
    }
    if TANKS.contains(&stack.item.as_str()) || SUITS.contains(&stack.item.as_str()) {
        button(ui, client, true, "Equip", ExpeditionAction::Equip(source));
    }
}

fn direction(delta: Vec2) -> String {
    let bearings = ["E", "NE", "N", "NW", "W", "SW", "S", "SE"];
    let sector = (delta.y.atan2(delta.x) / std::f32::consts::FRAC_PI_4).round() as i32;
    format!(
        "{}   {} tiles",
        bearings[sector.rem_euclid(8) as usize],
        (delta.length() / shared::world::TILE_SIZE).ceil() as u32
    )
}

fn instruments(ui: &mut egui::Ui, config: &InventoryConfig, view: &ExpeditionView) {
    let temp = view.gear.temperature_celsius;
    let tank = &view.gear.tank;
    let drain = oxygen_drain(&tank.item, temp);
    ui.add(
        egui::ProgressBar::new(
            f32::from(tank.charge) / f32::from(config.expedition.capacity(&tank.item).max(1)),
        )
        .desired_width(195.0)
        .fill(paint::GREEN)
        .text(format!(
            "O₂   {}   ·   {temp:.1} °C",
            duration(u32::from(tank.charge / drain))
        )),
    );
    if tank.charge / drain <= 30 {
        ui.colored_label(
            egui::Color32::LIGHT_RED,
            "Low oxygen • Change tank or return to ship",
        );
    }
    if temp <= -12.0 {
        let safe = view
            .gear
            .suit
            .as_ref()
            .is_some_and(|s| temp > -40.0 || (s.item == SUITS[1] && s.charge > 0));
        if !safe {
            ui.colored_label(
                egui::Color32::LIGHT_RED,
                "Cold injury • Thermal protection required",
            );
        }
    }
    if view.solar <= 20 && view.fuel == 0 {
        ui.colored_label(paint::ACCENT, "Low solar • Fuel the backup generator");
    }
    if view.gear.suit.as_ref().is_some_and(|s| s.item == SUITS[1]) {
        ui.label(format!(
            "Suit cell   {} s",
            view.gear.suit.as_ref().unwrap().charge
        ));
    }
    if view.sos {
        ui.colored_label(paint::GREEN, "SOS transmitted • Demo complete");
    }
}

pub(super) fn hud(
    ctx: &egui::Context,
    config: &InventoryConfig,
    client: &InventoryClient,
    pos: Vec2,
) {
    let Some(view) = &client.state.expedition else {
        return;
    };
    egui::Area::new("expedition_instruments".into())
        .anchor(egui::Align2::LEFT_BOTTOM, [14.0, -14.0])
        .interactable(false)
        .show(ctx, |ui| {
            paint::card().show(ui, |ui| {
                instruments(ui, config, view);
                let target = client
                    .waypoint
                    .and_then(|id| view.contacts.iter().find(|c| c.id == id));
                let (name, end) = target.map_or(("Main hull", Vec2::ZERO), |c| {
                    (
                        config.expedition.sites[usize::from(c.id)].name.as_str(),
                        location(c),
                    )
                });
                ui.label(format!("{name}   {}", direction(end - pos)));
                if let Some(site) = near_site(view, pos) {
                    ui.label(format!(
                        "[E] {}",
                        config.expedition.sites[usize::from(site.id)].name
                    ));
                } else if pos.length() <= SHIP_RADIUS {
                    ui.label("[I] Expedition • Ship stations");
                }
                if let Some(p) = client.beacon_signal {
                    ui.colored_label(
                        paint::ACCENT,
                        format!("Dropped beacon   {}", direction(p - pos)),
                    );
                }
                if let Some(error) = &client.error {
                    ui.colored_label(egui::Color32::LIGHT_RED, error);
                }
            });
        });
}

pub(super) fn console(ui: &mut egui::Ui, config: &InventoryConfig, client: &mut InventoryClient) {
    let Some(view) = client.state.expedition.clone() else {
        return;
    };
    let pos = client.position;
    let docked = pos.length() <= SHIP_RADIUS;
    instruments(ui, config, &view);
    let objective = if view.sos {
        "Signal received. The crew completed the expedition."
    } else if view.polar_scan {
        "Recover the polar beacon. Return it to the powered workbench with repair materials."
    } else if view.final_suit_built {
        "Scan for the polar emergency module. Pack charged tanks and cryo crystals for the return."
    } else if view.t3_built {
        "Gather cryo crystals on the glacial shelf and fabricate the crystal suit."
    } else if view.fragments == 3 {
        "Blueprint complete. Bring sulfur, quartz, basalt and iron to the workbench for a T3 tank."
    } else if view.scan == 3 {
        "Search the engineering and navigation wrecks for blueprint fragments."
    } else if view.scan_limit == 3 {
        "Scan for the wrecks and the glacial shelf."
    } else if view.scan == 2 {
        "Mine quartz on the glass flats and build an insulated suit."
    } else if view.scan_limit == 2 {
        "Scan for quartz and sulfur. Equip and fill your new tank aboard the ship."
    } else {
        "Mine coal before nightfall. Scan the nearby craters and collect 12 iron for a T2 tank."
    };
    ui.colored_label(paint::ACCENT, objective);
    ui.label(format!(
        "Equipped   {}   ·   {}",
        label(config, &view.gear.tank.item),
        view.gear
            .suit
            .as_ref()
            .map_or("No thermal suit", |s| label(config, &s.item))
    ));
    if let Some([x, y]) = view.gear.last_rescue {
        ui.label(format!(
            "Last rescue   {}",
            direction(Vec2::new(x as f32, y as f32) - pos)
        ));
    }
    if view.gear.rescues > 0 {
        ui.label(format!(
            "Emergency returns   {}   ·   Backpack cargo drops at the rescue site.",
            view.gear.rescues
        ));
    }
    if view.gear.suit.as_ref().is_some_and(|s| s.item == SUITS[1]) {
        button(
            ui,
            client,
            view.gear.suit.as_ref().is_some_and(|s| s.charge == 0),
            "Load cryo crystal",
            ExpeditionAction::ChargeSuit,
        );
    }
    ui.separator();
    ui.heading("Ship power");
    ui.label(format!(
        "{} in {}   ·   Solar {}%   ·   Battery {}/100   ·   Generator fuel {}",
        if view.solar == 0 { "Dawn" } else { "Nightfall" },
        duration(view.seconds_to_phase.into()),
        view.solar,
        view.battery,
        duration(view.fuel)
    ));
    if !docked {
        ui.colored_label(paint::MUTED, "Return to the main hull to use its stations.");
    }
    ui.horizontal_wrapped(|ui| {
        button(ui, client, docked, "Feed 1 coal", ExpeditionAction::Fuel);
        button(
            ui,
            client,
            docked
                && view.powered
                && view.battery >= 10
                && (view.scan < view.scan_limit || (view.final_suit_built && !view.polar_scan)),
            "Scan region · 10 power",
            ExpeditionAction::Scan,
        );
    });
    ui.separator();
    ui.heading("Crew radar");
    ui.label(format!(
        "Region scans   {}   ·   Blueprint fragments   {}/3",
        view.scan, view.fragments
    ));
    radar(ui, config, &view, client, pos);
    if let Some(site) = near_site(&view, pos) {
        button(
            ui,
            client,
            true,
            "Work nearby site",
            ExpeditionAction::WorkSite(site.id),
        );
    }
    ui.separator();
    ui.heading("Workbench");
    let pack = client
        .state
        .views
        .iter()
        .find(|v| Some(v.id) == client.state.player)
        .map(|v| v.inventory.clone());
    for (i, recipe) in config.expedition.recipes.iter().enumerate() {
        let repair = recipe.product == BEACON;
        let title = if repair {
            "Repair beacon and send SOS"
        } else {
            label(config, &recipe.product)
        };
        let needed: Vec<_> = recipe
            .ingredients
            .iter()
            .map(|(id, n)| {
                let held: u32 = pack
                    .iter()
                    .flat_map(|p| &p.slots)
                    .filter_map(|s| s.stack.as_ref())
                    .filter(|s| s.item == *id)
                    .map(|s| s.quantity)
                    .sum();
                (held >= *n, format!("{} {held}/{n}", label(config, id)))
            })
            .collect();
        let lock = match recipe.product.as_str() {
            "tank_t3" if view.fragments < 3 => Some("Find the missing blueprint fragments"),
            "suit_t2" if !view.t3_built => Some("Fabricate a T3 tank first"),
            BEACON if !view.polar_scan => Some("Locate the polar wreck"),
            _ => None,
        };
        ui.horizontal_wrapped(|ui| {
            button(
                ui,
                client,
                docked
                    && view.powered
                    && view.battery >= recipe.power
                    && !view.sos
                    && lock.is_none()
                    && needed.iter().all(|n| n.0),
                title,
                ExpeditionAction::Fabricate(i as u16),
            );
            if let Some(reason) = lock {
                ui.colored_label(paint::ACCENT, reason);
                return;
            }
            ui.label(format!(
                "{}   ·   {} power",
                needed
                    .iter()
                    .map(|x| x.1.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                recipe.power
            ));
        });
    }
    if let Some(error) = &client.error {
        ui.colored_label(egui::Color32::LIGHT_RED, error);
    }
}

fn radar(
    ui: &mut egui::Ui,
    config: &InventoryConfig,
    view: &ExpeditionView,
    client: &mut InventoryClient,
    pos: Vec2,
) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 170.0),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 6, paint::INK);
    let extent = view
        .contacts
        .iter()
        .map(|c| location(c).length())
        .fold(pos.length().max(500.0), f32::max);
    let point = |p: Vec2| {
        rect.center() + egui::vec2(p.x, -p.y) * (rect.height().min(rect.width()) * 0.44 / extent)
    };
    for scale in [0.25, 0.5, 1.0] {
        painter.circle_stroke(
            rect.center(),
            rect.height().min(rect.width()) * 0.44 * scale,
            egui::Stroke::new(1.0, paint::BORDER),
        );
    }
    painter.rect_filled(
        egui::Rect::from_center_size(rect.center(), egui::vec2(8.0, 8.0)),
        1,
        paint::CREAM,
    );
    for c in &view.contacts {
        painter.circle_filled(
            point(location(c)),
            3.5,
            if c.remaining > 0 {
                paint::ACCENT
            } else {
                paint::MUTED
            },
        );
    }
    painter.circle_stroke(point(pos), 6.0, egui::Stroke::new(2.0, paint::GREEN));
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(&mut client.waypoint, None, "Main hull");
        for c in &view.contacts {
            let site = &config.expedition.sites[usize::from(c.id)];
            ui.selectable_value(
                &mut client.waypoint,
                Some(c.id),
                format!("{} · {}", site.name, c.remaining),
            );
        }
    });
    let target = client
        .waypoint
        .and_then(|id| view.contacts.iter().find(|c| c.id == id))
        .map_or(Vec2::ZERO, location);
    ui.label(format!("Bearing   {}", direction(target - pos)));
}
