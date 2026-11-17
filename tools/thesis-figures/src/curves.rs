//! Charts computed from the library's own models: noise, feature profiles,
//! crater statistics, insolation, temperature, biome rules and feature scattering.

use std::path::Path;

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use worldgen::{
    BasinGenerator, BiomeMapDefinition, FbmNoise, FeatureGenerator, NoiseConfig, NoiseSource,
    PlanetParams, TemperatureCoefficients, TemperatureModel, TerrainFeature, TileDraft, TilePos,
    WorldConfig, WorldGeography,
};

use crate::chart::{Axes, AxisLabels, SERIES, legend};
use crate::palette::biome_hex;
use crate::svg::{MUTED, Svg, TextStyle};

pub struct FlatNoise;

impl NoiseSource for FlatNoise {
    fn sample(&self, _: f64, _: f64) -> f64 {
        0.0
    }
}

pub fn render_all(config: &WorldConfig, out: &Path) {
    fbm_octaves(config, &out.join("fbm_octaves.svg"));
    feature_scattering(config, &out.join("feature_scattering.svg"));
    feature_profiles(config, &out.join("feature_profiles.svg"));
    crater_statistics(config, &out.join("crater_statistics.svg"));
    insolation(&out.join("insolation.svg"));
    temperature_profile(config, &out.join("temperature_latitude.svg"));
    default_biome_rules(config, &out.join("biome_rules_default.svg"));
    feature_biome_rules(config, &out.join("biome_rules_features.svg"));
}

fn fbm_octaves(config: &WorldConfig, path: &Path) {
    let mut svg = Svg::new(1000.0, 460.0);
    let axes = Axes {
        left: 80.0,
        top: 50.0,
        width: 760.0,
        height: 340.0,
        x: (0.0, 4000.0),
        y: (-0.5, 4.2),
    };
    axes.draw(
        &mut svg,
        &AxisLabels {
            title: "fBm slice (y = 1234) with increasing octave count, offset for readability",
            x: "x (tiles)",
            y: "noise value + offset",
        },
    );
    let mut entries = Vec::new();
    for (index, octaves) in [1, 2, 4, 10].into_iter().enumerate() {
        let noise = FbmNoise::new(
            config.world.seed,
            NoiseConfig {
                octaves,
                ..config.world.elevation
            },
        );
        let offset = index as f64 * 1.15;
        let points: Vec<_> = (0..=2000)
            .map(|i| {
                let x = f64::from(i) * 2.0;
                (x, noise.sample(x, 1234.0) + offset)
            })
            .collect();
        axes.series(&mut svg, &points, SERIES[index], r#"stroke-width="1.4""#);
        entries.push((
            format!("{octaves} octave{}", if octaves == 1 { "" } else { "s" }),
            SERIES[index],
        ));
    }
    let entries: Vec<_> = entries
        .iter()
        .rev()
        .map(|(l, c)| (l.as_str(), *c, false))
        .collect();
    legend(&mut svg, 860.0, 80.0, &entries);
    svg.text(
        860.0,
        170.0,
        "frequency 0.002",
        TextStyle::new(11.0).color(MUTED),
    );
    svg.text(
        860.0,
        186.0,
        "lacunarity 3.0",
        TextStyle::new(11.0).color(MUTED),
    );
    svg.text(
        860.0,
        202.0,
        "persistence 0.5",
        TextStyle::new(11.0).color(MUTED),
    );
    svg.save(path);
}

fn mix64(mut x: u64) -> u64 {
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

fn cell_rng(seed: u64, cx: i64, cy: i64) -> ChaCha8Rng {
    ChaCha8Rng::seed_from_u64(mix64(
        seed ^ mix64(cx as u64) ^ mix64(cy as u64).rotate_left(32),
    ))
}

fn feature_scattering(config: &WorldConfig, path: &Path) {
    let craters = &config.craters;
    let cell = craters.cell_size as f64;
    let (cells_x, cells_y, scale, left, top) = (4, 3, 0.2, 40.0, 60.0);
    let generated: Vec<_> = (0..cells_y)
        .flat_map(|cy| (0..cells_x).map(move |cx| (cx, cy)))
        .flat_map(|(cx, cy)| {
            craters.generate_cell(
                cx,
                cy,
                &mut cell_rng(config.world.seed ^ 0x4352_4154, cx, cy),
            )
        })
        .collect();
    let chunk =
        query_chunk_beside_largest(&generated, (cell, 3.0 * cell), (cell * 0.5, 2.5 * cell));
    let padding = craters.max_influence_radius();
    let mut svg = Svg::new(
        left * 2.0 + cell * cells_x as f64 * scale + 260.0,
        top + cell * cells_y as f64 * scale + 50.0,
    );
    svg.text(
        left,
        30.0,
        "Crater layer: features per generation cell and the cells one chunk query visits",
        TextStyle::new(14.0).bold(),
    );
    let to_px = |x: f64, y: f64| (left + x * scale, top + y * scale);
    let touched = |c: i64, lo: f64, hi: f64| {
        (((lo - padding) / cell).floor() as i64..=((hi + padding) / cell).floor() as i64)
            .contains(&c)
    };
    for cy in 0..cells_y {
        for cx in 0..cells_x {
            let (x, y) = to_px(cx as f64 * cell, cy as f64 * cell);
            let visited =
                touched(cx, chunk.0, chunk.0 + chunk.2) && touched(cy, chunk.1, chunk.1 + chunk.2);
            let fill = if visited { "#fefcbf" } else { "#f7fafc" };
            svg.rect(x, y, cell * scale, cell * scale, fill, "#a0aec0");
            svg.text(
                x + 6.0,
                y + 16.0,
                &format!("cell ({cx}, {cy})"),
                TextStyle::new(10.0).color(MUTED),
            );
        }
    }
    let (px0, py0) = to_px(chunk.0 - padding, chunk.1 - padding);
    let side = (chunk.2 + 2.0 * padding) * scale;
    svg.rect_styled(
        px0,
        py0,
        side,
        side,
        "none",
        "#2d3748",
        r#"stroke-dasharray="6 4" stroke-width="1.5""#,
    );
    let mut hits = 0;
    for crater in &generated {
        let (x, y) = crater.center();
        let influence = crater.influence_radius();
        let hit = touches(crater, chunk);
        hits += usize::from(hit);
        let color = if hit { "#c53030" } else { "#2b6cb0" };
        svg.circle(
            to_px(x, y),
            influence * scale,
            "none",
            color,
            r#"stroke-dasharray="3 3""#,
        );
        svg.circle(
            to_px(x, y),
            influence / 1.5 * scale,
            color,
            color,
            r#"fill-opacity="0.18""#,
        );
        svg.circle(to_px(x, y), 2.0, color, "none", "");
    }
    let (cx, cy) = to_px(chunk.0, chunk.1);
    svg.rect(cx, cy, chunk.2 * scale, chunk.2 * scale, "#1a202c", "none");
    let notes_x = left + cell * cells_x as f64 * scale + 24.0;
    let note = TextStyle::new(11.0);
    let lines = [
        "■ queried chunk (32 × 32 tiles)".to_string(),
        "┅ chunk padded by max influence".to_string(),
        format!("   radius ({padding:.0} tiles)"),
        "yellow cells: generated for this query".to_string(),
        "filled circle: crater radius R".to_string(),
        "dashed circle: influence 1.5 R".to_string(),
        format!("red: {hits} crater(s) affecting the chunk"),
        String::new(),
        format!(
            "cell size {} tiles, density {}",
            craters.cell_size, craters.density
        ),
        "each cell seeded by hash(seed, cx, cy)".to_string(),
    ];
    for (index, line) in lines.iter().enumerate() {
        svg.text(notes_x, top + 14.0 + index as f64 * 18.0, line, note);
    }
    svg.save(path);
}

/// Whether a feature's influence circle reaches the square chunk `(x, y, side)`.
fn touches(feature: &impl TerrainFeature, (x0, y0, side): (f64, f64, f64)) -> bool {
    let (x, y) = feature.center();
    let closest = (x.clamp(x0, x0 + side), y.clamp(y0, y0 + side));
    (x - closest.0).hypot(y - closest.1) <= feature.influence_radius()
}

/// A chunk just inside the rim of the largest crater centred in the given x/y ranges.
fn query_chunk_beside_largest<F: TerrainFeature>(
    features: &[F],
    xs: (f64, f64),
    ys: (f64, f64),
) -> (f64, f64, f64) {
    let largest = features
        .iter()
        .filter(|f| (xs.0..xs.1).contains(&f.center().0) && (ys.0..ys.1).contains(&f.center().1))
        .max_by(|a, b| a.influence_radius().total_cmp(&b.influence_radius()))
        .expect("a crater in range");
    let (x, y) = largest.center();
    let align = |v: f64| (v / 32.0).floor() * 32.0;
    (align(x - largest.influence_radius() * 0.8), align(y), 32.0)
}

fn first_feature<G: FeatureGenerator>(
    generator: &G,
    accept: impl Fn(&G::Feature) -> bool,
) -> G::Feature {
    (0..10_000)
        .flat_map(|cell| generator.generate_cell(cell, 0, &mut cell_rng(7, cell, 0)))
        .find(|feature| accept(feature))
        .expect("a matching feature")
}

/// Height change along a ray from the centre, applied to flat terrain at 0.5.
fn radial_profile(feature: &impl TerrainFeature) -> Vec<(f64, f64)> {
    let (cx, cy) = feature.center();
    let reach = feature.influence_radius() * 1.15;
    let mut points: Vec<(f64, f64)> = (0..=reach.ceil() as i64)
        .map(|offset| {
            let pos = TilePos {
                x: cx.round() as i64 + offset,
                y: cy.round() as i64,
            };
            let mut tile = TileDraft {
                height: 0.5,
                ..Default::default()
            };
            feature.apply(pos, &mut tile);
            (
                (pos.x as f64 - cx).hypot(pos.y as f64 - cy),
                tile.height - 0.5,
            )
        })
        .collect();
    points.sort_by(|a, b| a.0.total_cmp(&b.0));
    points
}

fn feature_profiles(config: &WorldConfig, path: &Path) {
    let mut craters = config.craters.clone();
    craters.density = 4.0;
    let mut basins = config.basins.clone();
    basins.probability = 1.0;
    let basins = BasinGenerator::new(basins, FlatNoise);
    let mut volcanoes = config.volcanoes.clone();
    volcanoes.probability = 1.0;
    let crater = first_feature(&craters, |c| {
        (180.0..260.0).contains(&(c.influence_radius() / 1.5))
    });
    let basin = first_feature(&basins, |b| {
        (1200.0..1500.0).contains(&(b.influence_radius() / 1.5))
    });
    let volcano = first_feature(&volcanoes, |v| {
        (450.0..650.0).contains(&v.influence_radius())
    });
    let panels: [(&str, f64, Vec<(f64, f64)>, &str); 3] = [
        (
            "Crater",
            crater.influence_radius() / 1.5,
            radial_profile(&crater),
            SERIES[0],
        ),
        (
            "Basin",
            basin.influence_radius() / 1.5,
            radial_profile(&basin),
            SERIES[1],
        ),
        (
            "Volcano",
            volcano.influence_radius(),
            radial_profile(&volcano),
            SERIES[3],
        ),
    ];
    let mut svg = Svg::new(1100.0, 400.0);
    for (index, (name, radius, points, color)) in panels.iter().enumerate() {
        let (low, high) = points
            .iter()
            .fold((0.0_f64, 0.0_f64), |(lo, hi), p| (lo.min(p.1), hi.max(p.1)));
        let margin = (high - low) * 0.1;
        let axes = Axes {
            left: 80.0 + index as f64 * 350.0,
            top: 50.0,
            width: 270.0,
            height: 280.0,
            x: (0.0, points.last().unwrap().0),
            y: (low - margin, high + margin),
        };
        let title = format!("{name}, R = {radius:.0} tiles");
        axes.draw(
            &mut svg,
            &AxisLabels {
                title: &title,
                x: "distance from centre (tiles)",
                y: "Δ height",
            },
        );
        axes.series(
            &mut svg,
            &[(0.0, 0.0), (axes.x.1, 0.0)],
            "#a0aec0",
            r#"stroke-dasharray="4 4" stroke-width="1""#,
        );
        let edge = axes.px(*radius);
        svg.line_styled(
            (edge, axes.top),
            (edge, axes.top + axes.height),
            MUTED,
            1.0,
            r#"stroke-dasharray="2 3""#,
        );
        svg.text(
            edge + 4.0,
            axes.top + 14.0,
            "R",
            TextStyle::new(11.0).color(MUTED),
        );
        axes.series(&mut svg, points, color, "");
    }
    svg.save(path);
}

fn crater_statistics(config: &WorldConfig, path: &Path) {
    let craters = &config.craters;
    let mut radii = Vec::new();
    let mut depths = Vec::new();
    let mut per_cell = [0usize; 5];
    for cell in 0..20_000 {
        let found = craters.generate_cell(cell, 1, &mut cell_rng(11, cell, 1));
        per_cell[found.len()] += 1;
        for crater in found {
            let radius = crater.influence_radius() / 1.5;
            let (cx, cy) = crater.center();
            let mut tile = TileDraft {
                height: 0.5,
                ..Default::default()
            };
            crater.apply(
                TilePos {
                    x: cx.round() as i64,
                    y: cy.round() as i64,
                },
                &mut tile,
            );
            radii.push(radius);
            depths.push((radius, 0.5 - tile.height));
        }
    }
    let mut svg = Svg::new(1100.0, 400.0);
    let bins = 20;
    let width = (craters.max_radius - craters.min_radius) / bins as f64;
    let mut counts = vec![0usize; bins];
    for radius in &radii {
        counts[(((radius - craters.min_radius) / width) as usize).min(bins - 1)] += 1;
    }
    let share: Vec<(f64, f64)> = counts
        .iter()
        .enumerate()
        .map(|(i, &c)| {
            (
                craters.min_radius + (i as f64 + 0.5) * width,
                c as f64 / radii.len() as f64,
            )
        })
        .collect();
    let peak = share.iter().map(|b| b.1).fold(0.0, f64::max);
    let axes = Axes {
        left: 80.0,
        top: 50.0,
        width: 280.0,
        height: 270.0,
        x: (craters.min_radius, craters.max_radius),
        y: (0.0, peak * 1.1),
    };
    axes.draw(
        &mut svg,
        &AxisLabels {
            title: "Radius distribution (R = Rmin + ΔR·ξ²)",
            x: "radius (tiles)",
            y: "share of craters",
        },
    );
    axes.bars(&mut svg, &share, width * 0.9, SERIES[0]);

    let axes = Axes {
        left: 450.0,
        top: 50.0,
        width: 280.0,
        height: 270.0,
        x: (0.0, craters.max_radius),
        y: (0.0, 0.42),
    };
    axes.draw(
        &mut svg,
        &AxisLabels {
            title: "Depth and rim height vs radius",
            x: "radius (tiles)",
            y: "normalized height",
        },
    );
    let depth: Vec<_> = (0..=384)
        .map(|r| (f64::from(r), (f64::from(r) * 0.001).min(0.22)))
        .collect();
    let rim: Vec<_> = (0..=384)
        .map(|r| (f64::from(r), (f64::from(r) * 0.0002).min(0.06)))
        .collect();
    axes.series(&mut svg, &depth, SERIES[3], "");
    axes.series(&mut svg, &rim, SERIES[2], "");
    for &(radius, measured) in depths.iter().step_by(25) {
        svg.circle(
            axes.point((radius, measured)),
            2.0,
            "#2d3748",
            "none",
            r#"fill-opacity="0.5""#,
        );
    }
    legend(
        &mut svg,
        axes.left + 12.0,
        axes.top + 16.0,
        &[
            ("depth D = min(0.001 R, 0.22)", SERIES[3], false),
            ("rim = min(0.0002 R, 0.06)", SERIES[2], false),
        ],
    );
    svg.text(
        axes.left + 12.0,
        axes.top + 60.0,
        "dots: measured centre depth",
        TextStyle::new(11.0).color(MUTED),
    );

    let total = per_cell.iter().sum::<usize>() as f64;
    let bars: Vec<_> = per_cell
        .iter()
        .enumerate()
        .map(|(n, &c)| (n as f64, c as f64 / total))
        .collect();
    let axes = Axes {
        left: 820.0,
        top: 50.0,
        width: 240.0,
        height: 270.0,
        x: (-0.6, 4.6),
        y: (0.0, 0.5),
    };
    axes.draw(
        &mut svg,
        &AxisLabels {
            title: "Craters per cell (4 trials, p = d/4)",
            x: "craters in cell",
            y: "share of cells",
        },
    );
    axes.bars(&mut svg, &bars, 0.7, SERIES[1]);
    svg.save(path);
}

fn exposure_model(planet: PlanetParams) -> (TemperatureModel, WorldGeography) {
    let solar_only = TemperatureCoefficients {
        base: 0.0,
        solar: 1.0,
        region: 0.0,
        altitude: 0.0,
    };
    (
        TemperatureModel::with_config(solar_only, FlatNoise, WorldGeography::new(32, 250), planet),
        WorldGeography::new(32, 250),
    )
}

fn insolation(path: &Path) {
    let mut svg = Svg::new(900.0, 450.0);
    let axes = Axes {
        left: 80.0,
        top: 50.0,
        width: 560.0,
        height: 320.0,
        x: (-90.0, 90.0),
        y: (0.0, 0.5),
    };
    axes.draw(
        &mut svg,
        &AxisLabels {
            title: "Daily-average solar exposure S̄ (mean of max(0, cos θz) over 48 steps)",
            x: "latitude (°)",
            y: "S̄",
        },
    );
    let cases = [
        (
            "tilt 5°, northern summer (default)",
            5.0,
            0.25,
            SERIES[3],
            false,
        ),
        ("tilt 5°, equinox", 5.0, 0.0, SERIES[5], false),
        ("tilt 5°, northern winter", 5.0, 0.75, SERIES[0], false),
        ("tilt 23.44°, northern summer", 23.44, 0.25, SERIES[1], true),
    ];
    for (_, tilt, orbit, color, dashed) in cases {
        let (model, geography) = exposure_model(PlanetParams {
            orbit,
            axial_tilt_deg: tilt,
        });
        let points: Vec<_> = (0..=8000)
            .step_by(40)
            .map(|y| {
                let latitude = geography.latitude(y).to_degrees();
                (latitude, model.average_at(TilePos { x: 0, y }, 0.0))
            })
            .collect();
        axes.series(
            &mut svg,
            &points,
            color,
            if dashed {
                r#"stroke-dasharray="6 4""#
            } else {
                ""
            },
        );
    }
    let entries: Vec<_> = cases.iter().map(|c| (c.0, c.3, c.4)).collect();
    legend(&mut svg, 660.0, 80.0, &entries);
    svg.save(path);
}

fn temperature_profile(config: &WorldConfig, path: &Path) {
    let climate = &config.world.climate;
    let no_region = TemperatureCoefficients {
        region: 0.0,
        ..climate.coefficients
    };
    let geography = WorldGeography::new(config.world.chunk_size, config.world.world_size_chunks);
    let model = TemperatureModel::with_config(
        no_region,
        FlatNoise,
        WorldGeography::new(config.world.chunk_size, config.world.world_size_chunks),
        climate.planet,
    );
    let size = i64::from(geography.world_height);
    let curve = |height: f64| -> Vec<(f64, f64)> {
        (0..=size)
            .step_by(40)
            .map(|y| {
                (
                    geography.latitude(y).to_degrees(),
                    model.average_at(TilePos { x: 0, y }, height),
                )
            })
            .collect()
    };
    let mut svg = Svg::new(900.0, 450.0);
    let axes = Axes {
        left: 80.0,
        top: 50.0,
        width: 560.0,
        height: 320.0,
        x: (-90.0, 90.0),
        y: (-50.0, 40.0),
    };
    axes.draw(
        &mut svg,
        &AxisLabels {
            title: "Climate temperature by latitude (default planet, northern summer)",
            x: "latitude (°)",
            y: "temperature (°C)",
        },
    );
    let middle = curve(0.5);
    let amplitude = climate.coefficients.region;
    let mut band: Vec<_> = middle
        .iter()
        .map(|&(x, t)| axes.point((x, t + amplitude)))
        .collect();
    band.extend(
        middle
            .iter()
            .rev()
            .map(|&(x, t)| axes.point((x, t - amplitude))),
    );
    svg.polygon(&band, SERIES[2], r#"fill-opacity="0.12""#);
    axes.series(&mut svg, &curve(0.0), SERIES[3], "");
    axes.series(&mut svg, &middle, SERIES[2], "");
    axes.series(&mut svg, &curve(1.0), SERIES[0], "");
    axes.series(
        &mut svg,
        &[(-90.0, 0.0), (90.0, 0.0)],
        "#a0aec0",
        r#"stroke-dasharray="4 4" stroke-width="1""#,
    );
    legend(
        &mut svg,
        660.0,
        80.0,
        &[
            ("height 0", SERIES[3], false),
            ("height 0.5", SERIES[2], false),
            ("height 1", SERIES[0], false),
        ],
    );
    svg.rect(660.0, 132.0, 24.0, 12.0, SERIES[2], "none");
    svg.text(690.0, 142.0, "±10 °C regional noise", TextStyle::new(11.0));
    svg.save(path);
}

fn draw_rules(svg: &mut Svg, axes: &Axes, map: &BiomeMapDefinition, label_size: f64) {
    for rule in map.0.iter().rev() {
        let (x0, x1) = (axes.px(rule.temperature.min), axes.px(rule.temperature.max));
        let (y0, y1) = (axes.py(rule.height.max), axes.py(rule.height.min));
        svg.rect(x0, y0, x1 - x0, y1 - y0, biome_hex(&rule.biome), "#ffffff");
    }
    for rule in map.0.iter().rev() {
        let (x0, x1) = (axes.px(rule.temperature.min), axes.px(rule.temperature.max));
        let (y0, y1) = (axes.py(rule.height.max), axes.py(rule.height.min));
        let words: Vec<&str> = rule.biome.split('_').collect();
        let dark = is_dark(biome_hex(&rule.biome));
        let style =
            TextStyle::new(label_size)
                .middle()
                .color(if dark { "#ffffff" } else { "#1a202c" });
        svg.lines((x0 + x1) / 2.0, (y0 + y1) / 2.0, &words, style);
    }
}

fn is_dark(hex: &str) -> bool {
    let value =
        |i: usize| f64::from(u8::from_str_radix(&hex[1 + 2 * i..3 + 2 * i], 16).unwrap_or(0));
    0.299 * value(0) + 0.587 * value(1) + 0.114 * value(2) < 120.0
}

fn default_biome_rules(config: &WorldConfig, path: &Path) {
    let mut svg = Svg::new(1180.0, 660.0);
    let axes = Axes {
        left: 70.0,
        top: 50.0,
        width: 1080.0,
        height: 540.0,
        x: (-50.0, 40.0),
        y: (0.0, 1.0),
    };
    axes.draw(
        &mut svg,
        &AxisLabels {
            title: "Default biome map (biome_map.ron): first matching rule wins",
            x: "final temperature (°C)",
            y: "final height",
        },
    );
    draw_rules(&mut svg, &axes, &config.biome_map, 9.0);
    svg.save(path);
}

fn feature_biome_rules(config: &WorldConfig, path: &Path) {
    let maps = [
        ("Craters", &config.craters.biome_map),
        ("Basins", &config.basins.biome_map),
        ("Volcanoes", &config.volcanoes.biome_map),
    ];
    let mut svg = Svg::new(1180.0, 420.0);
    for (index, (name, map)) in maps.iter().enumerate() {
        let axes = Axes {
            left: 70.0 + index as f64 * 375.0,
            top: 50.0,
            width: 320.0,
            height: 300.0,
            x: (-50.0, 40.0),
            y: (0.0, 1.0),
        };
        let title = format!("{name}: territory biome map");
        axes.draw(
            &mut svg,
            &AxisLabels {
                title: &title,
                x: "temperature (°C)",
                y: "height",
            },
        );
        draw_rules(&mut svg, &axes, map, 9.5);
    }
    svg.save(path);
}
