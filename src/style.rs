/// How a note looks, the same in every app (the web editor's look is where these come from).
/// Colours are ARGB; sizes are in CSS pixels (dp on Android, points on macOS), or in ems of the
/// body text where they say so.
#[derive(Debug, Clone, uniffi::Record)]
#[cfg_attr(feature = "web", derive(serde::Serialize), serde(rename_all = "camelCase"))]
pub struct NoteStyle {
    pub background: u32,
    pub text: u32,
    /// Quotes, bullets.
    pub text_secondary: u32,
    /// Struck-through and done tasks, code fences, link addresses.
    pub text_tertiary: u32,
    pub accent: u32,
    /// The selection.
    pub accent_soft: u32,
    /// Code blocks, table headers.
    pub code_background: u32,
    /// Quote bars, table borders, rules.
    pub rule: u32,
    pub danger: u32,

    pub font_size: f32,
    /// Line height, in ems.
    pub line_height: f32,
    /// The text column's widest, and its padding.
    pub max_width: f32,
    pub padding_top: f32,
    pub padding_horizontal: f32,

    /// Heading sizes (in ems) for levels 1 to 6, and the space above each (in ems of its own size).
    pub heading_sizes: Vec<f32>,
    pub heading_space_above: Vec<f32>,
    /// Inline code and code blocks, in ems.
    pub code_size: f32,
    pub code_block_size: f32,
    /// A code fence's line, in ems.
    pub fence_size: f32,
    /// TeX as written (while editing it), in ems.
    pub math_source_size: f32,
    /// A quote's bar, and the space after it.
    pub quote_bar_width: f32,
    pub quote_indent: f32,
    /// Code blocks' corners and side padding.
    pub code_radius: f32,
    pub code_padding: f32,
}

#[uniffi::export]
pub fn note_style(dark: bool) -> NoteStyle {
    let (background, text, text_secondary, text_tertiary, accent, accent_soft, code_background, rule, danger) = if dark {
        (0xff1a1d24, 0xffe7e9ee, 0xffa3aab8, 0xff747c8c, 0xff7d9cff, 0xff24304f, 0xff232733, 0xff3b4252, 0xffff6b6b)
    } else {
        (0xffffffff, 0xff1b1f27, 0xff5b6372, 0xff8a91a0, 0xff2f5be0, 0xffe6ecfd, 0xffeef0f4, 0xffc4cad4, 0xffd13b3b)
    };
    NoteStyle {
        background, text, text_secondary, text_tertiary, accent, accent_soft, code_background, rule, danger,
        font_size: 16.0,
        line_height: 1.65,
        max_width: 760.0,
        padding_top: 28.0,
        padding_horizontal: 24.0,
        heading_sizes: vec![1.9, 1.5, 1.25, 1.05, 1.05, 1.05],
        heading_space_above: vec![0.6, 0.55, 0.45, 0.35, 0.35, 0.35],
        code_size: 0.9,
        code_block_size: 0.88,
        fence_size: 0.75,
        math_source_size: 0.9,
        quote_bar_width: 3.0,
        quote_indent: 14.0,
        code_radius: 8.0,
        code_padding: 14.0,
    }
}
