//! How the notebook document (Yjs, yrs) holds up to writing with a stylus: what each stroke costs to
//! make and to take in, what goes over the wire, and how the document grows with thousands of strokes
//! and erases. `cargo run --release --example stylus_bench`.
//!
//! A stroke is as the tablet makes one: ~100 inputs (a short pen stroke at the S Pen's rate), its
//! outline (~220 points) and its Jetpack Ink encoding (~1.2 KB).

use assistant_core::{NotebookDoc, NotebookEdit, NotebookPage, NotebookStroke};
use std::time::{Duration, Instant};

fn stroke(n: usize, page: &str) -> NotebookStroke {
    let mut points = Vec::with_capacity(300);
    for i in 0..100 {
        let t = i as f32 / 99.0;
        points.extend_from_slice(&[100.0 + 200.0 * t + (n % 7) as f32, 100.0 + 30.0 * (t * 6.0).sin() + (n / 7 % 40) as f32 * 18.0, 0.4 + 0.3 * t]);
    }
    let outline: Vec<f32> = (0..440).map(|i| 100.0 + (i as f32 * 0.37 + n as f32) % 300.0).collect();
    let ink: Vec<u8> = (0..1200).map(|i| ((i * 31 + n * 7) % 251) as u8).collect();
    NotebookStroke {
        id: format!("stroke-{n:06}"),
        page: page.into(),
        tool: "pen".into(),
        color: 0xff1b1b1f,
        width: 0.6,
        points,
        outline: vec![outline],
        ink: Some(ink),
        transform: None,
    }
}

fn pct(mut v: Vec<Duration>, p: f64) -> Duration {
    v.sort();
    v[((v.len() - 1) as f64 * p).round() as usize]
}

fn us(d: Duration) -> String {
    format!("{:.0} µs", d.as_secs_f64() * 1e6)
}

fn kb(n: usize) -> String {
    format!("{:.1} KB", n as f64 / 1024.0)
}

fn main() {
    let pages = 20;
    let total = 10_000;

    // The writer (a tablet) and a reader (another device, or the server), both on the same notebook.
    let writer = NotebookDoc::new();
    let mut edits = vec![];
    for i in 0..pages {
        edits.push(NotebookEdit::InsertPage { index: i, page: NotebookPage { id: format!("page-{i}"), base: -1, style: "plain".into(), width: 595.0, height: 842.0 } });
    }
    let first = writer.edit(edits);
    let reader = NotebookDoc::new();
    reader.receive(first).unwrap();

    println!("Writing {total} strokes ({pages} pages), each sent and taken in as it's finished:\n");
    let (mut make, mut take, mut sizes) = (vec![], vec![], vec![]);
    let mut checkpoints = vec![];
    for n in 0..total {
        let s = stroke(n, &format!("page-{}", n % pages as usize));
        let t = Instant::now();
        let message = writer.edit(vec![NotebookEdit::PutStroke { stroke: s }]);
        make.push(t.elapsed());
        sizes.push(message.len());
        let t = Instant::now();
        let got = reader.receive(message).unwrap();
        take.push(t.elapsed());
        assert_eq!(got.changes.len(), 1);
        if [100, 1000, 5000, 10_000].contains(&(n + 1)) {
            checkpoints.push((n + 1, pct(make.clone(), 0.5), pct(make.clone(), 0.95), pct(take.clone(), 0.5), pct(take.clone(), 0.95)));
        }
    }
    println!("  per stroke, message: {} (each, about the same)", kb(sizes.iter().sum::<usize>() / sizes.len()));
    println!("  {:>7}  {:>22}  {:>22}", "strokes", "make + encode (p50/p95)", "take in (p50/p95)");
    for (n, m50, m95, t50, t95) in &checkpoints {
        println!("  {n:>7}  {:>22}  {:>22}", format!("{} / {}", us(*m50), us(*m95)), format!("{} / {}", us(*t50), us(*t95)));
    }

    let raw: usize = (0..total).map(|n| { let s = stroke(n, "p"); s.points.len() * 4 + s.outline[0].len() * 4 + s.ink.as_ref().unwrap().len() }).sum();
    println!("\nThe document, {total} strokes:");
    report(&writer, raw);

    // Erasing half (whole strokes, as every device does): what's left behind of them.
    let t = Instant::now();
    let mut erase = vec![];
    for n in (0..total).step_by(2) {
        let message = writer.edit(vec![NotebookEdit::RemoveStroke { id: format!("stroke-{n:06}") }]);
        reader.receive(message).unwrap();
        erase.push(1);
    }
    println!("\nErased half ({} strokes, one by one, sent and taken in: {:.0} ms):", erase.len(), t.elapsed().as_secs_f64() * 1e3);
    report(&writer, raw / 2);
}

/** Its size, and the time to save it, open it from what's kept, list its strokes, and sync it to a new device. */
fn report(doc: &NotebookDoc, raw: usize) {
    let t = Instant::now();
    let state = doc.state();
    let save = t.elapsed();
    let t = Instant::now();
    let loaded = NotebookDoc::from_state(state.clone()).unwrap();
    let load = t.elapsed();
    let t = Instant::now();
    let strokes = loaded.strokes();
    let list = t.elapsed();
    // A device with nothing: its sync step 1, the other's step 2, taken in.
    let fresh = NotebookDoc::new();
    let t = Instant::now();
    let answer = doc.receive(fresh.start()).unwrap().reply.unwrap();
    let serve = t.elapsed();
    let t = Instant::now();
    fresh.receive(answer.clone()).unwrap();
    let sync = t.elapsed();
    println!("  {} strokes; state {} (the strokes' own data {}, {:.2}×)", strokes.len(), kb(state.len()), kb(raw), state.len() as f64 / raw.max(1) as f64);
    println!("  save {:.1} ms · open from what's kept {:.1} ms · list strokes {:.1} ms", save.as_secs_f64() * 1e3, load.as_secs_f64() * 1e3, list.as_secs_f64() * 1e3);
    println!("  a new device: answer {} made in {:.1} ms, taken in {:.1} ms", kb(answer.len()), serve.as_secs_f64() * 1e3, sync.as_secs_f64() * 1e3);
}
