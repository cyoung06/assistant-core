/// Something drawn: shapes in its own coordinates (y down), [width] by [height], and for math
/// where the baseline is (from the top), to line it up with the text around it.
#[derive(Debug, uniffi::Record)]
pub struct Drawing {
    pub width: f32,
    pub height: f32,
    pub baseline: f32,
    pub shapes: Vec<Shape>,
}

impl Drawing {
    /// As an SVG document, [width] by [height] (CSS pixels).
    pub fn svg(&self) -> String {
        use std::fmt::Write;
        let mut out = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#, w = self.width, h = self.height);
        for s in &self.shapes {
            let mut path = String::new();
            let mut p = s.points.iter();
            let mut next = |n: usize| (0..n * 2).map(|_| p.next().copied().unwrap_or(0.0).to_string()).collect::<Vec<_>>().join(" ");
            for verb in &s.verbs {
                match verb {
                    0 => _ = write!(path, "M{}", next(1)),
                    1 => _ = write!(path, "L{}", next(1)),
                    2 => _ = write!(path, "Q{}", next(2)),
                    3 => _ = write!(path, "C{}", next(3)),
                    _ => path.push('Z'),
                }
            }
            let rgb = format!("#{:06x}", s.color & 0xff_ffff);
            let alpha = (s.color >> 24) as f32 / 255.0;
            if s.stroke_width > 0.0 {
                _ = write!(out, r#"<path d="{path}" fill="none" stroke="{rgb}" stroke-opacity="{alpha}" stroke-width="{}"/>"#, s.stroke_width);
            } else {
                _ = write!(out, r#"<path d="{path}" fill="{rgb}" fill-opacity="{alpha}"/>"#);
            }
        }
        out.push_str("</svg>");
        out
    }
}

/// A path, filled or stroked in one colour. [verbs] are 0 move, 1 line, 2 quadratic, 3 cubic,
/// 4 close; [points] holds their points' x, y in order (1, 1, 2 and 3 points; none for close).
#[derive(Debug, uniffi::Record)]
pub struct Shape {
    pub verbs: Vec<u8>,
    pub points: Vec<f32>,
    /// ARGB.
    pub color: u32,
    /// Stroked this wide; 0 is filled.
    pub stroke_width: f32,
}

/// Builds a [Shape]'s path, each point placed by an affine transform.
pub(crate) struct PathBuilder {
    verbs: Vec<u8>,
    points: Vec<f32>,
    /// x' = a x + c y + e, y' = b x + d y + f.
    m: [f32; 6],
}

impl PathBuilder {
    pub fn new(m: [f32; 6]) -> Self {
        Self { verbs: Vec::new(), points: Vec::new(), m }
    }

    fn point(&mut self, x: f32, y: f32) {
        let [a, b, c, d, e, f] = self.m;
        self.points.extend([a * x + c * y + e, b * x + d * y + f]);
    }

    pub fn move_to(&mut self, x: f32, y: f32) {
        self.verbs.push(0);
        self.point(x, y);
    }
    pub fn line_to(&mut self, x: f32, y: f32) {
        self.verbs.push(1);
        self.point(x, y);
    }
    pub fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.verbs.push(2);
        self.point(x1, y1);
        self.point(x, y);
    }
    pub fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.verbs.push(3);
        self.point(x1, y1);
        self.point(x2, y2);
        self.point(x, y);
    }
    pub fn close(&mut self) {
        self.verbs.push(4);
    }

    pub fn is_empty(&self) -> bool {
        self.verbs.is_empty()
    }

    pub fn finish(self, color: u32, stroke_width: f32) -> Shape {
        Shape { verbs: self.verbs, points: self.points, color, stroke_width }
    }
}

/// A font's glyph outlines go straight into a path.
impl ttf_parser::OutlineBuilder for PathBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        PathBuilder::move_to(self, x, y)
    }
    fn line_to(&mut self, x: f32, y: f32) {
        PathBuilder::line_to(self, x, y)
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        PathBuilder::quad_to(self, x1, y1, x, y)
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        PathBuilder::curve_to(self, x1, y1, x2, y2, x, y)
    }
    fn close(&mut self) {
        PathBuilder::close(self)
    }
}
