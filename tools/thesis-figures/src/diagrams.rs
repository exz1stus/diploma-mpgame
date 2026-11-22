//! Schematic figures: architecture, pipeline, chunk flow, seeds, configuration,
//! object placement and streaming. Placement and streaming states are computed
//! with the same rules the generator and server use.

use std::path::Path;

use crate::svg::{MUTED, Svg, TextStyle};

const BOX: &str = "#edf2f7";
const ACCENT: &str = "#ebf8ff";
const WARM: &str = "#fffaf0";
const STROKE: &str = "#4a5568";

pub fn render_all(out: &Path) {
    architecture(&out.join("architecture.svg"));
    stage_pipeline(&out.join("stage_pipeline.svg"));
    chunk_flow(&out.join("chunk_flow.svg"));
    seed_derivation(&out.join("seed_derivation.svg"));
    config_files(&out.join("config_files.svg"));
    placement(&out.join("placement_resolution.svg"));
    streaming(&out.join("streaming.svg"));
}

/// A rounded box with a bold title and smaller detail lines.
fn node(
    svg: &mut Svg,
    (x, y, w, h): (f64, f64, f64, f64),
    title: &str,
    details: &[&str],
    fill: &str,
) {
    svg.rect_styled(x, y, w, h, fill, STROKE, r#"rx="6""#);
    let mut lines = vec![title];
    lines.extend_from_slice(details);
    let step = 15.0;
    let top = y + h / 2.0 - step * (lines.len() as f64 - 1.0) / 2.0 + 4.0;
    svg.text(
        x + w / 2.0,
        top,
        title,
        TextStyle::new(13.0).bold().middle(),
    );
    for (index, line) in details.iter().enumerate() {
        svg.text(
            x + w / 2.0,
            top + step * (index + 1) as f64,
            line,
            TextStyle::new(11.0).middle().color("#2d3748"),
        );
    }
}

fn architecture(path: &Path) {
    let mut svg = Svg::new(1100.0, 600.0);
    let title = TextStyle::new(12.0).bold().color(MUTED);
    svg.text(30.0, 30.0, "BUILD TIME (once per server start)", title);
    svg.text(30.0, 345.0, "RUN TIME (per chunk request)", title);
    svg.line_styled(
        (20.0, 320.0),
        (1080.0, 320.0),
        "#cbd5e0",
        1.0,
        r#"stroke-dasharray="6 5""#,
    );

    node(
        &mut svg,
        (30.0, 50.0, 200.0, 190.0),
        "Config files (RON)",
        &[
            "world.ron",
            "biomes.ron",
            "biome_map.ron",
            "objects.ron",
            "features/craters.ron",
            "features/basins.ron",
            "features/volcanoes.ron",
        ],
        WARM,
    );
    node(
        &mut svg,
        (30.0, 255.0, 200.0, 50.0),
        "Override folder",
        &["same layout, per-file"],
        WARM,
    );
    node(
        &mut svg,
        (280.0, 105.0, 170.0, 80.0),
        "ConfigSource",
        &["bundled text +", "overridden files"],
        BOX,
    );
    node(
        &mut svg,
        (490.0, 105.0, 170.0, 80.0),
        "WorldConfig::load",
        &["parse each file", "typed schema"],
        BOX,
    );
    node(
        &mut svg,
        (700.0, 105.0, 170.0, 80.0),
        "WorldConfig::build",
        &["validate, resolve names", "assemble stages"],
        BOX,
    );
    node(
        &mut svg,
        (910.0, 95.0, 170.0, 100.0),
        "WorldGenerator",
        &["immutable", "Send + Sync", "shared via Arc"],
        ACCENT,
    );
    svg.arrow((230.0, 145.0), (278.0, 145.0));
    svg.arrow((230.0, 280.0), (300.0, 187.0));
    svg.arrow((450.0, 145.0), (488.0, 145.0));
    svg.arrow((660.0, 145.0), (698.0, 145.0));
    svg.arrow((870.0, 145.0), (908.0, 145.0));

    node(
        &mut svg,
        (900.0, 375.0, 180.0, 90.0),
        "ChunkStreamer",
        &[
            "observers → wanted set",
            "async generation",
            "nearest first",
        ],
        ACCENT,
    );
    node(
        &mut svg,
        (650.0, 375.0, 190.0, 90.0),
        "Bevy ECS world",
        &["chunk entities", "object entities", "gameplay state"],
        BOX,
    );
    node(
        &mut svg,
        (650.0, 500.0, 190.0, 75.0),
        "ChunkStorage",
        &["recent ring", "lz4 + postcard archive"],
        BOX,
    );
    node(
        &mut svg,
        (400.0, 375.0, 190.0, 90.0),
        "World streaming",
        &["WorldMessage", "encoded once", "per-client interest"],
        BOX,
    );
    node(
        &mut svg,
        (150.0, 375.0, 190.0, 90.0),
        "Client",
        &["WorldManifest", "biome → texture", "object → sprite"],
        WARM,
    );
    svg.arrow((995.0, 195.0), (995.0, 373.0));
    svg.text(
        1003.0,
        290.0,
        "chunk_at(pos)",
        TextStyle::new(11.0).color(MUTED),
    );
    svg.arrow((898.0, 420.0), (842.0, 420.0));
    svg.arrow((720.0, 465.0), (720.0, 498.0));
    svg.arrow((770.0, 498.0), (770.0, 467.0));
    svg.text(778.0, 488.0, "restore", TextStyle::new(10.0).color(MUTED));
    svg.text(
        640.0,
        488.0,
        "unload",
        TextStyle::new(10.0).color(MUTED).end(),
    );
    svg.arrow((648.0, 420.0), (592.0, 420.0));
    svg.arrow((398.0, 420.0), (342.0, 420.0));
    svg.save(path);
}

fn stage_pipeline(path: &Path) {
    const FIELDS: [&str; 5] = ["height", "temp_offset", "biome_map", "temperature", "biome"];
    let rows: [(&str, &str, [&str; 5]); 8] = [
        ("Elevation", "fBm(seed), 10 octaves", ["W", "", "", "", ""]),
        ("Basins", "cells 6144, coarse fBm", ["RW", "W", "W", "", ""]),
        ("Craters", "cells 1024", ["RW", "W", "W", "", ""]),
        ("Volcanoes", "cells 4096", ["RW", "W", "W", "", ""]),
        ("Clamp", "[0, 1]", ["RW", "", "", "", ""]),
        (
            "Temperature",
            "geography, sun, region fBm",
            ["R", "R", "", "W", ""],
        ),
        (
            "Biome",
            "feature map, then default",
            ["R", "", "R", "R", "W"],
        ),
        (
            "TileData",
            "result of draft_at(pos)",
            ["R", "", "", "R", "R"],
        ),
    ];
    let mut svg = Svg::new(1060.0, 560.0);
    let (left, top, row_h) = (60.0, 70.0, 54.0);
    let (name_w, input_w, field_w) = (170.0, 240.0, 112.0);
    let header = TextStyle::new(12.0).bold().middle();
    svg.text(left + name_w / 2.0, top - 18.0, "Stage", header);
    svg.text(
        left + name_w + input_w / 2.0,
        top - 18.0,
        "Sampling inputs",
        header,
    );
    for (index, field) in FIELDS.iter().enumerate() {
        let x = left + name_w + input_w + field_w * (index as f64 + 0.5);
        svg.text(x, top - 18.0, field, header);
    }
    svg.text(
        left,
        28.0,
        "TileDraft fields read (R) and written (W) by each stage",
        TextStyle::new(14.0).bold(),
    );
    for (row, (name, inputs, access)) in rows.iter().enumerate() {
        let y = top + row as f64 * row_h;
        let fill = if row == 7 { ACCENT } else { BOX };
        node(
            &mut svg,
            (left, y + 6.0, name_w - 14.0, row_h - 12.0),
            name,
            &[],
            fill,
        );
        svg.text(
            left + name_w + 8.0,
            y + row_h / 2.0 + 4.0,
            inputs,
            TextStyle::new(11.0).color("#2d3748"),
        );
        for (column, mark) in access.iter().enumerate() {
            let x = left + name_w + input_w + field_w * column as f64;
            let color = match *mark {
                "W" => "#fbd38d",
                "RW" => "#f6ad55",
                "R" => "#bee3f8",
                _ => "#f7fafc",
            };
            svg.rect(
                x + 4.0,
                y + 6.0,
                field_w - 8.0,
                row_h - 12.0,
                color,
                "#e2e8f0",
            );
            let label = if *mark == "RW" { "R / W" } else { mark };
            svg.text(
                x + field_w / 2.0,
                y + row_h / 2.0 + 5.0,
                label,
                TextStyle::new(13.0).bold().middle(),
            );
        }
        if row < 7 {
            svg.arrow(
                (left - 22.0, y + row_h / 2.0),
                (left - 22.0, y + row_h * 1.5 - 2.0),
            );
        }
    }
    svg.vertical_text(
        left - 34.0,
        top + row_h * 4.0,
        "stage order per tile",
        TextStyle::new(11.0).middle().color(MUTED),
    );
    let legend_y = top + row_h * 8.0 + 26.0;
    for (index, (label, color)) in [
        ("written", "#fbd38d"),
        ("read and written", "#f6ad55"),
        ("read", "#bee3f8"),
    ]
    .iter()
    .enumerate()
    {
        let x = left + index as f64 * 170.0;
        svg.rect(x, legend_y - 11.0, 16.0, 14.0, color, "#a0aec0");
        svg.text(x + 22.0, legend_y, label, TextStyle::new(11.0));
    }
    svg.save(path);
}

fn chunk_flow(path: &Path) {
    let mut svg = Svg::new(1100.0, 430.0);
    svg.text(
        20.0,
        28.0,
        "WorldGenerator::chunk_at(ChunkPos)",
        TextStyle::new(14.0).bold(),
    );
    let top = [
        (
            (20.0, 60.0, 150.0, 70.0),
            "ChunkPos",
            vec!["(x, y) in chunks"],
        ),
        (
            (210.0, 60.0, 170.0, 70.0),
            "Bounds check",
            vec!["inside the world?", "→ TileRect"],
        ),
        (
            (420.0, 60.0, 200.0, 70.0),
            "Sample tiles",
            vec!["stage pipeline per tile", "size × size, row-major"],
        ),
        (
            (660.0, 60.0, 200.0, 70.0),
            "Quantize",
            vec!["TileSample", "biome u16, height u16"],
        ),
    ];
    for (rect, title, lines) in &top {
        node(&mut svg, *rect, title, lines, BOX);
    }
    svg.arrow((170.0, 95.0), (208.0, 95.0));
    svg.arrow((380.0, 95.0), (418.0, 95.0));
    svg.arrow((620.0, 95.0), (658.0, 95.0));
    let bottom = [
        (
            (20.0, 270.0, 190.0, 80.0),
            "Anchor region",
            vec!["bounds − 2p … bounds + p", "p = max footprint − 1"],
        ),
        (
            (240.0, 270.0, 180.0, 80.0),
            "Proposals",
            vec!["per rule × anchor", "hash roll < density"],
        ),
        (
            (450.0, 270.0, 190.0, 80.0),
            "Terrain checks",
            vec!["biome, height, temperature", "slope under footprint"],
        ),
        (
            (670.0, 270.0, 190.0, 80.0),
            "Overlap resolution",
            vec!["accept local priority", "maxima only"],
        ),
    ];
    for (rect, title, lines) in &bottom {
        node(&mut svg, *rect, title, lines, WARM);
    }
    svg.arrow((210.0, 310.0), (238.0, 310.0));
    svg.arrow((420.0, 310.0), (448.0, 310.0));
    svg.arrow((640.0, 310.0), (668.0, 310.0));
    svg.arrow((295.0, 130.0), (115.0, 268.0));
    node(
        &mut svg,
        (420.0, 165.0, 200.0, 60.0),
        "Terrain cache",
        &["HashMap<TilePos, TileData>"],
        ACCENT,
    );
    let dashed = r#"stroke-dasharray="5 4""#;
    svg.line_styled((520.0, 130.0), (520.0, 165.0), STROKE, 1.2, dashed);
    svg.line_styled((545.0, 225.0), (545.0, 270.0), STROKE, 1.2, dashed);
    svg.text(
        628.0,
        200.0,
        "footprint checks reuse samples,",
        TextStyle::new(10.0).color(MUTED),
    );
    svg.text(
        628.0,
        213.0,
        "incl. tiles beyond the chunk",
        TextStyle::new(10.0).color(MUTED),
    );
    node(
        &mut svg,
        (910.0, 160.0, 170.0, 110.0),
        "ChunkData",
        &[
            "position, size",
            "tiles: Vec<TileSample>",
            "objects: Vec<",
            "PlacedObject>",
        ],
        ACCENT,
    );
    svg.arrow((860.0, 95.0), (990.0, 158.0));
    svg.arrow((860.0, 310.0), (990.0, 272.0));
    svg.text(20.0, 400.0, "Ground tiles (top row) and objects (bottom row) share one per-request terrain cache; no global occupancy state exists.", TextStyle::new(11.0).color(MUTED));
    svg.save(path);
}

fn seed_derivation(path: &Path) {
    let mut svg = Svg::new(1100.0, 470.0);
    svg.text(
        20.0,
        28.0,
        "Every random decision is a hash of the world seed and a coordinate",
        TextStyle::new(14.0).bold(),
    );
    node(
        &mut svg,
        (20.0, 200.0, 130.0, 60.0),
        "world seed",
        &["u64, world.ron"],
        ACCENT,
    );
    let rows: [(f64, &str, &str, &str); 4] = [
        (
            60.0,
            "seed",
            "elevation + basin-floor fBm",
            "Perlin gradient lattice",
        ),
        (
            150.0,
            "seed ⊕ REGION",
            "temperature-region fBm",
            "Perlin gradient lattice",
        ),
        (
            240.0,
            "seed ⊕ BASIN/CRATER/VOLCANO",
            "hash_cell(layer, cx, cy)",
            "ChaCha8 per cell",
        ),
        (
            330.0,
            "seed ⊕ OBJECTS ⊕ version≪32",
            "hash(rule id) per rule",
            "two streams per anchor",
        ),
    ];
    for (y, first, second, third) in rows {
        node(&mut svg, (210.0, y, 240.0, 56.0), first, &[], BOX);
        node(&mut svg, (500.0, y, 230.0, 56.0), second, &[], BOX);
        node(&mut svg, (780.0, y, 190.0, 56.0), third, &[], WARM);
        svg.arrow((150.0, 230.0), (208.0, y + 28.0));
        svg.arrow((450.0, y + 28.0), (498.0, y + 28.0));
        svg.arrow((730.0, y + 28.0), (778.0, y + 28.0));
    }
    let note = TextStyle::new(11.0).color(MUTED);
    svg.text(780.0, 312.0, "→ features of that cell", note);
    svg.text(
        500.0,
        404.0,
        "rule id = FNV-1a(\"biome/<name>/<object>\")",
        note,
    );
    svg.text(780.0, 404.0, "spawn: hash(r ⊕ SPAWN, x, y)", note);
    svg.text(780.0, 420.0, "priority: hash(r ⊕ PRIORITY, x, y)", note);
    svg.text(20.0, 455.0, "hash_cell(s, x, y) = mix64(s ⊕ mix64(x) ⊕ rotl(mix64(y), 32)), mix64 = SplitMix64 finalizer", note);
    svg.save(path);
}

fn config_files(path: &Path) {
    let mut svg = Svg::new(1000.0, 470.0);
    svg.text(
        20.0,
        28.0,
        "Configuration files and the references between them",
        TextStyle::new(14.0).bold(),
    );
    node(
        &mut svg,
        (30.0, 60.0, 230.0, 100.0),
        "world.ron",
        &[
            "seed, generator_version",
            "world & chunk size",
            "elevation noise, climate",
        ],
        WARM,
    );
    node(
        &mut svg,
        (380.0, 60.0, 240.0, 100.0),
        "features/*.ron",
        &[
            "craters, basins, volcanoes",
            "shape parameters",
            "territory biome map",
        ],
        WARM,
    );
    node(
        &mut svg,
        (30.0, 200.0, 230.0, 80.0),
        "biome_map.ron",
        &["default height × temperature", "classification rules"],
        WARM,
    );
    node(
        &mut svg,
        (380.0, 200.0, 240.0, 80.0),
        "biomes.ron",
        &["biome list (order = ID)", "object sets & spawns"],
        WARM,
    );
    node(
        &mut svg,
        (740.0, 200.0, 230.0, 80.0),
        "objects.ron",
        &["kinds + footprints (order = ID)", "named spawn sets"],
        WARM,
    );
    svg.arrow((260.0, 240.0), (378.0, 240.0));
    svg.text(
        268.0,
        232.0,
        "biome names",
        TextStyle::new(10.0).color(MUTED),
    );
    svg.arrow((500.0, 160.0), (500.0, 198.0));
    svg.text(
        508.0,
        184.0,
        "biome names",
        TextStyle::new(10.0).color(MUTED),
    );
    svg.arrow((620.0, 240.0), (738.0, 240.0));
    svg.text(
        628.0,
        232.0,
        "set & object names",
        TextStyle::new(10.0).color(MUTED),
    );
    node(
        &mut svg,
        (200.0, 330.0, 600.0, 115.0),
        "WorldConfig::build — validation before any tile is sampled",
        &[
            "numeric ranges (radii, probabilities, densities, noise parameters)",
            "every referenced biome, object and set exists; names and rule IDs are unique",
            "ID spaces fit u16; rule ranges are finite and min < max",
            "errors name the offending file",
        ],
        ACCENT,
    );
    for x in [145.0, 500.0, 855.0] {
        svg.line_styled(
            (x, 282.0),
            (x.clamp(260.0, 740.0), 328.0),
            "#a0aec0",
            1.0,
            r#"stroke-dasharray="4 4""#,
        );
    }
    svg.save(path);
}

struct Candidate {
    name: &'static str,
    x: i64,
    y: i64,
    w: i64,
    h: i64,
    priority: u32,
}

impl Candidate {
    fn overlaps(&self, other: &Self) -> bool {
        self.x < other.x + other.w
            && other.x < self.x + self.w
            && self.y < other.y + other.h
            && other.y < self.y + self.h
    }
}

/// The generator's rule: accepted iff no overlapping candidate ranks higher.
fn accepted(candidates: &[Candidate], index: usize) -> bool {
    let me = &candidates[index];
    candidates
        .iter()
        .enumerate()
        .all(|(other, them)| other == index || !me.overlaps(them) || them.priority < me.priority)
}

fn placement(path: &Path) {
    let candidates = [
        Candidate {
            name: "A",
            x: 1,
            y: 1,
            w: 2,
            h: 2,
            priority: 91,
        },
        Candidate {
            name: "B",
            x: 2,
            y: 2,
            w: 2,
            h: 3,
            priority: 57,
        },
        Candidate {
            name: "C",
            x: 3,
            y: 4,
            w: 2,
            h: 2,
            priority: 40,
        },
        Candidate {
            name: "D",
            x: 6,
            y: 2,
            w: 1,
            h: 1,
            priority: 12,
        },
        Candidate {
            name: "E",
            x: 9,
            y: 4,
            w: 2,
            h: 3,
            priority: 73,
        },
        Candidate {
            name: "F",
            x: 10,
            y: 6,
            w: 2,
            h: 2,
            priority: 66,
        },
        Candidate {
            name: "G",
            x: 13,
            y: 2,
            w: 2,
            h: 2,
            priority: 30,
        },
        Candidate {
            name: "H",
            x: 12,
            y: 1,
            w: 2,
            h: 2,
            priority: 85,
        },
    ];
    let (cell, left, top, cols, rows) = (44.0, 40.0, 60.0, 16, 9);
    let mut svg = Svg::new(
        left * 2.0 + cell * cols as f64 + 230.0,
        top + cell * rows as f64 + 60.0,
    );
    svg.text(
        left,
        30.0,
        "Overlap resolution: a candidate wins iff no overlapping candidate has higher priority",
        TextStyle::new(14.0).bold(),
    );
    for row in 0..=rows {
        let y = top + row as f64 * cell;
        svg.line((left, y), (left + cell * cols as f64, y), "#e2e8f0", 1.0);
    }
    for col in 0..=cols {
        let x = left + col as f64 * cell;
        svg.line((x, top), (x, top + cell * rows as f64), "#e2e8f0", 1.0);
    }
    let border = left + 10.0 * cell;
    svg.line_styled(
        (border, top - 8.0),
        (border, top + cell * rows as f64 + 8.0),
        "#2d3748",
        2.5,
        r#"stroke-dasharray="8 5""#,
    );
    svg.text(
        border - 6.0,
        top + cell * rows as f64 + 24.0,
        "chunk 0",
        TextStyle::new(11.0).end().color(MUTED),
    );
    svg.text(
        border + 6.0,
        top + cell * rows as f64 + 24.0,
        "chunk 1",
        TextStyle::new(11.0).color(MUTED),
    );
    for (index, c) in candidates.iter().enumerate() {
        let (x, y) = (left + c.x as f64 * cell, top + c.y as f64 * cell);
        let (w, h) = (c.w as f64 * cell, c.h as f64 * cell);
        if accepted(&candidates, index) {
            svg.rect_styled(
                x + 2.0,
                y + 2.0,
                w - 4.0,
                h - 4.0,
                "#9ae6b4",
                "#276749",
                r#"stroke-width="2" fill-opacity="0.85""#,
            );
        } else {
            svg.rect_styled(
                x + 2.0,
                y + 2.0,
                w - 4.0,
                h - 4.0,
                "#fed7d7",
                "#c53030",
                r#"stroke-width="2" stroke-dasharray="6 4" fill-opacity="0.6""#,
            );
        }
        let label_y = if accepted(&candidates, index) {
            y + 18.0
        } else {
            y + h - 10.0
        };
        svg.text(
            x + 8.0,
            label_y,
            &format!("{} · {}", c.name, c.priority),
            TextStyle::new(12.0).bold(),
        );
    }
    let notes_x = left + cell * cols as f64 + 24.0;
    let note = TextStyle::new(11.0);
    let lines = [
        "label = candidate · priority",
        "",
        "A beats B (91 > 57).",
        "C loses to B although B itself",
        "lost: losers still block.",
        "No chain reaches further than",
        "one overlap from a candidate.",
        "",
        "E crosses the chunk border; both",
        "chunks report it, deduplicated",
        "by (rule id, anchor).",
        "",
        "H beats G (85 > 30).",
    ];
    for (index, line) in lines.iter().enumerate() {
        svg.text(notes_x, top + 14.0 + index as f64 * 17.0, line, note);
    }
    svg.rect_styled(notes_x, top + 250.0, 16.0, 14.0, "#9ae6b4", "#276749", "");
    svg.text(notes_x + 22.0, top + 262.0, "accepted", note);
    svg.rect_styled(
        notes_x,
        top + 274.0,
        16.0,
        14.0,
        "#fed7d7",
        "#c53030",
        r#"stroke-dasharray="3 2""#,
    );
    svg.text(notes_x + 22.0, top + 286.0, "rejected", note);
    svg.save(path);
}

fn streaming(path: &Path) {
    let (n, cell, left, top) = (15_i64, 34.0, 40.0, 60.0);
    let distance = 4;
    let (old, new) = ((5_i64, 7_i64), (7_i64, 7_i64));
    let within = |(cx, cy): (i64, i64), (x, y): (i64, i64), r: i64| {
        (x - cx).abs() <= r && (y - cy).abs() <= r
    };
    let mut svg = Svg::new(
        left * 2.0 + cell * n as f64 + 250.0,
        top + cell * n as f64 + 50.0,
    );
    svg.text(
        left,
        30.0,
        "Chunk streaming after an observer moves two chunks east",
        TextStyle::new(14.0).bold(),
    );
    for y in 0..n {
        for x in 0..n {
            let pos = (x, y);
            // Before the move the observer's wanted square was loaded.
            let was_loaded = within(old, pos, distance);
            let fill = match (
                was_loaded,
                within(new, pos, distance),
                within(new, pos, distance + 1),
            ) {
                (true, true, _) => "#68d391",
                (false, true, _) => "#f6e05e",
                (true, false, true) => "#c6f6d5",
                (true, false, false) => "#a3bffa",
                _ => "#f7fafc",
            };
            svg.rect(
                left + x as f64 * cell,
                top + y as f64 * cell,
                cell - 2.0,
                cell - 2.0,
                fill,
                "#e2e8f0",
            );
            if !was_loaded && within(new, pos, distance) {
                let priority = (x - new.0).abs().max((y - new.1).abs());
                let center = (
                    left + (x as f64 + 0.5) * cell - 1.0,
                    top + (y as f64 + 0.5) * cell + 4.0,
                );
                svg.text(
                    center.0,
                    center.1,
                    &priority.to_string(),
                    TextStyle::new(10.0).middle(),
                );
            }
        }
    }
    let center = |(x, y): (i64, i64)| {
        (
            left + (x as f64 + 0.5) * cell - 1.0,
            top + (y as f64 + 0.5) * cell - 1.0,
        )
    };
    svg.circle(
        center(old),
        8.0,
        "none",
        "#2d3748",
        r#"stroke-width="2" stroke-dasharray="3 2""#,
    );
    svg.circle(center(new), 9.0, "#2d3748", "none", "");
    svg.arrow(
        (center(old).0 + 10.0, center(old).1),
        (center(new).0 - 12.0, center(new).1),
    );
    let notes_x = left + cell * n as f64 + 24.0;
    let entries = [
        ("#68d391", "loaded, inside simulation distance"),
        ("#c6f6d5", "kept: +1 hysteresis margin"),
        ("#f6e05e", "requested; number = distance (low first)"),
        ("#a3bffa", "saved to storage and despawned"),
    ];
    for (index, (color, label)) in entries.iter().enumerate() {
        let y = top + 10.0 + index as f64 * 24.0;
        svg.rect(notes_x, y, 16.0, 16.0, color, "#a0aec0");
        svg.text(notes_x + 24.0, y + 12.0, label, TextStyle::new(11.0));
    }
    let note = TextStyle::new(11.0).color(MUTED);
    svg.text(notes_x, top + 130.0, "simulation distance = 4 chunks", note);
    svg.text(
        notes_x,
        top + 147.0,
        "(Chebyshev square around observer)",
        note,
    );
    svg.text(
        notes_x,
        top + 170.0,
        "generation tasks capped (2–16);",
        note,
    );
    svg.text(notes_x, top + 187.0, "loads 4 / unloads 8 per tick", note);
    svg.text(notes_x, top + 210.0, "stored chunks restore without", note);
    svg.text(notes_x, top + 227.0, "regeneration", note);
    svg.save(path);
}
