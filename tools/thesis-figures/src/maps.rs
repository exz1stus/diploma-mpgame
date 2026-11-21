//! Raster maps of a generated world, plus statistics and transects sampled from it.

use std::{collections::HashMap, fs::File, io::BufWriter, path::Path, thread};

use worldgen::{BiomeId, ChunkPos, FeatureKind, TileData, TilePos, WorldGenerator};

use crate::chart::{Axes, AxisLabels, SERIES, legend};
use crate::palette::{self, Rgb, TERRITORY};
use crate::svg::{MUTED, Svg, TextStyle};

/// Every `step`-th tile of the world, sampled in parallel.
pub struct WorldGrid {
    pub origin: TilePos,
    pub side: usize,
    pub step: i64,
    pub tiles: Vec<TileData>,
}

impl WorldGrid {
    pub fn sample(generator: &WorldGenerator, step: i64) -> Self {
        let world = i64::from(generator.config().world_size_chunks * generator.config().chunk_size);
        Self::sample_region(generator, TilePos { x: 0, y: 0 }, world, step)
    }

    /// A square of `extent` tiles starting at `origin`.
    pub fn sample_region(
        generator: &WorldGenerator,
        origin: TilePos,
        extent: i64,
        step: i64,
    ) -> Self {
        let side = (extent / step) as usize;
        let threads = thread::available_parallelism().map_or(4, |n| n.get());
        let rows_per_thread = side.div_ceil(threads);
        let tiles = thread::scope(|scope| {
            let jobs: Vec<_> = (0..threads)
                .map(|t| {
                    scope.spawn(move || {
                        let rows = t * rows_per_thread..((t + 1) * rows_per_thread).min(side);
                        rows.flat_map(|row| (0..side).map(move |col| (row, col)))
                            .map(|(row, col)| {
                                let pos = TilePos {
                                    x: origin.x + col as i64 * step,
                                    y: origin.y + row as i64 * step,
                                };
                                generator.tile_data_at(pos)
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            jobs.into_iter()
                .flat_map(|job| job.join().expect("sampling thread"))
                .collect()
        });
        Self {
            origin,
            side,
            step,
            tiles,
        }
    }

    /// Centre (in tiles) of the `window`-sample square with the most distinct biomes,
    /// favouring windows where iron deposits can spawn.
    pub fn most_varied_window(&self, generator: &WorldGenerator, window: usize) -> TilePos {
        let iron = [
            "iron_rich_regolith",
            "weathered_basalt",
            "volcanic_highlands",
        ]
        .map(|name| generator.biomes().id(name));
        let mut best = (0, TilePos { x: 0, y: 0 });
        for row in (0..self.side - window).step_by(8) {
            for col in (0..self.side - window).step_by(8) {
                let mut seen = std::collections::HashSet::new();
                for r in (row..row + window).step_by(2) {
                    for c in (col..col + window).step_by(2) {
                        seen.insert(self.at(c, r).biome);
                    }
                }
                let bonus = if iron.iter().flatten().any(|id| seen.contains(id)) {
                    4
                } else {
                    0
                };
                let center = TilePos {
                    x: self.origin.x + (col + window / 2) as i64 * self.step,
                    y: self.origin.y + (row + window / 2) as i64 * self.step,
                };
                if seen.len() + bonus > best.0 {
                    best = (seen.len() + bonus, center);
                }
            }
        }
        best.1
    }

    fn at(&self, col: usize, row: usize) -> &TileData {
        &self.tiles[row.min(self.side - 1) * self.side + col.min(self.side - 1)]
    }

    /// Lambertian light from the north-west, from height differences between grid samples.
    fn light(&self, col: usize, row: usize, exaggeration: f64) -> f64 {
        let h = |c: usize, r: usize| self.at(c, r).height;
        let dx =
            (h(col + 1, row) - h(col.saturating_sub(1), row)) * exaggeration / self.step as f64;
        let dy =
            (h(col, row + 1) - h(col, row.saturating_sub(1))) * exaggeration / self.step as f64;
        let normal = [-dx, -dy, 1.0];
        let length = (normal[0] * normal[0] + normal[1] * normal[1] + 1.0).sqrt();
        let sun = [-0.5, -0.5, 0.707];
        let lit = (normal[0] * sun[0] + normal[1] * sun[1] + normal[2] * sun[2]) / length;
        0.55 + 0.6 * lit.max(0.0)
    }
}

/// Index into [`TERRITORY`] of the feature whose territory contains the tile (0 = none).
pub fn territory(tile: &TileData) -> usize {
    match tile.feature.map(|mark| mark.kind) {
        None => 0,
        Some(FeatureKind::Basin) => 1,
        Some(FeatureKind::Crater) => 2,
        Some(FeatureKind::Volcano) => 3,
    }
}

pub fn write_png(path: &Path, width: usize, height: usize, rgb: &[u8]) {
    let file = BufWriter::new(File::create(path).expect("create png"));
    let mut encoder = png::Encoder::new(file, width as u32, height as u32);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("png header");
    writer.write_image_data(rgb).expect("png data");
}

fn raster(grid: &WorldGrid, path: &Path, color: impl Fn(usize, usize, &TileData) -> Rgb) {
    let mut rgb = Vec::with_capacity(grid.side * grid.side * 3);
    for row in 0..grid.side {
        for col in 0..grid.side {
            rgb.extend_from_slice(&color(col, row, grid.at(col, row)));
        }
    }
    write_png(path, grid.side, grid.side, &rgb);
}

pub fn world_maps(base: &WorldGrid, full: &WorldGrid, generator: &WorldGenerator, out: &Path) {
    let biome_name = |id: BiomeId| generator.biomes().get(id).map_or("", |b| b.name.as_str());
    const RELIEF: f64 = 150.0;
    raster(base, &out.join("map_base_elevation.png"), |_, _, t| {
        palette::elevation_rgb(t.height)
    });
    raster(full, &out.join("map_final_height.png"), |c, r, t| {
        palette::shade(palette::elevation_rgb(t.height), full.light(c, r, RELIEF))
    });
    raster(full, &out.join("map_temperature.png"), |_, _, t| {
        palette::temperature_rgb(t.temperature)
    });
    raster(full, &out.join("map_territories.png"), |c, r, t| {
        let hex = TERRITORY[territory(t)].1;
        palette::shade(hex_rgb(hex), full.light(c, r, RELIEF))
    });
    raster(full, &out.join("map_biomes.png"), |c, r, t| {
        palette::shade(
            palette::biome_rgb(biome_name(t.biome)),
            full.light(c, r, RELIEF),
        )
    });
}

/// Height added (red) or removed (blue) by terrain features: final minus base height.
pub fn feature_delta(base: &WorldGrid, full: &WorldGrid, path: &Path) {
    raster(full, path, |c, r, t| {
        palette::delta_rgb(t.height - base.at(c, r).height)
    });
}

fn hex_rgb(hex: &str) -> Rgb {
    let channel = |i: usize| u8::from_str_radix(&hex[1 + 2 * i..3 + 2 * i], 16).unwrap_or(0);
    [channel(0), channel(1), channel(2)]
}

pub fn map_legends(out: &Path) {
    let mut svg = Svg::new(900.0, 210.0);
    let gradient = |svg: &mut Svg,
                    (x, y): (f64, f64),
                    title: &str,
                    ticks: &[(f64, &str)],
                    color: &dyn Fn(f64) -> Rgb| {
        svg.text(x, y, title, TextStyle::new(12.0).bold());
        for i in 0..240 {
            let t = f64::from(i) / 239.0;
            let [r, g, b] = color(t);
            svg.rect(
                x + f64::from(i),
                y + 10.0,
                1.2,
                22.0,
                &format!("#{r:02x}{g:02x}{b:02x}"),
                "none",
            );
        }
        for (at, label) in ticks {
            svg.text(
                x + at * 240.0,
                y + 48.0,
                label,
                TextStyle::new(10.0).middle().color(MUTED),
            );
        }
    };
    gradient(
        &mut svg,
        (20.0, 30.0),
        "Height",
        &[(0.0, "0"), (0.5, "0.5"), (1.0, "1")],
        &palette::elevation_rgb,
    );
    gradient(
        &mut svg,
        (320.0, 30.0),
        "Temperature (°C)",
        &[(0.0, "−50"), (5.0 / 9.0, "0"), (1.0, "40")],
        &|t| palette::temperature_rgb(-50.0 + 90.0 * t),
    );
    gradient(
        &mut svg,
        (20.0, 130.0),
        "Height change by features",
        &[(0.0, "−0.3"), (0.5, "0"), (1.0, "+0.3")],
        &|t| palette::delta_rgb(-0.3 + 0.6 * t),
    );
    svg.text(
        620.0,
        30.0,
        "Feature territory",
        TextStyle::new(12.0).bold(),
    );
    for (index, (label, hex)) in TERRITORY.iter().enumerate() {
        let y = 40.0 + index as f64 * 22.0;
        svg.rect(620.0, y, 16.0, 16.0, hex, "#a0aec0");
        svg.text(642.0, y + 12.0, label, TextStyle::new(11.0));
    }
    svg.save(&out.join("map_legends.svg"));
}

pub fn distributions(full: &WorldGrid, path: &Path) {
    let histogram = |values: &mut dyn Iterator<Item = f64>, range: (f64, f64), bins: usize| {
        let mut counts = vec![0usize; bins];
        let mut total = 0usize;
        for value in values {
            let bin = ((value - range.0) / (range.1 - range.0) * bins as f64).floor();
            counts[(bin.max(0.0) as usize).min(bins - 1)] += 1;
            total += 1;
        }
        let width = (range.1 - range.0) / bins as f64;
        counts
            .iter()
            .enumerate()
            .map(|(i, &c)| (range.0 + (i as f64 + 0.5) * width, c as f64 / total as f64))
            .collect::<Vec<_>>()
    };
    let mut svg = Svg::new(1000.0, 400.0);
    let heights = histogram(&mut full.tiles.iter().map(|t| t.height), (0.0, 1.0), 50);
    let peak = heights.iter().map(|b| b.1).fold(0.0, f64::max);
    let axes = Axes {
        left: 80.0,
        top: 50.0,
        width: 360.0,
        height: 270.0,
        x: (0.0, 1.0),
        y: (0.0, peak * 1.1),
    };
    axes.draw(
        &mut svg,
        &AxisLabels {
            title: "Final height distribution",
            x: "height",
            y: "share of tiles",
        },
    );
    axes.bars(&mut svg, &heights, 0.018, SERIES[0]);
    let temperatures = histogram(
        &mut full.tiles.iter().map(|t| t.temperature),
        (-50.0, 40.0),
        45,
    );
    let peak = temperatures.iter().map(|b| b.1).fold(0.0, f64::max);
    let axes = Axes {
        left: 580.0,
        top: 50.0,
        width: 360.0,
        height: 270.0,
        x: (-50.0, 40.0),
        y: (0.0, peak * 1.1),
    };
    axes.draw(
        &mut svg,
        &AxisLabels {
            title: "Temperature distribution",
            x: "temperature (°C)",
            y: "share of tiles",
        },
    );
    axes.bars(&mut svg, &temperatures, 1.7, SERIES[3]);
    svg.save(path);
}

/// Share of tiles per biome, largest first. Returns the shares for the report.
pub fn biome_shares(
    full: &WorldGrid,
    generator: &WorldGenerator,
    path: &Path,
) -> Vec<(String, f64)> {
    let mut counts: HashMap<BiomeId, usize> = HashMap::new();
    for tile in &full.tiles {
        *counts.entry(tile.biome).or_default() += 1;
    }
    let mut shares: Vec<(String, f64)> = counts
        .into_iter()
        .map(|(id, n)| {
            (
                generator.biomes().get(id).unwrap().name.clone(),
                n as f64 / full.tiles.len() as f64,
            )
        })
        .collect();
    shares.sort_by(|a, b| b.1.total_cmp(&a.1));
    let row = 19.0;
    let mut svg = Svg::new(900.0, 80.0 + row * shares.len() as f64);
    svg.text(
        20.0,
        30.0,
        &format!(
            "Biome coverage ({} of {} biomes occur)",
            shares.len(),
            generator.biomes().biomes().len()
        ),
        TextStyle::new(14.0).bold(),
    );
    let max = shares[0].1;
    for (index, (name, share)) in shares.iter().enumerate() {
        let y = 50.0 + index as f64 * row;
        svg.text(250.0, y + 13.0, name, TextStyle::new(11.0).end());
        svg.rect(
            260.0,
            y + 2.0,
            share / max * 540.0,
            row - 5.0,
            palette::biome_hex(name),
            "#a0aec0",
        );
        svg.text(
            266.0 + share / max * 540.0,
            y + 13.0,
            &format!("{:.2} %", share * 100.0),
            TextStyle::new(10.0).color(MUTED),
        );
    }
    svg.save(path);
    shares
}

/// Largest connected group of grid samples owned by `owner`; returns its centre in tiles.
pub fn largest_territory(full: &WorldGrid, owner: usize) -> Option<TilePos> {
    let side = full.side;
    let mut seen = vec![false; side * side];
    let mut best: Option<(usize, (f64, f64))> = None;
    for start in 0..side * side {
        if seen[start] || territory(&full.tiles[start]) != owner {
            continue;
        }
        let (mut stack, mut count, mut sum) = (vec![start], 0usize, (0.0, 0.0));
        seen[start] = true;
        while let Some(index) = stack.pop() {
            let (row, col) = (index / side, index % side);
            count += 1;
            sum = (sum.0 + col as f64, sum.1 + row as f64);
            let neighbours = [
                (row.wrapping_sub(1), col),
                (row + 1, col),
                (row, col.wrapping_sub(1)),
                (row, col + 1),
            ];
            for (r, c) in neighbours {
                if r < side && c < side {
                    let next = r * side + c;
                    if !seen[next] && territory(&full.tiles[next]) == owner {
                        seen[next] = true;
                        stack.push(next);
                    }
                }
            }
        }
        if best.is_none_or(|(n, _)| count > n) {
            best = Some((count, (sum.0 / count as f64, sum.1 / count as f64)));
        }
    }
    best.map(|(_, (col, row))| TilePos {
        x: (col * full.step as f64) as i64,
        y: (row * full.step as f64) as i64,
    })
}

/// Grid sample owned by `owner` where features changed height the most in direction
/// `sign` (+1 highest uplift, −1 deepest depression): a feature centre to cut through.
pub fn strongest_change(
    base: &WorldGrid,
    full: &WorldGrid,
    owner: usize,
    sign: f64,
) -> Option<TilePos> {
    (0..full.tiles.len())
        .filter(|&i| territory(&full.tiles[i]) == owner)
        .max_by(|&a, &b| {
            let change = |i: usize| sign * (full.tiles[i].height - base.tiles[i].height);
            change(a).total_cmp(&change(b))
        })
        .map(|i| TilePos {
            x: full.origin.x + (i % full.side) as i64 * full.step,
            y: full.origin.y + (i / full.side) as i64 * full.step,
        })
}

pub struct Transect<'a> {
    pub name: &'a str,
    pub center: TilePos,
    pub half_length: i64,
    pub owner: usize,
}

/// Height along a west–east line, with and without features; territory shaded.
pub fn transects(base: &WorldGenerator, full: &WorldGenerator, lines: &[Transect], path: &Path) {
    let mut svg = Svg::new(1000.0, 60.0 + 330.0 * lines.len() as f64);
    for (index, line) in lines.iter().enumerate() {
        let samples: Vec<(f64, f64, f64, bool)> = (-line.half_length..=line.half_length)
            .step_by(2)
            .map(|offset| {
                let pos = TilePos {
                    x: line.center.x + offset,
                    y: line.center.y,
                };
                let final_tile = full.tile_data_at(pos);
                let owned = territory(&final_tile) == line.owner;
                (
                    offset as f64,
                    base.tile_data_at(pos).height,
                    final_tile.height,
                    owned,
                )
            })
            .collect();
        let axes = Axes {
            left: 80.0,
            top: 50.0 + index as f64 * 330.0,
            width: 820.0,
            height: 240.0,
            x: (-line.half_length as f64, line.half_length as f64),
            y: (0.0, 1.0),
        };
        for pair in samples.windows(2).filter(|p| p[0].3) {
            axes.band(&mut svg, pair[0].0, pair[1].0, "#fefcbf");
        }
        let title = format!(
            "{} transect through tile ({}, {}), west → east",
            line.name, line.center.x, line.center.y
        );
        axes.draw(
            &mut svg,
            &AxisLabels {
                title: &title,
                x: "offset from centre (tiles)",
                y: "height",
            },
        );
        let before: Vec<_> = samples.iter().map(|s| (s.0, s.1)).collect();
        let after: Vec<_> = samples.iter().map(|s| (s.0, s.2)).collect();
        axes.series(
            &mut svg,
            &before,
            SERIES[5],
            r#"stroke-dasharray="5 4" stroke-width="1.4""#,
        );
        axes.series(&mut svg, &after, SERIES[3], "");
        legend(
            &mut svg,
            700.0,
            axes.top + 16.0,
            &[
                ("elevation noise only", SERIES[5], true),
                ("after features + clamp", SERIES[3], false),
            ],
        );
        svg.rect(700.0, axes.top + 46.0, 24.0, 10.0, "#fefcbf", "#d69e2e");
        svg.text(
            730.0,
            axes.top + 55.0,
            "feature territory",
            TextStyle::new(11.0),
        );
    }
    svg.save(path);
}

/// Full-resolution chunks around `center`, with objects and chunk borders.
/// Returns object counts by kind name.
pub fn zoom(
    generator: &WorldGenerator,
    center: TilePos,
    chunks: i64,
    path: &Path,
) -> HashMap<String, usize> {
    let config = generator.config();
    let size = i64::from(config.chunk_size);
    let max_chunk = i64::from(config.world_size_chunks) - chunks;
    let first = ChunkPos {
        x: (center.x / size - chunks / 2).clamp(0, max_chunk),
        y: (center.y / size - chunks / 2).clamp(0, max_chunk),
    };
    let (side, scale) = ((chunks * size) as usize, 3usize);
    let mut heights = vec![0.0f64; side * side];
    let mut colors = vec![[0u8; 3]; side * side];
    let mut objects = HashMap::new();
    let manifest = generator.manifest();
    for cy in 0..chunks {
        for cx in 0..chunks {
            let chunk = generator
                .chunk_at(ChunkPos {
                    x: first.x + cx,
                    y: first.y + cy,
                })
                .expect("chunk");
            for (index, sample) in chunk.tiles.iter().enumerate() {
                let (x, y) = (
                    (cx * size) as usize + index % size as usize,
                    (cy * size) as usize + index / size as usize,
                );
                heights[y * side + x] = f64::from(sample.height());
                colors[y * side + x] =
                    palette::biome_rgb(manifest.biome_name(sample.biome).unwrap_or(""));
            }
            for object in chunk.objects {
                objects.insert(object.id, object);
            }
        }
    }
    let light = |x: usize, y: usize| {
        let h = |x: usize, y: usize| heights[y.min(side - 1) * side + x.min(side - 1)];
        let dx = (h(x + 1, y) - h(x.saturating_sub(1), y)) * 40.0;
        let dy = (h(x, y + 1) - h(x, y.saturating_sub(1))) * 40.0;
        let lit = (0.5 * dx + 0.5 * dy + 0.707) / (dx * dx + dy * dy + 1.0).sqrt();
        0.55 + 0.6 * lit.max(0.0)
    };
    let mut pixels: Vec<Rgb> = (0..side * side)
        .map(|i| palette::shade(colors[i], light(i % side, i / side)))
        .collect();
    let origin = TilePos {
        x: first.x * size,
        y: first.y * size,
    };
    let mut counts = HashMap::new();
    for object in objects.values() {
        let name = manifest
            .object(object.kind)
            .map_or("?", |k| k.name.as_str());
        *counts.entry(name.to_string()).or_default() += 1;
        for dy in 0..i64::from(object.footprint.height) {
            for dx in 0..i64::from(object.footprint.width) {
                let (x, y) = (
                    object.id.anchor.x + dx - origin.x,
                    object.id.anchor.y + dy - origin.y,
                );
                if (0..side as i64).contains(&x) && (0..side as i64).contains(&y) {
                    pixels[y as usize * side + x as usize] = palette::object_rgb(name);
                }
            }
        }
    }
    let out_side = side * scale;
    let mut rgb = Vec::with_capacity(out_side * out_side * 3);
    for py in 0..out_side {
        for px in 0..out_side {
            let (x, y) = (px / scale, py / scale);
            let border = (px % (size as usize * scale) == 0) || (py % (size as usize * scale) == 0);
            let color = if border {
                palette::shade(pixels[y * side + x], 0.7)
            } else {
                pixels[y * side + x]
            };
            rgb.extend_from_slice(&color);
        }
    }
    write_png(path, out_side, out_side, &rgb);
    counts
}

pub fn zoom_legend(counts: &HashMap<String, usize>, path: &Path) {
    let mut svg = Svg::new(900.0, 60.0);
    let mut names: Vec<_> = counts.keys().cloned().collect();
    names.sort();
    for (index, name) in names.iter().enumerate() {
        let x = 20.0 + index as f64 * 210.0;
        svg.rect(x, 20.0, 16.0, 16.0, &palette::object_hex(name), "#4a5568");
        svg.text(
            x + 22.0,
            33.0,
            &format!("{name} ({})", counts[name]),
            TextStyle::new(12.0),
        );
    }
    svg.save(path);
}
