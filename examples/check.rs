fn main() {
    assistant_core::register_font(std::fs::read("fonts/Pretendard-Regular.otf").unwrap());
    for (tex, display) in [(r"\int_0^1 x^2\,dx = \frac{1}{3}", true), ("x^2 + y^2 = r^2", false)] {
        match assistant_core::render_math(tex.into(), display, 16.0, 0xffe7e9ee) {
            Ok(d) => println!("{tex}: ok {} shapes, colours {:x?}", d.shapes.len(), d.shapes.iter().map(|s| s.color).collect::<std::collections::BTreeSet<_>>()),
            Err(e) => println!("{tex}: error {e}"),
        }
    }
    for s in assistant_core::markdown_spans("인라인 $x^2$ 이고\n\n$$\\int_0^1 x^2\\,dx = \\frac{1}{3}$$\n".into()) { println!("{:?} {:?}", s.element, s.range); }
}
