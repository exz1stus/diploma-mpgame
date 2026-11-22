//! Colours for biomes, objects and scalar fields. Figures only; the game uses textures.

pub type Rgb = [u8; 3];

const BIOMES: &[(&str, &str)] = &[
    ("permanent_polar_ice", "#f4f8fb"),
    ("fractured_ice_basins", "#c9dbe8"),
    ("polar_ice_highlands", "#e6eef5"),
    ("buried_ice_basins", "#a9c3d6"),
    ("blue_ice_plains", "#8fb8d8"),
    ("sintered_frost_fields", "#c3d3dc"),
    ("glacier_ridges", "#dfe9ef"),
    ("cold_trap_ice", "#7fa3bd"),
    ("frosted_crater_floors", "#9fb4c2"),
    ("ice_cemented_regolith", "#b4c0c6"),
    ("rime_covered_ridges", "#d2dadd"),
    ("frozen_shadow_basins", "#6f8796"),
    ("frost_margin_regolith", "#9aa5aa"),
    ("frosted_breccia", "#b0b3b0"),
    ("patchy_ice_highlands", "#c9cbc6"),
    ("dark_crater_floors", "#4b4f55"),
    ("salt_crust_pans", "#d8d2c0"),
    ("mixed_ice_regolith", "#a3a39b"),
    ("silicate_gravel_plains", "#b5ab95"),
    ("bright_ejecta_ridges", "#d9d4c7"),
    ("carbonaceous_basins", "#3e3a36"),
    ("ochre_dust_plains", "#c49a5a"),
    ("weathered_basalt", "#6d665c"),
    ("exposed_anorthosite", "#cfc8b8"),
    ("dark_basalt_depressions", "#3a3836"),
    ("iron_rich_regolith", "#9c5b3b"),
    ("rocky_crater_uplands", "#8f877a"),
    ("sunlit_crater_floors", "#a58e6a"),
    ("copper_toned_scree", "#b5774a"),
    ("barren_silicate_ridges", "#c8b89a"),
    ("warm_exposed_rock", "#a8683f"),
    ("hot_dark_regolith", "#5e3426"),
    ("basin", "#b9a27c"),
    ("volcano", "#7a2e24"),
    ("crater", "#5d5953"),
    ("frozen_basin", "#a7b6bd"),
    ("frozen_volcano", "#7d5d63"),
    ("frozen_crater", "#8696a0"),
    ("polar_crater", "#c4d2dc"),
    ("crater_ejecta", "#c7beb0"),
    ("frozen_crater_ejecta", "#d5dde2"),
    ("polar_basin", "#dbe6ec"),
    ("basin_salt_flats", "#e3dcc6"),
    ("weathered_basin", "#c29a6b"),
    ("polar_volcano", "#b7a9b5"),
    ("volcanic_highlands", "#3a1c1a"),
    ("frozen_volcanic_highlands", "#5a4248"),
];

pub fn biome_hex(name: &str) -> &'static str {
    BIOMES
        .iter()
        .find(|(biome, _)| *biome == name)
        .map_or("#888888", |(_, hex)| hex)
}

pub fn biome_rgb(name: &str) -> Rgb {
    hex_rgb(biome_hex(name))
}

pub fn object_rgb(name: &str) -> Rgb {
    match name {
        "rock" => [255, 214, 64],
        "large_rock" => [255, 128, 16],
        "iron_ore" => [25, 211, 240],
        "tree" => [60, 208, 112],
        _ => [255, 0, 255],
    }
}

pub fn object_hex(name: &str) -> String {
    let [r, g, b] = object_rgb(name);
    format!("#{r:02x}{g:02x}{b:02x}")
}

pub const TERRITORY: [(&str, &str); 4] = [
    ("no feature", "#e2e8f0"),
    ("basin", "#d69e2e"),
    ("crater", "#3182ce"),
    ("volcano", "#c53030"),
];

const ELEVATION: [(f64, &str); 6] = [
    (0.0, "#1c1b22"),
    (0.3, "#45404f"),
    (0.45, "#7d7468"),
    (0.55, "#a89a80"),
    (0.7, "#d4c4a0"),
    (1.0, "#ffffff"),
];

const TEMPERATURE: [(f64, &str); 5] = [
    (-50.0, "#2c3e91"),
    (-25.0, "#6fa8dc"),
    (0.0, "#f7f7f7"),
    (20.0, "#f4a261"),
    (40.0, "#9b2226"),
];

pub fn elevation_rgb(height: f64) -> Rgb {
    gradient(&ELEVATION, height)
}

const DELTA: [(f64, &str); 5] = [
    (-0.3, "#08306b"),
    (-0.1, "#6baed6"),
    (0.0, "#f7f7f7"),
    (0.1, "#fc9272"),
    (0.3, "#67000d"),
];

/// Diverging colours for a height change in [-0.3, 0.3].
pub fn delta_rgb(delta: f64) -> Rgb {
    gradient(&DELTA, delta)
}

pub fn temperature_rgb(celsius: f64) -> Rgb {
    gradient(&TEMPERATURE, celsius)
}

pub fn shade(color: Rgb, light: f64) -> Rgb {
    color.map(|c| (f64::from(c) * light).clamp(0.0, 255.0) as u8)
}

fn gradient(stops: &[(f64, &str)], value: f64) -> Rgb {
    let value = value.clamp(stops[0].0, stops[stops.len() - 1].0);
    let upper = stops
        .iter()
        .position(|(at, _)| *at >= value)
        .unwrap_or(stops.len() - 1)
        .max(1);
    let (a, b) = (stops[upper - 1], stops[upper]);
    let t = (value - a.0) / (b.0 - a.0);
    let (ca, cb) = (hex_rgb(a.1), hex_rgb(b.1));
    std::array::from_fn(|i| (f64::from(ca[i]) + (f64::from(cb[i]) - f64::from(ca[i])) * t) as u8)
}

fn hex_rgb(hex: &str) -> Rgb {
    let channel = |i: usize| u8::from_str_radix(&hex[1 + 2 * i..3 + 2 * i], 16).unwrap_or(0);
    [channel(0), channel(1), channel(2)]
}
