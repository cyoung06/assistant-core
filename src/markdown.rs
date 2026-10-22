use comrak::nodes::{AstNode, ListType, NodeValue, Sourcepos};
use comrak::{Arena, Options, parse_document};

/// What a span of a note is, as the editors show it (the web's live preview is the reference).
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
#[cfg_attr(feature = "web", derive(serde::Serialize), serde(tag = "kind", rename_all = "camelCase"))]
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
    /// A mention, `@Name` or `@[Name with spaces]`: a task, event, project, note or file, by its
    /// name (see [crate::mention_text]); shown as a link when not being edited. Its marks are the
    /// `@` (and the brackets).
    Mention { name: String },
}

/// How a table column's cells are aligned.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Enum)]
#[cfg_attr(feature = "web", derive(serde::Serialize), serde(rename_all = "camelCase"))]
pub enum Alignment {
    Left,
    Center,
    Right,
}

/// A range of the note in UTF-16 code units (what Android, macOS and the web index strings by).
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "web", derive(serde::Serialize), serde(rename_all = "camelCase"))]
pub struct TextRange {
    pub from: u32,
    pub to: u32,
}

/// An element over [range]; [marks] are its Markdown syntax, hidden (or replaced) while the
/// cursor isn't in [range].
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "web", derive(serde::Serialize), serde(rename_all = "camelCase"))]
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
    for m in mentions(&text, &literal(root, &text, &at)) {
        let marks = if m.bracketed { vec![at.range(m.from, m.from + 2), at.range(m.to - 1, m.to)] } else { vec![at.range(m.from, m.from + 1)] };
        spans.push(Span { element: Element::Mention { name: m.name }, range: at.range(m.from, m.to), marks });
    }
    spans.sort_by_key(|s| s.range.from);
    spans
}

/// A mention found in a note's text: its bytes [from, to), and the name in it.
pub(crate) struct Mention {
    pub from: usize,
    pub to: usize,
    pub name: String,
    pub bracketed: bool,
}

/// The bytes where an `@` isn't a mention: code, links, images, math, HTML.
pub(crate) fn literal<'a>(root: &'a AstNode<'a>, text: &str, at: &Offsets) -> Vec<(usize, usize)> {
    root.descendants()
        .filter(|n| {
            matches!(
                n.data.borrow().value,
                NodeValue::Code(_) | NodeValue::CodeBlock(_) | NodeValue::Link(_) | NodeValue::Image(_) | NodeValue::Math(_) | NodeValue::HtmlInline(_) | NodeValue::HtmlBlock(_)
            )
        })
        .map(|n| at.bytes(text, &n.data.borrow().sourcepos))
        .collect()
}

/// The mentions in [text] outside [literal]: `@[Name]`, or `@Name` (letters, digits, `_ - . #`; a
/// sentence's last `.` not part of it). An `@` right after a letter, digit or `._%+-` is an
/// address's (mail), not a mention.
pub(crate) fn mentions(text: &str, literal: &[(usize, usize)]) -> Vec<Mention> {
    let mut out = Vec::new();
    let mut prev: Option<char> = None;
    let mut skip_to = 0;
    for (i, c) in text.char_indices() {
        let before = prev;
        prev = Some(c);
        if c != '@' || i < skip_to || before.is_some_and(|p| p.is_alphanumeric() || "._%+-".contains(p)) {
            continue;
        }
        if literal.iter().any(|&(a, b)| i >= a && i < b) {
            continue;
        }
        let rest = &text[i + 1..];
        let found = if let Some(inner) = rest.strip_prefix('[') {
            inner.find(|ch| ch == ']' || ch == '\n').filter(|&e| inner[e..].starts_with(']')).and_then(|e| {
                let name = inner[..e].trim();
                (!name.is_empty()).then(|| Mention { from: i, to: i + 3 + e, name: name.to_string(), bracketed: true })
            })
        } else {
            let mut end = rest.char_indices().find(|&(_, ch)| !(ch.is_alphanumeric() || "_-.#".contains(ch))).map_or(rest.len(), |(e, _)| e);
            while end > 0 && rest[..end].ends_with(['.', '-']) {
                end -= 1;
            }
            (end > 0).then(|| Mention { from: i, to: i + 1 + end, name: rest[..end].to_string(), bracketed: false })
        };
        if let Some(m) = found {
            skip_to = m.to;
            out.push(m);
        }
    }
    out
}

#[cfg(test)]
mod mention_tests {
    use super::*;

    fn names(text: &str) -> Vec<String> {
        markdown_spans(text.into()).into_iter().filter_map(|s| if let Element::Mention { name } = s.element { Some(name) } else { None }).collect()
    }

    #[test]
    fn mentions_are_names_after_an_at() {
        assert_eq!(names("See @HW3. Then @[week 1 slides.pdf], and @#12-"), vec!["HW3", "week 1 slides.pdf", "#12"]);
        assert_eq!(names("확률 숙제 @[확률및통계 HW 3]!"), vec!["확률및통계 HW 3"]);
    }

    #[test]
    fn not_in_mail_code_or_links() {
        assert_eq!(names("me@postech.ac.kr `@code` [@link](https://x.org/@a) $@x$\n\n```\n@block\n```\n"), Vec::<String>::new());
        assert_eq!(names("@[unclosed\n] @ alone"), Vec::<String>::new());
    }

    #[test]
    fn marks_are_the_at_and_brackets() {
        let s = markdown_spans("a @[b c] d".into()).into_iter().find(|s| matches!(s.element, Element::Mention { .. })).unwrap();
        assert_eq!((s.range.from, s.range.to), (2, 8));
        assert_eq!(s.marks.iter().map(|m| (m.from, m.to)).collect::<Vec<_>>(), vec![(2, 4), (7, 8)]);
    }
}

/// Where each line starts (bytes), and each byte's place in UTF-16.
pub(crate) struct Offsets {
    lines: Vec<usize>,
    utf16: Vec<u32>,
}

impl Offsets {
    pub(crate) fn new(text: &str) -> Self {
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
    pub(crate) fn bytes(&self, text: &str, pos: &Sourcepos) -> (usize, usize) {
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
