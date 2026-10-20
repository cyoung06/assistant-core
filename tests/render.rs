//! Drawings come out the same whatever fonts the machine has: only the registered ones are used.

fn fonts() {
    for f in ["Pretendard-Regular.otf", "Pretendard-SemiBold.otf", "Pretendard-Bold.otf"] {
        assistant_core::register_font(std::fs::read(format!("fonts/{f}")).unwrap());
    }
}

#[test]
fn math_is_laid_out_with_katex_and_korean_in_the_text_font() {
    fonts();
    let d = assistant_core::render_math(r"\int_0^1 x^2\,dx = \frac{1}{3} \quad \text{넓이}".into(), true, 40.0, 0xff000000).unwrap();
    assert!(d.width > 200.0 && d.height > 40.0, "{}x{}", d.width, d.height);
    assert!(d.baseline > 0.0 && d.baseline < d.height);
    // Integral, x, 2, 0, 1, d, x, =, 1, 3, the fraction bar, and the two Hangul syllables.
    assert!(d.shapes.len() >= 13, "{} shapes", d.shapes.len());
    let e = assistant_core::render_math(r"\frac{".into(), false, 16.0, 0xff000000);
    assert!(matches!(e, Err(assistant_core::RenderError::Invalid(_))));
}

#[test]
fn mermaid_text_is_drawn_in_the_text_font() {
    fonts();
    let d = assistant_core::render_mermaid("flowchart LR\n  A[노트 쓰기] --> B{수식?}\n  B -->|예| C[KaTeX]".into(), false).unwrap();
    assert!(d.width > 100.0 && d.height > 30.0);
    // Each label is one path of its glyphs, in the theme's text colour: the three nodes and the edge's.
    let labels = d.shapes.iter().filter(|s| s.color == 0xff0f172a && s.stroke_width == 0.0 && s.verbs.len() > 20).count();
    assert_eq!(labels, 4, "{} labels", labels);
    assert!(d.shapes.iter().all(|s| s.color >> 24 > 0), "nothing invisible");
}
