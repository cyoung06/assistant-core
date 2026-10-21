//! The core for the web (wasm32, `--features web`): the same functions as the apps get, for
//! JavaScript. Records come as plain objects (camelCase); drawings as SVG, which the browser paints.

use std::fmt::Write;

use wasm_bindgen::prelude::*;

use crate::{Drawing, RenderError};

fn js<T: serde::Serialize>(value: &T) -> JsValue {
    value.serialize(&serde_wasm_bindgen::Serializer::json_compatible()).unwrap_or(JsValue::NULL)
}

/// A drawing as an SVG document, [width] by [height] in CSS pixels, with where its baseline is.
#[wasm_bindgen(getter_with_clone)]
pub struct Svg {
    pub svg: String,
    pub width: f32,
    pub height: f32,
    pub baseline: f32,
}

fn svg(d: Drawing) -> Svg {
    let mut out = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#, w = d.width, h = d.height);
    for s in &d.shapes {
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
    Svg { svg: out, width: d.width, height: d.height, baseline: d.baseline }
}

fn drawn(result: Result<Drawing, RenderError>) -> Result<Svg, JsError> {
    result.map(svg).map_err(|e| JsError::new(&e.to_string()))
}

#[wasm_bindgen(js_name = registerFont)]
pub fn register_font(data: Vec<u8>) {
    crate::register_font(data)
}

#[wasm_bindgen(js_name = markdownSpans)]
pub fn markdown_spans(text: String) -> JsValue {
    js(&crate::markdown_spans(text))
}

#[wasm_bindgen(js_name = renderMath)]
pub fn render_math(tex: String, display: bool, font_size: f32, color: u32) -> Result<Svg, JsError> {
    drawn(crate::render_math(tex, display, font_size, color))
}

#[wasm_bindgen(js_name = renderMermaid)]
pub fn render_mermaid(source: String, dark: bool) -> Result<Svg, JsError> {
    drawn(crate::render_mermaid(source, dark))
}

#[wasm_bindgen(js_name = noteStyle)]
pub fn note_style(dark: bool) -> JsValue {
    js(&crate::note_style(dark))
}

#[wasm_bindgen(js_name = toggleWrap)]
pub fn toggle_wrap(text: String, from: u32, to: u32, mark: String) -> JsValue {
    js(&crate::toggle_wrap(text, from, to, mark))
}

#[wasm_bindgen(js_name = slashItems)]
pub fn slash_items() -> JsValue {
    js(&crate::slash_items())
}

#[wasm_bindgen(js_name = linkPath)]
pub fn link_path(folder: String, href: String) -> Option<String> {
    crate::link_path(folder, href)
}

#[wasm_bindgen(js_name = relativeLink)]
pub fn relative_link(folder: String, path: String) -> String {
    crate::relative_link(folder, path)
}

#[wasm_bindgen(js_name = pastedName)]
pub fn pasted_name(name: String, mime: String, now: String) -> String {
    crate::pasted_name(name, mime, now)
}
