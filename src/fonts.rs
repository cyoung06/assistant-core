use std::sync::{Arc, RwLock};
use usvg::fontdb::Database;

/// The fonts the app registered: what text in math and diagrams is set in.
static FONTS: RwLock<Option<Arc<Database>>> = RwLock::new(None);

/// Adds a font (its file's bytes, OpenType or TrueType): the app registers the fonts its editor
/// shows text in, so text in drawings matches it. Register them before drawing.
#[uniffi::export]
pub fn register_font(data: Vec<u8>) {
    mermaid_rs_renderer::register_font(data.clone());
    let mut fonts = FONTS.write().unwrap_or_else(|e| e.into_inner());
    let mut db = fonts.as_deref().cloned().unwrap_or_default();
    db.load_font_data(data);
    *fonts = Some(Arc::new(db));
}

/// The registered fonts (none yet: an empty set).
pub(crate) fn database() -> Arc<Database> {
    FONTS.read().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_default()
}

/// The registered font text is set in by default: the first one registered.
pub(crate) fn text_family() -> Option<String> {
    database().faces().next().and_then(|f| f.families.first().map(|(name, _)| name.clone()))
}
