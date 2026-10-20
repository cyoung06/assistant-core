//! The structure every editor styles: ranges and hidden syntax in UTF-16 units.
use assistant_core::{Element, Span, TextRange, markdown_spans};

fn r(from: u32, to: u32) -> TextRange {
    TextRange { from, to }
}

/// The spans, as (element, the text it covers, the texts of its marks).
fn spans(text: &str) -> Vec<(Element, String, Vec<String>)> {
    let units: Vec<u16> = text.encode_utf16().collect();
    let cut = |t: &TextRange| String::from_utf16(&units[t.from as usize..t.to as usize]).unwrap();
    markdown_spans(text.into()).iter().map(|s: &Span| (s.element.clone(), cut(&s.range), s.marks.iter().map(cut).collect())).collect()
}

#[test]
fn headings_and_emphasis_hide_their_marks() {
    assert_eq!(
        spans("# 제목 *기울임*\n\n**굵게** and ~~지움~~ `코드`\n"),
        vec![
            (Element::Heading { level: 1 }, "# 제목 *기울임*".into(), vec!["# ".into()]),
            (Element::Emphasis, "*기울임*".into(), vec!["*".into(), "*".into()]),
            (Element::Strong, "**굵게**".into(), vec!["**".into(), "**".into()]),
            (Element::Strikethrough, "~~지움~~".into(), vec!["~~".into(), "~~".into()]),
            (Element::InlineCode, "`코드`".into(), vec!["`".into(), "`".into()]),
        ]
    );
}

#[test]
fn offsets_are_utf16() {
    // An emoji is two UTF-16 units; the emphasis after it starts at 3.
    let s = markdown_spans("😀 *a*".into());
    assert_eq!(s[0].range, r(3, 6));
}

#[test]
fn links_images_and_math() {
    assert_eq!(
        spans("[글](https://a.b \"t\") ![그림](img.png) https://c.d $x^2$\n\n$$\\frac{1}{2}$$\n"),
        vec![
            (Element::Link { url: "https://a.b".into() }, "[글](https://a.b \"t\")".into(), vec!["[".into(), "](https://a.b \"t\")".into()]),
            (Element::Image { url: "img.png".into(), alt: "그림".into() }, "![그림](img.png)".into(), vec![]),
            (Element::Autolink { url: "https://c.d".into() }, "https://c.d".into(), vec![]),
            (Element::Math { tex: "x^2".into(), display: false }, "$x^2$".into(), vec![]),
            (Element::Math { tex: "\\frac{1}{2}".into(), display: true }, "$$\\frac{1}{2}$$".into(), vec![]),
        ]
    );
}

#[test]
fn money_isnt_math() {
    assert!(spans("$5 and $10\n").is_empty());
}

#[test]
fn lists_tasks_quotes_rules_code_tables() {
    let s = spans("- 하나\n- [x] 끝\n- [ ] 할 일\n\n> 인용\n> 둘째\n\n---\n\n```rust\nfn x() {}\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n");
    let kinds: Vec<_> = s.iter().map(|(e, _, m)| (e.clone(), m.clone())).collect();
    assert_eq!(
        kinds,
        vec![
            (Element::Bullet, vec!["-".into()]),
            (Element::Task { checked: true }, vec!["- [x]".into()]),
            (Element::Task { checked: false }, vec!["- [ ]".into()]),
            (Element::Quote, vec!["> ".into(), "> ".into()]),
            (Element::Rule, vec![]),
            (Element::CodeBlock { language: "rust".into() }, vec!["```rust".into(), "```".into()]),
            (Element::Table, vec![]),
        ]
    );
}

#[test]
fn setext_headings_hide_their_underline() {
    assert_eq!(spans("제목\n===\n"), vec![(Element::Heading { level: 1 }, "제목\n===".into(), vec!["===".into()])]);
}
