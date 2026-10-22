//! The core for the web (wasm32, `--features web`): the same functions as the apps get, for
//! JavaScript. Records come as plain objects (camelCase); drawings as SVG, which the browser paints.

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
    Svg { svg: d.svg(), width: d.width, height: d.height, baseline: d.baseline }
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

#[wasm_bindgen(js_name = mentionText)]
pub fn mention_text(name: String) -> String {
    crate::mention_text(name)
}

#[wasm_bindgen(js_name = relativeLink)]
pub fn relative_link(folder: String, path: String) -> String {
    crate::relative_link(folder, path)
}

#[wasm_bindgen(js_name = pastedName)]
pub fn pasted_name(name: String, mime: String, now: String) -> String {
    crate::pasted_name(name, mime, now)
}
