//! Turns the spawns declared per biome, per object set and per feature into placement rules.

use super::{ObjectPlacer, ObjectRepository, ObjectRule, ObjectsError};
use crate::hash::hash_name;
use crate::{BiomeRepository, BiomesFile, FeatureKind, ObjectSpawn, ObjectsFile};

/// The spawns one feature file declares.
pub type FeatureSpawns<'a> = (FeatureKind, &'a [ObjectSpawn]);

impl ObjectPlacer {
    /// Placement for the objects declared in `biomes.ron`, `objects.ron` and `features/*.ron`.
    pub fn from_config(
        biomes_file: &BiomesFile,
        objects_file: &ObjectsFile,
        features: &[FeatureSpawns],
        biomes: &BiomeRepository,
    ) -> Result<Self, ObjectsError> {
        let mut rules = spawn_rules(biomes_file, objects_file)?;
        rules.extend(
            features
                .iter()
                .flat_map(|&(kind, spawns)| feature_rules(kind, spawns)),
        );
        Self::new(
            rules,
            ObjectRepository::new(objects_file.kinds.clone())?,
            biomes,
        )
    }
}

/// One rule per spawn. A biome's own spawn is limited to that biome; a set's spawn
/// may cover every biome including the set, so its footprint can straddle them.
pub fn spawn_rules(
    biomes: &BiomesFile,
    objects: &ObjectsFile,
) -> Result<Vec<ObjectRule>, ObjectsError> {
    check_set_references(biomes, objects)?;
    let own = biomes.biomes.iter().flat_map(|biome| {
        biome
            .objects
            .iter()
            .map(|spawn| rule("biome", &biome.name, spawn, vec![biome.name.clone()]))
    });
    // A set no biome includes spawns nothing (an empty biome list would mean "everywhere").
    let shared = objects.sets.iter().flat_map(|(set, spawns)| {
        let members = biomes_including(biomes, set);
        let spawns = if members.is_empty() { &[][..] } else { spawns };
        spawns
            .iter()
            .map(move |spawn| rule("set", set, spawn, members.clone()))
    });
    Ok(own.chain(shared).collect())
}

/// A feature's spawns apply in any biome, but only inside that feature's territory.
pub fn feature_rules(
    kind: FeatureKind,
    spawns: &[ObjectSpawn],
) -> impl Iterator<Item = ObjectRule> {
    spawns.iter().map(move |spawn| ObjectRule {
        feature: Some(kind),
        ..rule("feature", kind.name(), spawn, Vec::new())
    })
}

/// Stable rule ID: changes only when the source or object is renamed.
pub fn rule_id(source_kind: &str, source: &str, object: &str) -> u32 {
    hash_name(&format!("{source_kind}/{source}/{object}")) as u32
}

fn check_set_references(biomes: &BiomesFile, objects: &ObjectsFile) -> Result<(), ObjectsError> {
    let unknown = biomes
        .biomes
        .iter()
        .flat_map(|biome| biomes.object_sets_of(biome))
        .find(|set| !objects.sets.contains_key(*set));
    match unknown {
        Some(set) => Err(ObjectsError::UnknownSet(set.clone())),
        None => Ok(()),
    }
}

fn biomes_including(biomes: &BiomesFile, set: &str) -> Vec<String> {
    biomes
        .biomes
        .iter()
        .filter(|biome| biomes.object_sets_of(biome).iter().any(|name| name == set))
        .map(|biome| biome.name.clone())
        .collect()
}

/// Biome and set spawns carry no feature; a `distance` on them is rejected by the placer.
fn rule(source_kind: &str, source: &str, spawn: &ObjectSpawn, biomes: Vec<String>) -> ObjectRule {
    ObjectRule {
        id: rule_id(source_kind, source, &spawn.object),
        object: spawn.object.clone(),
        density: spawn.density,
        biomes,
        height: spawn.height,
        temperature: spawn.temperature,
        max_height_difference: spawn.max_height_difference,
        feature: None,
        distance: spawn.distance,
    }
}
