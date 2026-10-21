//! Notebooks (PDFs written on with a pen) edited live on several devices, as notes are (see
//! [crate::sync]): a Yjs document synced with the y-websocket protocol. What's in it, by name (the
//! web reads the same with Yjs itself):
//!
//! - "pages": an array of {id, base, style, width, height}: the base PDF's page `base`, or a blank
//!   page (`base` -1) in `style`; sizes in points. Pages have ids, so pages added at once on two
//!   devices, and what's on them, don't get mixed up.
//! - "strokes": a map of id to {page, tool, color, width, points, outline, ink?, transform?}: whole
//!   strokes (the eraser takes strokes whole). `points` are x, y, pressure (f32, little-endian);
//!   `outline` its filled shape (polygons of x, y), what any platform draws; `ink` the Android
//!   app's own encoding of it (Jetpack Ink), when it was written there; `transform` (a, b, c, d,
//!   e, f) where it was moved to since. A stroke changed is put again whole.
//! - "images": a map of id to {page, rect (x, y, w, h), crop (fractions), blob, mime}: `blob` is
//!   the picture's content hash, kept by the server.
//! - "bookmarks": a map of page id to true.
//! - "meta": {base}: the base PDF's content hash (the server's).
//!
//! Positions are PDF points from the page's top-left.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use yrs::sync::{Message, SyncMessage};
use yrs::types::map::MapEvent;
use yrs::types::{EntryChange, Observable};
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::{Encode, Encoder, EncoderV1};
use yrs::{Any, Array, ArrayRef, Doc, Map, MapRef, Out, ReadTxn, StateVector, Transact, TransactionMut, Update};

use crate::sync::SyncError;

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct NotebookPage {
    pub id: String,
    /// The base PDF's page (from 0), or -1 for a blank one.
    pub base: i32,
    /// A blank page's look ("plain", "lines", "grid", "dots"); empty for the base's pages.
    pub style: String,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct NotebookStroke {
    pub id: String,
    pub page: String,
    /// "pen" or "highlighter".
    pub tool: String,
    /// ARGB.
    pub color: u32,
    /// Points.
    pub width: f32,
    /// x, y, pressure (0..1) per point.
    pub points: Vec<f32>,
    /// The filled shape: polygons, each x, y pairs.
    pub outline: Vec<Vec<f32>>,
    /// The Android app's Jetpack Ink encoding of the stroke's inputs, if it was written there.
    pub ink: Option<Vec<u8>>,
    /// Moved or scaled since: a, b, c, d, e, f (x' = a x + c y + e, y' = b x + d y + f).
    pub transform: Option<Vec<f32>>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct NotebookImage {
    pub id: String,
    pub page: String,
    /// x, y, width, height.
    pub rect: Vec<f32>,
    /// The part of the picture shown: left, top, right, bottom as fractions of it.
    pub crop: Vec<f32>,
    /// The picture's content hash (the server keeps the bytes).
    pub blob: String,
    pub mime: String,
}

/// A change to a notebook.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum NotebookEdit {
    /// Added, or put again changed (moved, recoloured).
    PutStroke { stroke: NotebookStroke },
    RemoveStroke { id: String },
    PutImage { image: NotebookImage },
    RemoveImage { id: String },
    /// A page put at [index] (in the pages as they are).
    InsertPage { index: u32, page: NotebookPage },
    /// A page removed, with what's on it.
    RemovePage { id: String },
    /// A page's entry put again where it is (what it shows changed; what's on it stays).
    PutPage { page: NotebookPage },
    Bookmark { page: String, on: bool },
    /// The base PDF (its content hash).
    SetBase { blob: String },
}

/// What a message (or an edit) changed, for an app to show.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum NotebookChange {
    /// A stroke added or changed ([stroke] its value now), or removed (none).
    Stroke { id: String, stroke: Option<NotebookStroke> },
    Image { id: String, image: Option<NotebookImage> },
    /// The pages changed (their order, or which there are): read them again.
    Pages,
    Bookmarks,
    Base,
}

/// What a message received calls for, as for notes ([crate::Received]).
#[derive(Debug, Clone, Default, uniffi::Record)]
pub struct NotebookReceived {
    pub reply: Option<Vec<u8>>,
    pub broadcast: Option<Vec<u8>>,
    pub changes: Vec<NotebookChange>,
}

fn invalid(e: impl std::fmt::Display) -> SyncError {
    SyncError::Invalid(e.to_string())
}

struct Inner {
    doc: Doc,
    pages: ArrayRef,
    strokes: MapRef,
    images: MapRef,
    bookmarks: MapRef,
    meta: MapRef,
    changes: Arc<Mutex<Vec<NotebookChange>>>,
}

/// A notebook's shared document.
#[derive(uniffi::Object)]
pub struct NotebookDoc {
    inner: Mutex<Inner>,
}

impl NotebookDoc {
    fn with_doc(doc: Doc) -> Arc<Self> {
        let pages = doc.get_or_insert_array("pages");
        let strokes = doc.get_or_insert_map("strokes");
        let images = doc.get_or_insert_map("images");
        let bookmarks = doc.get_or_insert_map("bookmarks");
        let meta = doc.get_or_insert_map("meta");
        let changes = Arc::new(Mutex::new(Vec::new()));
        {
            let sink = changes.clone();
            strokes.observe("changes", move |txn, e: &MapEvent| {
                let mut out = sink.lock().unwrap();
                for (key, change) in e.keys(txn) {
                    let stroke = match change {
                        EntryChange::Removed(_) => None,
                        EntryChange::Inserted(v) | EntryChange::Updated(_, v) => stroke_of(key, v),
                    };
                    out.push(NotebookChange::Stroke { id: key.to_string(), stroke });
                }
            });
            let sink = changes.clone();
            images.observe("changes", move |txn, e: &MapEvent| {
                let mut out = sink.lock().unwrap();
                for (key, change) in e.keys(txn) {
                    let image = match change {
                        EntryChange::Removed(_) => None,
                        EntryChange::Inserted(v) | EntryChange::Updated(_, v) => image_of(key, v),
                    };
                    out.push(NotebookChange::Image { id: key.to_string(), image });
                }
            });
            let sink = changes.clone();
            pages.observe("changes", move |_, _| sink.lock().unwrap().push(NotebookChange::Pages));
            let sink = changes.clone();
            bookmarks.observe("changes", move |_, _| sink.lock().unwrap().push(NotebookChange::Bookmarks));
            let sink = changes.clone();
            meta.observe("changes", move |_, _| sink.lock().unwrap().push(NotebookChange::Base));
        }
        Arc::new(Self { inner: Mutex::new(Inner { doc, pages, strokes, images, bookmarks, meta, changes }) })
    }

    fn message(m: Message) -> Vec<u8> {
        let mut e = EncoderV1::new();
        m.encode(&mut e);
        e.to_vec()
    }
}

fn floats(v: &[f32]) -> Any {
    Any::Buffer(v.iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<u8>>().into())
}

fn floats_of(a: Option<&Any>) -> Vec<f32> {
    match a {
        Some(Any::Buffer(b)) => b.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect(),
        Some(Any::Array(items)) => items.iter().filter_map(number).map(|n| n as f32).collect(),
        _ => Vec::new(),
    }
}

fn number(a: &Any) -> Option<f64> {
    match a {
        Any::Number(n) => n.clone().as_f64(),
        _ => None,
    }
}

fn record(out: &Out) -> Option<&HashMap<String, Any>> {
    match out {
        Out::Any(Any::Map(m)) => Some(m.as_ref()),
        _ => None,
    }
}

fn string(m: &HashMap<String, Any>, key: &str) -> String {
    match m.get(key) {
        Some(Any::String(s)) => s.to_string(),
        _ => String::new(),
    }
}

fn num(m: &HashMap<String, Any>, key: &str) -> f64 {
    m.get(key).and_then(number).unwrap_or(0.0)
}

fn stroke_of(id: &str, v: &Out) -> Option<NotebookStroke> {
    let m = record(v)?;
    Some(NotebookStroke {
        id: id.to_string(),
        page: string(m, "page"),
        tool: string(m, "tool"),
        color: num(m, "color") as u32,
        width: num(m, "width") as f32,
        points: floats_of(m.get("points")),
        outline: match m.get("outline") {
            Some(Any::Array(polys)) => polys.iter().map(|p| floats_of(Some(p))).collect(),
            _ => Vec::new(),
        },
        ink: match m.get("ink") {
            Some(Any::Buffer(b)) => Some(b.to_vec()),
            _ => None,
        },
        transform: m.get("transform").map(|t| floats_of(Some(t))).filter(|t| t.len() == 6),
    })
}

fn stroke_value(s: &NotebookStroke) -> Any {
    let mut m: HashMap<String, Any> = HashMap::new();
    m.insert("page".into(), Any::from(s.page.clone()));
    m.insert("tool".into(), Any::from(s.tool.clone()));
    m.insert("color".into(), Any::from(s.color as f64));
    m.insert("width".into(), Any::from(s.width as f64));
    m.insert("points".into(), floats(&s.points));
    m.insert("outline".into(), Any::Array(s.outline.iter().map(|p| floats(p)).collect::<Vec<_>>().into()));
    if let Some(ink) = &s.ink {
        m.insert("ink".into(), Any::Buffer(ink.clone().into()));
    }
    if let Some(t) = &s.transform {
        m.insert("transform".into(), Any::Array(t.iter().map(|v| Any::from(*v as f64)).collect::<Vec<_>>().into()));
    }
    Any::Map(m.into())
}

fn image_of(id: &str, v: &Out) -> Option<NotebookImage> {
    let m = record(v)?;
    Some(NotebookImage {
        id: id.to_string(),
        page: string(m, "page"),
        rect: floats_of(m.get("rect")),
        crop: floats_of(m.get("crop")),
        blob: string(m, "blob"),
        mime: string(m, "mime"),
    })
}

fn image_value(i: &NotebookImage) -> Any {
    let mut m: HashMap<String, Any> = HashMap::new();
    m.insert("page".into(), Any::from(i.page.clone()));
    m.insert("rect".into(), Any::Array(i.rect.iter().map(|v| Any::from(*v as f64)).collect::<Vec<_>>().into()));
    m.insert("crop".into(), Any::Array(i.crop.iter().map(|v| Any::from(*v as f64)).collect::<Vec<_>>().into()));
    m.insert("blob".into(), Any::from(i.blob.clone()));
    m.insert("mime".into(), Any::from(i.mime.clone()));
    Any::Map(m.into())
}

fn page_of(v: &Out) -> Option<NotebookPage> {
    let m = record(v)?;
    Some(NotebookPage {
        id: string(m, "id"),
        base: num(m, "base") as i32,
        style: string(m, "style"),
        width: num(m, "width") as f32,
        height: num(m, "height") as f32,
    })
}

fn page_value(p: &NotebookPage) -> Any {
    let mut m: HashMap<String, Any> = HashMap::new();
    m.insert("id".into(), Any::from(p.id.clone()));
    m.insert("base".into(), Any::from(p.base as f64));
    m.insert("style".into(), Any::from(p.style.clone()));
    m.insert("width".into(), Any::from(p.width as f64));
    m.insert("height".into(), Any::from(p.height as f64));
    Any::Map(m.into())
}

impl Inner {
    fn pages<T: ReadTxn>(&self, txn: &T) -> Vec<NotebookPage> {
        self.pages.iter(txn).filter_map(|v| page_of(&v)).collect()
    }

    fn apply(&self, txn: &mut TransactionMut, edit: NotebookEdit) {
        match edit {
            NotebookEdit::PutStroke { stroke } => {
                self.strokes.insert(txn, stroke.id.clone(), stroke_value(&stroke));
            }
            NotebookEdit::RemoveStroke { id } => {
                self.strokes.remove(txn, &id);
            }
            NotebookEdit::PutImage { image } => {
                self.images.insert(txn, image.id.clone(), image_value(&image));
            }
            NotebookEdit::RemoveImage { id } => {
                self.images.remove(txn, &id);
            }
            NotebookEdit::InsertPage { index, page } => {
                let len = self.pages.len(txn);
                self.pages.insert(txn, index.min(len), page_value(&page));
            }
            NotebookEdit::RemovePage { id } => {
                if let Some(i) = self.pages(txn).iter().position(|p| p.id == id) {
                    self.pages.remove(txn, i as u32);
                }
                // What was on it goes with it.
                let strokes: Vec<String> = self.strokes.iter(txn).filter(|(_, v)| stroke_of("", v).is_some_and(|s| s.page == id)).map(|(k, _)| k.to_string()).collect();
                for k in strokes {
                    self.strokes.remove(txn, &k);
                }
                let images: Vec<String> = self.images.iter(txn).filter(|(_, v)| image_of("", v).is_some_and(|i| i.page == id)).map(|(k, _)| k.to_string()).collect();
                for k in images {
                    self.images.remove(txn, &k);
                }
                self.bookmarks.remove(txn, &id);
            }
            NotebookEdit::PutPage { page } => {
                if let Some(i) = self.pages(txn).iter().position(|p| p.id == page.id) {
                    self.pages.remove(txn, i as u32);
                    self.pages.insert(txn, i as u32, page_value(&page));
                }
            }
            NotebookEdit::Bookmark { page, on } => {
                if on {
                    self.bookmarks.insert(txn, page, Any::Bool(true));
                } else {
                    self.bookmarks.remove(txn, &page);
                }
            }
            NotebookEdit::SetBase { blob } => {
                self.meta.insert(txn, "base", Any::from(blob));
            }
        }
    }
}

#[uniffi::export]
impl NotebookDoc {
    /// An empty document: an app's, filled by syncing with the server (which makes the first one from the PDF, with [NotebookDoc::edit]).
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Self::with_doc(Doc::new())
    }

    /// A document as [state] (from [NotebookDoc::state]) saved it.
    #[uniffi::constructor]
    pub fn from_state(state: Vec<u8>) -> Result<Arc<Self>, SyncError> {
        let doc = Self::with_doc(Doc::new());
        {
            let inner = doc.inner.lock().unwrap();
            let update = Update::decode_v1(&state).map_err(invalid)?;
            inner.doc.transact_mut().apply_update(update).map_err(invalid)?;
            inner.changes.lock().unwrap().clear();
        }
        Ok(doc)
    }

    /// Everything in the document, to be kept and loaded with [NotebookDoc::from_state].
    pub fn state(&self) -> Vec<u8> {
        let inner = self.inner.lock().unwrap();
        inner.doc.transact().encode_state_as_update_v1(&StateVector::default())
    }

    /// The message that starts a sync (sync step 1), sent on connecting.
    pub fn start(&self) -> Vec<u8> {
        let inner = self.inner.lock().unwrap();
        let sv = inner.doc.transact().state_vector();
        Self::message(Message::Sync(SyncMessage::SyncStep1(sv)))
    }

    /// A message from the other side, as for notes ([crate::SyncDoc::receive]).
    pub fn receive(&self, message: Vec<u8>) -> Result<NotebookReceived, SyncError> {
        let mut out = NotebookReceived::default();
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
        Ok(out)
    }

    /// Edits made here, in one go: the message that sends them.
    pub fn edit(&self, edits: Vec<NotebookEdit>) -> Vec<u8> {
        let inner = self.inner.lock().unwrap();
        let update = {
            let mut txn = inner.doc.transact_mut();
            for e in edits {
                inner.apply(&mut txn, e);
            }
            txn.encode_update_v1()
        };
        // Its own changes aren't news to whoever made them.
        inner.changes.lock().unwrap().clear();
        Self::message(Message::Sync(SyncMessage::Update(update)))
    }

    pub fn pages(&self) -> Vec<NotebookPage> {
        let inner = self.inner.lock().unwrap();
        inner.pages(&inner.doc.transact())
    }

    pub fn strokes(&self) -> Vec<NotebookStroke> {
        let inner = self.inner.lock().unwrap();
        let txn = inner.doc.transact();
        inner.strokes.iter(&txn).filter_map(|(k, v)| stroke_of(k, &v)).collect()
    }

    pub fn images(&self) -> Vec<NotebookImage> {
        let inner = self.inner.lock().unwrap();
        let txn = inner.doc.transact();
        inner.images.iter(&txn).filter_map(|(k, v)| image_of(k, &v)).collect()
    }

    pub fn bookmarks(&self) -> Vec<String> {
        let inner = self.inner.lock().unwrap();
        let txn = inner.doc.transact();
        inner.bookmarks.keys(&txn).map(|k| k.to_string()).collect()
    }

    /// The base PDF's content hash; empty until the server sets it.
    pub fn base(&self) -> String {
        let inner = self.inner.lock().unwrap();
        let txn = inner.doc.transact();
        match inner.meta.get(&txn, "base") {
            Some(Out::Any(Any::String(s))) => s.to_string(),
            _ => String::new(),
        }
    }
}

/// What a blank page in [style] ("dots", "grid", "lines"; anything else is plain) of [width] by
/// [height] points looks like, as PDF content: plain vector paths, so it stays exact at any zoom
/// (a tiling pattern is drawn by repeating a pixel cell, and drifts against the ink). Dots and
/// grid 17pt (6 mm) apart, lines 22pt.
#[uniffi::export]
pub fn blank_page(style: String, width: f32, height: f32) -> String {
    const STEP: f32 = 17.0;
    const LINE: f32 = 22.0;
    const MARGIN: f32 = 28.0;
    let n = |v: f32| format!("{v:.2}");
    let mut out = String::new();
    match style.as_str() {
        "dots" => {
            // Each dot a zero-length segment with a round cap: one stroked path for the page.
            out.push_str("0.75 0.75 0.78 RG 1.4 w 1 J\n");
            let mut y = MARGIN;
            while y <= height - MARGIN + 0.01 {
                let mut x = MARGIN;
                while x <= width - MARGIN + 0.01 {
                    out.push_str(&format!("{} {} m {} {} l\n", n(x), n(y), n(x), n(y)));
                    x += STEP;
                }
                y += STEP;
            }
            out.push('S');
        }
        "grid" => {
            let cols = ((width - 2.0 * MARGIN) / STEP) as i32;
            let rows = ((height - 2.0 * MARGIN) / STEP) as i32;
            let right = MARGIN + cols as f32 * STEP;
            let top = MARGIN + rows as f32 * STEP;
            out.push_str("0.85 0.85 0.88 RG 0.4 w\n");
            for c in 0..=cols {
                let x = MARGIN + c as f32 * STEP;
                out.push_str(&format!("{} {} m {} {} l\n", n(x), n(MARGIN), n(x), n(top)));
            }
            for r in 0..=rows {
                let y = MARGIN + r as f32 * STEP;
                out.push_str(&format!("{} {} m {} {} l\n", n(MARGIN), n(y), n(right), n(y)));
            }
            out.push('S');
        }
        "lines" => {
            out.push_str("0.8 0.82 0.88 RG 0.5 w\n");
            let mut y = height - MARGIN * 2.2;
            while y > MARGIN {
                out.push_str(&format!("{} {} m {} {} l\n", n(MARGIN), n(y), n(width - MARGIN), n(y)));
                y -= LINE;
            }
            out.push('S');
        }
        _ => {}
    }
    out
}
