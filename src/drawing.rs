/// Something drawn: shapes in its own coordinates (y down), [width] by [height], and for math
/// where the baseline is (from the top), to line it up with the text around it.
#[derive(Debug, uniffi::Record)]
pub struct Drawing {
    pub width: f32,
    pub height: f32,
    pub baseline: f32,
    pub shapes: Vec<Shape>,
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
