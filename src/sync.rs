//! Notes edited live on several devices: a note is a Yjs document (yrs, the Rust Yjs) whose text is
//! "note", synced with the standard y-websocket protocol (y-protocols: sync steps 1 and 2, updates,
//! awareness). The server and the apps use the same [SyncDoc]; the web uses Yjs itself, which reads
//! and writes the same format. Positions are UTF-16 code units, as the editors index text.

use std::sync::{Arc, Mutex};

use yrs::sync::{Message, SyncMessage};
use yrs::types::Observable;
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::{Encode, Encoder, EncoderV1};
use yrs::{Doc, GetString, OffsetKind, Options, ReadTxn, StateVector, Text, TextRef, Transact, Update};

/// The text's name in the document (the web's binding opens the same).
const TEXT: &str = "note";

/// The text changed: [to] - [from] UTF-16 units from [from] replaced by [insert]. In order, each
/// against the text as the ones before it left it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct TextChange {
    pub from: u32,
    pub to: u32,
    pub insert: String,
}

/// What a message received calls for: [reply] goes back to whoever sent it, [broadcast] (the
/// server's) to everyone else on the document, and [changes] are what it did to the text.
#[derive(Debug, Clone, Default, uniffi::Record)]
pub struct Received {
    pub reply: Option<Vec<u8>>,
    pub broadcast: Option<Vec<u8>>,
    pub changes: Vec<TextChange>,
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum SyncError {
    /// Not a y-websocket message, or an update that doesn't decode.
    #[error("{0}")]
    Invalid(String),
}

fn invalid(e: impl std::fmt::Display) -> SyncError {
    SyncError::Invalid(e.to_string())
}

struct Inner {
    doc: Doc,
    text: TextRef,
    /// Filled by the text's observer as transactions commit; taken by whoever made them.
    changes: Arc<Mutex<Vec<TextChange>>>,
}

/// A note's shared document.
#[derive(uniffi::Object)]
pub struct SyncDoc {
    inner: Mutex<Inner>,
}

impl SyncDoc {
    fn with_doc(doc: Doc) -> Arc<Self> {
        let text = doc.get_or_insert_text(TEXT);
        let changes = Arc::new(Mutex::new(Vec::new()));
        let sink = changes.clone();
        text.observe("changes", move |txn, event| {
            let mut out = sink.lock().unwrap();
            let mut at = 0u32;
            for delta in event.delta(txn) {
                match delta {
                    yrs::types::Delta::Retain(n, _) => at += n,
                    yrs::types::Delta::Deleted(n) => out.push(TextChange { from: at, to: at + n, insert: String::new() }),
                    yrs::types::Delta::Inserted(value, _) => {
                        let s = value.clone().cast::<String>().unwrap_or_default();
                        let n = s.encode_utf16().count() as u32;
                        out.push(TextChange { from: at, to: at, insert: s });
                        at += n;
                    }
                }
            }
        });
        Arc::new(Self { inner: Mutex::new(Inner { doc, text, changes }) })
    }

    fn new_doc() -> Doc {
        Doc::with_options(Options { offset_kind: OffsetKind::Utf16, ..Options::default() })
    }

    fn message(m: Message) -> Vec<u8> {
        let mut e = EncoderV1::new();
        m.encode(&mut e);
        e.to_vec()
    }
}

#[uniffi::export]
impl SyncDoc {
    /// An empty document: an app's, filled by syncing with the server.
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Self::with_doc(Self::new_doc())
    }

    /// The server's first document for a note: its text as the file has it. Only the server
    /// makes one this way (two would hold the same text twice once merged).
    #[uniffi::constructor]
    pub fn from_text(text: String) -> Arc<Self> {
        let doc = Self::with_doc(Self::new_doc());
        {
            let inner = doc.inner.lock().unwrap();
            inner.text.insert(&mut inner.doc.transact_mut(), 0, &text);
            inner.changes.lock().unwrap().clear();
        }
        doc
    }

    /// A document as [state] (from [SyncDoc::state]) saved it.
    #[uniffi::constructor]
    pub fn from_state(state: Vec<u8>) -> Result<Arc<Self>, SyncError> {
        let doc = Self::with_doc(Self::new_doc());
        {
            let inner = doc.inner.lock().unwrap();
            let update = Update::decode_v1(&state).map_err(invalid)?;
            inner.doc.transact_mut().apply_update(update).map_err(invalid)?;
            inner.changes.lock().unwrap().clear();
        }
        Ok(doc)
    }

    /// The text now.
    pub fn text(&self) -> String {
        let inner = self.inner.lock().unwrap();
        inner.text.get_string(&inner.doc.transact())
    }

    /// Everything in the document, to be kept and loaded with [SyncDoc::from_state].
    pub fn state(&self) -> Vec<u8> {
        let inner = self.inner.lock().unwrap();
        inner.doc.transact().encode_state_as_update_v1(&StateVector::default())
    }

    /// The message that starts a sync (sync step 1: what this side has), sent on connecting.
    pub fn start(&self) -> Vec<u8> {
        let inner = self.inner.lock().unwrap();
        let sv = inner.doc.transact().state_vector();
        Self::message(Message::Sync(SyncMessage::SyncStep1(sv)))
    }

    /// A message from the other side. Awareness (others' cursors) is passed on as it came.
    pub fn receive(&self, message: Vec<u8>) -> Result<Received, SyncError> {
        let mut out = Received::default();
        // One message to a frame (as y-websocket sends them).
        {
            match Message::decode_v1(&message).map_err(invalid)? {
                Message::Sync(SyncMessage::SyncStep1(sv)) => {
                    let inner = self.inner.lock().unwrap();
                    let diff = inner.doc.transact().encode_state_as_update_v1(&sv);
                    out.reply = Some(Self::message(Message::Sync(SyncMessage::SyncStep2(diff))));
                }
                Message::Sync(SyncMessage::SyncStep2(update)) | Message::Sync(SyncMessage::Update(update)) => {
                    let inner = self.inner.lock().unwrap();
                    let decoded = Update::decode_v1(&update).map_err(invalid)?;
                    inner.doc.transact_mut().apply_update(decoded).map_err(invalid)?;
                    out.changes.extend(inner.changes.lock().unwrap().drain(..));
                    out.broadcast = Some(Self::message(Message::Sync(SyncMessage::Update(update))));
                }
                Message::Awareness(_) => out.broadcast = Some(message.clone()),
                Message::AwarenessQuery | Message::Auth(_) | Message::Custom(_, _) => {}
            }
        }
        Ok(out)
    }

    /// An edit made here: [to] - [from] UTF-16 units from [from] replaced by [insert]. Returns the
    /// message that sends it.
    pub fn edit(&self, from: u32, to: u32, insert: String) -> Vec<u8> {
        let inner = self.inner.lock().unwrap();
        let update = {
            let mut txn = inner.doc.transact_mut();
            let len = inner.text.len(&txn);
            let (from, to) = (from.min(len), to.min(len).max(from.min(len)));
            if to > from {
                inner.text.remove_range(&mut txn, from, to - from);
            }
            if !insert.is_empty() {
                inner.text.insert(&mut txn, from, &insert);
            }
            txn.encode_update_v1()
        };
        // Its own changes aren't news to whoever made them.
        inner.changes.lock().unwrap().clear();
        Self::message(Message::Sync(SyncMessage::Update(update)))
    }
}
