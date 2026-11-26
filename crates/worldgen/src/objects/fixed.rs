//! Fixed objects (`placements.ron`): each is placed exactly once per world at a named site,
//! whatever the terrain there, and keeps random objects off its footprint and a clearance
//! around it.

use serde::{Deserialize, Serialize};

use super::{ObjectInstanceId, ObjectRepository, ObjectsError, PlacedObject, repeated_rule_id};
use crate::{TilePos, TileRect, WorldGeneratorConfig};

/// `placements.ron`: the objects placed once per world.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacementsFile {
    #[serde(default)]
    pub placements: Vec<FixedPlacement>,
}

/// One object placed once per world; its footprint is centred on the site plus `offset`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixedPlacement {
    pub object: String,
    pub at: PlacementSite,
    /// Tiles from the site to the footprint's centre.
    #[serde(default)]
    pub offset: (i64, i64),
    /// Tiles around the footprint kept free of random objects.
    #[serde(default)]
    pub clearance: u32,
}

/// Where a fixed object goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlacementSite {
    /// The tile the game centres its map on, where players spawn.
    Spawn,
    /// On the spawn's meridian, `inset` tiles in from the north or south edge. The seed picks
    /// the pole: the south edge (largest y) for an even number of set seed bits, the north
    /// edge otherwise, like the expedition's polar site.
    Pole { inset: u32 },
}

impl PlacementSite {
    pub fn tile(self, config: WorldGeneratorConfig) -> TilePos {
        let spawn = spawn_tile(config);
        match self {
            Self::Spawn => spawn,
            Self::Pole { inset } => TilePos {
                x: spawn.x,
                y: pole_row(config, inset),
            },
        }
    }
}

/// The game maps chunk `world_size_chunks / 2` to its origin (see the server's `WorldFrame`).
fn spawn_tile(config: WorldGeneratorConfig) -> TilePos {
    let half = i64::from(config.world_size_chunks / 2) * i64::from(config.chunk_size);
    TilePos { x: half, y: half }
}

fn pole_row(config: WorldGeneratorConfig, inset: u32) -> i64 {
    let side = i64::from(config.world_size_chunks) * i64::from(config.chunk_size);
    let inset = i64::from(inset);
    if config.seed.count_ones().is_multiple_of(2) {
        side - inset
    } else {
        inset
    }
}

/// A fixed object as placed, with the ground it keeps clear of random objects.
#[derive(Clone, Debug)]
pub(super) struct FixedObject {
    pub object: PlacedObject,
    pub bounds: TileRect,
    pub cleared: TileRect,
}

/// Places every fixed object in a world of these dimensions.
pub(super) fn resolve_fixed(
    placements: &[FixedPlacement],
    objects: &ObjectRepository,
    config: WorldGeneratorConfig,
) -> Result<Vec<FixedObject>, ObjectsError> {
    placements
        .iter()
        .enumerate()
        .map(|(index, placement)| {
            let repeat = placements[..index]
                .iter()
                .filter(|earlier| earlier.object == placement.object)
                .count();
            resolve(placement, repeat, objects, config)
        })
        .collect()
}

fn resolve(
    placement: &FixedPlacement,
    repeat: usize,
    objects: &ObjectRepository,
    config: WorldGeneratorConfig,
) -> Result<FixedObject, ObjectsError> {
    let outside = || ObjectsError::PlacementOutsideWorld(placement.object.clone());
    let kind = objects
        .id(&placement.object)
        .ok_or_else(|| ObjectsError::UnknownObject(placement.object.clone()))?;
    let footprint = objects
        .get(kind)
        .expect("IDs come from the repository")
        .footprint;
    let world = config.tile_bounds().map_err(|_| outside())?;
    let site = placement.at.tile(config);
    let centre = TilePos {
        x: site.x + placement.offset.0,
        y: site.y + placement.offset.1,
    };
    let anchor = TilePos {
        x: centred_start(centre.x, footprint.width, world.min.x, world.max.x)
            .ok_or_else(outside)?,
        y: centred_start(centre.y, footprint.height, world.min.y, world.max.y)
            .ok_or_else(outside)?,
    };
    let bounds = footprint.bounds(anchor).ok_or_else(outside)?;
    Ok(FixedObject {
        object: PlacedObject {
            id: ObjectInstanceId {
                rule_id: repeated_rule_id("placement", "world", &placement.object, repeat),
                anchor,
            },
            kind,
            footprint,
        },
        bounds,
        cleared: grow(bounds, placement.clearance),
    })
}

/// Where a footprint of `length` tiles starts when centred on `centre`, moved back inside
/// `[min, max)` if it would reach past an end; `None` if it is longer than that.
fn centred_start(centre: i64, length: u32, min: i64, max: i64) -> Option<i64> {
    let last = max - i64::from(length);
    (last >= min).then(|| (centre - i64::from(length / 2)).clamp(min, last))
}

fn grow(rect: TileRect, by: u32) -> TileRect {
    let by = i64::from(by);
    TileRect {
        min: TilePos {
            x: rect.min.x - by,
            y: rect.min.y - by,
        },
        max: TilePos {
            x: rect.max.x + by,
            y: rect.max.y + by,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn objects() -> ObjectRepository {
        ObjectRepository::from_ron(
            r#"[
            (name: "hull", footprint: (width: 6, height: 4)),
            (name: "engine", footprint: (width: 3, height: 2)),
        ]"#,
        )
        .unwrap()
    }

    fn config(seed: u64) -> WorldGeneratorConfig {
        WorldGeneratorConfig {
            seed,
            chunk_size: 8,
            world_size_chunks: 8,
            ..Default::default()
        }
    }

    fn placements(source: &str) -> Vec<FixedPlacement> {
        crate::config::parse_ron::<PlacementsFile>(source)
            .unwrap()
            .placements
    }

    #[test]
    fn spawn_centres_the_footprint_on_the_game_origin_plus_offset() {
        let fixed = resolve_fixed(
            &placements(
                "(placements: [(object: \"hull\", at: Spawn, offset: (0, 5), clearance: 2)])",
            ),
            &objects(),
            config(0),
        )
        .unwrap();
        // The game origin is tile (32, 32); the 6×4 footprint centres on (32, 37).
        assert_eq!(fixed[0].object.id.anchor, TilePos { x: 29, y: 35 });
        assert_eq!(fixed[0].cleared.min, TilePos { x: 27, y: 33 });
        assert_eq!(fixed[0].cleared.max, TilePos { x: 37, y: 41 });
    }

    #[test]
    fn the_seed_picks_the_pole_and_footprints_stay_inside_the_world() {
        let engine = |seed, inset| {
            let source = format!("(placements: [(object: \"engine\", at: Pole(inset: {inset}))])");
            resolve_fixed(&placements(&source), &objects(), config(seed)).unwrap()[0]
                .object
                .id
                .anchor
        };
        // 0b11 has an even number of set bits: south edge (largest y), 64 tiles down.
        assert_eq!(engine(0b11, 10), TilePos { x: 31, y: 53 });
        assert_eq!(engine(0b1, 10), TilePos { x: 31, y: 9 });
        // Without an inset the footprint is pushed back inside the world.
        assert_eq!(engine(0b11, 0).y, 62);
        assert_eq!(engine(0b1, 0).y, 0);
    }

    #[test]
    fn unknown_objects_and_objects_larger_than_the_world_are_rejected() {
        let missing = placements("(placements: [(object: \"wing\", at: Spawn)])");
        assert!(matches!(
            resolve_fixed(&missing, &objects(), config(0)),
            Err(ObjectsError::UnknownObject(_))
        ));
        let tiny = WorldGeneratorConfig {
            chunk_size: 2,
            world_size_chunks: 2,
            ..config(0)
        };
        let hull = placements("(placements: [(object: \"hull\", at: Spawn)])");
        assert!(matches!(
            resolve_fixed(&hull, &objects(), tiny),
            Err(ObjectsError::PlacementOutsideWorld(_))
        ));
    }

    #[test]
    fn repeated_objects_get_distinct_stable_ids() {
        let fixed = resolve_fixed(
            &placements(
                "(placements: [(object: \"engine\", at: Spawn), (object: \"engine\", at: Pole(inset: 4))])",
            ),
            &objects(),
            config(0),
        )
        .unwrap();
        assert_ne!(fixed[0].object.id.rule_id, fixed[1].object.id.rule_id);
        assert_eq!(
            fixed[0].object.id.rule_id,
            repeated_rule_id("placement", "world", "engine", 0)
        );
    }
}
