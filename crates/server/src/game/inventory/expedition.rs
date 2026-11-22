use super::{
    Id, InventoryService, Result, access::within, lifecycle::spawn_ground, transactions::Plan,
};
use crate::{
    game::world::{WorldGenerator, coords::WorldFrame},
    net::entity::{NetworkCommandsExt, reserve_network_ids},
};
use avian2d::prelude::{LinearVelocity, Position};
use bevy::prelude::*;
use shared::{
    components::{Health, NetworkPhysicsState},
    inventory::{
        Chest, Destination, GroundItem, Inventory, InventoryConfig, InventoryError as Error,
        ItemStack, survival::*,
    },
};

#[derive(Clone, Copy)]
pub(crate) struct Environment {
    sheltered: bool,
    pub(crate) celsius: f64,
}

#[derive(Resource, Clone)]
pub(crate) struct Expedition {
    seconds: u32,
    fuel: u32,
    battery: u16,
    pub(super) scan: u8,
    fragments: u8,
    made_t2: bool,
    made_suit: bool,
    made_t3: bool,
    made_final: bool,
    polar_scan: bool,
    sos: bool,
    pub(super) sites: Vec<SiteContact>,
}

pub(crate) fn start(
    mut commands: Commands,
    generator: Res<WorldGenerator>,
    service: Res<InventoryService>,
) {
    let cfg = generator.config();
    let half =
        (cfg.world_size_chunks / 2 * cfg.chunk_size) as i32 * shared::world::TILE_SIZE as i32;
    commands.insert_resource(Expedition::new(&service.config.expedition, cfg.seed, half));
    let store = Vec2::new(-70.0, 50.0);
    commands.network_spawn((
        Chest,
        service.config.inventory("wooden_chest"),
        Position(store),
        NetworkPhysicsState {
            position: store,
            ..default()
        },
        Transform::from_translation(store.extend(0.0)),
    ));
}

impl Expedition {
    fn new(rules: &ExpeditionRules, seed: u64, half: i32) -> Self {
        let sign = if seed.count_ones().is_multiple_of(2) {
            1
        } else {
            -1
        };
        let sites = rules
            .sites
            .iter()
            .enumerate()
            .map(|(i, site)| SiteContact {
                id: i as u16,
                remaining: site.uses,
                position: if site.kind == SiteKind::Pole {
                    [0, sign * (half - 512)]
                } else {
                    [site.offset.0 * sign, site.offset.1]
                },
            })
            .collect();
        Self {
            seconds: 0,
            fuel: 0,
            battery: 100,
            scan: 0,
            fragments: 1,
            made_t2: false,
            made_suit: false,
            made_t3: false,
            made_final: false,
            polar_scan: false,
            sos: false,
            sites,
        }
    }

    fn phase(&self, rules: &ExpeditionRules) -> (u8, u16) {
        let day = u32::from(rules.daylight);
        let cycle = day + u32::from(rules.night);
        let time = self.seconds % cycle;
        if time < day {
            (
                (100 * (day - time)).div_ceil(day) as u8,
                (day - time) as u16,
            )
        } else {
            (0, (cycle - time) as u16)
        }
    }

    fn powered(&self, rules: &ExpeditionRules) -> bool {
        self.phase(rules).0 > 0 || self.fuel > 0
    }
    pub(crate) fn environment_at(
        &self,
        rules: &ExpeditionRules,
        generator: &worldgen::WorldGenerator,
        position: Vec2,
    ) -> Environment {
        let sheltered =
            position.length_squared() <= SHIP_RADIUS * SHIP_RADIUS && self.powered(rules);
        let celsius = if sheltered {
            18.0
        } else {
            let tile = WorldFrame::new(generator.config()).nearest_generator_tile(position);
            // This is the completed temperature passed to BiomeStage, including local terrain offsets.
            let daily_average = generator.draft_at(tile).temperature;
            daily_average - if self.phase(rules).0 == 0 { 20.0 } else { 0.0 }
        };
        Environment { sheltered, celsius }
    }

    fn scan_limit(&self) -> u8 {
        if self.made_suit {
            3
        } else if self.made_t2 {
            2
        } else {
            1
        }
    }
    fn visible(&self, site: &Site) -> bool {
        site.scan <= self.scan && (site.kind != SiteKind::Pole || self.polar_scan)
    }
    fn draw_power(&mut self, rules: &ExpeditionRules, cost: u16) -> Result<()> {
        if !self.powered(rules) {
            return Err(Error::NoPower);
        }
        self.battery = self.battery.checked_sub(cost).ok_or(Error::NoPower)?;
        Ok(())
    }

    pub(super) fn view(&self, rules: &ExpeditionRules, gear: &LifeSupport) -> ExpeditionView {
        let (solar, seconds_to_phase) = self.phase(rules);
        ExpeditionView {
            catalog: rules.signature,
            gear: gear.clone(),
            solar,
            seconds_to_phase,
            powered: self.powered(rules),
            battery: self.battery,
            fuel: self.fuel,
            scan: self.scan,
            fragments: self.fragments,
            scan_limit: self.scan_limit(),
            sos: self.sos,
            t3_built: self.made_t3,
            polar_scan: self.polar_scan,
            final_suit_built: self.made_final,
            contacts: self
                .sites
                .iter()
                .filter(|contact| self.visible(&rules.sites[usize::from(contact.id)]))
                .cloned()
                .collect(),
        }
    }

    fn work(
        &mut self,
        world: &World,
        config: &InventoryConfig,
        plan: &mut Plan,
        actor: Id,
        origin: Vec2,
        id: u16,
    ) -> Result<()> {
        let index = usize::from(id);
        let site = config.expedition.sites.get(index).ok_or(Error::NotFound)?;
        if !self.visible(site) {
            return Err(Error::RequiresDiscovery);
        }
        let contact = &mut self.sites[index];
        within(
            origin,
            Vec2::new(contact.position[0] as f32, contact.position[1] as f32),
            SITE_RADIUS,
        )?;
        contact.remaining = contact.remaining.checked_sub(1).ok_or(Error::Depleted)?;
        match site.kind {
            SiteKind::Wreck => self.fragments += 1,
            SiteKind::Pole => plan.put(
                world,
                config,
                Destination::Auto(actor),
                ItemStack {
                    item: BEACON.into(),
                    quantity: 1,
                    charge: 0,
                },
            )?,
        }
        Ok(())
    }

    fn advance(&mut self, rules: &ExpeditionRules) {
        let solar = self.phase(rules).0;
        let generated = if solar <= 20 && self.fuel > 0 {
            2
        } else {
            u16::from(solar).div_ceil(50)
        };
        self.battery = (self.battery + generated).min(100);
        if solar <= 20 && self.fuel > 0 {
            self.fuel -= 1;
        }
        self.seconds = self.seconds.wrapping_add(1);
    }
}

impl InventoryService {
    pub(super) fn expedition_action(
        &self,
        world: &World,
        plan: &mut Plan,
        actor: Id,
        entity: Entity,
        origin: Vec2,
        action: ExpeditionAction,
    ) -> Result<()> {
        let rules = &self.config.expedition;
        let mut mission = world
            .get_resource::<Expedition>()
            .cloned()
            .ok_or(Error::AccessDenied)?;
        let mut gear = world
            .get::<LifeSupport>(entity)
            .cloned()
            .ok_or(Error::AccessDenied)?;
        if mission.sos {
            return Err(Error::RequiresDiscovery);
        }
        let at_ship = || within(origin, Vec2::ZERO, SHIP_RADIUS);
        match action {
            ExpeditionAction::WorkSite(id) => {
                mission.work(world, &self.config, plan, actor, origin, id)?
            }
            ExpeditionAction::Fuel => {
                at_ship()?;
                plan.spend(world, actor, "coal", 1)?;
                mission.fuel = mission
                    .fuel
                    .checked_add(u32::from(rules.coal_seconds))
                    .ok_or(Error::Overflow)?;
            }
            ExpeditionAction::Scan => {
                at_ship()?;
                if mission.scan < mission.scan_limit() {
                    mission.scan += 1;
                } else if mission.made_final && mission.fragments == 3 && !mission.polar_scan {
                    mission.polar_scan = true;
                } else {
                    return Err(Error::RequiresDiscovery);
                }
                mission.draw_power(rules, 10)?;
            }
            ExpeditionAction::Fabricate(index) => {
                at_ship()?;
                let recipe = rules
                    .recipes
                    .get(usize::from(index))
                    .ok_or(Error::NotFound)?;
                let product = recipe.product.as_str();
                let unlocked = match product {
                    "tank_t3" => mission.fragments == 3,
                    "suit_t2" => mission.made_t3,
                    BEACON => mission.polar_scan && !mission.sos,
                    _ => true,
                };
                if !unlocked {
                    return Err(Error::RequiresDiscovery);
                }
                mission.draw_power(rules, recipe.power)?;
                for (item, qty) in &recipe.ingredients {
                    plan.spend(world, actor, item, *qty)?;
                }
                if product == BEACON {
                    mission.sos = true;
                } else {
                    plan.put(
                        world,
                        &self.config,
                        Destination::Auto(actor),
                        ItemStack {
                            item: product.into(),
                            quantity: 1,
                            charge: 0,
                        },
                    )?;
                    match product {
                        "tank_t2" => mission.made_t2 = true,
                        "suit_t1" => mission.made_suit = true,
                        "tank_t3" => mission.made_t3 = true,
                        "suit_t2" => mission.made_final = true,
                        _ => return Err(Error::InvalidItem),
                    }
                }
            }
            ExpeditionAction::Equip(source) => {
                if source.inventory != actor {
                    return Err(Error::AccessDenied);
                }
                let new = plan.take(world, source, 1)?;
                let previous = if TANKS.contains(&new.item.as_str()) {
                    Some(std::mem::replace(&mut gear.tank, new))
                } else if SUITS.contains(&new.item.as_str()) {
                    gear.suit.replace(new)
                } else {
                    return Err(Error::InvalidItem);
                };
                if let Some(stack) = previous {
                    plan.put(world, &self.config, Destination::Auto(actor), stack)?;
                }
            }
            ExpeditionAction::ChargeSuit => {
                let suit = gear
                    .suit
                    .as_mut()
                    .filter(|s| s.item == SUITS[1])
                    .ok_or(Error::CannotConsume)?;
                if suit.charge != 0 {
                    return Err(Error::CannotConsume);
                }
                plan.spend(world, actor, "cryo_crystal", 1)?;
                suit.charge = rules.crystal_seconds;
            }
        }
        plan.expedition = Some((mission, entity, gear));
        Ok(())
    }
}

// Fixed simulation time controls life support on all server tick rates.
pub(super) fn tick(world: &mut World, mut elapsed: Local<std::time::Duration>) {
    if !world.contains_resource::<Expedition>()
        || world.query::<&LifeSupport>().iter(world).next().is_none()
    {
        return;
    }
    *elapsed += world.resource::<Time<Fixed>>().delta();
    while *elapsed >= std::time::Duration::from_secs(1) {
        *elapsed -= std::time::Duration::from_secs(1);
        world.resource_scope(|world, service: Mut<InventoryService>| {
            let rules = &service.config.expedition;
            let generator = world.resource::<WorldGenerator>().0.clone();
            let mut players = world.query::<(Entity, &Position, &mut LifeSupport, &mut Health)>();
            let mut rescues = Vec::new();
            world.resource_scope(|world, mut mission: Mut<Expedition>| {
                if mission.sos {
                    return;
                }
                for (entity, pos, mut gear, mut health) in players.iter_mut(world) {
                    let environment = mission.environment_at(rules, &generator, pos.0);
                    let damage = breathe(&mut gear, rules, environment);
                    if environment.sheltered {
                        health.0 = health
                            .0
                            .saturating_add(2)
                            .min(service.config.rules.max_health);
                    }
                    health.0 = health.0.saturating_sub(damage);
                    if health.0 == 0 {
                        rescues.push((entity, pos.0));
                    }
                }
                mission.advance(rules);
                if !rescues.is_empty() {
                    let hull = mission.environment_at(rules, &generator, Vec2::ZERO);
                    for (entity, origin) in rescues {
                        rescue(
                            world,
                            entity,
                            origin,
                            service.config.rules.max_health,
                            hull.celsius,
                        );
                    }
                }
            });
        });
    }
}

fn breathe(gear: &mut LifeSupport, rules: &ExpeditionRules, environment: Environment) -> u16 {
    let celsius = environment.celsius;
    gear.temperature_celsius = celsius;
    if environment.sheltered {
        gear.tank.charge = gear
            .tank
            .charge
            .saturating_add(24 * OXYGEN_SECOND)
            .min(rules.capacity(&gear.tank.item));
        return 0;
    }
    let cold = celsius <= 0.0;
    let exposure = if celsius <= -40.0 {
        3
    } else if celsius <= -12.0 {
        2
    } else if cold {
        1
    } else {
        0
    };
    let protection = match gear.suit.as_mut() {
        Some(s) if s.item == SUITS[1] && s.charge > 0 => {
            if cold {
                s.charge -= 1;
            }
            3
        }
        Some(_) => 2,
        None => 0,
    };
    let drain = oxygen_drain(&gear.tank.item, celsius);
    let suffocating = gear.tank.charge < drain;
    gear.tank.charge = gear.tank.charge.saturating_sub(drain);
    // Mild chill penalizes oxygen. Deep cold also causes sustained exposure damage.
    u16::from(suffocating) * 8
        + if exposure >= 2 && protection < exposure {
            4
        } else {
            0
        }
}

fn rescue(world: &mut World, entity: Entity, origin: Vec2, max_health: u16, hull_celsius: f64) {
    let Some(inv) = world.get::<Inventory>(entity) else {
        return;
    };
    let count = inv.slots.iter().filter(|s| s.stack.is_some()).count();
    let Ok(ids) = reserve_network_ids(world, count) else {
        return;
    };
    let mut inventory = world.get_mut::<Inventory>(entity).unwrap();
    let stacks: Vec<_> = inventory
        .slots
        .iter_mut()
        .filter_map(|s| {
            let stack = s.stack.take()?;
            s.revision += 1;
            Some(stack)
        })
        .collect();
    inventory.revision += 1;
    for (id, stack) in ids.into_iter().zip(stacks) {
        spawn_ground(world, id, origin, GroundItem { revision: 0, stack });
    }
    world.entity_mut(entity).insert((
        Position(Vec2::ZERO),
        Transform::default(),
        LinearVelocity::ZERO,
        Health(max_health),
    ));
    let mut gear = world.get_mut::<LifeSupport>(entity).unwrap();
    gear.temperature_celsius = hull_celsius;
    gear.rescues = gear.rescues.saturating_add(1);
    gear.last_rescue = Some([origin.x.round() as i32, origin.y.round() as i32]);
}

#[cfg(test)]
#[path = "expedition_tests.rs"]
mod tests;
