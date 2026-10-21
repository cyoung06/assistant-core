use assistant_core::{SyncDoc, TextChange};

/// The text as an editor holds it, kept up by applying the changes it's told about (UTF-16).
struct Editor(Vec<u16>);

impl Editor {
    fn apply(&mut self, changes: &[TextChange]) {
        for c in changes {
            let insert: Vec<u16> = c.insert.encode_utf16().collect();
            self.0.splice(c.from as usize..c.to as usize, insert);
        }
    }
    fn text(&self) -> String {
        String::from_utf16(&self.0).unwrap()
    }
}

/// A client joining: its sync step 1 to the server, the server's reply back, and the server's own
/// step 1 answered (as y-websocket does both ways).
fn join(server: &SyncDoc, client: &SyncDoc) -> Vec<TextChange> {
    let reply = server.receive(client.start()).unwrap().reply.unwrap();
    let got = client.receive(reply).unwrap();
    let back = client.receive(server.start()).unwrap().reply.unwrap();
    server.receive(back).unwrap();
    got.changes
}

#[test]
fn a_client_gets_the_note_as_changes_it_can_apply() {
    let server = SyncDoc::from_text("# 노트\n수식 $x^2$ 😀 끝".into());
    let client = SyncDoc::new();
    let mut editor = Editor(Vec::new());
    editor.apply(&join(&server, &client));
    assert_eq!(editor.text(), "# 노트\n수식 $x^2$ 😀 끝");
    assert_eq!(client.text(), server.text());
}

#[test]
fn edits_on_two_devices_meet_in_the_same_text() {
    let server = SyncDoc::from_text("하나 둘 셋".into());
    let (a, b) = (SyncDoc::new(), SyncDoc::new());
    let mut ea = Editor(Vec::new());
    let mut eb = Editor(Vec::new());
    ea.apply(&join(&server, &a));
    eb.apply(&join(&server, &b));

    // At the same time: A types after "하나" (UTF-16 index 2), B replaces "셋" (5..6) with "넷 😀".
    let from_a = a.edit(2, 2, "!".into());
    ea.apply(&[TextChange { from: 2, to: 2, insert: "!".into() }]);
    let from_b = b.edit(5, 6, "넷 😀".into());
    eb.apply(&[TextChange { from: 5, to: 6, insert: "넷 😀".into() }]);

    // The server takes both and passes each on to the other device.
    let to_b = server.receive(from_a).unwrap().broadcast.unwrap();
    let to_a = server.receive(from_b).unwrap().broadcast.unwrap();
    ea.apply(&a.receive(to_a).unwrap().changes);
    eb.apply(&b.receive(to_b).unwrap().changes);

    assert_eq!(server.text(), "하나! 둘 넷 😀");
    assert_eq!(a.text(), server.text());
    assert_eq!(b.text(), server.text());
    // What each editor shows, from the changes alone, is the same text.
    assert_eq!(ea.text(), server.text());
    assert_eq!(eb.text(), server.text());
}

#[test]
fn a_device_is_not_told_about_its_own_edits() {
    let server = SyncDoc::from_text("abc".into());
    let client = SyncDoc::new();
    join(&server, &client);
    client.edit(3, 3, "d".into());
    // Nothing of its own comes back as a change.
    assert!(client.receive(server.start()).unwrap().changes.is_empty());
}

#[test]
fn a_saved_document_loads_as_it_was() {
    let server = SyncDoc::from_text("before".into());
    let client = SyncDoc::new();
    join(&server, &client);
    server.receive(client.edit(6, 6, " after".into())).unwrap();
    let reloaded = SyncDoc::from_state(server.state()).unwrap();
    assert_eq!(reloaded.text(), "before after");
    // A device that was in sync stays in sync with the reloaded one: nothing new either way.
    let again = reloaded.receive(client.start()).unwrap().reply.unwrap();
    assert!(client.receive(again).unwrap().changes.is_empty());
}

#[test]
fn awareness_is_passed_on_as_it_came() {
    let server = SyncDoc::new();
    // y-protocols awareness: message type 1, then the update (no clients in it here).
    let awareness = vec![1u8, 1, 0];
    let got = server.receive(awareness.clone()).unwrap();
    assert_eq!(got.broadcast, Some(awareness));
    assert!(got.reply.is_none());
}

#[test]
fn garbage_is_refused() {
    assert!(SyncDoc::new().receive(vec![0, 9, 9]).is_err());
}
