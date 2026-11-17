//! Local-only HTTP adapter for the engine-independent worldgen library.
use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex, mpsc},
    time::{SystemTime, UNIX_EPOCH},
};

use worldgen::{
    BasinSettings, BiomeId, BiomeMap, BiomeMapDefinition, BiomeRepository, CraterGenerator,
    FbmNoise, NoiseConfig, NoiseSource, PlanetParams, Range, TemperatureCoefficients,
    TerrainSources, TilePos, VolcanoGenerator, WorldConfig, WorldGenerator, WorldGeneratorConfig,
};

#[derive(serde::Serialize)]
struct Control {
    key: &'static str,
    label: &'static str,
    group: &'static str,
    default: Option<f64>,
    min: f64,
    max: f64,
    step: &'static str,
}

fn controls() -> Vec<Control> {
    let config = WorldConfig::bundled();
    let world = config.world.dimensions();
    let temperature = config.world.climate.coefficients;
    let planet = config.world.climate.planet;
    let elevation = config.world.elevation;
    let region = config.world.climate.region_noise;
    let craters = config.craters;
    let basins = config.basins;
    let volcanoes = config.volcanoes;
    let mut controls = Vec::new();
    let mut add = |key, label, group, default, min, max, step| {
        controls.push(Control {
            key,
            label,
            group,
            default,
            min,
            max,
            step,
        });
    };
    add(
        "seed",
        "World seed",
        "World",
        Some(world.seed as f64),
        0.0,
        u32::MAX as f64,
        "1",
    );
    add(
        "size",
        "Chunk size (tiles)",
        "World",
        Some(world.chunk_size as f64),
        4.0,
        128.0,
        "1",
    );
    add(
        "frequency",
        "Terrain coordinate scale",
        "World",
        Some(0.025),
        0.000001,
        1.0,
        "any",
    );
    add(
        "elevation_seed",
        "Seed override",
        "Elevation noise",
        None,
        0.0,
        u32::MAX as f64,
        "1",
    );
    add(
        "elevation_frequency",
        "Frequency (per tile)",
        "Elevation noise",
        Some(elevation.frequency),
        0.00000001,
        1.0,
        "any",
    );
    add(
        "elevation_octaves",
        "Octaves",
        "Elevation noise",
        Some(elevation.octaves as f64),
        1.0,
        12.0,
        "1",
    );
    add(
        "elevation_lacunarity",
        "Lacunarity",
        "Elevation noise",
        Some(elevation.lacunarity),
        0.1,
        8.0,
        "any",
    );
    add(
        "elevation_persistence",
        "Persistence",
        "Elevation noise",
        Some(elevation.persistence),
        0.01,
        1.0,
        "any",
    );
    add(
        "region_seed",
        "Seed override",
        "Temperature region noise",
        None,
        0.0,
        u32::MAX as f64,
        "1",
    );
    add(
        "region_frequency",
        "Frequency (per tile)",
        "Temperature region noise",
        Some(region.frequency),
        0.00000001,
        1.0,
        "any",
    );
    add(
        "region_octaves",
        "Octaves",
        "Temperature region noise",
        Some(region.octaves as f64),
        1.0,
        12.0,
        "1",
    );
    add(
        "region_lacunarity",
        "Lacunarity",
        "Temperature region noise",
        Some(region.lacunarity),
        0.1,
        8.0,
        "any",
    );
    add(
        "region_persistence",
        "Persistence",
        "Temperature region noise",
        Some(region.persistence),
        0.01,
        1.0,
        "any",
    );
    add(
        "temperature_base",
        "Base (C)",
        "Temperature coefficients",
        Some(temperature.base),
        -1000.0,
        1000.0,
        "any",
    );
    add(
        "temperature_solar",
        "Solar heating",
        "Temperature coefficients",
        Some(temperature.solar),
        -1000.0,
        1000.0,
        "any",
    );
    add(
        "temperature_altitude",
        "Altitude cooling (C/km)",
        "Temperature coefficients",
        Some(temperature.altitude),
        -1000.0,
        1000.0,
        "any",
    );
    add(
        "temperature_region",
        "Regional variation (C)",
        "Temperature coefficients",
        Some(temperature.region),
        -1000.0,
        1000.0,
        "any",
    );
    add(
        "orbit",
        "Orbital phase (0-1)",
        "Planet",
        Some(planet.orbit),
        0.0,
        1.0,
        "any",
    );
    add(
        "axial_tilt",
        "Axial tilt (degrees)",
        "Planet",
        Some(planet.axial_tilt_deg),
        0.0,
        90.0,
        "any",
    );
    add(
        "crater_cell_size",
        "Cell size (tiles)",
        "Craters",
        Some(craters.cell_size as f64),
        32.0,
        4096.0,
        "1",
    );
    add(
        "crater_min_radius",
        "Minimum radius (tiles)",
        "Craters",
        Some(craters.min_radius),
        1.0,
        2048.0,
        "any",
    );
    add(
        "crater_max_radius",
        "Maximum radius (tiles)",
        "Craters",
        Some(craters.max_radius),
        1.0,
        2048.0,
        "any",
    );
    add(
        "crater_density",
        "Craters per cell",
        "Craters",
        Some(craters.density),
        0.0,
        4.0,
        "any",
    );
    add(
        "basin_cell_size",
        "Cell size (tiles)",
        "Impact basins",
        Some(basins.cell_size as f64),
        128.0,
        65536.0,
        "1",
    );
    add(
        "basin_probability",
        "Probability per cell",
        "Impact basins",
        Some(basins.probability),
        0.0,
        1.0,
        "any",
    );
    add(
        "basin_min_radius",
        "Minimum radius (tiles)",
        "Impact basins",
        Some(basins.min_radius),
        1.0,
        16384.0,
        "any",
    );
    add(
        "basin_max_radius",
        "Maximum radius (tiles)",
        "Impact basins",
        Some(basins.max_radius),
        1.0,
        16384.0,
        "any",
    );
    add(
        "basin_min_depth",
        "Minimum depth",
        "Impact basins",
        Some(basins.min_depth),
        0.0,
        1.0,
        "any",
    );
    add(
        "basin_max_depth",
        "Maximum depth",
        "Impact basins",
        Some(basins.max_depth),
        0.0,
        1.0,
        "any",
    );
    add(
        "basin_min_rim_height",
        "Minimum rim height",
        "Impact basins",
        Some(basins.min_rim_height),
        0.0,
        1.0,
        "any",
    );
    add(
        "basin_max_rim_height",
        "Maximum rim height",
        "Impact basins",
        Some(basins.max_rim_height),
        0.0,
        1.0,
        "any",
    );
    add(
        "basin_smoothing",
        "Interior smoothing",
        "Impact basins",
        Some(basins.smoothing),
        0.0,
        1.0,
        "any",
    );
    add(
        "basin_coarse_octaves",
        "Smoothing octaves",
        "Impact basins",
        Some(basins.smoothing_octaves as f64),
        1.0,
        12.0,
        "1",
    );
    for (key, label, default, min, max, step) in [
        (
            "volcano_cell_size",
            "Cell size (tiles)",
            volcanoes.cell_size as f64,
            32.0,
            65536.0,
            "1",
        ),
        (
            "volcano_probability",
            "Probability per cell",
            volcanoes.probability,
            0.0,
            1.0,
            "any",
        ),
        (
            "volcano_min_radius",
            "Minimum radius (tiles)",
            volcanoes.min_radius,
            1.0,
            16384.0,
            "any",
        ),
        (
            "volcano_max_radius",
            "Maximum radius (tiles)",
            volcanoes.max_radius,
            1.0,
            16384.0,
            "any",
        ),
        (
            "volcano_min_height",
            "Minimum cone height",
            volcanoes.min_height,
            0.0,
            1.0,
            "any",
        ),
        (
            "volcano_max_height",
            "Maximum cone height",
            volcanoes.max_height,
            0.0,
            1.0,
            "any",
        ),
        (
            "volcano_caldera_ratio",
            "Caldera radius / cone radius",
            volcanoes.caldera_ratio,
            0.0,
            0.9,
            "any",
        ),
        (
            "volcano_caldera_depth",
            "Caldera depth",
            volcanoes.caldera_depth,
            0.0,
            1.0,
            "any",
        ),
    ] {
        add(key, label, "Volcanoes", Some(default), min, max, step);
    }
    for (key, group, default) in [
        (
            "crater_temperature_offset",
            "Craters",
            craters.temperature_offset,
        ),
        (
            "basin_temperature_offset",
            "Impact basins",
            basins.temperature_offset,
        ),
        (
            "volcano_temperature_offset",
            "Volcanoes",
            volcanoes.temperature_offset,
        ),
    ] {
        add(
            key,
            "Peak temperature offset (°C)",
            group,
            Some(default),
            -1000.0,
            1000.0,
            "any",
        );
    }
    controls
}

/// The world config with the preview's URL controls applied, plus the adapter-only
/// controls (coordinate frequency and independent noise seeds).
struct PreviewSettings {
    world: WorldConfig,
    frequency: f64,
    elevation_seed: u64,
    region_seed: u64,
}

impl PreviewSettings {
    fn from_params(params: &HashMap<&str, &str>) -> Result<Self, String> {
        let values = control_values(params)?;
        let mut world = WorldConfig::bundled();
        apply_world_controls(&mut world, &values);
        apply_crater_controls(&mut world.craters, &values)?;
        apply_basin_controls(&mut world.basins, &values)?;
        apply_volcano_controls(&mut world.volcanoes, &values)?;
        world.biome_map = selected_biomes(params, &bundled_biomes())?.1;
        let seed = world.world.seed;
        Ok(Self {
            world,
            frequency: values["frequency"],
            elevation_seed: values
                .get("elevation_seed")
                .map_or(seed, |seed| *seed as u64),
            region_seed: values
                .get("region_seed")
                .map_or(seed ^ 0x5445_4d50, |seed| *seed as u64),
        })
    }
}

fn bundled_biomes() -> BiomeRepository {
    BiomeRepository::parse()
}

/// Every control's value: from the query, else its default. Rejects out-of-range values.
fn control_values(params: &HashMap<&str, &str>) -> Result<HashMap<&'static str, f64>, String> {
    let mut values = HashMap::new();
    for control in controls() {
        let value = if params.contains_key(control.key) {
            Some(parse::<f64>(params, control.key)?)
        } else {
            control.default
        };
        let Some(value) = value else { continue };
        let integer = control.step == "1";
        if !value.is_finite()
            || value < control.min
            || value > control.max
            || (integer && value.fract() != 0.0)
        {
            return Err(format!(
                "Invalid {}: expected {} to {}{}",
                control.key,
                control.min,
                control.max,
                if integer { " (integer)" } else { "" }
            ));
        }
        values.insert(control.key, value);
    }
    Ok(values)
}

fn noise_controls(values: &HashMap<&str, f64>, prefix: &str) -> NoiseConfig {
    NoiseConfig {
        frequency: values[format!("{prefix}_frequency").as_str()],
        octaves: values[format!("{prefix}_octaves").as_str()] as usize,
        lacunarity: values[format!("{prefix}_lacunarity").as_str()],
        persistence: values[format!("{prefix}_persistence").as_str()],
    }
}

fn apply_world_controls(config: &mut WorldConfig, values: &HashMap<&str, f64>) {
    let world = &mut config.world;
    world.seed = values["seed"] as u64;
    world.chunk_size = values["size"] as u32;
    world.elevation = noise_controls(values, "elevation");
    world.climate.region_noise = noise_controls(values, "region");
    world.climate.coefficients = TemperatureCoefficients {
        base: values["temperature_base"],
        solar: values["temperature_solar"],
        altitude: values["temperature_altitude"],
        region: values["temperature_region"],
    };
    world.climate.planet = PlanetParams {
        orbit: values["orbit"],
        axial_tilt_deg: values["axial_tilt"],
    };
}

fn apply_crater_controls(
    craters: &mut CraterGenerator,
    values: &HashMap<&str, f64>,
) -> Result<(), String> {
    craters.cell_size = values["crater_cell_size"] as i64;
    craters.min_radius = values["crater_min_radius"];
    craters.max_radius = values["crater_max_radius"];
    craters.density = values["crater_density"];
    craters.temperature_offset = values["crater_temperature_offset"];
    if craters.min_radius > craters.max_radius
        || craters.max_radius * 1.5 > craters.cell_size as f64
    {
        return Err(
            "Crater radii must satisfy minimum <= maximum and maximum * 1.5 <= cell size".into(),
        );
    }
    Ok(())
}

fn apply_basin_controls(
    basins: &mut BasinSettings,
    values: &HashMap<&str, f64>,
) -> Result<(), String> {
    basins.cell_size = values["basin_cell_size"] as i64;
    basins.probability = values["basin_probability"];
    basins.min_radius = values["basin_min_radius"];
    basins.max_radius = values["basin_max_radius"];
    basins.min_depth = values["basin_min_depth"];
    basins.max_depth = values["basin_max_depth"];
    basins.min_rim_height = values["basin_min_rim_height"];
    basins.max_rim_height = values["basin_max_rim_height"];
    basins.smoothing = values["basin_smoothing"];
    basins.smoothing_octaves = values["basin_coarse_octaves"] as usize;
    basins.temperature_offset = values["basin_temperature_offset"];
    if basins.min_radius > basins.max_radius
        || basins.max_radius * 1.5 > basins.cell_size as f64
        || basins.min_depth > basins.max_depth
        || basins.min_rim_height > basins.max_rim_height
    {
        return Err("Basin minimums must not exceed maximums, and maximum radius * 1.5 must fit within the cell size".into());
    }
    Ok(())
}

fn apply_volcano_controls(
    volcanoes: &mut VolcanoGenerator,
    values: &HashMap<&str, f64>,
) -> Result<(), String> {
    volcanoes.cell_size = values["volcano_cell_size"] as i64;
    volcanoes.probability = values["volcano_probability"];
    volcanoes.min_radius = values["volcano_min_radius"];
    volcanoes.max_radius = values["volcano_max_radius"];
    volcanoes.min_height = values["volcano_min_height"];
    volcanoes.max_height = values["volcano_max_height"];
    volcanoes.caldera_ratio = values["volcano_caldera_ratio"];
    volcanoes.caldera_depth = values["volcano_caldera_depth"];
    volcanoes.temperature_offset = values["volcano_temperature_offset"];
    if volcanoes.min_radius > volcanoes.max_radius
        || volcanoes.max_radius > volcanoes.cell_size as f64
        || volcanoes.min_height > volcanoes.max_height
    {
        return Err("Volcano minimums must not exceed maximums, and maximum radius must fit within the cell size".into());
    }
    Ok(())
}

// Preview controls are an adapter concern, separate from the world configuration.
struct PreviewNoise {
    source: FbmNoise,
    frequency: f64,
}

impl NoiseSource for PreviewNoise {
    fn sample(&self, x: f64, y: f64) -> f64 {
        self.source.sample(x * self.frequency, y * self.frequency)
    }
}

fn preview_generator(settings: &PreviewSettings) -> WorldGenerator {
    let world = &settings.world.world;
    let scaled = |config| PreviewNoise {
        source: FbmNoise::new(settings.elevation_seed, config),
        frequency: settings.frequency,
    };
    let mut basin_floor = world.elevation;
    basin_floor.octaves = settings
        .world
        .basins
        .smoothing_octaves
        .min(world.elevation.octaves);
    settings
        .world
        .build_with(TerrainSources {
            elevation: Box::new(scaled(world.elevation)),
            basin_floor: Box::new(scaled(basin_floor)),
            temperature_regions: Box::new(FbmNoise::new(
                settings.region_seed,
                world.climate.region_noise,
            )),
        })
        .expect("validated preview controls produce a valid world config")
}

fn main() -> std::io::Result<()> {
    let address =
        std::env::var("WORLDGEN_PREVIEW_ADDR").unwrap_or_else(|_| "127.0.0.1:4174".to_owned());
    let listener = TcpListener::bind(&address)?;
    let build = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let workers = std::thread::available_parallelism()
        .map_or(4, |count| count.get())
        .clamp(2, 16);
    let (sender, receiver) = mpsc::sync_channel::<TcpStream>(workers * 2);
    let receiver = Arc::new(Mutex::new(receiver));
    for _ in 0..workers {
        let receiver = Arc::clone(&receiver);
        std::thread::spawn(move || {
            loop {
                let stream = receiver.lock().unwrap().recv();
                let Ok(mut stream) = stream else { break };
                if let Err(error) = handle(&mut stream, build) {
                    eprintln!("preview request failed: {error}");
                }
            }
        });
    }
    println!("Worldgen API: http://{address} ({workers} workers)");
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if sender.send(stream).is_err() {
                    break;
                }
            }
            Err(error) => eprintln!("preview connection failed: {error}"),
        }
    }
    Ok(())
}

fn handle(stream: &mut TcpStream, build: u128) -> std::io::Result<()> {
    let mut request = [0u8; 8192];
    let size = stream.read(&mut request)?;
    let first_line = String::from_utf8_lossy(&request[..size]);
    let Some(target) = first_line
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
    else {
        return respond(stream, 400, "text/plain", b"Bad request");
    };
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    match path {
        "/api/meta" => match metadata(query, build) {
            Ok(body) => respond(stream, 200, "application/json", body.as_bytes()),
            Err(error) => respond(stream, 400, "text/plain", error.as_bytes()),
        },
        "/api/region" => match region_bytes(query) {
            Ok(body) => respond(stream, 200, "application/octet-stream", &body),
            Err(error) => respond(stream, 400, "text/plain", error.as_bytes()),
        },
        "/api/chunk" => match chunk_json(query) {
            Ok(body) => respond(stream, 200, "application/json", body.as_bytes()),
            Err(error) => respond(stream, 400, "text/plain", error.as_bytes()),
        },
        _ => respond(stream, 404, "text/plain", b"Not found"),
    }
}

fn respond(stream: &mut TcpStream, status: u16, mime: &str, body: &[u8]) -> std::io::Result<()> {
    let header = format!(
        "HTTP/1.1 {status} {}\r\nContent-Type: {mime}; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        if status == 200 { "OK" } else { "Error" },
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body)
}

const SAMPLE_BYTES: usize = 16;

fn chunk_json(query: &str) -> Result<String, String> {
    let params = params(query);
    let position = worldgen::ChunkPos {
        x: parse(&params, "x")?,
        y: parse(&params, "y")?,
    };
    let settings = PreviewSettings::from_params(&params)?;
    let chunk = preview_generator(&settings)
        .chunk_at(position)
        .map_err(|error| error.to_string())?;
    serde_json::to_string(&chunk).map_err(|error| error.to_string())
}

fn params(query: &str) -> HashMap<&str, &str> {
    query
        .split('&')
        .filter_map(|part| part.split_once('='))
        .collect()
}

fn parse<T: std::str::FromStr>(params: &HashMap<&str, &str>, key: &str) -> Result<T, String> {
    params
        .get(key)
        .ok_or_else(|| format!("Missing {key}"))?
        .parse()
        .map_err(|_| format!("Invalid {key}"))
}

fn request_config(params: &HashMap<&str, &str>) -> Result<WorldGeneratorConfig, String> {
    let mut config = WorldGeneratorConfig::default();
    if params.contains_key("seed") {
        config.seed = parse(params, "seed")?;
    }
    if params.contains_key("size") {
        config.chunk_size = parse(params, "size")?;
    }
    if !(4..=128).contains(&config.chunk_size) {
        return Err("Chunk size must be between 4 and 128".into());
    }
    world_side(config)?;
    Ok(config)
}

fn world_side(config: WorldGeneratorConfig) -> Result<u32, String> {
    config
        .world_size_chunks
        .checked_mul(config.chunk_size)
        .filter(|side| *side > 0)
        .ok_or_else(|| "Invalid world dimensions".into())
}

fn metadata(query: &str, build: u128) -> Result<String, String> {
    let params = params(query);
    let config = request_config(&params)?;
    let side = world_side(config)?;
    let biomes = bundled_biomes();
    let (biome_file, biome_map) = selected_biomes(&params, &biomes)?;
    let biome_map = biome_map
        .resolve(&biomes)
        .map_err(|error| format!("{biome_file}: {error}"))?;
    let colors = biome_colors(&biomes)?;
    let map_rules = |map: &BiomeMap| -> Vec<serde_json::Value> {
        map.rules()
            .iter()
            .map(|rule| {
                let biome = biomes.get(rule.biome).expect("resolved biome exists");
                serde_json::json!({ "name": biome.name, "biome": rule.biome,
                "height": rule.height, "temperature": rule.temperature })
            })
            .collect()
    };
    let bundled = WorldConfig::bundled();
    let mut feature_maps = serde_json::Map::new();
    for (name, definition) in [
        ("Basins", bundled.basins.biome_map),
        ("Craters", bundled.craters.biome_map),
        ("Volcanoes", bundled.volcanoes.biome_map),
    ] {
        let map = definition
            .resolve(&biomes)
            .map_err(|error| error.to_string())?;
        feature_maps.insert(name.into(), serde_json::json!(map_rules(&map)));
    }
    Ok(serde_json::json!({
        "build": build.to_string(), "protocol": 6,
        "world": { "width": side, "height": side }, "controls": controls(),
        "biome_files": biome_files()?, "biome_file": biome_file, "biomes": biomes.biomes(),
        "biome_rules": map_rules(&biome_map), "feature_biome_maps": feature_maps,
        "tile_palette": biome_palette(&biomes, &colors), "tile_rules": biome_tile_rules(&biomes),
        "object_types": bundled.objects.kinds,
    })
    .to_string())
}

/// The selectable terrain biome map: `config/biome_map.ron`, read on every request so
/// edits show up without restarting.
const DEFAULT_BIOME_FILE: &str = "default.ron";

fn biome_map_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("config/biome_map.ron")
}

fn biome_files() -> Result<Vec<String>, String> {
    Ok(vec![DEFAULT_BIOME_FILE.to_owned()])
}

/// Flat preview colors: the client's placeholder colors for each biome's texture.
#[derive(serde::Deserialize)]
struct TilePresentation {
    #[serde(default)]
    textures: HashMap<String, String>,
    #[serde(default)]
    placeholder_colors: HashMap<String, String>,
    default_placeholder: String,
}

fn client_assets() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../client/assets")
}

fn presentation_path() -> std::path::PathBuf {
    client_assets().join("world/tiles.ron")
}

/// Flat preview look of each biome, from the client's tile presentation.
struct BiomeLooks {
    /// RGB per biome ID: the client's placeholder color of the biome's texture.
    colors: Vec<[u8; 3]>,
    /// Per biome ID: whether the biome has tiles of its own, i.e. `tiles.ron` lists it or
    /// its texture file exists. Biomes without draw the ground biome's tiles, as in the client.
    own_tiles: Vec<bool>,
}

impl BiomeLooks {
    fn load(biomes: &BiomeRepository) -> Result<Self, String> {
        let path = presentation_path();
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
        let presentation: TilePresentation =
            ron::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
        let textures: Vec<&str> = biomes
            .biomes()
            .iter()
            .map(|biome| presentation.texture_for(&biome.name))
            .collect();
        Ok(Self {
            colors: textures
                .iter()
                .map(|texture| color_rgb(presentation.placeholder(texture)))
                .collect::<Result<_, _>>()?,
            own_tiles: biomes
                .biomes()
                .iter()
                .zip(&textures)
                .map(|(biome, texture)| {
                    presentation.lists(&biome.name) || tile_texture_exists(texture)
                })
                .collect(),
        })
    }

    /// The biome whose tiles a tile shows: its own, or its ground's.
    fn drawn(&self, biome: BiomeId, ground: BiomeId) -> BiomeId {
        if self.own_tiles.get(biome.index()).copied().unwrap_or(false) {
            biome
        } else {
            ground
        }
    }
}

impl TilePresentation {
    fn texture_for<'a>(&'a self, name: &'a str) -> &'a str {
        self.textures.get(name).map_or(name, String::as_str)
    }

    fn placeholder(&self, texture: &str) -> &str {
        self.placeholder_colors
            .get(texture)
            .unwrap_or(&self.default_placeholder)
    }

    fn lists(&self, name: &str) -> bool {
        self.textures.contains_key(name)
            || self.placeholder_colors.contains_key(self.texture_for(name))
    }
}

fn tile_texture_exists(texture: &str) -> bool {
    client_assets()
        .join(format!("textures/tiles/{texture}.png"))
        .is_file()
}

/// RGB per biome ID.
fn biome_colors(biomes: &BiomeRepository) -> Result<Vec<[u8; 3]>, String> {
    Ok(BiomeLooks::load(biomes)?.colors)
}

/// In protocol 6 terms, every biome is its own "tile": palette entry `i` is biome `i`.
fn biome_palette(biomes: &BiomeRepository, colors: &[[u8; 3]]) -> Vec<serde_json::Value> {
    biomes
        .biomes()
        .iter()
        .zip(colors)
        .map(|(biome, [r, g, b])| {
            serde_json::json!({ "name": biome.name, "color": format!("#{r:02X}{g:02X}{b:02X}") })
        })
        .collect()
}

fn biome_tile_rules(biomes: &BiomeRepository) -> Vec<serde_json::Value> {
    (0..biomes.biomes().len())
        .map(|id| {
            serde_json::json!({ "tile": id, "biome": id,
            "height": Range::full_height(), "temperature": Range::unbounded() })
        })
        .collect()
}

fn decode_query_value(value: &str) -> Result<String, String> {
    let mut decoded = Vec::new();
    let mut bytes = value.bytes();
    while let Some(byte) = bytes.next() {
        decoded.push(match byte {
            b'+' => b' ',
            b'%' => {
                let hi = bytes.next().and_then(|b| (b as char).to_digit(16));
                let lo = bytes.next().and_then(|b| (b as char).to_digit(16));
                match (hi, lo) {
                    (Some(hi), Some(lo)) => (hi * 16 + lo) as u8,
                    _ => return Err("Invalid biome filename encoding".into()),
                }
            }
            _ => byte,
        });
    }
    String::from_utf8(decoded).map_err(|_| "Invalid biome filename encoding".into())
}

fn selected_biomes(
    params: &HashMap<&str, &str>,
    repository: &BiomeRepository,
) -> Result<(String, BiomeMapDefinition), String> {
    let filename = decode_query_value(params.get("biome").unwrap_or(&DEFAULT_BIOME_FILE))?;
    // Keep previously shared preview URLs working after the default map rename.
    let filename = if filename == "biomes.ron" {
        DEFAULT_BIOME_FILE.to_owned()
    } else {
        filename
    };
    if !biome_files()?.contains(&filename) {
        return Err(format!("Choose the biome map {DEFAULT_BIOME_FILE}"));
    }
    let source = std::fs::read_to_string(biome_map_path())
        .map_err(|error| format!("Cannot read {filename}: {error}"))?;
    let definition =
        BiomeMapDefinition::from_ron(&source).map_err(|error| format!("{filename}: {error}"))?;
    definition
        .resolve(repository)
        .map_err(|error| format!("{filename}: {error}"))?;
    Ok((filename, definition))
}

fn color_rgb(color: &str) -> Result<[u8; 3], String> {
    let hex = color
        .strip_prefix('#')
        .ok_or_else(|| format!("Invalid tile color: {color}"))?;
    let expanded;
    let hex = if hex.len() == 3 && hex.is_ascii() {
        expanded = hex.chars().flat_map(|c| [c, c]).collect::<String>();
        expanded.as_str()
    } else {
        hex
    };
    if hex.len() != 6 || !hex.is_ascii() {
        return Err(format!("Expected #RGB or #RRGGBB tile color, got {color}"));
    }
    let mut rgb = [0; 3];
    for (index, channel) in rgb.iter_mut().enumerate() {
        *channel = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16)
            .map_err(|_| format!("Invalid tile color: {color}"))?;
    }
    Ok(rgb)
}

// Wire record: status u8 (0=unclassified, 1=colored biome, 255=outside), RGB u8[3],
// normalized height f32 LE, Celsius temperature f32 LE, biome ID u16 LE, tile ID u16 LE.
// Appearance belongs to the client: the tile ID is the biome whose tiles the client draws
// (the biome, or its ground biome if it has no tiles of its own) and the color is that
// biome's placeholder color in `crates/client/assets/world/tiles.ron`.
fn region_bytes(query: &str) -> Result<Vec<u8>, String> {
    let params = params(query);
    let min_x: i32 = parse(&params, "min_x")?;
    let min_y: i32 = parse(&params, "min_y")?;
    let max_x: i32 = parse(&params, "max_x")?;
    let max_y: i32 = parse(&params, "max_y")?;
    let lod: u32 = parse(&params, "lod")?;
    let settings = PreviewSettings::from_params(&params)?;
    let config = settings.world.world.dimensions();
    let width = i64::from(max_x) - i64::from(min_x) + 1;
    let height = i64::from(max_y) - i64::from(min_y) + 1;
    let tile_count = width
        .checked_mul(height)
        .and_then(|count| count.checked_mul(i64::from(config.chunk_size).pow(2)))
        .unwrap_or(i64::MAX);
    if width <= 0 || height <= 0 || tile_count > 2_000_000 || lod > 60 {
        return Err("Invalid region or generation settings".into());
    }
    let side = i128::from(world_side(config)?);
    let size = config.chunk_size;
    let stride = 1_i128 << lod;
    let span = i128::from(size) * stride;
    let max_chunk = (side - 1) / span;
    if min_x < 0 || min_y < 0 || i128::from(max_x) > max_chunk || i128::from(max_y) > max_chunk {
        return Err("Region lies outside the world".into());
    }
    let generator = preview_generator(&settings);
    let looks = BiomeLooks::load(generator.biomes())?;
    let offset = if lod == 0 { 0 } else { stride / 2 };
    let mut output = Vec::with_capacity(tile_count as usize * SAMPLE_BYTES);
    for cy in min_y..=max_y {
        for cx in min_x..=max_x {
            for y in 0..size {
                for x in 0..size {
                    let base_x = (i128::from(cx) * i128::from(size) + i128::from(x)) * stride;
                    let base_y = (i128::from(cy) * i128::from(size) + i128::from(y)) * stride;
                    if base_x >= side || base_y >= side {
                        output.extend_from_slice(&[255, 0, 0, 0]);
                        output.extend_from_slice(&f32::NAN.to_le_bytes());
                        output.extend_from_slice(&f32::NAN.to_le_bytes());
                        output.extend_from_slice(&[0; 4]);
                        continue;
                    }
                    let pos = TilePos {
                        x: (base_x + offset).min(side - 1) as i64,
                        y: (base_y + offset).min(side - 1) as i64,
                    };
                    let tile = generator.draft_at(pos);
                    let drawn = tile
                        .biome
                        .map(|biome| looks.drawn(biome, tile.ground.unwrap_or(biome)));
                    let color = drawn.and_then(|biome| looks.colors.get(biome.index()));
                    match color {
                        Some(rgb) => {
                            output.push(1);
                            output.extend_from_slice(rgb);
                        }
                        None => output.extend_from_slice(&[0; 4]),
                    }
                    let biome = tile.biome.map_or(0, |id| id.0);
                    output.extend_from_slice(&(tile.height as f32).to_le_bytes());
                    output.extend_from_slice(&(tile.temperature as f32).to_le_bytes());
                    output.extend_from_slice(&biome.to_le_bytes());
                    // The "tile" of a sample is the biome it is drawn as (see `biome_palette`).
                    output.extend_from_slice(&drawn.map_or(0, |id| id.0).to_le_bytes());
                }
            }
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EQUATOR_QUERY: &str = "min_x=253&max_x=253&min_y=250&max_y=250&lod=0&size=4&seed=42";

    #[test]
    fn single_chunk_api_returns_tiles_and_complete_object_instances() {
        let query = "x=250&y=250&size=32&seed=42&temperature_base=0&temperature_solar=0&temperature_region=0&temperature_altitude=0&crater_density=0&basin_probability=0&volcano_probability=0";
        let chunk: worldgen::ChunkData = serde_json::from_str(&chunk_json(query).unwrap()).unwrap();
        assert_eq!(chunk.position, worldgen::ChunkPos { x: 250, y: 250 });
        assert_eq!(chunk.tiles.len(), 32 * 32);
        assert!(!chunk.objects.is_empty());
        let settings = PreviewSettings::from_params(&params(query)).unwrap();
        let generator = preview_generator(&settings);
        assert_eq!(chunk, generator.chunk_at(chunk.position).unwrap());
        assert_eq!(chunk_json(query).unwrap(), chunk_json(query).unwrap());
        let region = region_bytes(&format!(
            "{query}&min_x=250&max_x=250&min_y=250&max_y=250&lod=0"
        ))
        .unwrap();
        for (tile, record) in chunk.tiles.iter().zip(region.chunks_exact(SAMPLE_BYTES)) {
            assert_eq!(record[0], 1);
            assert_eq!(
                tile.biome.0,
                u16::from_le_bytes(record[14..16].try_into().unwrap())
            );
            let height = f32::from_le_bytes(record[4..8].try_into().unwrap());
            assert!((tile.height() - height).abs() < 1e-4);
        }
    }

    #[test]
    fn single_chunk_api_rejects_invalid_requests_and_reports_missing_biomes() {
        for query in [
            "",
            "x=0",
            "x=-1&y=0",
            "x=500&y=0",
            "x=0&y=0&size=0",
            "x=9223372036854775807&y=0",
        ] {
            assert!(chunk_json(query).is_err(), "{query}");
        }
        let error = chunk_json("x=0&y=0&size=4&temperature_base=1000&temperature_solar=0&temperature_region=0&temperature_altitude=0&crater_density=0&basin_probability=0&volcano_probability=0").unwrap_err();
        assert!(error.contains("no biome"), "{error}");
    }

    fn first_metrics(query: &str) -> (f32, f32) {
        let bytes = region_bytes(query).unwrap();
        (
            f32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            f32::from_le_bytes(bytes[8..12].try_into().unwrap()),
        )
    }

    const BASIN_QUERY: &str = "min_x=0&max_x=0&min_y=0&max_y=0&lod=9&size=16&seed=42&crater_density=0&basin_cell_size=4096&basin_probability=1&basin_min_radius=800&basin_max_radius=1500&basin_min_depth=0.06&basin_max_depth=0.18&basin_min_rim_height=0.005&basin_max_rim_height=0.02&basin_smoothing=0.8";

    #[test]
    fn crater_and_basin_territories_report_their_biome_and_the_ground_tiles() {
        let colors = biome_colors(&bundled_biomes()).unwrap();
        for query in [
            "min_x=0&max_x=0&min_y=0&max_y=0&lod=7&size=16&seed=42&crater_density=4&basin_probability=0&volcano_probability=0",
            BASIN_QUERY,
        ] {
            let query = format!(
                "{query}&volcano_probability=0&temperature_base=0&temperature_solar=0&temperature_region=0&temperature_altitude=0"
            );
            let bytes = region_bytes(&query).unwrap();
            let mut marked = 0;
            for record in bytes.chunks_exact(SAMPLE_BYTES) {
                assert_eq!(record[0], 1);
                let biome = u16::from_le_bytes(record[12..14].try_into().unwrap());
                if biome >= 32 {
                    marked += 1;
                }
                let tile = u16::from_le_bytes(record[14..16].try_into().unwrap());
                assert!(
                    tile < 32,
                    "crater and basin biomes are drawn with terrain tiles"
                );
                assert_eq!(record[1..4], colors[usize::from(tile)]);
            }
            assert!(
                marked > 0,
                "feature territory must be represented in the API"
            );
        }
    }

    #[test]
    fn feature_temperature_controls_change_temperature_without_changing_height() {
        for (prefix, query) in [
            (
                "crater",
                "min_x=0&max_x=0&min_y=0&max_y=0&lod=7&size=16&seed=42&crater_density=4&basin_probability=0&volcano_probability=0",
            ),
            ("basin", BASIN_QUERY),
            (
                "volcano",
                "min_x=0&max_x=0&min_y=0&max_y=0&lod=5&size=32&seed=42&crater_density=0&basin_probability=0&volcano_cell_size=1024&volcano_probability=1&volcano_min_radius=256&volcano_max_radius=512",
            ),
        ] {
            let baseline = region_bytes(query).unwrap();
            for offset in [-20.0, 20.0] {
                let changed =
                    region_bytes(&format!("{query}&{prefix}_temperature_offset={offset}")).unwrap();
                let mut affected = false;
                for (a, b) in baseline
                    .chunks_exact(SAMPLE_BYTES)
                    .zip(changed.chunks_exact(SAMPLE_BYTES))
                {
                    assert_eq!(
                        &a[4..8],
                        &b[4..8],
                        "temperature control must preserve height"
                    );
                    let before = f32::from_le_bytes(a[8..12].try_into().unwrap());
                    let after = f32::from_le_bytes(b[8..12].try_into().unwrap());
                    if before != after {
                        affected = true;
                        assert!((after - before) * offset > 0.0);
                    }
                }
                assert!(
                    affected,
                    "{prefix} temperature control must reach generation"
                );
            }
        }
    }

    #[test]
    fn volcano_controls_change_terrain_and_zero_probability_disables_the_layer() {
        let query = "min_x=0&max_x=0&min_y=0&max_y=0&lod=5&size=32&seed=42&crater_density=0&basin_probability=0&volcano_cell_size=1024&volcano_probability=1&volcano_min_radius=256&volcano_max_radius=512&volcano_min_height=0.1&volcano_max_height=0.25&volcano_caldera_ratio=0.3&volcano_caldera_depth=0.08";
        let original = region_bytes(query).unwrap();
        assert_eq!(original, region_bytes(query).unwrap());
        for setting in [
            "volcano_cell_size=2048",
            "volcano_probability=0",
            "volcano_min_radius=128",
            "volcano_max_radius=768",
            "volcano_min_height=0.2",
            "volcano_max_height=0.4",
            "volcano_caldera_ratio=0",
            "volcano_caldera_depth=0.2",
        ] {
            let changed = region_bytes(&format!("{query}&{setting}")).unwrap();
            assert!(
                original
                    .chunks_exact(SAMPLE_BYTES)
                    .zip(changed.chunks_exact(SAMPLE_BYTES))
                    .any(|(a, b)| a[4..8] != b[4..8]),
                "{setting} must affect height"
            );
        }
        let disabled = region_bytes(&format!("{query}&volcano_probability=0")).unwrap();
        let zero_height = region_bytes(&format!(
            "{query}&volcano_min_height=0&volcano_max_height=0&volcano_caldera_depth=0"
        ))
        .unwrap();
        for (a, b) in disabled
            .chunks_exact(SAMPLE_BYTES)
            .zip(zero_height.chunks_exact(SAMPLE_BYTES))
        {
            assert_eq!(
                &a[4..12],
                &b[4..12],
                "flat features preserve terrain metrics"
            );
        }
        assert!(
            disabled
                .chunks_exact(SAMPLE_BYTES)
                .zip(zero_height.chunks_exact(SAMPLE_BYTES))
                .any(|(a, b)| a[12..14] != b[12..14]),
            "flat feature territory must still be API-marked"
        );
    }

    #[test]
    fn every_basin_control_changes_preview_terrain() {
        let original = region_bytes(BASIN_QUERY).unwrap();
        for setting in [
            "basin_cell_size=8192",
            "basin_probability=0",
            "basin_min_radius=400",
            "basin_max_radius=2400",
            "basin_min_depth=0.1",
            "basin_max_depth=0.3",
            "basin_min_rim_height=0.01",
            "basin_max_rim_height=0.05",
            "basin_smoothing=0",
            "basin_coarse_octaves=1",
        ] {
            let changed = region_bytes(&format!("{BASIN_QUERY}&{setting}")).unwrap();
            assert!(
                original
                    .chunks_exact(SAMPLE_BYTES)
                    .zip(changed.chunks_exact(SAMPLE_BYTES))
                    .any(|(a, b)| a[4..8] != b[4..8]),
                "{setting} must affect height"
            );
        }
    }

    #[test]
    fn basin_smoothing_uses_the_same_elevation_seed_and_coordinate_scale() {
        let query = format!(
            "{BASIN_QUERY}&elevation_seed=123&frequency=0.125&basin_min_depth=0&basin_max_depth=0&basin_min_rim_height=0&basin_max_rim_height=0&basin_smoothing=1"
        );
        let disabled = region_bytes(&format!("{query}&basin_probability=0")).unwrap();
        let matching_source = region_bytes(&format!("{query}&basin_coarse_octaves=12")).unwrap();
        // Asking for more coarse octaves than elevation has is capped to match. Terrain
        // metrics must be identical; basin territory still changes biome (and its color).
        for (a, b) in disabled
            .chunks_exact(SAMPLE_BYTES)
            .zip(matching_source.chunks_exact(SAMPLE_BYTES))
        {
            assert_eq!(a[0], b[0]);
            assert_eq!(&a[4..12], &b[4..12]);
        }
        let coarse = region_bytes(&format!("{query}&basin_coarse_octaves=1")).unwrap();
        assert_ne!(disabled, coarse);
    }

    #[test]
    fn basin_height_changes_feed_average_temperature() {
        let query = format!(
            "{BASIN_QUERY}&temperature_base=20&temperature_solar=0&temperature_region=0&temperature_altitude=10"
        );
        let bytes = region_bytes(&query).unwrap();
        for record in bytes.chunks_exact(SAMPLE_BYTES) {
            let height = f32::from_le_bytes(record[4..8].try_into().unwrap());
            let temperature = f32::from_le_bytes(record[8..12].try_into().unwrap());
            assert!((temperature - (20.0 - 10.0 * height)).abs() < 0.00001);
        }
    }

    #[test]
    fn preview_uses_average_temperature_and_exposes_crater_controls() {
        let meta: serde_json::Value = serde_json::from_str(&metadata("", 123).unwrap()).unwrap();
        let keys: Vec<_> = meta["controls"]
            .as_array()
            .unwrap()
            .iter()
            .map(|control| control["key"].as_str().unwrap())
            .collect();
        assert!(!keys.contains(&"time"));
        assert!(!keys.contains(&"instant"));
        assert!(!keys.contains(&"temperature_daily"));
        assert!(keys.contains(&"crater_density"));
        assert!(keys.contains(&"crater_cell_size"));
        assert!(keys.contains(&"basin_probability"));
        assert!(keys.contains(&"basin_coarse_octaves"));
        assert!(keys.contains(&"volcano_probability"));
        assert!(keys.contains(&"volcano_caldera_depth"));
        let baseline = region_bytes(EQUATOR_QUERY).unwrap();
        assert_eq!(
            baseline,
            region_bytes(&format!(
                "{EQUATOR_QUERY}&instant=1&time=0.25&temperature_daily=100"
            ))
            .unwrap()
        );
        let coarse = "min_x=0&max_x=0&min_y=0&max_y=0&lod=7&size=16&seed=42";
        assert_ne!(
            region_bytes(&format!("{coarse}&crater_density=0")).unwrap(),
            region_bytes(&format!("{coarse}&crater_density=4")).unwrap()
        );
    }

    #[test]
    fn noise_controls_change_their_own_output() {
        let original = first_metrics(EQUATOR_QUERY);
        for setting in [
            "elevation_seed=17",
            "elevation_frequency=0.02",
            "elevation_octaves=2",
            "elevation_lacunarity=2",
            "elevation_persistence=0.8",
            "frequency=0.1",
        ] {
            let changed = first_metrics(&format!("{EQUATOR_QUERY}&{setting}"));
            assert_ne!(changed.0, original.0, "{setting} must affect elevation");
        }
        for setting in [
            "region_seed=17",
            "region_frequency=0.02",
            "region_octaves=2",
            "region_lacunarity=2",
            "region_persistence=0.8",
        ] {
            let changed = first_metrics(&format!("{EQUATOR_QUERY}&{setting}"));
            assert_eq!(changed.0, original.0, "{setting} must preserve elevation");
            assert_ne!(changed.1, original.1, "{setting} must affect temperature");
        }
    }

    #[test]
    fn temperature_coefficients_and_planet_controls_reach_the_model() {
        let original = first_metrics(EQUATOR_QUERY);
        for setting in [
            "temperature_base=-20",
            "temperature_solar=200",
            "temperature_altitude=20",
            "temperature_region=50",
            "orbit=0",
            "axial_tilt=45",
        ] {
            let changed = first_metrics(&format!("{EQUATOR_QUERY}&{setting}"));
            assert_eq!(changed.0, original.0);
            assert_ne!(changed.1, original.1, "{setting} must affect temperature");
        }
        let warmer = first_metrics(&format!("{EQUATOR_QUERY}&temperature_base=-20"));
        assert!((warmer.1 - original.1 - 10.0).abs() < 0.00001);
    }

    #[test]
    fn seed_overrides_are_independent_and_blank_defaults_follow_world_seed() {
        let a = PreviewSettings::from_params(&params("seed=42")).unwrap();
        assert_eq!(a.elevation_seed, 42);
        assert_eq!(a.region_seed, 42 ^ 0x5445_4d50);
        let fixed = format!(
            "{EQUATOR_QUERY}&elevation_seed=17&region_seed=29&crater_density=0&basin_probability=0&volcano_probability=0"
        );
        assert_eq!(
            region_bytes(&fixed).unwrap(),
            region_bytes(&format!("{fixed}&seed=99")).unwrap()
        );
        assert_ne!(
            first_metrics(EQUATOR_QUERY),
            first_metrics(&format!("{EQUATOR_QUERY}&seed=99"))
        );
    }

    #[test]
    fn invalid_controls_are_rejected_before_sampling() {
        for setting in [
            "temperature_base=NaN",
            "temperature_solar=inf",
            "elevation_octaves=0",
            "elevation_octaves=1.5",
            "region_octaves=33",
            "region_persistence=0",
            "elevation_persistence=-1",
            "region_lacunarity=0",
            "elevation_frequency=-1",
            "region_seed=4294967296",
            "seed=-1",
            "axial_tilt=91",
            "orbit=-0.1",
            "crater_density=5",
            "crater_cell_size=0",
            "crater_min_radius=200&crater_max_radius=100",
            "crater_max_radius=512&crater_cell_size=32",
            "basin_probability=NaN",
            "basin_probability=1.1",
            "basin_min_radius=3000&basin_max_radius=2000",
            "basin_max_radius=2400&basin_cell_size=128",
            "basin_min_depth=0.2&basin_max_depth=0.1",
            "basin_min_rim_height=0.1&basin_max_rim_height=0.01",
            "basin_smoothing=-1",
            "basin_coarse_octaves=1.5",
            "volcano_probability=NaN",
            "volcano_probability=1.1",
            "volcano_cell_size=0",
            "volcano_cell_size=32&volcano_max_radius=64",
            "volcano_min_radius=512&volcano_max_radius=256",
            "volcano_min_height=0.5&volcano_max_height=0.2",
            "volcano_caldera_ratio=1",
            "volcano_caldera_depth=-0.1",
        ] {
            assert!(
                region_bytes(&format!("{EQUATOR_QUERY}&{setting}")).is_err(),
                "{setting}"
            );
        }
    }

    fn assert_record(bytes: &[u8], generator: &WorldGenerator, pos: TilePos) {
        let tile = generator.draft_at(pos);
        assert_eq!(
            f32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            tile.height as f32
        );
        assert_eq!(
            f32::from_le_bytes(bytes[8..12].try_into().unwrap()),
            tile.temperature as f32
        );
        let looks = BiomeLooks::load(generator.biomes()).unwrap();
        match tile.biome {
            Some(biome) => {
                let drawn = looks.drawn(biome, tile.ground.unwrap_or(biome));
                assert_eq!(bytes[0], 1);
                assert_eq!(bytes[1..4], looks.colors[drawn.index()]);
                assert_eq!(
                    u16::from_le_bytes(bytes[12..14].try_into().unwrap()),
                    biome.0
                );
                assert_eq!(
                    u16::from_le_bytes(bytes[14..16].try_into().unwrap()),
                    drawn.0
                );
            }
            None => assert_eq!(bytes[0], 0),
        }
    }

    #[test]
    fn exact_region_preserves_generator_colors_and_metrics() {
        let bytes =
            region_bytes("min_x=0&min_y=0&max_x=1&max_y=0&lod=0&size=4&seed=42&frequency=0.025")
                .unwrap();
        assert_eq!(bytes.len(), 2 * 4 * 4 * SAMPLE_BYTES);
        let generator = preview_generator(
            &PreviewSettings::from_params(&params("seed=42&size=4&frequency=0.025")).unwrap(),
        );
        for chunk in 0..2 {
            for y in 0..4 {
                for x in 0..4 {
                    let offset = (chunk * 16 + y * 4 + x) * SAMPLE_BYTES;
                    assert_record(
                        &bytes[offset..offset + SAMPLE_BYTES],
                        &generator,
                        TilePos {
                            x: (chunk * 4 + x) as i64,
                            y: y as i64,
                        },
                    );
                }
            }
        }
    }

    #[test]
    fn coarser_region_samples_representative_global_tile() {
        let bytes =
            region_bytes("min_x=1&min_y=1&max_x=1&max_y=1&lod=2&size=4&seed=0&frequency=0.025")
                .unwrap();
        let generator = preview_generator(
            &PreviewSettings::from_params(&params("size=4&frequency=0.025")).unwrap(),
        );
        assert_record(&bytes[..SAMPLE_BYTES], &generator, TilePos { x: 18, y: 18 });
    }

    #[test]
    fn metadata_and_region_bounds_use_the_same_world_size() {
        let side = world_side(WorldGeneratorConfig {
            chunk_size: 4,
            ..Default::default()
        })
        .unwrap();
        let meta: serde_json::Value =
            serde_json::from_str(&metadata("size=4", 123).unwrap()).unwrap();
        assert_eq!(meta["world"]["width"], side);
        assert_eq!(meta["world"]["height"], side);
        assert!(
            region_bytes("min_x=-1&min_y=0&max_x=0&max_y=0&lod=0&size=4&frequency=0.025").is_err()
        );
        let outside = side / 4;
        assert!(
            region_bytes(&format!(
                "min_x={outside}&max_x={outside}&min_y=0&max_y=0&lod=0&size=4&frequency=0.025"
            ))
            .is_err()
        );
    }

    #[test]
    fn partial_lod_chunks_keep_edge_samples_inside_the_world() {
        let side = world_side(WorldGeneratorConfig {
            chunk_size: 4,
            ..Default::default()
        })
        .unwrap();
        let bytes =
            region_bytes("min_x=0&max_x=0&min_y=0&max_y=0&lod=13&size=4&frequency=0.025").unwrap();
        let generator = preview_generator(
            &PreviewSettings::from_params(&params("size=4&frequency=0.025")).unwrap(),
        );
        assert_record(
            &bytes[..SAMPLE_BYTES],
            &generator,
            TilePos {
                x: i64::from(side - 1),
                y: i64::from(side - 1),
            },
        );
        assert_eq!(bytes[SAMPLE_BYTES], 255);
    }

    #[test]
    fn color_strings_are_parsed_without_changing_the_palette() {
        assert_eq!(color_rgb("#38476a").unwrap(), [56, 71, 106]);
        assert_eq!(color_rgb("#abc").unwrap(), [170, 187, 204]);
        assert!(color_rgb("#zzzzzz").is_err());
        assert!(color_rgb("#ééé").is_err());
    }

    #[test]
    fn selected_file_supplies_biomes_and_each_biome_is_a_colored_tile() {
        let default_meta: serde_json::Value =
            serde_json::from_str(&metadata("", 123).unwrap()).unwrap();
        assert_eq!(default_meta["biome_file"], "default.ron");
        assert_eq!(default_meta["protocol"], 6);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&metadata("biome=biomes.ron", 123).unwrap())
                .unwrap()["biome_file"],
            "default.ron"
        );
        let original = region_bytes(EQUATOR_QUERY).unwrap();
        let query = format!("{EQUATOR_QUERY}&biome=default%2Eron");
        let selected = region_bytes(&query).unwrap();
        assert_eq!(original, selected);
        let generator = preview_generator(&PreviewSettings::from_params(&params(&query)).unwrap());
        for (index, (old, new)) in original
            .chunks_exact(SAMPLE_BYTES)
            .zip(selected.chunks_exact(SAMPLE_BYTES))
            .enumerate()
        {
            assert_eq!(
                &old[4..],
                &new[4..],
                "file selection must preserve terrain metrics"
            );
            assert_record(
                new,
                &generator,
                TilePos {
                    x: 253 * 4 + (index % 4) as i64,
                    y: 250 * 4 + (index / 4) as i64,
                },
            );
        }
        let meta: serde_json::Value =
            serde_json::from_str(&metadata("biome=default%2Eron", 123).unwrap()).unwrap();
        assert_eq!(meta["biome_file"], "default.ron");
        assert!(
            meta["biome_files"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("default.ron"))
        );
        assert_eq!(
            meta["biomes"],
            serde_json::to_value(generator_biomes()).unwrap()
        );
        assert!(meta["biomes"].as_array().unwrap().iter().all(
            |biome| biome.get("height").is_none()
                && biome.get("temperature").is_none()
                && biome.get("palette").is_none()
        ));
        let colors = biome_colors(generator.biomes()).unwrap();
        let palette = meta["tile_palette"].as_array().unwrap();
        assert_eq!(palette.len(), generator.biomes().biomes().len());
        for ((entry, biome), rgb) in palette.iter().zip(generator.biomes().biomes()).zip(&colors) {
            assert_eq!(entry["name"], biome.name);
            assert_eq!(color_rgb(entry["color"].as_str().unwrap()).unwrap(), *rgb);
        }
        assert_eq!(
            meta["object_types"],
            serde_json::to_value(generator.object_placer().objects().objects()).unwrap()
        );
        let rules = meta["tile_rules"].as_array().unwrap();
        assert_eq!(rules.len(), palette.len());
        assert!(
            rules
                .iter()
                .enumerate()
                .all(|(id, rule)| rule["tile"] == id && rule["biome"] == id)
        );
        assert_eq!(meta["biome_rules"].as_array().unwrap().len(), 32);
        for name in ["Basins", "Craters", "Volcanoes"] {
            let rules = meta["feature_biome_maps"][name].as_array().unwrap();
            assert!(rules.len() >= 2);
            for rule in rules {
                let biome = generator
                    .biomes()
                    .id(rule["name"].as_str().unwrap())
                    .unwrap();
                assert_eq!(rule["biome"], serde_json::to_value(biome).unwrap());
                assert!(rule.get("palette").is_none());
            }
        }
    }

    fn generator_biomes() -> Vec<worldgen::Biome> {
        BiomeRepository::parse().biomes().to_vec()
    }

    #[test]
    fn biome_selection_rejects_unknown_files_and_paths() {
        for file in [
            "missing.ron",
            "../default.ron",
            "%2E%2E%2Fdefault.ron",
            "biome.rs",
            "%FF",
            "%2",
        ] {
            let query = format!("biome={file}");
            assert!(metadata(&query, 123).is_err(), "{file}");
            assert!(
                region_bytes(&format!("{EQUATOR_QUERY}&{query}")).is_err(),
                "{file}"
            );
        }
        assert_eq!(
            selected_biomes(&params("biome=default%2Eron"), &BiomeRepository::parse())
                .unwrap()
                .0,
            "default.ron"
        );
    }
}
