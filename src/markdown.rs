use comrak::nodes::{AstNode, ListType, NodeValue, Sourcepos};
use comrak::{Arena, Options, parse_document};

/// What a span of a note is, as the editors show it (the web's live preview is the reference).
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum Element {
    Heading { level: u8 },
    Emphasis,
    Strong,
    Strikethrough,
    InlineCode,
    /// A link (its text is the span's content).
    Link { url: String },
    /// A bare address (GFM autolink): shown as a link, nothing hidden.
    Autolink { url: String },
    /// Shown as the image when not being edited.
    Image { url: String, alt: String },
    /// Shown typeset when not being edited.
    Math { tex: String, display: bool },
    /// An unordered list item's marker: shown as a bullet when its line isn't being edited.
    Bullet,
    /// A task's [ ] or [x]: shown as a checkbox when not being edited.
    Task { checked: bool },
    /// A block quote (each of its lines); its "> " are its marks.
    Quote,
    /// A thematic break: shown as a line when not being edited.
    Rule,
    /// Fenced or indented code ([code] is what's in it); its fences are its marks (shown small, not
    /// hidden). A `mermaid` block is shown as its diagram when not being edited.
    CodeBlock { language: String, code: String },
    /// A GFM table: shown as a table when not being edited. [rows] are its cells' Markdown (the
    /// first row is the header), [alignments] each column's.
    Table { rows: Vec<Vec<String>>, alignments: Vec<Alignment> },
}

/// How a table column's cells are aligned.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Enum)]
pub enum Alignment {
    Left,
    Center,
    Right,
}

/// A range of the note in UTF-16 code units (what Android, macOS and the web index strings by).
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct TextRange {
    pub from: u32,
    pub to: u32,
}

/// An element over [range]; [marks] are its Markdown syntax, hidden (or replaced) while the
/// cursor isn't in [range].
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Span {
    pub element: Element,
    pub range: TextRange,
    pub marks: Vec<TextRange>,
}

/// The note's structure: CommonMark with GitHub's extensions (tables, task lists, strikethrough,
/// autolinks) and $…$ / $$…$$ math, as spans in document order.
#[uniffi::export]
pub fn markdown_spans(text: String) -> Vec<Span> {
    let mut options = Options::default();
    options.extension.strikethrough = true;
    options.extension.table = true;
    options.extension.autolink = true;
    options.extension.tasklist = true;
    options.extension.math_dollars = true;
    let arena = Arena::new();
    let root = parse_document(&arena, &text, &options);
    let at = Offsets::new(&text);
    let mut spans = Vec::new();
    for node in root.descendants() {
        if let Some(span) = span(node, &text, &at) {
            spans.push(span);
        }
    }
    spans
}

/// Where each line starts (bytes), and each byte's place in UTF-16.
struct Offsets {
    lines: Vec<usize>,
    utf16: Vec<u32>,
}

impl Offsets {
    fn new(text: &str) -> Self {
        let mut lines = vec![0];
        let mut utf16 = vec![0u32; text.len() + 1];
        let mut u = 0u32;
        for (i, c) in text.char_indices() {
            for b in 0..c.len_utf8() {
                utf16[i + b] = u;
            }
            u += c.len_utf16() as u32;
            if c == '\n' {
                lines.push(i + 1);
            }
        }
        utf16[text.len()] = u;
        Self { lines, utf16 }
    }

    fn byte(&self, line: usize, column: usize) -> usize {
        (self.lines.get(line.saturating_sub(1)).copied().unwrap_or(0) + column.saturating_sub(1)).min(self.utf16.len() - 1)
    }

    /// A node's bytes, [start, end): comrak's end is its last byte, inclusive.
    fn bytes(&self, text: &str, pos: &Sourcepos) -> (usize, usize) {
        let start = self.byte(pos.start.line, pos.start.column);
        let mut end = (self.byte(pos.end.line, pos.end.column) + 1).min(text.len());
        while !text.is_char_boundary(end) {
            end += 1;
        }
        (start, end.max(start))
    }

    fn range(&self, from: usize, to: usize) -> TextRange {
        TextRange { from: self.utf16[from], to: self.utf16[to] }
    }
}

/// The span a node makes, if it's one the editors show differently.
fn span<'a>(node: &'a AstNode<'a>, text: &str, at: &Offsets) -> Option<Span> {
    let data = node.data.borrow();
    let (from, to) = at.bytes(text, &data.sourcepos);
    let whole = |element| Some(Span { element, range: at.range(from, to), marks: Vec::new() });
    // The syntax around a node's children: before the first and after the last.
    let around = |element| {
        let (inner_from, inner_to) = children(node, text, at).unwrap_or((to, to));
        let marks = [(from, inner_from), (inner_to, to)].into_iter().filter(|(a, b)| b > a).map(|(a, b)| at.range(a, b)).collect();
        Some(Span { element, range: at.range(from, to), marks })
    };
    match &data.value {
        NodeValue::Heading(h) => {
            let (inner_from, inner_to) = children(node, text, at).unwrap_or((to, to));
            let marks: Vec<TextRange> = if h.setext {
                // The underline (=== or ---), on the line after the text.
                let underline = text[inner_to..to].find('\n').map_or(to, |i| inner_to + i + 1);
                (underline < to).then(|| at.range(underline, to)).into_iter().collect()
            } else {
                // "# " up to the text, and any closing #s after it.
                [(from, inner_from), (inner_to, to)].into_iter().filter(|(a, b)| b > a).map(|(a, b)| at.range(a, b)).collect()
            };
            Some(Span { element: Element::Heading { level: h.level }, range: at.range(from, to), marks })
        }
        NodeValue::Emph => around(Element::Emphasis),
        NodeValue::Strong => around(Element::Strong),
        NodeValue::Strikethrough => around(Element::Strikethrough),
        NodeValue::Code(c) => {
            let n = c.num_backticks;
            let marks = vec![at.range(from, (from + n).min(to)), at.range(to.saturating_sub(n).max(from), to)];
            Some(Span { element: Element::InlineCode, range: at.range(from, to), marks })
        }
        NodeValue::Link(l) => {
            let source = &text[from..to];
            if !source.starts_with('[') && !source.starts_with('<') {
                return whole(Element::Autolink { url: l.url.clone() });
            }
            around(Element::Link { url: l.url.clone() })
        }
        NodeValue::Image(l) => {
            let alt = node.children().map(|c| plain(c)).collect::<String>();
            whole(Element::Image { url: l.url.clone(), alt })
        }
        NodeValue::Math(m) if m.dollar_math => whole(Element::Math { tex: m.literal.trim().to_string(), display: m.display_math }),
        NodeValue::Item(list) if list.list_type == ListType::Bullet => {
            let marker = from + list.marker_offset;
            let is_task = node.first_child().is_some_and(|c| matches!(c.data.borrow().value, NodeValue::TaskItem(_)));
            (!is_task && marker < to).then(|| Span { element: Element::Bullet, range: at.range(from, to), marks: vec![at.range(marker, marker + 1)] })
        }
        NodeValue::TaskItem(t) => {
            // "[ ]" around the symbol, and the list marker (and its space) before it, are what's replaced.
            let symbol = at.byte(t.symbol_sourcepos.start.line, t.symbol_sourcepos.start.column);
            let open = symbol.saturating_sub(1);
            let close = (symbol + 2).min(text.len());
            let line_start = text[..open].rfind('\n').map_or(0, |i| i + 1);
            let checked = t.symbol.is_some_and(|c| c != ' ');
            Some(Span { element: Element::Task { checked }, range: at.range(line_start, close), marks: vec![at.range(line_start, close)] })
        }
        NodeValue::BlockQuote => {
            // Each line's "> " (after any indentation).
            let mut marks = Vec::new();
            let mut line = from;
            while line < to {
                let end = text[line..to].find('\n').map_or(to, |i| line + i + 1);
                let content = &text[line..end];
                if let Some(gt) = content.find('>').filter(|&i| content[..i].chars().all(|c| c == ' ')) {
                    let after = line + gt + 1;
                    let mark_end = if text[after..].starts_with(' ') { after + 1 } else { after };
                    marks.push(at.range(line + gt, mark_end));
                }
                line = end;
            }
            Some(Span { element: Element::Quote, range: at.range(from, to), marks })
        }
        NodeValue::ThematicBreak => whole(Element::Rule),
        NodeValue::CodeBlock(c) => {
            let language = c.info.split_whitespace().next().unwrap_or("").to_string();
            let code = c.literal.clone();
            let mut marks = Vec::new();
            if c.fenced {
                let first_end = text[from..to].find('\n').map_or(to, |i| from + i);
                marks.push(at.range(from, first_end));
                let body = text[from..to].trim_end_matches('\n');
                if let Some(last_start) = body.rfind('\n').map(|i| from + i + 1) {
                    if text[last_start..from + body.len()].trim_start().starts_with(&text[from..from + c.fence_length]) {
                        marks.push(at.range(last_start, from + body.len()));
                    }
                }
            }
            Some(Span { element: Element::CodeBlock { language, code }, range: at.range(from, to), marks })
        }
        NodeValue::Table(t) => {
            let alignments = t.alignments.iter().map(|a| match a {
                comrak::nodes::TableAlignment::Center => Alignment::Center,
                comrak::nodes::TableAlignment::Right => Alignment::Right,
                _ => Alignment::Left,
            }).collect();
            // Each cell's Markdown as written (so it's styled by the same rules as the text).
            let rows = node.children().map(|row| row.children().map(|cell| {
                let (a, b) = children(cell, text, at).unwrap_or((0, 0));
                if b > a { text[a..b].trim().to_string() } else { String::new() }
            }).collect()).collect();
            whole(Element::Table { rows, alignments })
        }
        _ => None,
    }
}

/// The bytes a node's children cover, from the first's start to the last's end.
fn children<'a>(node: &'a AstNode<'a>, text: &str, at: &Offsets) -> Option<(usize, usize)> {
    let first = node.first_child()?;
    let last = node.last_child()?;
    let (start, _) = at.bytes(text, &first.data.borrow().sourcepos);
    let (_, end) = at.bytes(text, &last.data.borrow().sourcepos);
    Some((start, end))
}

/// A node's text (an image's alt text), without its formatting.
fn plain<'a>(node: &'a AstNode<'a>) -> String {
    match &node.data.borrow().value {
        NodeValue::Text(t) => t.to_string(),
        NodeValue::Code(c) => c.literal.clone(),
        _ => node.children().map(|c| plain(c)).collect(),
    }
}
