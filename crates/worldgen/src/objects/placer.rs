use std::collections::{HashMap, HashSet};

use super::rule::ResolvedObjectRule;
use super::{ObjectInstanceId, ObjectRepository, ObjectRule, ObjectsError, PlacedObject};
use crate::hash::hash_cell;
use crate::{BiomeRepository, GenerationError, TileData, TilePos, TileRect, WorldGeneratorConfig};

/// Immutable placement settings shared between chunk workers.
#[derive(Clone, Debug)]
pub struct ObjectPlacer {
    objects: ObjectRepository,
    rules: Vec<ResolvedObjectRule>,
    padding_x: i64,
    padding_y: i64,
}

struct Candidate {
    object: PlacedObject,
    bounds: TileRect,
    priority: u64,
}

impl Candidate {
    fn rank(&self) -> (u64, ObjectInstanceId) {
        (self.priority, self.object.id)
    }
}

impl ObjectPlacer {
    pub fn parse(biomes: &BiomeRepository) -> Self {
        Self::from_ron(
            include_str!("placement.ron"),
            ObjectRepository::parse(),
            biomes,
        )
        .expect("bundled object placement must be valid")
    }

    pub fn empty() -> Self {
        Self {
            objects: ObjectRepository::default(),
            rules: Vec::new(),
            padding_x: 0,
            padding_y: 0,
        }
    }

    pub fn from_ron(
        source: &str,
        objects: ObjectRepository,
        biomes: &BiomeRepository,
    ) -> Result<Self, ObjectsError> {
        Self::new(
            ron::from_str(source).map_err(ObjectsError::Parse)?,
            objects,
            biomes,
        )
    }

    pub fn new(
        definitions: Vec<ObjectRule>,
        objects: ObjectRepository,
        biomes: &BiomeRepository,
    ) -> Result<Self, ObjectsError> {
        let mut ids = HashSet::with_capacity(definitions.len());
        let mut rules = Vec::with_capacity(definitions.len());
        let (mut padding_x, mut padding_y) = (0, 0);
        for rule in definitions {
            if !ids.insert(rule.id) {
                return Err(ObjectsError::DuplicateRuleId(rule.id));
            }
            if !rule.density.is_finite() || !(0.0..=1.0).contains(&rule.density) {
                return Err(ObjectsError::InvalidRule {
                    id: rule.id,
                    field: "density",
                });
            }
            for (field, range) in [("height", rule.height), ("temperature", rule.temperature)] {
                if !range.is_valid() || (field == "height" && (range.min < 0.0 || range.max > 1.0))
                {
                    return Err(ObjectsError::InvalidRule { id: rule.id, field });
                }
            }
            if rule
                .max_height_difference
                .is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
            {
                return Err(ObjectsError::InvalidRule {
                    id: rule.id,
                    field: "max_height_difference",
                });
            }
            let object = objects
                .id(&rule.object)
                .ok_or_else(|| ObjectsError::UnknownObject(rule.object.clone()))?;
            let footprint = objects.get(object).unwrap().footprint;
            let allowed = rule
                .biomes
                .into_iter()
                .map(|name| biomes.id(&name).ok_or(ObjectsError::UnknownBiome(name)))
                .collect::<Result<Vec<_>, _>>()?;
            if rule.density > 0.0 {
                padding_x = padding_x.max(i64::from(footprint.width) - 1);
                padding_y = padding_y.max(i64::from(footprint.height) - 1);
            }
            rules.push(ResolvedObjectRule {
                id: rule.id,
                object,
                footprint,
                density: rule.density,
                biomes: allowed,
                height: rule.height,
                temperature: rule.temperature,
                max_height_difference: rule.max_height_difference,
            });
        }
        Ok(Self {
            objects,
            rules,
            padding_x,
            padding_y,
        })
    }

    pub fn objects(&self) -> &ObjectRepository {
        &self.objects
    }

    /// Return complete object instances intersecting these half-open tile bounds.
    /// The caller can cache terrain samples locally to reuse core chunk data.
    /// Decisions depend only on global coordinates, settings, and terrain data.
    pub fn place_in(
        &self,
        bounds: TileRect,
        config: WorldGeneratorConfig,
        mut sample: impl FnMut(TilePos) -> Result<TileData, GenerationError>,
    ) -> Result<Vec<PlacedObject>, GenerationError> {
        let world = config.tile_bounds()?;
        if !world.contains_rect(bounds) {
            return Err(GenerationError::InvalidRegion);
        }

        // An intersecting object's anchor lies in [min-padding, max).
        // Its competitors can start another padding away on either side.
        // Their complete footprints fit in a terrain margin of 2*padding.
        let anchors = TileRect {
            min: TilePos {
                x: bounds
                    .min
                    .x
                    .saturating_sub(2 * self.padding_x)
                    .max(world.min.x),
                y: bounds
                    .min
                    .y
                    .saturating_sub(2 * self.padding_y)
                    .max(world.min.y),
            },
            max: TilePos {
                x: bounds.max.x.saturating_add(self.padding_x).min(world.max.x),
                y: bounds.max.y.saturating_add(self.padding_y).min(world.max.y),
            },
        };

        let seed =
            config.seed ^ 0x4F42_4A45_4354_5300 ^ (u64::from(config.generator_version) << 32);
        let mut candidates = Vec::new();
        for rule in &self.rules {
            if rule.density == 0.0 {
                continue;
            }
            let rule_seed = hash_cell(seed, i64::from(rule.id), 0);
            for anchor in anchors.positions() {
                let roll = hash_cell(rule_seed ^ 0x5350_4157_4E00, anchor.x, anchor.y);
                let probability = (roll >> 11) as f64 / ((1_u64 << 53) as f64);
                if probability >= rule.density {
                    continue;
                }
                let Some(footprint_bounds) = rule.footprint.bounds(anchor) else {
                    continue;
                };
                if !world.contains_rect(footprint_bounds) {
                    continue;
                }
                let mut low = f64::INFINITY;
                let mut high = f64::NEG_INFINITY;
                let mut suitable = true;
                for pos in footprint_bounds.positions() {
                    let data = sample(pos)?;
                    if !rule.allows(&data) {
                        suitable = false;
                        break;
                    }
                    low = low.min(data.height);
                    high = high.max(data.height);
                    if rule
                        .max_height_difference
                        .is_some_and(|maximum| high - low > maximum)
                    {
                        suitable = false;
                        break;
                    }
                }
                if suitable {
                    candidates.push(Candidate {
                        object: PlacedObject {
                            id: ObjectInstanceId {
                                rule_id: rule.id,
                                anchor,
                            },
                            kind: rule.object,
                            footprint: rule.footprint,
                        },
                        bounds: footprint_bounds,
                        priority: hash_cell(rule_seed ^ 0x5052_494F_5249_5459, anchor.x, anchor.y),
                    });
                }
            }
        }
        Ok(resolve_candidates(candidates, bounds))
    }
}

fn resolve_candidates(candidates: Vec<Candidate>, bounds: TileRect) -> Vec<PlacedObject> {
    // Index every eligible proposal, including proposals that later lose.
    // This makes rejection local; acceptance never depends on a recursive chain.
    let mut occupancy: HashMap<TilePos, Vec<usize>> = HashMap::new();
    for (index, candidate) in candidates.iter().enumerate() {
        for pos in candidate.bounds.positions() {
            occupancy.entry(pos).or_default().push(index);
        }
    }
    let mut accepted = Vec::new();
    for candidate in &candidates {
        if !candidate.bounds.intersects(bounds) {
            continue;
        }
        let blocked = candidate.bounds.positions().any(|pos| {
            occupancy[&pos]
                .iter()
                .any(|&other| candidates[other].rank() > candidate.rank())
        });
        if !blocked {
            accepted.push(candidate.object.clone());
        }
    }
    accepted.sort_unstable_by_key(|object| object.id);
    accepted
}
