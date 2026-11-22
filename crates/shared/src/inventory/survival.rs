//! The expedition catalog, shared action vocabulary and private instrument readout.
use super::{ItemStack, SlotRef};
use bevy::prelude::Component;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SHIP_RADIUS: f32 = 180.0;
pub const SITE_RADIUS: f32 = 240.0;
// Twelve units per warm second represent both cold penalties exactly.
pub const OXYGEN_SECOND: u16 = 12;
pub const BEACON: &str = "distress_beacon";
pub const TANKS: [&str; 3] = ["tank_t1", "tank_t2", "tank_t3"];
pub const SUITS: [&str; 2] = ["suit_t1", "suit_t2"];

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpeditionRules {
    #[serde(skip)]
    pub signature: u64,
    pub world_size_chunks: u16,
    pub daylight: u16,
    pub night: u16,
    pub coal_seconds: u16,
    pub crystal_seconds: u16,
    pub tank_seconds: [u16; 3],
    pub recipes: Vec<Recipe>,
    pub sites: Vec<Site>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    pub product: String,
    pub ingredients: Vec<(String, u32)>,
    pub power: u16,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Site {
    pub name: String,
    // World-unit offsets from the hull. Polar Y is calculated from the world bounds.
    pub offset: (i32, i32),
    pub scan: u8,
    pub kind: SiteKind,
    pub uses: u16,
    pub yields: Vec<(String, u32)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SiteKind {
    Crater,
    Quartz,
    Sulfur,
    Ice,
    Wreck,
    Pole,
}

impl ExpeditionRules {
    pub(super) fn parse(
        source: &str,
        items: &BTreeMap<String, super::definitions::ItemDefinition>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let mut rules: Self = ron::from_str(source)?;
        // Stable across whitespace and comments. Catalog indices are part of the wire protocol.
        rules.signature = postcard::to_allocvec(&rules)?
            .iter()
            .fold(0xcbf29ce484222325u64, |hash, byte| {
                (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
            });
        if rules.world_size_chunks < 2
            || rules.daylight == 0
            || rules.night == 0
            || rules.coal_seconds == 0
            || rules.crystal_seconds == 0
            || rules
                .tank_seconds
                .iter()
                .any(|s| *s == 0 || s.checked_mul(OXYGEN_SECOND).is_none())
        {
            return Err("invalid survival durations".into());
        }
        let valid_cost = |(id, qty): &(String, u32)| items.contains_key(id) && *qty > 0;
        for id in TANKS.into_iter().chain(SUITS).chain([BEACON]) {
            if items.get(id).is_none_or(|d| d.max_stack != 1) {
                return Err(format!("{id} must have max_stack 1").into());
            }
        }
        for id in ["coal", "cryo_crystal"] {
            if !items.contains_key(id) {
                return Err(format!("missing {id}").into());
            }
        }
        let mut products = std::collections::BTreeSet::new();
        for r in &rules.recipes {
            if ![TANKS[1], TANKS[2], SUITS[0], SUITS[1], BEACON].contains(&r.product.as_str())
                || !products.insert(r.product.as_str())
                || r.ingredients.is_empty()
                || !r.ingredients.iter().all(valid_cost)
                || r.power == 0
                || r.power > 100
            {
                return Err(format!("invalid recipe {}", r.product).into());
            }
        }
        if products.len() != 5
            || !rules.recipes.iter().any(|r| {
                r.product == BEACON && r.ingredients.iter().any(|(id, n)| id == BEACON && *n == 1)
            })
        {
            return Err("missing expedition recipe or beacon repair input".into());
        }
        if rules.sites.len() > u16::MAX as usize
            || rules
                .sites
                .iter()
                .filter(|s| s.kind == SiteKind::Wreck)
                .count()
                != 2
            || rules
                .sites
                .iter()
                .filter(|s| s.kind == SiteKind::Pole)
                .count()
                != 1
        {
            return Err("expected two blueprint wrecks and one polar wreck".into());
        }
        for s in &rules.sites {
            let wreck = matches!(s.kind, SiteKind::Wreck | SiteKind::Pole);
            if s.scan > 3
                || s.uses == 0
                || (wreck && (s.uses != 1 || !s.yields.is_empty()))
                || (!wreck && (s.yields.is_empty() || !s.yields.iter().all(valid_cost)))
            {
                return Err(format!("invalid expedition site {}", s.name).into());
            }
        }
        Ok(rules)
    }

    pub fn capacity(&self, tank: &str) -> u16 {
        TANKS
            .iter()
            .position(|id| *id == tank)
            .map_or(0, |i| self.tank_seconds[i] * OXYGEN_SECOND)
    }
}

#[derive(Component, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LifeSupport {
    /// Local exposure measured by the server during the last survival update.
    pub temperature_celsius: f64,
    pub tank: ItemStack,
    pub suit: Option<ItemStack>,
    pub rescues: u32,
    pub last_rescue: Option<[i32; 2]>,
}

impl LifeSupport {
    pub fn new(rules: &ExpeditionRules, temperature_celsius: f64) -> Self {
        Self {
            temperature_celsius,
            tank: ItemStack {
                item: TANKS[0].into(),
                quantity: 1,
                charge: rules.capacity(TANKS[0]),
            },
            suit: None,
            rescues: 0,
            last_rescue: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExpeditionAction {
    WorkSite(u16),
    Fuel,
    Scan,
    Fabricate(u16),
    Equip(SlotRef),
    ChargeSuit,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SiteContact {
    pub id: u16,
    pub position: [i32; 2],
    pub remaining: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExpeditionView {
    pub catalog: u64,
    pub gear: LifeSupport,
    pub solar: u8,
    pub fuel: u32,
    pub battery: u16,
    pub powered: bool,
    pub seconds_to_phase: u16,
    pub scan: u8,
    pub fragments: u8,
    pub scan_limit: u8,
    pub sos: bool,
    pub t3_built: bool,
    pub polar_scan: bool,
    pub final_suit_built: bool,
    pub contacts: Vec<SiteContact>,
}

pub fn oxygen_drain(tank: &str, celsius: f64) -> u16 {
    match (tank, celsius <= 0.0) {
        ("tank_t1", true) => 24,
        ("tank_t2", true) => 16,
        _ => OXYGEN_SECOND,
    }
}
