use super::*;
use crate::game::world::{
    self as game_world,
    components::{ChunkTiles, InChunk, WorldObject, WorldObserver},
    storage::{ChunkStorage, StoredChunk},
    streaming::ChunkStreamer,
};
use worldgen::{ChunkPos, ObjectInstanceId, PlacedObject, TilePos};

use crate::game::inventory::sessions::Session;
use shared::{
    components::{DisconnectPolicy, Ownership, Player},
    inventory::{InventoryAction, InventoryCommand, InventoryOutcome, SlotRef},
    replication::NetworkEntityMap,
};

struct Crew {
    world: World,
    service: InventoryService,
    players: [Entity; 2],
    sessions: [Session; 2],
    counters: [u64; 2],
    next_anchor: i64,
}

impl Crew {
    fn new() -> Self {
        let config = InventoryConfig::load().unwrap();
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default());
        let generator = worldgen::WorldConfig::bundled().build().unwrap();
        let size = generator.config().chunk_size;
        let plain = generator.biomes().id("plain_mid").unwrap();
        game_world::install(&mut app, std::sync::Arc::new(generator), 1);
        app.world_mut()
            .resource_mut::<ChunkStorage>()
            .put(StoredChunk {
                pos: ChunkPos { x: 0, y: 0 },
                size,
                tiles: vec![worldgen::TileSample::new(plain, 0.5); (size * size) as usize],
                objects: vec![],
            });
        app.world_mut().spawn((
            WorldObserver {
                simulation_distance: 0,
            },
            Position(Vec2::ZERO),
        ));
        app.world_mut().run_schedule(FixedPreUpdate);
        let mut world = std::mem::take(app.world_mut());
        world.init_resource::<NetworkEntityMap>();
        world.insert_resource(Expedition::new(&config.expedition, 0, 256_000));
        let players = std::array::from_fn(|i| {
            let id = (i + 1) as u64;
            let entity = world
                .spawn((
                    Player,
                    Health(100),
                    Position(Vec2::ZERO),
                    LifeSupport::new(&config.expedition, 18.0),
                    config.inventory("player"),
                    Ownership {
                        client_id: id,
                        disconnect_policy: DisconnectPolicy::Despawn,
                    },
                ))
                .id();
            world.resource_mut::<NetworkEntityMap>().insert(entity, id);
            entity
        });
        Self {
            world,
            players,
            service: InventoryService::new(config),
            sessions: [Session::new(1, 1, 0.0, 40), Session::new(1, 2, 0.0, 40)],
            counters: [0, 0],
            next_anchor: 0,
        }
    }
    fn send(&mut self, i: usize, action: InventoryAction) -> Result<InventoryOutcome> {
        self.counters[i] += 1;
        let counter = self.counters[i];
        self.service.execute(
            &mut self.world,
            (i + 1) as u64,
            &mut self.sessions[i],
            &InventoryCommand {
                epoch: 1,
                counter,
                action,
            },
            counter as f64,
        )
    }
    fn act(&mut self, i: usize, action: ExpeditionAction) -> Result<InventoryOutcome> {
        self.send(i, InventoryAction::Expedition(action))
    }
    fn stand(&mut self, i: usize, pos: Vec2) {
        self.world.get_mut::<Position>(self.players[i]).unwrap().0 = pos;
    }
    fn visit(&mut self, i: usize, id: u16, times: usize) {
        let p = self.world.resource::<Expedition>().sites[usize::from(id)].position;
        self.stand(i, Vec2::new(p[0] as f32, p[1] as f32));
        for _ in 0..times {
            self.act(i, ExpeditionAction::WorkSite(id)).unwrap();
        }
    }
    fn deposit(&mut self, kind: &str, biome: &str) -> TilePos {
        let g = self.world.resource::<WorldGenerator>();
        let id = g.object_placer().objects().id(kind).unwrap();
        let footprint = g.object_placer().objects().get(id).unwrap().footprint;
        let biome = g.biomes().id(biome).unwrap();
        let size = i64::from(g.config().chunk_size);
        let index = self.next_anchor;
        self.next_anchor += 1;
        let anchor = TilePos {
            x: index % size,
            y: index / size,
        };
        let chunk = self
            .world
            .resource::<ChunkStreamer>()
            .loaded(ChunkPos { x: 0, y: 0 })
            .unwrap();
        self.world.get_mut::<ChunkTiles>(chunk).unwrap().tiles[index as usize].biome = biome;
        self.world.spawn((
            InChunk(chunk),
            WorldObject {
                placed: PlacedObject {
                    id: ObjectInstanceId { rule_id: 1, anchor },
                    kind: id,
                    footprint,
                },
            },
        ));
        anchor
    }
    fn mine(&mut self, i: usize, kind: &str, biome: &str, count: usize) {
        for _ in 0..count {
            let anchor = self.deposit(kind, biome);
            self.stand(
                i,
                shared::world::object_center(
                    anchor,
                    worldgen::Footprint {
                        width: 1,
                        height: 1,
                    },
                ),
            );
            self.send(i, InventoryAction::MineObject { anchor })
                .unwrap();
        }
    }
    fn source(&self, i: usize, item: &str) -> SlotRef {
        let inv = self.world.get::<Inventory>(self.players[i]).unwrap();
        let (n, s) = inv
            .slots
            .iter()
            .enumerate()
            .find(|(_, s)| s.stack.as_ref().is_some_and(|s| s.item == item))
            .unwrap();
        SlotRef {
            inventory: (i + 1) as u64,
            slot: n as u16,
            expected_revision: s.revision,
        }
    }
    fn base(&mut self, i: usize, action: ExpeditionAction) {
        self.stand(i, Vec2::ZERO);
        self.act(i, action).unwrap();
    }
    fn recharge_station(&mut self) {
        let rules = &self.service.config.expedition;
        for _ in 0..25 {
            self.world.resource_mut::<Expedition>().advance(rules);
        }
    }
}

#[test]
fn two_crew_members_can_complete_the_loop_through_inventory_commands() {
    use ExpeditionAction::*;
    let mut crew = Crew::new();
    assert_eq!(crew.act(0, WorkSite(0)), Err(Error::RequiresDiscovery));
    assert_eq!(crew.act(0, Fabricate(2)), Err(Error::RequiresDiscovery));
    let battery = crew.world.resource::<Expedition>().battery;
    assert_eq!(crew.act(0, Fabricate(0)), Err(Error::MissingMaterials));
    assert_eq!(crew.world.resource::<Expedition>().battery, battery);
    crew.mine(0, "rock", "basalt_low", 5);
    crew.mine(0, "iron_ore", "basalt_low", 6);
    crew.base(0, Fuel);
    crew.base(1, Scan);
    crew.base(0, Fabricate(0));
    crew.base(1, Scan);
    crew.mine(0, "iron_ore", "plain_mid", 9);
    crew.mine(0, "large_rock", "plain_mid", 2);
    crew.base(0, Fabricate(1));
    crew.base(1, Scan);
    crew.visit(0, 0, 1);
    crew.visit(1, 0, 0);
    assert_eq!(crew.act(1, WorkSite(0)), Err(Error::Depleted));
    crew.visit(1, 1, 1);
    assert_eq!(crew.world.resource::<Expedition>().fragments, 3);

    crew.mine(0, "rock", "basalt_low", 4);
    crew.mine(0, "iron_ore", "plain_mid", 12);
    crew.mine(0, "large_rock", "plain_mid", 2);
    crew.mine(0, "large_rock", "sulfur_ridge", 4);
    crew.recharge_station();
    crew.base(0, Fabricate(2));
    crew.mine(0, "crystal", "snow_floor", 5);
    crew.mine(0, "iron_ore", "plain_mid", 8);
    crew.base(0, Fabricate(3));
    let suit = crew.source(0, SUITS[1]);
    crew.act(0, Equip(suit)).unwrap();
    crew.act(0, ChargeSuit).unwrap();
    crew.recharge_station();
    crew.base(1, Scan);
    assert!(crew.world.resource::<Expedition>().polar_scan);
    crew.visit(1, 2, 1);
    assert_eq!(
        crew.world
            .get::<Inventory>(crew.players[1])
            .unwrap()
            .used_slots(),
        14
    );
    crew.mine(1, "iron_ore", "plain_mid", 2);
    crew.mine(1, "large_rock", "plain_mid", 1);
    crew.base(1, Fabricate(4));
    assert!(crew.world.resource::<Expedition>().sos);
}

#[test]
fn cold_endurance_and_suit_energy_follow_elapsed_seconds() {
    let config = InventoryConfig::load().unwrap();
    let rules = &config.expedition;
    for (id, warm, cold) in [
        (TANKS[0], 180, 90),
        (TANKS[1], 360, 270),
        (TANKS[2], 450, 450),
    ] {
        for (temp, duration) in [(2.0, warm), (-2.0, cold)] {
            let outside = Environment {
                sheltered: false,
                celsius: temp,
            };
            let mut gear = LifeSupport::new(rules, 18.0);
            gear.tank.item = id.into();
            gear.tank.charge = rules.capacity(id);
            for _ in 0..duration {
                assert_eq!(breathe(&mut gear, rules, outside), 0);
            }
            assert_eq!(gear.tank.charge, 0);
            assert_eq!(breathe(&mut gear, rules, outside), 8);
            breathe(
                &mut gear,
                rules,
                Environment {
                    sheltered: true,
                    celsius: 18.0,
                },
            );
            assert_eq!(gear.tank.charge, 24 * OXYGEN_SECOND);
        }
    }
    let mut gear = LifeSupport::new(rules, 18.0);
    gear.suit = Some(ItemStack {
        item: SUITS[1].into(),
        quantity: 1,
        charge: 1,
    });
    assert_eq!(
        breathe(
            &mut gear,
            rules,
            Environment {
                sheltered: false,
                celsius: -42.5
            }
        ),
        0
    );
    assert_eq!(
        breathe(
            &mut gear,
            rules,
            Environment {
                sheltered: false,
                celsius: -42.5
            }
        ),
        4
    );
}

#[test]
fn charging_giving_and_swapping_preserve_individual_tanks() {
    let mut crew = Crew::new();
    crew.world
        .get_mut::<LifeSupport>(crew.players[0])
        .unwrap()
        .tank
        .charge = 77;
    crew.world
        .get_mut::<Inventory>(crew.players[0])
        .unwrap()
        .slots[0]
        .stack = Some(ItemStack {
        item: TANKS[1].into(),
        quantity: 1,
        charge: 0,
    });
    let source = crew.source(0, TANKS[1]);
    crew.act(0, ExpeditionAction::Equip(source)).unwrap();
    let source = crew.source(0, TANKS[0]);
    crew.send(
        0,
        InventoryAction::TransferContainerItem {
            source,
            destination: Destination::Auto(2),
            quantity: 1,
        },
    )
    .unwrap();
    let source = crew.source(1, TANKS[0]);
    crew.act(1, ExpeditionAction::Equip(source)).unwrap();
    assert_eq!(
        crew.world
            .get::<LifeSupport>(crew.players[1])
            .unwrap()
            .tank
            .charge,
        77
    );
    assert_eq!(
        crew.world
            .get::<LifeSupport>(crew.players[0])
            .unwrap()
            .tank
            .charge,
        0
    );
}

#[test]
fn night_requires_generation_and_duplicate_fuel_receipts_do_not_burn_extra_coal() {
    let mut crew = Crew::new();
    crew.mine(0, "rock", "plain_mid", 1);
    crew.stand(0, Vec2::ZERO);
    let daylight = crew.service.config.expedition.daylight;
    crew.world.resource_mut::<Expedition>().seconds = u32::from(daylight);
    assert_eq!(crew.act(0, ExpeditionAction::Scan), Err(Error::NoPower));
    assert_eq!(crew.world.resource::<Expedition>().scan, 0);
    crew.act(0, ExpeditionAction::Fuel).unwrap();
    let counter = crew.counters[0];
    let cmd = InventoryCommand {
        epoch: 1,
        counter,
        action: InventoryAction::Expedition(ExpeditionAction::Fuel),
    };
    crew.service
        .execute(
            &mut crew.world,
            1,
            &mut crew.sessions[0],
            &cmd,
            counter as f64,
        )
        .unwrap();
    assert_eq!(crew.world.resource::<Expedition>().fuel, 120);
    crew.act(0, ExpeditionAction::Scan).unwrap();
}

#[test]
fn a_fixed_tick_rescues_the_player_and_spills_the_beacon_at_the_loss_site() {
    use crate::net::{entity, messages::MessageBuffer, server::TickCounter};
    use shared::role::NetworkRole;
    let mut app = shared::app_init(NetworkRole::Server);
    app.init_resource::<shared::replication::BufferedComponentChanges>()
        .init_resource::<NetworkEntityMap>()
        .init_resource::<MessageBuffer>()
        .insert_resource(TickCounter(0));
    entity::plugin(&mut app);
    let config = InventoryConfig::load().unwrap();
    let mut gear = LifeSupport::new(&config.expedition, 18.0);
    gear.tank.charge = 0;
    let mut pack = config.inventory("player");
    pack.slots[0].stack = Some(ItemStack {
        item: BEACON.into(),
        quantity: 1,
        charge: 0,
    });
    let origin = Vec2::new(250000., 10.);
    let who = app
        .world_mut()
        .spawn((gear, pack, Health(1), Position(origin)))
        .id();
    app.insert_resource(WorldGenerator(std::sync::Arc::new(
        crate::game::world::test_support::test_generator(),
    )));
    app.insert_resource(Expedition::new(&config.expedition, 0, 256000));
    app.insert_resource(InventoryService::new(config));
    let mut time = Time::<Fixed>::from_hz(1.);
    time.advance_by(std::time::Duration::from_secs(1));
    app.insert_resource(time).add_systems(FixedPostUpdate, tick);
    app.world_mut().run_schedule(FixedPostUpdate);
    assert_eq!(app.world().get::<Position>(who).unwrap().0, Vec2::ZERO);
    assert_eq!(app.world().get::<Inventory>(who).unwrap().used_slots(), 0);
    assert_eq!(
        app.world().get::<LifeSupport>(who).unwrap().last_rescue,
        Some([250000, 10])
    );
    let (ground, position) = app
        .world_mut()
        .query::<(&GroundItem, &Position)>()
        .single(app.world())
        .unwrap();
    assert_eq!(ground.stack.item, BEACON);
    assert_eq!(position.0, origin);
}

#[test]
fn expedition_places_the_polar_module_inside_the_configured_world() {
    let config = InventoryConfig::load().unwrap();
    let generator = worldgen::WorldConfig::bundled().build().unwrap();
    let cfg = generator.config();
    let half = (u32::from(config.expedition.world_size_chunks) / 2 * cfg.chunk_size) as i32
        * shared::world::TILE_SIZE as i32;
    let mission = Expedition::new(&config.expedition, cfg.seed, half);
    let pole = config
        .expedition
        .sites
        .iter()
        .position(|site| site.kind == SiteKind::Pole)
        .unwrap();
    let distance = mission.sites[pole].position[1].abs();
    assert_eq!(distance, half - 512);
    let seconds = (distance as f32 - SITE_RADIUS) / 500.0;
    let tank = f32::from(config.expedition.tank_seconds[2]);
    assert!(seconds > tank && seconds < 2.0 * tank);
    assert!(
        mission
            .sites
            .iter()
            .all(|s| s.position.iter().all(|v| v.abs() < half))
    );
}

#[test]
fn mining_is_atomic_and_a_broken_object_stays_gone_after_reload() {
    let mut crew = Crew::new();
    let anchor = crew.deposit("iron_ore", "basalt_floor");
    let mine = || InventoryAction::MineObject { anchor };
    crew.stand(0, Vec2::splat(-1000.0));
    assert_eq!(crew.send(0, mine()), Err(Error::OutOfRange));
    crew.stand(0, Vec2::ZERO);
    for slot in &mut crew
        .world
        .get_mut::<Inventory>(crew.players[0])
        .unwrap()
        .slots
    {
        slot.stack = Some(ItemStack {
            item: "coal".into(),
            quantity: 50,
            charge: 0,
        });
    }
    assert_eq!(crew.send(0, mine()), Err(Error::InventoryFull));
    crew.send(1, mine()).unwrap();
    assert_eq!(crew.send(1, mine()), Err(Error::NotFound));
    assert_eq!(crew.send(0, mine()), Err(Error::NotFound));
    let inv = crew.world.get::<Inventory>(crew.players[1]).unwrap();
    assert_eq!(inv.slots[0].stack.as_ref().unwrap().quantity, 2);

    let observer = crew
        .world
        .query_filtered::<Entity, With<WorldObserver>>()
        .single(&crew.world)
        .unwrap();
    crew.world.despawn(observer);
    crew.world.run_schedule(FixedPreUpdate);
    crew.world.spawn((
        WorldObserver {
            simulation_distance: 0,
        },
        Position(Vec2::ZERO),
    ));
    crew.world.run_schedule(FixedPreUpdate);
    assert!(
        crew.world
            .resource::<ChunkStreamer>()
            .loaded(ChunkPos { x: 0, y: 0 })
            .is_some()
    );
    assert_eq!(crew.send(1, mine()), Err(Error::NotFound));
}

#[test]
fn local_exposure_matches_the_biome_map_at_each_players_tile() {
    use worldgen::{TilePos, WorldConfig};
    let config = InventoryConfig::load().unwrap();
    let rules = &config.expedition;
    let mut world = WorldConfig::bundled();
    world.world.seed = 73;
    world.world.world_size_chunks = 16;
    world.world.chunk_size = 32;
    let generator = world.build().unwrap();
    // Both positions are equally far from the ship. The map origin is tile (256, 256).
    let east = Vec2::new(7000.0, -0.5);
    let north = Vec2::new(-0.5, 7000.0);
    let east_mean = generator
        .tile_data_at(TilePos { x: 474, y: 255 })
        .temperature;
    let north_mean = generator
        .tile_data_at(TilePos { x: 255, y: 474 })
        .temperature;
    assert_ne!(east_mean, north_mean);
    let mut mission = Expedition::new(rules, 73, 8192);
    for (pos, expected) in [(east, east_mean), (north, north_mean)] {
        let environment = mission.environment_at(rules, &generator, pos);
        assert_eq!(environment.celsius, expected);
        let mut gear = LifeSupport::new(rules, 18.0);
        breathe(&mut gear, rules, environment);
        assert_eq!(
            mission.view(rules, &gear).gear.temperature_celsius,
            expected
        );
        assert_eq!(
            gear.tank.charge,
            rules.capacity(TANKS[0]) - oxygen_drain(TANKS[0], expected)
        );
    }
    mission.seconds = u32::from(rules.daylight);
    assert_eq!(
        mission.environment_at(rules, &generator, north).celsius,
        north_mean - 20.0
    );
    let hull_mean = generator
        .tile_data_at(TilePos { x: 256, y: 256 })
        .temperature;
    assert_eq!(
        mission
            .environment_at(rules, &generator, Vec2::ZERO)
            .celsius,
        hull_mean - 20.0
    );
    mission.fuel = 1;
    assert_eq!(
        mission
            .environment_at(rules, &generator, Vec2::ZERO)
            .celsius,
        18.0
    );
    let edge = generator.tile_data_at(TilePos { x: 0, y: 511 }).temperature;
    assert_eq!(
        mission
            .environment_at(rules, &generator, Vec2::new(-9000., 9000.))
            .celsius,
        edge - 20.0
    );
}
