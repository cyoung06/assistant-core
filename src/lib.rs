//! What the Assistant apps draw the same everywhere: math (RaTeX, KaTeX's fonts and metrics) and
//! diagrams (Mermaid, by mmdr), as vector shapes each platform paints with its own canvas. Text in
//! them is in the fonts the app registers (the ones its editor shows), never the system's.

uniffi::setup_scaffolding!();

mod diagram;
mod drawing;
mod fonts;
mod markdown;
mod math;
mod style;

pub use diagram::render_mermaid;
pub use drawing::{Drawing, Shape};
pub use fonts::register_font;
pub use markdown::{Element, Span, TextRange, markdown_spans};
pub use math::render_math;
pub use style::{NoteStyle, note_style};

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum RenderError {
    /// The source isn't valid (a TeX or Mermaid syntax error), with why.
    #[error("{0}")]
    Invalid(String),
    /// It's valid but too big to draw.
    #[error("{0}")]
    TooLarge(String),
}
