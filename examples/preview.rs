//! Writes drawings as SVG files to look at: `cargo run --example preview -- out/`.
use assistant_core::{Drawing, register_font, render_math, render_mermaid};

fn svg(d: &Drawing) -> String {
    let mut out = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}"><rect width="100%" height="100%" fill="white"/>"#, w = d.width, h = d.height);
    for s in &d.shapes {
        let (mut path, mut i) = (String::new(), 0);
        for v in &s.verbs {
            let n = [1, 1, 2, 3, 0][*v as usize];
            path.push(['M', 'L', 'Q', 'C', 'Z'][*v as usize]);
            for k in 0..n { path.push_str(&format!(" {} {}", s.points[i + 2 * k], s.points[i + 2 * k + 1])); }
            i += 2 * n;
        }
        let color = format!("#{:06x}", s.color & 0xffffff);
        let alpha = (s.color >> 24) as f32 / 255.0;
        if s.stroke_width > 0.0 { out += &format!(r#"<path d="{path}" fill="none" stroke="{color}" stroke-opacity="{alpha}" stroke-width="{}"/>"#, s.stroke_width); }
        else { out += &format!(r#"<path d="{path}" fill="{color}" fill-opacity="{alpha}"/>"#); }
    }
    out + "</svg>"
}

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "out".into());
    std::fs::create_dir_all(&dir).unwrap();
    for f in ["Pretendard-Regular.otf", "Pretendard-SemiBold.otf", "Pretendard-Bold.otf"] { register_font(std::fs::read(format!("fonts/{f}")).unwrap()); }
    let math = render_math(r"\int_0^1 x^2\,dx = \frac{1}{3}, \quad \sqrt{a^2+b^2} \le \left(\sum_{i=1}^n x_i\right)^2 \quad \text{넓이}".into(), true, 40.0, 0xff111111).unwrap();
    std::fs::write(format!("{dir}/math.svg"), svg(&math)).unwrap();
    let diagram = render_mermaid("flowchart LR\n  A[노트 쓰기] --> B{수식?}\n  B -->|예| C[KaTeX]\n  B -->|아니오| D[그냥 글]".into(), false).unwrap();
    std::fs::write(format!("{dir}/diagram.svg"), svg(&diagram)).unwrap();
}
