//! A note to print (made into a PDF, to hand in): its HTML, drawn as the apps show it.

use comrak::nodes::{NodeHtmlBlock, NodeValue};
use comrak::{Arena, Options, format_html, parse_document};

use crate::{link_path, render_math, render_mermaid};

/// The text's colour on paper.
const INK: u32 = 0xff1b1b1f;
/// The size math is set at (CSS pixels, as the text around it).
const MATH_SIZE: f32 = 16.0;

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// [text], a note in [folder], as HTML to print: math typeset and `mermaid` diagrams drawn as SVG
/// (as the apps show them; what doesn't typeset shown as written, marked), and each image's source
/// the drive path it points at, as `drive:<path>` (percent-encoded; whoever prints it puts the files
/// beside it).
/// Only the body: the page around it (fonts, styles) is the printer's.
#[uniffi::export]
pub fn note_html(text: String, folder: String) -> String {
    let mut options = Options::default();
    options.extension.strikethrough = true;
    options.extension.table = true;
    options.extension.autolink = true;
    options.extension.tasklist = true;
    options.extension.math_dollars = true;
    // The SVG put in below; and the note's own HTML, which is the person's (and only goes to their PDF).
    options.render.r#unsafe = true;
    let arena = Arena::new();
    let root = parse_document(&arena, &text, &options);
    for node in root.descendants() {
        let mut data = node.data.borrow_mut();
        let replaced = match &mut data.value {
            NodeValue::Math(m) => {
                let tex = m.literal.trim().to_string();
                let html = match render_math(tex.clone(), m.display_math, MATH_SIZE, INK) {
                    Ok(d) if m.display_math => format!(r#"<span class="math display">{}</span>"#, d.svg()),
                    Ok(d) => format!(r#"<span class="math" style="vertical-align: {}px">{}</span>"#, d.baseline - d.height, d.svg()),
                    Err(_) => format!(r#"<code class="math-error">{}</code>"#, escape(&tex)),
                };
                Some(NodeValue::HtmlInline(html))
            }
            NodeValue::CodeBlock(c) if c.info.trim() == "mermaid" => render_mermaid(c.literal.clone(), false)
                .ok()
                .map(|d| NodeValue::HtmlBlock(NodeHtmlBlock { block_type: 6, literal: format!("<div class=\"diagram\">{}</div>\n", d.svg()) })),
            NodeValue::Image(link) => {
                if let Some(path) = link_path(folder.clone(), link.url.clone()) {
                    link.url = format!("drive:{path}");
                }
                None
            }
            _ => None,
        };
        if let Some(v) = replaced {
            data.value = v;
        }
    }
    let mut html = String::new();
    _ = format_html(root, &options, &mut html);
    html
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn math_is_drawn_and_images_point_at_the_drive() {
        let html = note_html("# 확률\n\nIf $P(A)=1$ then\n\n$$\\sum_i x_i$$\n\n![plot](img/a%20b.png) and ![web](https://x.org/y.png)\n\n$\\frac{$\n".into(), "Projects/P/Notes/".into());
        assert!(html.contains("<h1>확률</h1>"), "{html}");
        assert!(html.contains(r#"<span class="math" style="vertical-align: "#), "{html}");
        assert!(html.contains(r#"<span class="math display"><svg"#), "{html}");
        assert!(html.contains(r#"src="drive:Projects/P/Notes/img/a%20b.png""#), "{html}");
        assert!(html.contains(r#"src="https://x.org/y.png""#), "{html}");
        assert!(html.contains(r#"<code class="math-error">\frac{</code>"#), "{html}");
    }
}
