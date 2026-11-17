//! Axes, series and legends on top of [`Svg`].

use crate::svg::{MUTED, Svg, TextStyle};

pub const SERIES: [&str; 6] = [
    "#2b6cb0", "#dd6b20", "#2f855a", "#c53030", "#6b46c1", "#4a5568",
];
const GRID: &str = "#e2e8f0";

/// A plot rectangle mapping data ranges to pixels (y grows upward in data space).
#[derive(Clone, Copy)]
pub struct Axes {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
    pub x: (f64, f64),
    pub y: (f64, f64),
}

pub struct AxisLabels<'a> {
    pub title: &'a str,
    pub x: &'a str,
    pub y: &'a str,
}

impl Axes {
    pub fn px(&self, x: f64) -> f64 {
        self.left + (x - self.x.0) / (self.x.1 - self.x.0) * self.width
    }

    pub fn py(&self, y: f64) -> f64 {
        self.top + self.height - (y - self.y.0) / (self.y.1 - self.y.0) * self.height
    }

    pub fn point(&self, (x, y): (f64, f64)) -> (f64, f64) {
        (self.px(x), self.py(y))
    }

    /// Grid, ticks, frame and labels.
    pub fn draw(&self, svg: &mut Svg, labels: &AxisLabels) {
        self.draw_with(svg, labels, true);
    }

    /// Like [`Axes::draw`] without x ticks, for categorical bars labelled by the caller.
    pub fn draw_categorical(&self, svg: &mut Svg, labels: &AxisLabels) {
        self.draw_with(svg, labels, false);
    }

    fn draw_with(&self, svg: &mut Svg, labels: &AxisLabels, x_ticks: bool) {
        let small = TextStyle::new(11.0).color(MUTED);
        let xs = if x_ticks {
            nice_ticks(self.x.0, self.x.1, 8)
        } else {
            Vec::new()
        };
        for x in xs {
            let px = self.px(x);
            svg.line((px, self.top), (px, self.top + self.height), GRID, 1.0);
            svg.text(
                px,
                self.top + self.height + 16.0,
                &format_tick(x),
                small.middle(),
            );
        }
        for y in nice_ticks(self.y.0, self.y.1, 6) {
            let py = self.py(y);
            svg.line((self.left, py), (self.left + self.width, py), GRID, 1.0);
            svg.text(self.left - 6.0, py + 4.0, &format_tick(y), small.end());
        }
        svg.rect(
            self.left,
            self.top,
            self.width,
            self.height,
            "none",
            "#a0aec0",
        );
        svg.text(
            self.left,
            self.top - 10.0,
            labels.title,
            TextStyle::new(13.0).bold(),
        );
        svg.text(
            self.left + self.width / 2.0,
            self.top + self.height + 36.0,
            labels.x,
            TextStyle::new(12.0).middle(),
        );
        svg.vertical_text(
            self.left - 44.0,
            self.top + self.height / 2.0,
            labels.y,
            TextStyle::new(12.0).middle(),
        );
    }

    pub fn series(&self, svg: &mut Svg, points: &[(f64, f64)], color: &str, extra: &str) {
        let pixels: Vec<_> = points.iter().map(|&p| self.point(p)).collect();
        svg.polyline(&pixels, color, 2.0, extra);
    }

    /// Shaded vertical band between two x values.
    pub fn band(&self, svg: &mut Svg, x0: f64, x1: f64, fill: &str) {
        let (a, b) = (self.px(x0.max(self.x.0)), self.px(x1.min(self.x.1)));
        svg.rect(a, self.top, b - a, self.height, fill, "none");
    }

    /// Bars from y = 0, one per `(x_center, value)`, each `bar_width` data units wide.
    pub fn bars(&self, svg: &mut Svg, bars: &[(f64, f64)], bar_width: f64, color: &str) {
        for &(x, value) in bars {
            let (x0, x1) = (self.px(x - bar_width / 2.0), self.px(x + bar_width / 2.0));
            let (y0, y1) = (self.py(0.0_f64.max(self.y.0)), self.py(value));
            svg.rect(
                x0,
                y1.min(y0),
                (x1 - x0).max(0.5),
                (y0 - y1).abs(),
                color,
                "none",
            );
        }
    }
}

/// Legend entries: label, colour, dashed.
pub fn legend(svg: &mut Svg, x: f64, y: f64, entries: &[(&str, &str, bool)]) {
    for (index, (label, color, dashed)) in entries.iter().enumerate() {
        let row = y + index as f64 * 18.0;
        let dash = if *dashed {
            r#"stroke-dasharray="6 4""#
        } else {
            ""
        };
        svg.line_styled((x, row), (x + 24.0, row), color, 2.5, dash);
        svg.text(x + 30.0, row + 4.0, label, TextStyle::new(11.0));
    }
}

pub fn nice_ticks(min: f64, max: f64, count: usize) -> Vec<f64> {
    let raw = (max - min) / count as f64;
    let magnitude = 10f64.powf(raw.log10().floor());
    let step = [1.0, 2.0, 2.5, 5.0, 10.0]
        .iter()
        .map(|m| m * magnitude)
        .find(|step| *step >= raw)
        .unwrap_or(10.0 * magnitude);
    let first = (min / step).ceil() as i64;
    let last = (max / step + 1e-9).floor() as i64;
    (first..=last).map(|i| i as f64 * step).collect()
}

fn format_tick(value: f64) -> String {
    if value == 0.0 {
        "0".into()
    } else if value.abs() >= 100.0 || value.fract().abs() < 1e-9 {
        format!("{value:.0}")
    } else if (value * 10.0).fract().abs() < 1e-6 {
        format!("{value:.1}")
    } else {
        format!("{value:.2}")
    }
}
