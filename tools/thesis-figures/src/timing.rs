//! Chunk generation cost for progressively complete pipelines.

use std::{path::Path, thread, time::Instant};

use worldgen::{ChunkPos, WorldConfig, WorldGenerator};

use crate::chart::{Axes, AxisLabels, SERIES};
use crate::svg::{Svg, TextStyle};

const BLOCK: i64 = 24;
const REPEATS: usize = 3;
const ORIGIN: i64 = 100;

pub struct Timing {
    pub label: &'static str,
    pub ms_per_chunk: f64,
}

pub fn without_features(config: &WorldConfig) -> WorldConfig {
    let mut config = config.clone();
    config.basins.probability = 0.0;
    config.craters.density = 0.0;
    config.volcanoes.probability = 0.0;
    config
}

fn without_objects(config: &WorldConfig) -> WorldConfig {
    let mut config = config.clone();
    config.objects.sets.values_mut().for_each(Vec::clear);
    config
        .biomes
        .biomes
        .iter_mut()
        .for_each(|biome| biome.objects.clear());
    config
}

fn block() -> impl Iterator<Item = ChunkPos> {
    (0..BLOCK).flat_map(|y| {
        (0..BLOCK).map(move |x| ChunkPos {
            x: ORIGIN + x,
            y: ORIGIN + y,
        })
    })
}

/// Mean single-thread time per chunk over a 24 × 24 block, from a fresh generator.
fn single_thread(generator: &WorldGenerator) -> f64 {
    let start = Instant::now();
    for pos in block() {
        generator.chunk_at(pos).expect("chunk");
    }
    start.elapsed().as_secs_f64() * 1000.0 / (BLOCK * BLOCK) as f64
}

/// Wall time per chunk when all hardware threads share one generator.
fn parallel(generator: &WorldGenerator, threads: usize) -> f64 {
    let positions: Vec<_> = block().collect();
    let start = Instant::now();
    thread::scope(|scope| {
        for part in positions.chunks(positions.len().div_ceil(threads)) {
            scope.spawn(move || {
                part.iter()
                    .for_each(|&pos| drop(generator.chunk_at(pos).expect("chunk")))
            });
        }
    });
    start.elapsed().as_secs_f64() * 1000.0 / positions.len() as f64
}

pub fn measure(config: &WorldConfig, path: &Path) -> Vec<Timing> {
    let build = |config: WorldConfig| config.build().expect("generator");
    let best = |config: &WorldConfig, run: &dyn Fn(&WorldGenerator) -> f64| {
        (0..REPEATS)
            .map(|_| run(&build(config.clone())))
            .fold(f64::INFINITY, f64::min)
    };
    let terrain = without_objects(&without_features(config));
    let mut timings = vec![
        Timing {
            label: "noise + climate + biome",
            ms_per_chunk: best(&terrain, &single_thread),
        },
        Timing {
            label: "+ terrain features",
            ms_per_chunk: best(&without_objects(config), &single_thread),
        },
        Timing {
            label: "+ object placement",
            ms_per_chunk: best(config, &single_thread),
        },
    ];
    let threads = thread::available_parallelism().map_or(4, |n| n.get());
    let ms = best(config, &|generator| parallel(generator, threads));
    let label: &'static str = Box::leak(format!("full, {threads} threads").into_boxed_str());
    timings.push(Timing {
        label,
        ms_per_chunk: ms,
    });
    chart(&timings, path);
    timings
}

fn chart(timings: &[Timing], path: &Path) {
    let peak = timings.iter().map(|t| t.ms_per_chunk).fold(0.0, f64::max);
    let mut svg = Svg::new(820.0, 400.0);
    let axes = Axes {
        left: 80.0,
        top: 50.0,
        width: 620.0,
        height: 260.0,
        x: (-0.5, timings.len() as f64 - 0.5),
        y: (0.0, peak * 1.2),
    };
    axes.draw_categorical(
        &mut svg,
        &AxisLabels {
            title: "Time per 32 × 32 chunk (24 × 24 chunks, best of 3, release build, cold caches)",
            x: "",
            y: "milliseconds per chunk",
        },
    );
    for (index, timing) in timings.iter().enumerate() {
        let x = index as f64;
        axes.bars(
            &mut svg,
            &[(x, timing.ms_per_chunk)],
            0.6,
            SERIES[index.min(3)],
        );
        svg.text(
            axes.px(x),
            axes.py(timing.ms_per_chunk) - 6.0,
            &format!("{:.2} ms", timing.ms_per_chunk),
            TextStyle::new(11.0).middle().bold(),
        );
        svg.text(
            axes.px(x),
            axes.top + axes.height + 18.0,
            timing.label,
            TextStyle::new(11.0).middle(),
        );
    }
    svg.save(path);
}
