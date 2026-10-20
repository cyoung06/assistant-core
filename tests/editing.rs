//! The web editor's own cases (md.test.ts), for the shared editing rules.
use assistant_core::*;

fn r(from: u32, to: u32, insert: &str) -> Replacement {
    Replacement { from, to, insert: insert.into() }
}

#[test]
fn wraps_and_unwraps_a_selection() {
    assert_eq!(toggle_wrap("a word b".into(), 2, 6, "**".into()), Edit { changes: vec![r(2, 2, "**"), r(6, 6, "**")], anchor: 4, head: 8 });
    // Selected inside the marks, or the marks selected too: unwrapped either way.
    assert_eq!(toggle_wrap("a **word** b".into(), 4, 8, "**".into()).changes, vec![r(2, 4, ""), r(8, 10, "")]);
    assert_eq!(toggle_wrap("a **word** b".into(), 2, 10, "**".into()).changes, vec![r(2, 4, ""), r(8, 10, "")]);
    // UTF-16 units: after an emoji (two units).
    assert_eq!(toggle_wrap("😀 단어".into(), 3, 5, "*".into()).changes, vec![r(3, 3, "*"), r(5, 5, "*")]);
}

#[test]
fn places_the_cursor_where_a_template_says() {
    let items = slash_items();
    let find = |label: &str| items.iter().find(|i| i.label == label).unwrap().clone();
    assert_eq!((find("Heading 2").text, find("Heading 2").cursor), ("## ".into(), 3));
    assert_eq!((find("Math block").text, find("Math block").cursor), ("$$\n\n$$".into(), 3));
    assert_eq!(find("Table").cursor, 2);
}

#[test]
fn resolves_links_relative_to_the_notes_folder() {
    let p = |folder: &str, href: &str| link_path(folder.into(), href.into());
    assert_eq!(p("Projects/A/Notes/", "img/x%20y.png").as_deref(), Some("Projects/A/Notes/img/x y.png"));
    assert_eq!(p("Projects/A/Notes/", "../Course files/L1.pdf#page=2").as_deref(), Some("Projects/A/Course files/L1.pdf"));
    assert_eq!(p("Projects/A/", "./a.md").as_deref(), Some("Projects/A/a.md"));
    assert_eq!(p("Projects/A/", "/Projects/B/b.md").as_deref(), Some("Projects/B/b.md"));
    assert_eq!(p("Projects/한글/", "그림.png").as_deref(), Some("Projects/한글/그림.png"));
    assert_eq!(p("Projects/A/", "https://example.com"), None);
    assert_eq!(p("Projects/A/", "mailto:x@y.z"), None);
    assert_eq!(p("Projects/A/", "#section"), None);
}

#[test]
fn writes_links_relative_to_the_note_and_they_resolve_back() {
    assert_eq!(relative_link("Projects/A/".into(), "Projects/A/attachments/Pasted image 1.png".into()), "attachments/Pasted%20image%201.png");
    let link = relative_link("Projects/A/Notes/".into(), "Projects/A/Course files/L1.pdf".into());
    assert_eq!(link, "../Course%20files/L1.pdf");
    assert_eq!(link_path("Projects/A/Notes/".into(), link).as_deref(), Some("Projects/A/Course files/L1.pdf"));
}

#[test]
fn names_pasted_images() {
    assert_eq!(pasted_name("image.png".into(), "image/png".into(), "20261020030405".into()), "Pasted image 20261020030405.png");
    assert_eq!(pasted_name("".into(), "image/jpeg".into(), "20261020030405".into()), "Pasted image 20261020030405.jpg");
    assert_eq!(pasted_name("diagram.png".into(), "image/png".into(), "x".into()), "diagram.png");
}
