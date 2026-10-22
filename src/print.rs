//! Notes set as PDF pages (the server's build, `--features print`): a note's Markdown typeset by
//! Typst, as a library (no browser, no server), in the fonts the app registered, with math and
//! diagrams drawn as the apps draw them.

use std::collections::HashMap;
use std::fmt::Write;

use comrak::nodes::{AstNode, ListType, NodeValue, TableAlignment};
use comrak::{Arena, Options, parse_document};
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime};
use typst_layout::PagedDocument;
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};

use crate::{RenderError, link_path, render_math, render_mermaid};

/// The text's colour on paper.
const INK: u32 = 0xff1b1b1f;
/// The text's size (points); math is set at it.
const SIZE: f32 = 10.5;

/// A task list's boxes, done and not.
const CHECKED: &str = "#box(baseline: 1pt, width: 8pt, height: 8pt, radius: 1.5pt, fill: rgb(\"#1b1b1f\"), align(center + horizon, text(fill: white, size: 6pt, weight: \"bold\")[#sym.checkmark]))#h(4pt)";
const UNCHECKED: &str = "#box(baseline: 1pt, width: 8pt, height: 8pt, radius: 1.5pt, stroke: 0.7pt + rgb(\"#1b1b1f\"))#h(4pt)";

/// How every page is set up: A4, the app's fonts.
const PREAMBLE: &str = r##"#set page(paper: "a4", margin: (x: 18mm, y: 20mm))
#set text(font: ("Pretendard", "JetBrains Mono"), size: 10.5pt, fill: rgb("#1b1b1f"), lang: "ko")
#set par(leading: 0.8em, spacing: 1.1em)
#show heading: set block(above: 1.5em, below: 0.8em)
#show heading.where(level: 1): set text(size: 18pt)
#show heading.where(level: 2): set text(size: 14pt)
#show heading.where(level: 3): set text(size: 12pt)
#show raw: set text(font: "JetBrains Mono", size: 9pt)
#show raw.where(block: false): box.with(fill: rgb("#f1f1f4"), inset: (x: 3pt), outset: (y: 3pt), radius: 2pt)
#show raw.where(block: true): block.with(fill: rgb("#f6f6f8"), inset: 10pt, radius: 4pt, width: 100%)
#show quote: set text(fill: rgb("#55555f"))
#show quote: it => block(stroke: (left: 2pt + rgb("#d0d0d8")), inset: (left: 10pt, y: 2pt), it.body)
#show link: set text(fill: rgb("#1e5bd8"))
#set table(stroke: 0.5pt + rgb("#d0d0d8"), inset: 6pt)
"##;

/// The drive paths of the images [text] (a note in [folder]) shows: what [note_pdf] needs given.
#[uniffi::export]
pub fn note_images(text: String, folder: String) -> Vec<String> {
    let arena = Arena::new();
    let root = parse_document(&arena, &text, &options());
    let mut paths = Vec::new();
    for node in root.descendants() {
        if let NodeValue::Image(link) = &node.data.borrow().value
            && let Some(p) = link_path(folder.clone(), link.url.clone())
            && !paths.contains(&p)
        {
            paths.push(p);
        }
    }
    paths
}

/// [text], a note in [folder], as PDF pages: [images] are its images' bytes by drive path (as
/// [note_images] lists them; one not given shows as its description). Set in the registered fonts.
#[uniffi::export]
pub fn note_pdf(text: String, folder: String, images: HashMap<String, Vec<u8>>) -> Result<Vec<u8>, RenderError> {
    let arena = Arena::new();
    let root = parse_document(&arena, &text, &options());
    let mut out = Typst { source: String::from(PREAMBLE), folder, images: &images, files: HashMap::new() };
    out.blocks(root);
    let files = out.files;
    typeset(out.source, files)
}

/// A cover page: [title] large, then [lines] (course, name, date...) under it.
#[uniffi::export]
pub fn cover_pdf(title: String, lines: Vec<String>) -> Result<Vec<u8>, RenderError> {
    let mut source = String::from(PREAMBLE);
    source.push_str("#v(1fr)\n#text(size: 26pt, weight: \"bold\")[");
    text(&mut source, &title);
    source.push_str("]\n#v(6mm)\n");
    for line in lines.iter().filter(|l| !l.trim().is_empty()) {
        source.push_str("#text(size: 13pt, fill: rgb(\"#44444c\"))[");
        text(&mut source, line);
        source.push_str("]\n\n");
    }
    source.push_str("#v(2fr)\n");
    typeset(source, HashMap::new())
}

fn options() -> Options<'static> {
    let mut options = Options::default();
    options.extension.strikethrough = true;
    options.extension.table = true;
    options.extension.autolink = true;
    options.extension.tasklist = true;
    options.extension.math_dollars = true;
    options
}

/// [s] as a Typst string literal.
fn string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Text in markup, as it is (nothing in it read as markup).
fn text(out: &mut String, s: &str) {
    if !s.is_empty() {
        _ = write!(out, "#{};", string(s));
    }
}

/// A note's Markdown, written out as Typst markup.
struct Typst<'a> {
    source: String,
    folder: String,
    images: &'a HashMap<String, Vec<u8>>,
    /// The images used, by the names they're read as (reused from note to note: Typst keeps every
    /// file name it's seen for good).
    files: HashMap<String, Vec<u8>>,
}

impl Typst<'_> {
    fn blocks<'a>(&mut self, node: &'a AstNode<'a>) {
        for child in node.children() {
            self.block(child);
        }
    }

    fn inlines<'a>(&mut self, node: &'a AstNode<'a>) {
        for child in node.children() {
            self.inline(child);
        }
    }

    /// [node]'s inlines as a content block.
    fn content<'a>(&mut self, node: &'a AstNode<'a>) {
        self.source.push('[');
        self.inlines(node);
        self.source.push(']');
    }

    fn block<'a>(&mut self, node: &'a AstNode<'a>) {
        let value = node.data.borrow().value.clone();
        match value {
            NodeValue::Heading(h) => {
                _ = write!(self.source, "#heading(level: {})", h.level);
                self.content(node);
                self.source.push_str("\n\n");
            }
            NodeValue::Paragraph => {
                self.inlines(node);
                self.source.push_str("\n\n");
            }
            NodeValue::CodeBlock(c) => {
                let lang = c.info.split_whitespace().next().unwrap_or("").to_string();
                let drawn = (lang == "mermaid").then(|| render_mermaid(c.literal.clone(), false).ok()).flatten();
                match drawn {
                    Some(d) => _ = write!(self.source, "#align(center, image(bytes({}), format: \"svg\", width: {}pt))\n\n", string(&d.svg()), d.width.min(480.0)),
                    None if lang.is_empty() => _ = write!(self.source, "#raw(block: true, {})\n\n", string(c.literal.trim_end_matches('\n'))),
                    None => _ = write!(self.source, "#raw(block: true, lang: {}, {})\n\n", string(&lang), string(c.literal.trim_end_matches('\n'))),
                }
            }
            NodeValue::List(l) => {
                let items: Vec<_> = node.children().collect();
                match l.list_type {
                    ListType::Bullet => _ = write!(self.source, "#list(tight: {}", l.tight),
                    ListType::Ordered => _ = write!(self.source, "#enum(tight: {}, start: {}", l.tight, l.start),
                }
                for item in items {
                    self.source.push_str(", [");
                    if let NodeValue::TaskItem(t) = &item.data.borrow().value {
                        // Drawn (the fonts have no ballot boxes): filled when done.
                        self.source.push_str(if t.symbol.is_some() { CHECKED } else { UNCHECKED });
                    }
                    self.item(item, l.tight);
                    self.source.push(']');
                }
                self.source.push_str(")\n\n");
            }
            NodeValue::BlockQuote => {
                self.source.push_str("#quote(block: true)[");
                self.blocks(node);
                self.source.push_str("]\n\n");
            }
            NodeValue::ThematicBreak => self.source.push_str("#line(length: 100%, stroke: 0.5pt + rgb(\"#d0d0d8\"))\n\n"),
            NodeValue::Table(t) => {
                let align: Vec<&str> = t
                    .alignments
                    .iter()
                    .map(|a| match a {
                        TableAlignment::Center => "center",
                        TableAlignment::Right => "right",
                        _ => "left",
                    })
                    .collect();
                _ = write!(self.source, "#table(columns: {}, align: ({},)", align.len(), align.join(", "));
                for row in node.children() {
                    let header = matches!(row.data.borrow().value, NodeValue::TableRow(true));
                    for cell in row.children() {
                        self.source.push_str(if header { ", strong[" } else { ", [" });
                        self.inlines(cell);
                        self.source.push(']');
                    }
                }
                self.source.push_str(")\n\n");
            }
            NodeValue::HtmlBlock(h) => {
                text(&mut self.source, h.literal.trim_end());
                self.source.push_str("\n\n");
            }
            _ => self.blocks(node),
        }
    }

    /// A list item: a tight one's paragraphs as lines, a loose one's as paragraphs.
    fn item<'a>(&mut self, item: &'a AstNode<'a>, tight: bool) {
        let children: Vec<_> = item.children().collect();
        for (i, child) in children.iter().enumerate() {
            if matches!(child.data.borrow().value, NodeValue::Paragraph) {
                self.inlines(child);
                if i + 1 < children.len() {
                    self.source.push_str(if tight { "\n" } else { "\n\n" });
                }
            } else {
                self.source.push('\n');
                self.block(child);
            }
        }
    }

    fn inline<'a>(&mut self, node: &'a AstNode<'a>) {
        let value = node.data.borrow().value.clone();
        match value {
            NodeValue::Text(t) => text(&mut self.source, &t),
            NodeValue::SoftBreak => self.source.push(' '),
            NodeValue::LineBreak => self.source.push_str("#linebreak();"),
            NodeValue::Code(c) => _ = write!(self.source, "#raw({});", string(&c.literal)),
            NodeValue::Emph => {
                self.source.push_str("#emph");
                self.content(node);
            }
            NodeValue::Strong => {
                self.source.push_str("#strong");
                self.content(node);
            }
            NodeValue::Strikethrough => {
                self.source.push_str("#strike");
                self.content(node);
            }
            NodeValue::Link(l) => {
                _ = write!(self.source, "#link({})", string(&l.url));
                self.content(node);
            }
            NodeValue::Image(l) => {
                let bytes = link_path(self.folder.clone(), l.url.clone()).and_then(|p| self.images.get(&p));
                match bytes {
                    Some(b) if !b.is_empty() => {
                        let name = format!("/image-{}", self.files.len() + 1);
                        self.files.insert(name.clone(), b.clone());
                        _ = write!(self.source, "#box(image({}, width: auto));", string(&name));
                    }
                    // Not there: what it shows, in words.
                    _ => {
                        self.source.push_str("#text(fill: rgb(\"#86868f\"))[");
                        text(&mut self.source, "[");
                        self.inlines(node);
                        text(&mut self.source, "]");
                        self.source.push(']');
                    }
                }
            }
            NodeValue::Math(m) => {
                let tex = m.literal.trim().to_string();
                match render_math(tex.clone(), m.display_math, SIZE, INK) {
                    Ok(d) if m.display_math => {
                        _ = write!(self.source, "#align(center, image(bytes({}), format: \"svg\", width: {}pt));", string(&d.svg()), d.width)
                    }
                    Ok(d) => _ = write!(
                        self.source,
                        "#box(baseline: {}pt, image(bytes({}), format: \"svg\", width: {}pt));",
                        d.height - d.baseline,
                        string(&d.svg()),
                        d.width
                    ),
                    Err(_) => _ = write!(self.source, "#text(fill: rgb(\"#b3261e\"), raw({}));", string(&tex)),
                }
            }
            NodeValue::HtmlInline(h) => text(&mut self.source, &h),
            _ => self.inlines(node),
        }
    }
}

/// Typst [source] (with [files] beside it) as a PDF.
fn typeset(source: String, files: HashMap<String, Vec<u8>>) -> Result<Vec<u8>, RenderError> {
    let world = Paper::new(source, files);
    let document = typst::compile::<PagedDocument>(&world).output.map_err(|e| RenderError::Invalid(errors(e.iter())))?;
    let pdf = typst_pdf::pdf(&document, &typst_pdf::PdfOptions::default()).map_err(|e| RenderError::Invalid(errors(e.iter())))?;
    // What Typst remembers between compiles, let go of (a server sets many notes).
    typst::comemo::evict(0);
    Ok(pdf)
}

fn errors<'a>(e: impl Iterator<Item = &'a typst::diag::SourceDiagnostic>) -> String {
    e.map(|d| d.message.to_string()).collect::<Vec<_>>().join("; ")
}

fn id(path: &str) -> FileId {
    RootedPath::new(VirtualRoot::Project, VirtualPath::new(path).expect("a valid path")).intern()
}

/// What Typst sees: the one source, the files beside it, and the registered fonts.
struct Paper {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
    main: Source,
    files: HashMap<FileId, Bytes>,
}

impl Paper {
    fn new(source: String, files: HashMap<String, Vec<u8>>) -> Self {
        let mut fonts = Vec::new();
        let db = crate::fonts::database();
        for face in db.faces() {
            db.with_face_data(face.id, |data, index| {
                if let Some(f) = Font::new(Bytes::new(data.to_vec()), index) {
                    fonts.push(f);
                }
            });
        }
        let main = Source::new(id("/main.typ"), source);
        Self {
            library: LazyHash::new(Library::default()),
            book: LazyHash::new(FontBook::from_fonts(&fonts)),
            fonts,
            main,
            files: files.into_iter().map(|(name, bytes)| (id(&name), Bytes::new(bytes))).collect(),
        }
    }
}

impl World for Paper {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }

    fn main(&self) -> FileId {
        self.main.id()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main.id() { Ok(self.main.clone()) } else { Err(FileError::NotSource) }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.files.get(&id).cloned().ok_or(FileError::AccessDenied)
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index).cloned()
    }

    fn today(&self, _offset: Option<typst::foundations::Duration>) -> Option<Datetime> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fonts() {
        for f in ["Pretendard-Regular.otf", "Pretendard-SemiBold.otf", "Pretendard-Bold.otf", "JetBrainsMono-Regular.ttf"] {
            crate::register_font(std::fs::read(format!("fonts/{f}")).unwrap());
        }
    }

    #[test]
    fn a_note_is_set_with_its_math_lists_tables_and_images() {
        fonts();
        let note = "# 확률및통계 HW3\n\n**풀이.** 베이즈 정리에 의해 $P(A\\mid B) = \\frac{P(B\\mid A)P(A)}{P(B)}$ 이다. *\"따옴표\"* `x[i] = #y`\n\n$$\\sum_{i=1}^{n} x_i^2$$\n\n- [x] 끝\n- [ ] 남음\n  1. 안\n\n> 인용 $x$\n\n| x | P(x) |\n|:-:|-----:|\n| 0 | 0.5 |\n\n```python\nprint(\"hi\")\n```\n\n```mermaid\ngraph LR; A-->B\n```\n\n![plot](img/p.png) ![gone](img/none.png)\n\n$\\frac{$\n";
        assert_eq!(note_images(note.into(), "Projects/P/".into()), vec!["Projects/P/img/p.png", "Projects/P/img/none.png"]);
        // A 1x1 PNG.
        let png = vec![
            0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d, 0x49, 0x48, 0x44, 0x52, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 0x1f, 0x15, 0xc4, 0x89, 0, 0, 0, 0x0d, 0x49, 0x44, 0x41,
            0x54, 0x78, 0x9c, 0x63, 0x60, 0x60, 0x60, 0xf8, 0x0f, 0, 0x01, 0x04, 0x01, 0, 0x5f, 0xe5, 0xc3, 0x4b, 0, 0, 0, 0, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
        ];
        let pdf = note_pdf(note.into(), "Projects/P/".into(), HashMap::from([("Projects/P/img/p.png".to_string(), png)])).unwrap();
        assert!(pdf.starts_with(b"%PDF"));
        if let Ok(dir) = std::env::var("PRINT_TEST_OUT") {
            std::fs::write(format!("{dir}/note.pdf"), &pdf).unwrap();
        }
        let cover = cover_pdf("HW3".into(), vec!["확률및통계".into(), "".into(), "2026-10-22".into()]).unwrap();
        assert!(cover.starts_with(b"%PDF"));
        if let Ok(dir) = std::env::var("PRINT_TEST_OUT") {
            std::fs::write(format!("{dir}/cover.pdf"), &cover).unwrap();
        }
    }
}
