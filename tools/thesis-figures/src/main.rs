//! Renders the figures of the world generation chapter from the real generator.
//!
//! cargo run --release -p thesis-figures -- docs/figures/worldgen

mod chart;
mod curves;
mod diagrams;
mod maps;
mod palette;
mod svg;
mod timing;

use std::{fs, path::PathBuf};

use maps::{Transect, WorldGrid};
use worldgen::{TilePos, WorldConfig};

/// World tiles per map pixel: an 8000-tile world becomes a 1000-pixel map.
const MAP_STEP: i64 = 8;
/// Side of the higher-resolution relief map, in tiles.
const REGION: i64 = 2560;
/// Side of the full-resolution object map, in chunks.
const ZOOM_CHUNKS: i64 = 12;

fn main() {
    let out = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "docs/figures/worldgen".into()),
    );
    fs::create_dir_all(&out).expect("create output folder");
    let config = WorldConfig::bundled();

    diagrams::render_all(&out);
    curves::render_all(&config, &out);
    println!("diagrams and charts written");

    let full = config.build().expect("generator");
    let base = timing::without_features(&config)
        .build()
        .expect("generator");
    let full_grid = WorldGrid::sample(&full, MAP_STEP);
    let base_grid = WorldGrid::sample(&base, MAP_STEP);
    maps::world_maps(&base_grid, &full_grid, &full, &out);
    maps::map_legends(&out);
    maps::distributions(&full_grid, &out.join("distributions.svg"));
    let shares = maps::biome_shares(&full_grid, &full, &out.join("biome_coverage.svg"));
    report_territories(&full_grid);
    println!("top biomes: {:?}", &shares[..shares.len().min(8)]);

    let crater = maps::largest_territory(&full_grid, 2).expect("a crater");
    let centre = |owner, sign| {
        maps::strongest_change(&base_grid, &full_grid, owner, sign).expect("a feature")
    };
    let lines = [
        Transect {
            name: "Volcano",
            center: centre(3, 1.0),
            half_length: 1100,
            owner: 3,
        },
        Transect {
            name: "Crater",
            center: centre(2, -1.0),
            half_length: 600,
            owner: 2,
        },
        Transect {
            name: "Basin",
            center: centre(1, -1.0),
            half_length: 2600,
            owner: 1,
        },
    ];
    println!(
        "transect centres {:?}",
        lines.iter().map(|l| l.center).collect::<Vec<_>>()
    );
    maps::transects(&base, &full, &lines, &out.join("transects.svg"));
    let region_origin = TilePos {
        x: crater.x - REGION / 2,
        y: crater.y - REGION / 2,
    };
    let region = WorldGrid::sample_region(&full, region_origin, REGION, 2);
    let region_base = WorldGrid::sample_region(&base, region_origin, REGION, 2);
    maps::feature_delta(&region_base, &region, &out.join("region_feature_delta.png"));
    println!("feature delta region origin {region_origin:?}, {REGION} tiles");

    let zoom_center = full_grid.most_varied_window(&full, (ZOOM_CHUNKS * 32 / MAP_STEP) as usize);
    println!("zoom centre {zoom_center:?}");
    let counts = maps::zoom(
        &full,
        zoom_center,
        ZOOM_CHUNKS,
        &out.join("zoom_objects.png"),
    );
    maps::zoom_legend(&counts, &out.join("zoom_legend.svg"));
    println!("objects in zoom: {counts:?}");

    for timing in timing::measure(&config, &out.join("timing.svg")) {
        println!("{:<28} {:.3} ms/chunk", timing.label, timing.ms_per_chunk);
    }
}

fn report_territories(grid: &WorldGrid) {
    let mut counts = [0usize; 4];
    for tile in &grid.tiles {
        counts[maps::territory(tile)] += 1;
    }
    let total = grid.tiles.len() as f64;
    let shares: Vec<_> = counts
        .iter()
        .map(|&c| format!("{:.2}%", c as f64 / total * 100.0))
        .collect();
    println!("territory shares [none, basin, crater, volcano]: {shares:?}");
}
