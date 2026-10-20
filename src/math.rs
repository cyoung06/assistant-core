use std::collections::HashMap;
use std::sync::Arc;

use ratex_layout::{FallbackGlyphMetrics, LayoutOptions, layout_with_fallback_metrics, to_display_list, validate_layout};
use ratex_types::color::Color;
use ratex_types::display_item::DisplayItem;
use ratex_types::math_style::MathStyle;
use ratex_types::path_command::PathCommand;
use ttf_parser::Face;

use crate::drawing::{Drawing, PathBuilder, Shape};
use crate::{RenderError, fonts};

/// TeX math (KaTeX's syntax), laid out as KaTeX does with its fonts: [display] for a display
/// formula ($$…$$), else inline. [font_size] is the em in the drawing's units, [color] ARGB.
/// Characters KaTeX's fonts don't have (Korean in \text{…}) are set in the registered text font.
#[uniffi::export]
pub fn render_math(tex: String, display: bool, font_size: f32, color: u32) -> Result<Drawing, RenderError> {
    let nodes = ratex_parser::parse(&tex).map_err(|e| RenderError::Invalid(e.to_string()))?;
    let fallback = Fallback::new();
    let metrics = Arc::new(fallback.metrics(&tex));
    let options = LayoutOptions {
        style: if display { MathStyle::Display } else { MathStyle::Text },
        color: Color { r: ((color >> 16) & 0xff) as f32 / 255.0, g: ((color >> 8) & 0xff) as f32 / 255.0, b: (color & 0xff) as f32 / 255.0, a: (color >> 24) as f32 / 255.0 },
        ..LayoutOptions::default()
    };
    let tree = layout_with_fallback_metrics(&nodes, &options, metrics);
    validate_layout(&tree).map_err(|e| RenderError::TooLarge(e.to_string()))?;
    let list = to_display_list(&tree);

    let em = font_size;
    let mut katex: HashMap<String, Option<Face<'static>>> = HashMap::new();
    let mut shapes = Vec::new();
    for item in &list.items {
        match item {
            DisplayItem::GlyphPath { x, y, scale, font, char_code, color } => {
                let Some(ch) = char::from_u32(*char_code) else { continue };
                let face = katex.entry(font.clone()).or_insert_with(|| katex_face(font));
                let size = (*scale as f32) * em;
                let (x, y) = (*x as f32 * em, *y as f32 * em);
                let glyph = face.as_ref().and_then(|f| f.glyph_index(ch).map(|g| (f, g)));
                let shape = match glyph {
                    Some((f, g)) => outline(f, g, x, y, size, argb(color)),
                    None => fallback.outline(ch, x, y, size, argb(color)),
                };
                shapes.extend(shape);
            }
            DisplayItem::Line { x, y, width, thickness, color, dashed } => {
                let (x, y, w, t) = (*x as f32 * em, *y as f32 * em, *width as f32 * em, (*thickness as f32 * em).max(1e-3));
                let mut p = PathBuilder::new([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
                if *dashed {
                    // Dashes and gaps three times the rule's thickness, as KaTeX draws \hdashline.
                    let mut at = x;
                    while at < x + w {
                        let end = (at + 3.0 * t).min(x + w);
                        rect(&mut p, at, y - t / 2.0, end - at, t);
                        at += 6.0 * t;
                    }
                } else {
                    rect(&mut p, x, y - t / 2.0, w, t);
                }
                shapes.push(p.finish(argb(color), 0.0));
            }
            DisplayItem::Rect { x, y, width, height, color } => {
                let mut p = PathBuilder::new([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
                rect(&mut p, *x as f32 * em, *y as f32 * em, *width as f32 * em, *height as f32 * em);
                shapes.push(p.finish(argb(color), 0.0));
            }
            DisplayItem::Path { x, y, commands, fill, color } => {
                let mut p = PathBuilder::new([em, 0.0, 0.0, em, *x as f32 * em, *y as f32 * em]);
                for c in commands {
                    match *c {
                        PathCommand::MoveTo { x, y } => p.move_to(x as f32, y as f32),
                        PathCommand::LineTo { x, y } => p.line_to(x as f32, y as f32),
                        PathCommand::QuadTo { x1, y1, x, y } => p.quad_to(x1 as f32, y1 as f32, x as f32, y as f32),
                        PathCommand::CubicTo { x1, y1, x2, y2, x, y } => p.curve_to(x1 as f32, y1 as f32, x2 as f32, y2 as f32, x as f32, y as f32),
                        PathCommand::Close => p.close(),
                    }
                }
                // Unfilled paths are stroked as RaTeX's own SVG export does (1.5 at 40 to the em).
                shapes.push(p.finish(argb(color), if *fill { 0.0 } else { 0.0375 * em }));
            }
        }
    }
    Ok(Drawing { width: list.width as f32 * em, height: list.total_height() as f32 * em, baseline: list.height as f32 * em, shapes })
}

fn argb(c: &Color) -> u32 {
    let ch = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    ch(c.a) << 24 | ch(c.r) << 16 | ch(c.g) << 8 | ch(c.b)
}

fn rect(p: &mut PathBuilder, x: f32, y: f32, w: f32, h: f32) {
    p.move_to(x, y);
    p.line_to(x + w, y);
    p.line_to(x + w, y + h);
    p.line_to(x, y + h);
    p.close();
}

/// A KaTeX font by the name RaTeX gives it ("Main-Regular" is KaTeX_Main-Regular.ttf).
fn katex_face(name: &str) -> Option<Face<'static>> {
    let bytes = ratex_katex_fonts::ttf_bytes(&format!("KaTeX_{name}.ttf"))?;
    // The embedded fonts live as long as the program.
    let bytes: &'static [u8] = match bytes {
        std::borrow::Cow::Borrowed(b) => b,
        std::borrow::Cow::Owned(v) => Box::leak(v.into_boxed_slice()),
    };
    Face::parse(bytes, 0).ok()
}

/// Glyph [g] of [face] with its baseline origin at [x], [y], [size] high an em (font units are y up).
fn outline(face: &Face, g: ttf_parser::GlyphId, x: f32, y: f32, size: f32, color: u32) -> Option<Shape> {
    let s = size / face.units_per_em() as f32;
    let mut p = PathBuilder::new([s, 0.0, 0.0, -s, x, y]);
    face.outline_glyph(g, &mut p)?;
    (!p.is_empty()).then(|| p.finish(color, 0.0))
}

/// The registered text font, for what KaTeX's fonts don't have.
struct Fallback {
    db: Arc<usvg::fontdb::Database>,
    id: Option<usvg::fontdb::ID>,
}

impl Fallback {
    fn new() -> Self {
        let db = fonts::database();
        let id = fonts::text_family().and_then(|family| {
            db.query(&usvg::fontdb::Query { families: &[usvg::fontdb::Family::Name(&family)], ..Default::default() })
        });
        Self { db, id }
    }

    fn with_face<T>(&self, f: impl FnOnce(&Face) -> Option<T>) -> Option<T> {
        self.db.with_face_data(self.id?, |data, index| Face::parse(data, index).ok().and_then(|face| f(&face)))?
    }

    /// Each character of [tex] beyond ASCII (KaTeX's fonts cover what TeX commands make from
    /// ASCII), measured in this font, in em.
    fn metrics(&self, tex: &str) -> HashMap<char, FallbackGlyphMetrics> {
        self.with_face(|face| {
            let upem = face.units_per_em() as f64;
            Some(
                tex.chars().filter(|c| !c.is_ascii()).filter_map(|c| {
                    let g = face.glyph_index(c)?;
                    let advance = face.glyph_hor_advance(g)? as f64;
                    let (top, bottom) = face.glyph_bounding_box(g).map_or((0.0, 0.0), |b| (b.y_max as f64, b.y_min as f64));
                    Some((c, FallbackGlyphMetrics { width: advance / upem, height: top.max(0.0) / upem, depth: (-bottom).max(0.0) / upem }))
                }).collect(),
            )
        })
        .unwrap_or_default()
    }

    fn outline(&self, ch: char, x: f32, y: f32, size: f32, color: u32) -> Option<Shape> {
        self.with_face(|face| outline(face, face.glyph_index(ch)?, x, y, size, color))
    }
}
