use assistant_core::{NotebookChange, NotebookDoc, NotebookEdit, NotebookPage, NotebookStroke};

fn page(id: &str, base: i32) -> NotebookPage {
    NotebookPage { id: id.into(), base, style: if base < 0 { "grid".into() } else { String::new() }, width: 595.0, height: 842.0 }
}

fn stroke(id: &str, page: &str, ink: Option<Vec<u8>>) -> NotebookStroke {
    NotebookStroke {
        id: id.into(),
        page: page.into(),
        tool: "pen".into(),
        color: 0xFF1B1B1F,
        width: 0.45,
        points: vec![10.0, 20.0, 0.5, 30.0, 40.0, 0.8],
        outline: vec![vec![9.0, 19.0, 31.0, 39.0, 29.0, 41.0, 11.0, 21.0]],
        ink,
        transform: None,
    }
}

/// The server's notebook (made from a PDF) and a device joined to it, as y-websocket does.
fn join(server: &NotebookDoc, device: &NotebookDoc) {
    let reply = server.receive(device.start()).unwrap().reply.unwrap();
    device.receive(reply).unwrap();
    let back = device.receive(server.start()).unwrap().reply.unwrap();
    server.receive(back).unwrap();
}

/// [message] from one device, through the server, to another; what that one was told changed.
fn pass(server: &NotebookDoc, to: &NotebookDoc, message: Vec<u8>) -> Vec<NotebookChange> {
    let out = server.receive(message).unwrap().broadcast.unwrap();
    to.receive(out).unwrap().changes
}

fn notebook() -> (NotebookDoc, NotebookDoc, NotebookDoc) {
    let server = NotebookDoc::new();
    server.edit(vec![
        NotebookEdit::SetBase { blob: "sha256-base".into() },
        NotebookEdit::InsertPage { index: 0, page: page("p1", 0) },
        NotebookEdit::InsertPage { index: 1, page: page("p2", 1) },
    ]);
    let (tablet, mac) = (NotebookDoc::new(), NotebookDoc::new());
    join(&server, &tablet);
    join(&server, &mac);
    (std::sync::Arc::try_unwrap(server).ok().unwrap(), std::sync::Arc::try_unwrap(tablet).ok().unwrap(), std::sync::Arc::try_unwrap(mac).ok().unwrap())
}

#[test]
fn a_device_joins_with_the_pages() {
    let (_, tablet, _) = notebook();
    assert_eq!(tablet.pages(), vec![page("p1", 0), page("p2", 1)]);
    assert_eq!(tablet.base(), "sha256-base");
}

#[test]
fn a_tablet_stroke_reaches_the_mac_whole_and_the_mac_can_erase_it() {
    let (server, tablet, mac) = notebook();
    let s = stroke("s1", "p1", Some(vec![1, 2, 3, 4]));
    let changes = pass(&server, &mac, tablet.edit(vec![NotebookEdit::PutStroke { stroke: s.clone() }]));
    assert_eq!(changes, vec![NotebookChange::Stroke { id: "s1".into(), stroke: Some(s.clone()) }]);
    assert_eq!(mac.strokes(), vec![s]);

    // Erased on the Mac (whole, as the eraser takes strokes): gone on the tablet.
    let changes = pass(&server, &tablet, mac.edit(vec![NotebookEdit::RemoveStroke { id: "s1".into() }]));
    assert_eq!(changes, vec![NotebookChange::Stroke { id: "s1".into(), stroke: None }]);
    assert!(tablet.strokes().is_empty());
}

#[test]
fn a_mac_stroke_moved_on_the_tablet_keeps_what_it_was() {
    let (server, tablet, mac) = notebook();
    let s = stroke("m1", "p2", None);
    pass(&server, &tablet, mac.edit(vec![NotebookEdit::PutStroke { stroke: s.clone() }]));
    // Moved on the tablet: the same stroke, with where it went.
    let moved = NotebookStroke { transform: Some(vec![1.0, 0.0, 0.0, 1.0, 50.0, 60.0]), ..s.clone() };
    let changes = pass(&server, &mac, tablet.edit(vec![NotebookEdit::PutStroke { stroke: moved.clone() }]));
    assert_eq!(changes, vec![NotebookChange::Stroke { id: "m1".into(), stroke: Some(moved.clone()) }]);
    assert_eq!(mac.strokes()[0].points, s.points);
    assert!(mac.strokes()[0].ink.is_none());
}

#[test]
fn pages_added_on_two_devices_at_once_are_both_kept() {
    let (server, tablet, mac) = notebook();
    let a = tablet.edit(vec![NotebookEdit::InsertPage { index: 1, page: page("t", -1) }]);
    let b = mac.edit(vec![NotebookEdit::InsertPage { index: 1, page: page("m", -1) }]);
    pass(&server, &mac, a);
    pass(&server, &tablet, b);
    let ids = |d: &NotebookDoc| d.pages().into_iter().map(|p| p.id).collect::<Vec<_>>();
    assert_eq!(ids(&tablet), ids(&mac));
    assert_eq!(ids(&tablet).len(), 4);
    assert_eq!(ids(&tablet).first().map(String::as_str), Some("p1"));
    assert_eq!(ids(&tablet).last().map(String::as_str), Some("p2"));
}

#[test]
fn a_page_removed_takes_what_was_on_it() {
    let (server, tablet, mac) = notebook();
    pass(&server, &mac, tablet.edit(vec![
        NotebookEdit::PutStroke { stroke: stroke("a", "p1", None) },
        NotebookEdit::PutStroke { stroke: stroke("b", "p2", None) },
        NotebookEdit::Bookmark { page: "p1".into(), on: true },
    ]));
    assert_eq!(mac.bookmarks(), vec!["p1".to_string()]);
    let changes = pass(&server, &tablet, mac.edit(vec![NotebookEdit::RemovePage { id: "p1".into() }]));
    assert!(changes.contains(&NotebookChange::Pages));
    assert!(changes.contains(&NotebookChange::Stroke { id: "a".into(), stroke: None }));
    assert_eq!(tablet.pages().into_iter().map(|p| p.id).collect::<Vec<_>>(), vec!["p2"]);
    assert_eq!(tablet.strokes().into_iter().map(|s| s.id).collect::<Vec<_>>(), vec!["b"]);
    assert!(tablet.bookmarks().is_empty());
}

#[test]
fn a_saved_notebook_loads_as_it_was() {
    let (server, tablet, _) = notebook();
    server.receive(tablet.edit(vec![NotebookEdit::PutStroke { stroke: stroke("s", "p1", Some(vec![9])) }])).unwrap();
    let reloaded = NotebookDoc::from_state(server.state()).unwrap();
    assert_eq!(reloaded.strokes(), tablet.strokes());
    assert_eq!(reloaded.pages(), tablet.pages());
}
