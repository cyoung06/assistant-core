//! How notes are edited, the same in every app: wrapping a selection in marks, the "/" menu,
//! links between drive files, names for pasted images. Offsets are UTF-16 units.

use percent_encoding::{NON_ALPHANUMERIC, AsciiSet, percent_decode_str, utf8_percent_encode};
use url::Url;

/// What's left as it is in a path part of a link (RFC 3986's unreserved characters).
const PART: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'.').remove(b'_').remove(b'~');

/// One change: [from]..[to] (UTF-16 units of the text before any change) becomes [insert].
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "web", derive(serde::Serialize), serde(rename_all = "camelCase"))]
pub struct Replacement {
    pub from: u32,
    pub to: u32,
    pub insert: String,
}

/// Changes to make (each against the text before any of them), and the selection after them.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "web", derive(serde::Serialize), serde(rename_all = "camelCase"))]
pub struct Edit {
    pub changes: Vec<Replacement>,
    pub anchor: u32,
    pub head: u32,
}

/// Wraps the selection [from]..[to] in [mark] (** for bold), or unwraps it if it's wrapped already
/// (the marks selected too, or just outside it).
#[uniffi::export]
pub fn toggle_wrap(text: String, from: u32, to: u32, mark: String) -> Edit {
    let units: Vec<u16> = text.encode_utf16().collect();
    let m: Vec<u16> = mark.encode_utf16().collect();
    let (f, t, n) = (from as usize, to as usize, m.len());
    let at = |i: usize| units.get(i..i + n) == Some(&m[..]);
    let r = |from: usize, to: usize, insert: &str| Replacement { from: from as u32, to: to as u32, insert: insert.into() };
    if t >= f + 2 * n && at(f) && at(t - n) {
        return Edit { changes: vec![r(f, f + n, ""), r(t - n, t, "")], anchor: from, head: (t - 2 * n) as u32 };
    }
    if f >= n && at(f - n) && at(t) {
        return Edit { changes: vec![r(f - n, f, ""), r(t, t + n, "")], anchor: (f - n) as u32, head: (t - n) as u32 };
    }
    Edit { changes: vec![r(f, f, &mark), r(t, t, &mark)], anchor: (f + n) as u32, head: (t + n) as u32 }
}

/// Something the "/" menu inserts: [text], with the cursor at [cursor] (UTF-16 units into it).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "web", derive(serde::Serialize), serde(rename_all = "camelCase"))]
pub struct SlashItem {
    pub label: String,
    /// Other words it's found by.
    pub keywords: String,
    pub text: String,
    pub cursor: u32,
}

/// What the "/" menu offers, in order.
#[uniffi::export]
pub fn slash_items() -> Vec<SlashItem> {
    // "|" marks where the cursor goes (a table's are its own: the cursor goes in its first header cell).
    [
        ("Heading 1", "h1 title", "# |"),
        ("Heading 2", "h2", "## |"),
        ("Heading 3", "h3", "### |"),
        ("Bulleted list", "bullet ul", "- |"),
        ("Numbered list", "ordered ol", "1. |"),
        ("To-do", "task checkbox todo", "- [ ] |"),
        ("Quote", "blockquote", "> |"),
        ("Code block", "pre fence", "```\n|\n```"),
        ("Math block", "equation latex tex katex display", "$$\n|\n$$"),
        ("Inline math", "equation latex tex", "$|$"),
        ("Diagram", "mermaid flowchart chart graph sequence", "```mermaid\nflowchart LR\n  A[|] --> B[ ]\n```"),
        ("Table", "grid", "| Column | Column |\n| --- | --- |\n| | |"),
        ("Divider", "rule hr line", "---\n|"),
    ]
    .into_iter()
    .map(|(label, keywords, template)| {
        let (text, cursor) = if template.starts_with("| ") {
            (template.to_string(), 2)
        } else {
            match template.find('|') {
                Some(at) => (format!("{}{}", &template[..at], &template[at + 1..]), template[..at].encode_utf16().count()),
                None => (template.to_string(), template.encode_utf16().count()),
            }
        };
        SlashItem { label: label.into(), keywords: keywords.into(), text, cursor: cursor as u32 }
    })
    .collect()
}

/// The drive path a link or image in a note in [folder] points at (RFC 3986, against the folder);
/// none for web addresses, other schemes and anchors.
#[uniffi::export]
pub fn link_path(folder: String, href: String) -> Option<String> {
    if href.is_empty() || href.starts_with('#') || href.starts_with("//") || Url::parse(&href).is_ok() {
        return None;
    }
    let base = Url::parse("drive:///").ok()?.join(&encode_path(&folder)).ok()?;
    let target = base.join(&href).ok()?;
    let decoded = percent_decode_str(target.path()).decode_utf8().ok()?;
    Some(decoded.trim_start_matches('/').to_string())
}

/// How a note in [folder] links to the drive file [path]: relative, each part percent-encoded.
#[uniffi::export]
pub fn relative_link(folder: String, path: String) -> String {
    let from: Vec<&str> = folder.split('/').filter(|p| !p.is_empty()).collect();
    let to: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    let mut same = 0;
    while same < from.len() && same + 1 < to.len() && from[same] == to[same] {
        same += 1;
    }
    let up = std::iter::repeat_n("..".to_string(), from.len() - same);
    let down = to[same..].iter().map(|p| utf8_percent_encode(p, PART).to_string());
    up.chain(down).collect::<Vec<_>>().join("/")
}

/// A name for a pasted image: its own, unless it's a clipboard's generic one ("image.png"), then
/// "Pasted image <now>.<ext>". [now] is the local time as YYYYMMDDhhmmss.
#[uniffi::export]
pub fn pasted_name(name: String, mime: String, now: String) -> String {
    let generic = name.is_empty() || {
        let lower = name.to_lowercase();
        ["png", "jpg", "jpeg", "gif", "webp"].iter().any(|e| lower == format!("image.{e}"))
    };
    if !generic {
        return name.replace('/', " ");
    }
    let ext = mime.split('/').nth(1).map(|e| e.replace("jpeg", "jpg")).unwrap_or_else(|| "png".into());
    format!("Pasted image {now}.{ext}")
}

/// A folder path as a URL path (each part encoded, ending in "/").
fn encode_path(folder: &str) -> String {
    let parts: Vec<String> = folder.split('/').filter(|p| !p.is_empty()).map(|p| utf8_percent_encode(p, PART).to_string()).collect();
    if parts.is_empty() { "/".into() } else { format!("/{}/", parts.join("/")) }
}

/// What a note in [folder] links to, in order, once each: web and app addresses as written, links to
/// drive files (relative, as notes write them) as `drive:<path>`, and mentions as `mention:<name>`
/// (resolved by whoever reads them). Links in code aren't links; images count (a note shows them).
#[uniffi::export]
pub fn note_links(text: String, folder: String) -> Vec<String> {
    use comrak::nodes::NodeValue;
    let mut options = comrak::Options::default();
    options.extension.autolink = true;
    options.extension.table = true;
    options.extension.tasklist = true;
    options.extension.strikethrough = true;
    options.extension.math_dollars = true;
    let arena = comrak::Arena::new();
    let root = comrak::parse_document(&arena, &text, &options);
    let mut out: Vec<String> = Vec::new();
    for node in root.descendants() {
        let url = match &node.data.borrow().value {
            NodeValue::Link(l) | NodeValue::Image(l) => l.url.clone(),
            _ => continue,
        };
        let target = match link_path(folder.clone(), url.clone()) {
            Some(path) => format!("drive:{path}"),
            None if url.starts_with('#') || url.is_empty() => continue,
            None => url,
        };
        if !out.contains(&target) {
            out.push(target);
        }
    }
    let at = crate::markdown::Offsets::new(&text);
    for m in crate::markdown::mentions(&text, &crate::markdown::literal(root, &text, &at)) {
        let target = format!("mention:{}", m.name);
        if !out.contains(&target) {
            out.push(target);
        }
    }
    out
}

/// How a mention of [name] is written: `@Name` when it's only letters, digits and `_ - . #` (not
/// ending in `.` or `-`), `@[Name]` otherwise (a `]` in it can't be written, so it's left out).
#[uniffi::export]
pub fn mention_text(name: String) -> String {
    let plain = !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || "_-.#".contains(c)) && !name.ends_with(['.', '-']);
    if plain { format!("@{name}") } else { format!("@[{}]", name.replace(']', "").trim()) }
}

#[cfg(test)]
mod note_links_tests {
    use super::*;

    #[test]
    fn links_in_a_note() {
        let text = "See [HW3](https://assistant.example/task/abc) and [slides](../Course%20files/w1.pdf), ![plot](img/p.png)\n\n`[not](a.md)`\n\n```\n[nor](b.md)\n```\n\n<https://x.org> [again](https://assistant.example/task/abc) [top](#a)";
        assert_eq!(
            note_links(text.into(), "Projects/P/Notes/".into()),
            vec!["https://assistant.example/task/abc", "drive:Projects/P/Course files/w1.pdf", "drive:Projects/P/Notes/img/p.png", "https://x.org"]
        );
        assert_eq!(note_links("Ask @prof about @[HW 3], @HW3".into(), "".into()), vec!["mention:prof", "mention:HW 3", "mention:HW3"]);
    }

    #[test]
    fn mentions_written_plain_or_bracketed() {
        assert_eq!(mention_text("HW3".into()), "@HW3");
        assert_eq!(mention_text("#12".into()), "@#12");
        assert_eq!(mention_text("확률및통계".into()), "@확률및통계");
        assert_eq!(mention_text("week 1 slides.pdf".into()), "@[week 1 slides.pdf]");
        assert_eq!(mention_text("v2.".into()), "@[v2.]");
    }
}
