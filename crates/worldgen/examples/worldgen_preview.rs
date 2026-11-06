//! Local-only HTTP adapter for the engine-independent worldgen library.
use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex, mpsc},
    time::{SystemTime, UNIX_EPOCH},
};

use worldgen::{
    BiomeMap, ElevationNoise, ElevationSampler, NoiseConfig, NoiseSource, PlanetParams,
    TemperatureCoefficients, TemperatureRegionsNoise, TemperatureSampler, TilePos, WorldGenerator,
    WorldGeneratorConfig,
};

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
        "temperature_daily",
        "Daily variation (C)",
        "Temperature coefficients",
        Some(temperature.daily),
        -1000.0,
        1000.0,
        "any",
    );
    add(
        "orbit",
        "Orbital phase (0-1)",
        "Planet and time",
        Some(planet.orbit),
        0.0,
        1.0,
        "any",
    );
    add(
        "axial_tilt",
        "Axial tilt (degrees)",
        "Planet and time",
        Some(planet.axial_tilt_deg),
        0.0,
        90.0,
        "any",
    );
    add(
        "instant",
        "Temperature display",
        "Planet and time",
        Some(0.0),
        0.0,
        1.0,
        "1",
    );
    add(
        "time",
        "Time of day (UTC, 0-1)",
        "Planet and time",
        Some(0.5),
        0.0,
        1.0,
        "any",
    );
    controls
}

struct PreviewSettings {
    config: WorldGeneratorConfig,
    frequency: f64,
    elevation_seed: u64,
    elevation: NoiseConfig,
    region_seed: u64,
    region: NoiseConfig,
    temperature: TemperatureCoefficients,
    planet: PlanetParams,
    time: Option<f64>,
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
        Ok(Self {
            config,
            frequency: values["frequency"],
            elevation_seed: values
                .get("elevation_seed")
                .map_or(config.seed, |seed| *seed as u64),
            elevation: noise("elevation"),
            region_seed: values
                .get("region_seed")
                .map_or(config.seed ^ 0x5445_4d50, |seed| *seed as u64),
            region: noise("region"),
            temperature: TemperatureCoefficients {
                base: values["temperature_base"],
                solar: values["temperature_solar"],
                altitude: values["temperature_altitude"],
                region: values["temperature_region"],
                daily: values["temperature_daily"],
            },
            planet: PlanetParams {
                orbit: values["orbit"],
                axial_tilt_deg: values["axial_tilt"],
            },
            time: (values["instant"] == 1.0).then_some(values["time"]),
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
    WorldGenerator::new(
        settings.config,
        BiomeMap::parse(),
        settings.planet,
        ElevationSampler::new(PreviewNoise {
            source,
            frequency: settings.frequency,
        }),
        TemperatureSampler::with_config(
            settings.temperature,
            TemperatureRegionsNoise::with_config(settings.region_seed, settings.region),
        ),
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

const SAMPLE_BYTES: usize = 12;

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
    let config = request_config(&params(query))?;
    let side = world_side(config)?;
    let controls = controls().iter().map(|c| format!(
        "{{\"key\":{:?},\"label\":{:?},\"group\":{:?},\"default\":{},\"min\":{},\"max\":{},\"step\":{:?}}}",
        c.key, c.label, c.group, c.default.map_or("null".to_owned(), |v| v.to_string()), c.min, c.max, c.step,
    )).collect::<Vec<_>>().join(",");
    Ok(format!(
        "{{\"build\":\"{build}\",\"protocol\":2,\"world\":{{\"width\":{side},\"height\":{side}}},\"controls\":[{controls}]}}"
    ))
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

// Wire record: status u8 (0=unclassified, 1=color, 255=outside), RGB u8[3],
// normalized height f32 LE, Celsius temperature f32 LE (average or instant).
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
                        continue;
                    }
                    let pos = TilePos {
                        x: (base_x + offset).min(side - 1) as i64,
                        y: (base_y + offset).min(side - 1) as i64,
                    };
                    let (sample, tile) = if let Some(time) = settings.time {
                        generator.sample_tile_instant(pos, time)
                    } else {
                        generator.sample_tile(pos)
                    };
                    if let Some(tile) = tile {
                        output.push(1);
                        output.extend_from_slice(&color_rgb(&tile.0)?);
                    } else {
                        output.extend_from_slice(&[0, 0, 0, 0]);
                    }
                    output.extend_from_slice(&(sample.height as f32).to_le_bytes());
                    output.extend_from_slice(&(sample.temperature as f32).to_le_bytes());
                }
            }
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EQUATOR_QUERY: &str = "min_x=503&max_x=503&min_y=500&max_y=500&lod=0&size=4&seed=42";

    fn first_metrics(query: &str) -> (f32, f32) {
        let bytes = region_bytes(query).unwrap();
        (
            f32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            f32::from_le_bytes(bytes[8..12].try_into().unwrap()),
        )
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
    fn temperature_coefficients_and_planet_controls_reach_the_sampler() {
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
    fn time_of_day_and_daily_coefficient_only_affect_instant_mode() {
        let fixed =
            format!("{EQUATOR_QUERY}&temperature_region=0&temperature_altitude=0&axial_tilt=0");
        let average = first_metrics(&fixed);
        assert_eq!(
            average,
            first_metrics(&format!("{fixed}&time=0.625&temperature_daily=100"))
        );
        let afternoon = first_metrics(&format!("{fixed}&instant=1&time=0.625"));
        let night = first_metrics(&format!("{fixed}&instant=1&time=0.125"));
        assert!((afternoon.1 - 25.5).abs() < 0.1);
        assert!((night.1 + 4.5).abs() < 0.1);
        let stronger = first_metrics(&format!(
            "{fixed}&instant=1&time=0.625&temperature_daily=60"
        ));
        assert!(stronger.1 > afternoon.1);
        let no_cycle = format!("{fixed}&instant=1&temperature_daily=0");
        assert_eq!(average, first_metrics(&no_cycle));
    }

    #[test]
    fn seed_overrides_are_independent_and_blank_defaults_follow_world_seed() {
        let a = PreviewSettings::from_params(&params("seed=42")).unwrap();
        assert_eq!(a.elevation_seed, 42);
        assert_eq!(a.region_seed, 42 ^ 0x5445_4d50);
        let fixed = format!("{EQUATOR_QUERY}&elevation_seed=17&region_seed=29");
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
            "temperature_daily=inf",
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
            "time=1.1",
            "instant=2",
        ] {
            assert!(
                region_bytes(&format!("{EQUATOR_QUERY}&{setting}")).is_err(),
                "{setting}"
            );
        }
    }

    fn assert_record(bytes: &[u8], generator: &WorldGenerator, pos: TilePos) {
        let (sample, tile) = generator.sample_tile(pos);
        assert_eq!(
            f32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            sample.height as f32
        );
        assert_eq!(
            f32::from_le_bytes(bytes[8..12].try_into().unwrap()),
            sample.temperature as f32
        );
        if let Some(tile) = tile {
            assert_eq!(bytes[0], 1);
            assert_eq!(bytes[1..4], color_rgb(&tile.0).unwrap());
        } else {
            assert_eq!(bytes[0], 0);
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
        let meta = metadata("size=4", 123).unwrap();
        assert!(meta.contains(&format!("\"width\":{side},\"height\":{side}")));
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
}
