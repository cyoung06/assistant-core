use mermaid_rs_renderer::{RenderOptions, Theme};

use crate::drawing::{Drawing, PathBuilder, Shape};
use crate::{RenderError, fonts};

/// A Mermaid diagram, laid out and drawn by mmdr with the registered text font, in [dark] or light
/// colours. Text comes out as glyph outlines, so it looks the same wherever it's drawn.
#[uniffi::export]
pub fn render_mermaid(source: String, dark: bool) -> Result<Drawing, RenderError> {
    let family = fonts::text_family().ok_or_else(|| RenderError::Invalid("no font registered".into()))?;
    let mut options = RenderOptions { theme: if dark { Theme::dark() } else { Theme::modern() }, ..RenderOptions::default() };
    options.theme.font_family = family.clone();
    // On the note's own background (the theme's would be a box around it); it's also what gaps
    // between parts (a pie's slices) are drawn in.
    options.theme.background = format!("#{:06x}", crate::note_style(dark).background & 0xffffff);
    let svg = mermaid_rs_renderer::render_with_options(&source, options).map_err(|e| RenderError::Invalid(e.to_string()))?;
    let svg_options = usvg::Options { font_family: family, fontdb: fonts::database(), ..usvg::Options::default() };
    let tree = usvg::Tree::from_str(&svg, &svg_options).map_err(|e| RenderError::Invalid(e.to_string()))?;
    let mut shapes = Vec::new();
    collect(tree.root(), &mut shapes);
    let size = tree.size();
    Ok(Drawing { width: size.width(), height: size.height(), baseline: size.height(), shapes })
}

/// Solid colours only (mmdr's diagrams use nothing else); anything else, or fully transparent, isn't drawn.
fn argb(paint: &usvg::Paint, opacity: f32) -> Option<u32> {
    let alpha = (opacity * 255.0).round() as u32;
    match paint {
        usvg::Paint::Color(c) if alpha > 0 => Some(alpha << 24 | (c.red as u32) << 16 | (c.green as u32) << 8 | c.blue as u32),
        _ => None,
    }
}

fn collect(group: &usvg::Group, out: &mut Vec<Shape>) {
    for node in group.children() {
        match node {
            usvg::Node::Group(g) => collect(g, out),
            usvg::Node::Text(t) => collect(t.flattened(), out),
            usvg::Node::Path(p) => {
                let Some(data) = p.data().clone().transform(p.abs_transform()) else { continue };
                let path = || {
                    let mut b = PathBuilder::new([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
                    for seg in data.segments() {
                        use usvg::tiny_skia_path::PathSegment::*;
                        match seg {
                            MoveTo(a) => b.move_to(a.x, a.y),
                            LineTo(a) => b.line_to(a.x, a.y),
                            QuadTo(a, c) => b.quad_to(a.x, a.y, c.x, c.y),
                            CubicTo(a, c, d) => b.curve_to(a.x, a.y, c.x, c.y, d.x, d.y),
                            Close => b.close(),
                        }
                    }
                    b
                };
                if let Some(color) = p.fill().and_then(|f| argb(f.paint(), f.opacity().get())) {
                    out.push(path().finish(color, 0.0));
                }
                if let Some(s) = p.stroke() {
                    if let Some(color) = argb(s.paint(), s.opacity().get()) {
                        let (sx, sy) = p.abs_transform().get_scale();
                        out.push(path().finish(color, s.width().get() * (sx * sy).sqrt()));
                    }
                }
            }
            _ => {}
        }
    }
}
