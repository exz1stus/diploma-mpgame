//! Local-only HTTP adapter for the engine-independent worldgen library.
use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex, mpsc},
    time::{SystemTime, UNIX_EPOCH},
};

use worldgen::{
    BasinGenerator, BiomeMap, BiomeRepository, CraterGenerator, ElevationNoise, NoiseConfig,
    NoiseSource, PlanetParams, TemperatureCoefficients, TemperatureModel, TemperatureRegionsNoise,
    TilePos, VolcanoGenerator, WorldGenerator, WorldGeneratorConfig, WorldGeography,
    build_world_generator,
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
    let world = WorldGeneratorConfig::default();
    let temperature = TemperatureCoefficients::default();
    let planet = PlanetParams::default();
    let elevation = ElevationNoise::default_config();
    let region = TemperatureRegionsNoise::default_config();
    let craters = CraterGenerator::default();
    let basins = BasinGenerator::new(ElevationNoise::new(world.seed));
    let volcanoes = VolcanoGenerator::default();
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
        Some(2.0),
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

struct PreviewSettings {
    biomes: BiomeRepository,
    biome_map: BiomeMap,
    config: WorldGeneratorConfig,
    frequency: f64,
    elevation_seed: u64,
    elevation: NoiseConfig,
    region_seed: u64,
    region: NoiseConfig,
    temperature: TemperatureCoefficients,
    planet: PlanetParams,
    craters: CraterGenerator,
    basins: BasinGenerator,
    volcanoes: VolcanoGenerator,
}

impl PreviewSettings {
    fn from_params(params: &HashMap<&str, &str>) -> Result<Self, String> {
        let mut values = HashMap::new();
        for control in controls() {
            let value = if params.contains_key(control.key) {
                Some(parse::<f64>(params, control.key)?)
            } else {
                control.default
            };
            if let Some(value) = value {
                if !value.is_finite()
                    || value < control.min
                    || value > control.max
                    || (control.step == "1" && value.fract() != 0.0)
                {
                    return Err(format!(
                        "Invalid {}: expected {} to {}{}",
                        control.key,
                        control.min,
                        control.max,
                        if control.step == "1" {
                            " (integer)"
                        } else {
                            ""
                        }
                    ));
                }
                values.insert(control.key, value);
            }
        }
        let config = WorldGeneratorConfig {
            seed: values["seed"] as u64,
            chunk_size: values["size"] as u32,
            ..Default::default()
        };
        let noise = |prefix: &str| NoiseConfig {
            frequency: values[format!("{prefix}_frequency").as_str()],
            octaves: values[format!("{prefix}_octaves").as_str()] as usize,
            lacunarity: values[format!("{prefix}_lacunarity").as_str()],
            persistence: values[format!("{prefix}_persistence").as_str()],
        };
        let craters = CraterGenerator {
            cell_size: values["crater_cell_size"] as i64,
            min_radius: values["crater_min_radius"],
            max_radius: values["crater_max_radius"],
            density: values["crater_density"],
            temperature_offset: values["crater_temperature_offset"],
            ..Default::default()
        };
        if craters.min_radius > craters.max_radius
            || craters.max_radius * 1.5 > craters.cell_size as f64
        {
            return Err(
                "Crater radii must satisfy minimum <= maximum and maximum * 1.5 <= cell size"
                    .into(),
            );
        }
        let elevation = noise("elevation");
        let elevation_seed = values
            .get("elevation_seed")
            .map_or(config.seed, |seed| *seed as u64);
        let mut coarse_elevation = elevation;
        coarse_elevation.octaves = (values["basin_coarse_octaves"] as usize).min(elevation.octaves);
        let mut basins = BasinGenerator::new(PreviewNoise {
            source: ElevationNoise::with_config(elevation_seed, coarse_elevation),
            frequency: values["frequency"],
        });
        basins.cell_size = values["basin_cell_size"] as i64;
        basins.probability = values["basin_probability"];
        basins.min_radius = values["basin_min_radius"];
        basins.max_radius = values["basin_max_radius"];
        basins.min_depth = values["basin_min_depth"];
        basins.max_depth = values["basin_max_depth"];
        basins.min_rim_height = values["basin_min_rim_height"];
        basins.max_rim_height = values["basin_max_rim_height"];
        basins.smoothing = values["basin_smoothing"];
        basins.temperature_offset = values["basin_temperature_offset"];
        if basins.min_radius > basins.max_radius
            || basins.max_radius * 1.5 > basins.cell_size as f64
            || basins.min_depth > basins.max_depth
            || basins.min_rim_height > basins.max_rim_height
        {
            return Err("Basin minimums must not exceed maximums, and maximum radius * 1.5 must fit within the cell size".into());
        }
        let volcanoes = VolcanoGenerator {
            cell_size: values["volcano_cell_size"] as i64,
            probability: values["volcano_probability"],
            min_radius: values["volcano_min_radius"],
            max_radius: values["volcano_max_radius"],
            min_height: values["volcano_min_height"],
            max_height: values["volcano_max_height"],
            caldera_ratio: values["volcano_caldera_ratio"],
            caldera_depth: values["volcano_caldera_depth"],
            temperature_offset: values["volcano_temperature_offset"],
            ..Default::default()
        };
        if volcanoes.min_radius > volcanoes.max_radius
            || volcanoes.max_radius > volcanoes.cell_size as f64
            || volcanoes.min_height > volcanoes.max_height
        {
            return Err("Volcano minimums must not exceed maximums, and maximum radius must fit within the cell size".into());
        }
        let biomes = BiomeRepository::parse();
        let biome_map = selected_biomes(params, &biomes)?.1;
        Ok(Self {
            biomes,
            biome_map,
            config,
            frequency: values["frequency"],
            elevation_seed,
            elevation,
            region_seed: values
                .get("region_seed")
                .map_or(config.seed ^ 0x5445_4d50, |seed| *seed as u64),
            region: noise("region"),
            temperature: TemperatureCoefficients {
                base: values["temperature_base"],
                solar: values["temperature_solar"],
                altitude: values["temperature_altitude"],
                region: values["temperature_region"],
            },
            planet: PlanetParams {
                orbit: values["orbit"],
                axial_tilt_deg: values["axial_tilt"],
            },
            craters,
            basins,
            volcanoes,
        })
    }
}

// Preview controls are an adapter concern, separate from the world configuration.
struct PreviewNoise {
    source: ElevationNoise,
    frequency: f64,
}

impl NoiseSource for PreviewNoise {
    fn sample(&self, x: f64, y: f64) -> f64 {
        self.source.sample(x * self.frequency, y * self.frequency)
    }
}

fn preview_generator(settings: &PreviewSettings) -> WorldGenerator {
    let source = ElevationNoise::with_config(settings.elevation_seed, settings.elevation);
    build_world_generator(
        settings.config,
        settings.biomes.clone(),
        settings.biome_map.clone(),
        PreviewNoise {
            source,
            frequency: settings.frequency,
        },
        TemperatureModel::with_config(
            settings.temperature,
            TemperatureRegionsNoise::with_config(settings.region_seed, settings.region),
            WorldGeography::new(
                settings.config.chunk_size,
                settings.config.world_size_chunks,
            ),
            settings.planet,
        ),
        settings.craters.clone(),
        settings.basins.clone(),
        settings.volcanoes.clone(),
    )
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
    let (sender, receiver) = mpsc::channel::<TcpStream>();
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

const SAMPLE_BYTES: usize = 18;

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
    let biomes = BiomeRepository::parse();
    let (biome_file, biome_map) = selected_biomes(&params, &biomes)?;
    let map_rules = |map: &BiomeMap| -> Vec<serde_json::Value> {
        map.rules()
            .iter()
            .map(|rule| {
                let biome = biomes.get(rule.biome).expect("resolved biome exists");
                serde_json::json!({ "name": biome.name, "palette": biome.palette, "palette_height_threshold": biome.palette_height_threshold,
                "height": rule.height, "temperature": rule.temperature })
            })
            .collect()
    };
    let mut feature_maps = serde_json::Map::new();
    for (name, definition) in [
        (
            "Basins",
            BasinGenerator::new(ElevationNoise::new(config.seed)).biome_map,
        ),
        ("Craters", CraterGenerator::default().biome_map),
        ("Volcanoes", VolcanoGenerator::default().biome_map),
    ] {
        let map = definition
            .resolve(&biomes)
            .map_err(|error| error.to_string())?;
        feature_maps.insert(name.into(), serde_json::json!(map_rules(&map)));
    }
    Ok(serde_json::json!({
        "build": build.to_string(), "protocol": 5,
        "world": { "width": side, "height": side }, "controls": controls(),
        "biome_files": biome_files()?, "biome_file": biome_file, "biomes": biomes.biomes(),
        "biome_rules": map_rules(&biome_map), "feature_biome_maps": feature_maps,
    })
    .to_string())
}

fn biome_directory() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/biomes/maps")
}

fn biome_files() -> Result<Vec<String>, String> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(biome_directory()).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        if entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_file()
            && entry.path().extension().is_some_and(|ext| ext == "ron")
        {
            files.push(
                entry
                    .file_name()
                    .into_string()
                    .map_err(|_| "Invalid biome filename")?,
            );
        }
    }
    files.sort();
    Ok(files)
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
) -> Result<(String, BiomeMap), String> {
    let filename = decode_query_value(params.get("biome").unwrap_or(&"default.ron"))?;
    // Keep previously shared preview URLs working after the default map rename.
    let filename = if filename == "biomes.ron" {
        "default.ron".to_owned()
    } else {
        filename
    };
    if !biome_files()?.contains(&filename) {
        return Err("Choose a RON map from src/biomes/maps".into());
    }
    let source = std::fs::read_to_string(biome_directory().join(&filename))
        .map_err(|error| format!("Cannot read {filename}: {error}"))?;
    let biomes =
        BiomeMap::from_ron(&source, repository).map_err(|error| format!("{filename}: {error}"))?;
    for biome in repository.biomes() {
        for entry in &biome.palette {
            color_rgb(&entry.color)?;
        }
    }
    Ok((filename, biomes))
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

// Wire record: status u8 (0=unclassified, 1=color, 2=territory without appearance,
// 255=outside), RGB u8[3],
// normalized height f32 LE, daily-average Celsius temperature f32 LE,
// biome ID u16 LE, surface biome ID u16 LE, palette index u16 LE.
// Biome ID is valid for status 1 or 2; appearance IDs are valid for status 1.
fn region_bytes(query: &str) -> Result<Vec<u8>, String> {
    let params = params(query);
    let min_x: i32 = parse(&params, "min_x")?;
    let min_y: i32 = parse(&params, "min_y")?;
    let max_x: i32 = parse(&params, "max_x")?;
    let max_y: i32 = parse(&params, "max_y")?;
    let lod: u32 = parse(&params, "lod")?;
    let settings = PreviewSettings::from_params(&params)?;
    let config = settings.config;
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
                        output.extend_from_slice(&[0; 6]);
                        continue;
                    }
                    let pos = TilePos {
                        x: (base_x + offset).min(side - 1) as i64,
                        y: (base_y + offset).min(side - 1) as i64,
                    };
                    let tile = generator.draft_at(pos);
                    if let Some(biome) =
                        tile.surface_biome.and_then(|id| generator.biomes().get(id))
                    {
                        output.push(1);
                        output.extend_from_slice(&color_rgb(
                            &biome.palette[usize::from(tile.palette_index)].color,
                        )?);
                    } else {
                        output.extend_from_slice(&[
                            if tile.biome.is_some() { 2 } else { 0 },
                            0,
                            0,
                            0,
                        ]);
                    }
                    output.extend_from_slice(&(tile.height as f32).to_le_bytes());
                    output.extend_from_slice(&(tile.temperature as f32).to_le_bytes());
                    output.extend_from_slice(&tile.biome.map_or(0, |id| id.0).to_le_bytes());
                    output
                        .extend_from_slice(&tile.surface_biome.map_or(0, |id| id.0).to_le_bytes());
                    output.extend_from_slice(&tile.palette_index.to_le_bytes());
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

    fn first_metrics(query: &str) -> (f32, f32) {
        let bytes = region_bytes(query).unwrap();
        (
            f32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            f32::from_le_bytes(bytes[8..12].try_into().unwrap()),
        )
    }

    const BASIN_QUERY: &str = "min_x=0&max_x=0&min_y=0&max_y=0&lod=9&size=16&seed=42&crater_density=0&basin_cell_size=4096&basin_probability=1&basin_min_radius=800&basin_max_radius=1500&basin_min_depth=0.06&basin_max_depth=0.18&basin_min_rim_height=0.005&basin_max_rim_height=0.02&basin_smoothing=0.8";

    #[test]
    fn crater_and_basin_api_markers_use_default_terrain_appearance() {
        for query in [
            "min_x=0&max_x=0&min_y=0&max_y=0&lod=7&size=16&seed=42&crater_density=4&basin_probability=0&volcano_probability=0",
            BASIN_QUERY,
        ] {
            let query = format!(
                "{query}&volcano_probability=0&temperature_base=0&temperature_solar=0&temperature_region=0&temperature_altitude=0"
            );
            let settings = PreviewSettings::from_params(&params(&query)).unwrap();
            let bytes = region_bytes(&query).unwrap();
            let mut marked = 0;
            for record in bytes.chunks_exact(SAMPLE_BYTES) {
                assert_eq!(record[0], 1);
                let height = f32::from_le_bytes(record[4..8].try_into().unwrap()) as f64;
                let temperature = f32::from_le_bytes(record[8..12].try_into().unwrap()) as f64;
                let biome =
                    worldgen::BiomeId(u16::from_le_bytes(record[12..14].try_into().unwrap()));
                let surface =
                    worldgen::BiomeId(u16::from_le_bytes(record[14..16].try_into().unwrap()));
                if settings.biomes.get(biome).unwrap().palette.is_empty() {
                    marked += 1;
                }
                assert_eq!(
                    Some(surface),
                    settings.biome_map.determine_biome(temperature, height)
                );
                let index = u16::from_le_bytes(record[16..18].try_into().unwrap());
                let appearance = settings.biomes.get(surface).unwrap();
                assert_eq!(
                    record[1..4],
                    color_rgb(&appearance.palette[usize::from(index)].color).unwrap()
                );
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
            assert_eq!(&a[..12], &b[..12]);
            assert_eq!(&a[14..], &b[14..]);
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
        // Asking for more coarse octaves than elevation has is capped to match.
        for (a, b) in disabled
            .chunks_exact(SAMPLE_BYTES)
            .zip(matching_source.chunks_exact(SAMPLE_BYTES))
        {
            assert_eq!(&a[..12], &b[..12]);
            assert_eq!(&a[14..], &b[14..]);
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
        if let Some(biome) = tile.surface_biome.and_then(|id| generator.biomes().get(id)) {
            assert_eq!(bytes[0], 1);
            assert_eq!(
                bytes[1..4],
                color_rgb(&biome.palette[usize::from(tile.palette_index)].color).unwrap()
            );
            assert_eq!(
                u16::from_le_bytes(bytes[16..18].try_into().unwrap()),
                tile.palette_index
            );
            assert_eq!(
                u16::from_le_bytes(bytes[12..14].try_into().unwrap()),
                tile.biome.unwrap().0
            );
            assert_eq!(
                u16::from_le_bytes(bytes[14..16].try_into().unwrap()),
                tile.surface_biome.unwrap().0
            );
        } else {
            assert_eq!(bytes[0], if tile.biome.is_some() { 2 } else { 0 });
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
    fn selected_file_supplies_metadata_and_generator_palette() {
        let default_meta: serde_json::Value =
            serde_json::from_str(&metadata("", 123).unwrap()).unwrap();
        assert_eq!(default_meta["biome_file"], "default.ron");
        assert_eq!(default_meta["protocol"], 5);
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
        assert!(
            meta["biomes"]
                .as_array()
                .unwrap()
                .iter()
                .all(|biome| biome.get("height").is_none() && biome.get("temperature").is_none())
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
                assert_eq!(
                    rule["palette"],
                    serde_json::to_value(&generator.biomes().get(biome).unwrap().palette).unwrap()
                );
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
