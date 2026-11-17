//! Minimal SVG writer: shapes, text and arrows on a white page.

use std::{fmt::Write as _, fs, path::Path};

pub const FONT: &str = "Helvetica, Arial, sans-serif";
pub const INK: &str = "#1a202c";
pub const MUTED: &str = "#718096";

#[derive(Clone, Copy)]
pub enum Anchor {
    Start,
    Middle,
    End,
}

impl Anchor {
    fn name(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Middle => "middle",
            Self::End => "end",
        }
    }
}

#[derive(Clone, Copy)]
pub struct TextStyle {
    pub size: f64,
    pub anchor: Anchor,
    pub bold: bool,
    pub color: &'static str,
}

impl TextStyle {
    pub const fn new(size: f64) -> Self {
        Self {
            size,
            anchor: Anchor::Start,
            bold: false,
            color: INK,
        }
    }

    pub const fn middle(mut self) -> Self {
        self.anchor = Anchor::Middle;
        self
    }

    pub const fn end(mut self) -> Self {
        self.anchor = Anchor::End;
        self
    }

    pub const fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    pub const fn color(mut self, color: &'static str) -> Self {
        self.color = color;
        self
    }
}

pub struct Svg {
    width: f64,
    height: f64,
    body: String,
}

impl Svg {
    pub fn new(width: f64, height: f64) -> Self {
        let mut svg = Self {
            width,
            height,
            body: String::new(),
        };
        svg.rect(0.0, 0.0, width, height, "#ffffff", "none");
        svg
    }

    pub fn rect(&mut self, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str) {
        self.rect_styled(x, y, w, h, fill, stroke, "");
    }

    #[allow(clippy::too_many_arguments)]
    pub fn rect_styled(
        &mut self,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        fill: &str,
        stroke: &str,
        extra: &str,
    ) {
        let _ = writeln!(
            self.body,
            r#"<rect x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{h:.2}" fill="{fill}" stroke="{stroke}" {extra}/>"#
        );
    }

    pub fn line(&mut self, from: (f64, f64), to: (f64, f64), stroke: &str, width: f64) {
        self.line_styled(from, to, stroke, width, "");
    }

    pub fn line_styled(
        &mut self,
        from: (f64, f64),
        to: (f64, f64),
        stroke: &str,
        width: f64,
        extra: &str,
    ) {
        let _ = writeln!(
            self.body,
            r#"<line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="{stroke}" stroke-width="{width}" {extra}/>"#,
            from.0, from.1, to.0, to.1
        );
    }

    pub fn arrow(&mut self, from: (f64, f64), to: (f64, f64)) {
        self.line_styled(from, to, INK, 1.4, r#"marker-end="url(#arrow)""#);
    }

    pub fn polyline(&mut self, points: &[(f64, f64)], stroke: &str, width: f64, extra: &str) {
        let coords: Vec<String> = points
            .iter()
            .map(|(x, y)| format!("{x:.2},{y:.2}"))
            .collect();
        let width = if extra.contains("stroke-width") {
            String::new()
        } else {
            format!(r#"stroke-width="{width}""#)
        };
        let _ = writeln!(
            self.body,
            r#"<polyline points="{}" fill="none" stroke="{stroke}" {width} stroke-linejoin="round" {extra}/>"#,
            coords.join(" ")
        );
    }

    pub fn polygon(&mut self, points: &[(f64, f64)], fill: &str, extra: &str) {
        let coords: Vec<String> = points
            .iter()
            .map(|(x, y)| format!("{x:.2},{y:.2}"))
            .collect();
        let _ = writeln!(
            self.body,
            r#"<polygon points="{}" fill="{fill}" stroke="none" {extra}/>"#,
            coords.join(" ")
        );
    }

    pub fn circle(
        &mut self,
        center: (f64, f64),
        radius: f64,
        fill: &str,
        stroke: &str,
        extra: &str,
    ) {
        let _ = writeln!(
            self.body,
            r#"<circle cx="{:.2}" cy="{:.2}" r="{radius:.2}" fill="{fill}" stroke="{stroke}" {extra}/>"#,
            center.0, center.1
        );
    }

    pub fn text(&mut self, x: f64, y: f64, content: &str, style: TextStyle) {
        let weight = if style.bold { "bold" } else { "normal" };
        let _ = writeln!(
            self.body,
            r#"<text x="{x:.2}" y="{y:.2}" font-family="{FONT}" font-size="{}" font-weight="{weight}" fill="{}" text-anchor="{}">{}</text>"#,
            style.size,
            style.color,
            style.anchor.name(),
            escape(content)
        );
    }

    /// Text rotated -90° around its anchor, for vertical axis labels.
    pub fn vertical_text(&mut self, x: f64, y: f64, content: &str, style: TextStyle) {
        let _ = writeln!(self.body, r#"<g transform="rotate(-90 {x:.2} {y:.2})">"#);
        self.text(x, y, content, style);
        self.body.push_str("</g>\n");
    }

    /// Several lines centred vertically around `y`.
    pub fn lines(&mut self, x: f64, y: f64, lines: &[&str], style: TextStyle) {
        let step = style.size * 1.25;
        let top = y - step * (lines.len() as f64 - 1.0) / 2.0 + style.size * 0.35;
        for (index, line) in lines.iter().enumerate() {
            self.text(x, top + step * index as f64, line, style);
        }
    }

    pub fn save(&self, path: &Path) {
        let document = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">
<defs><marker id="arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M0,0 L10,5 L0,10 z" fill="{INK}"/></marker></defs>
{body}</svg>
"#,
            w = self.width,
            h = self.height,
            body = self.body
        );
        fs::write(path, document).expect("write svg");
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
