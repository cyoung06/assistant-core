# Vendored crates

## mermaid-rs-renderer 0.3.1 (MIT, https://github.com/1jehuang/mermaid-rs-renderer)

Copied from crates.io with one change, marked `// Assistant:` in `src/text_metrics.rs` and
`src/lib.rs`: `register_font(data)` lets the app supply the fonts text is measured with. Without
it, diagrams are laid out with whatever fonts the machine has (and a disk cache of them), so the
same diagram came out differently on the Mac and on Android. Drop this copy if upstream gains a
way to supply fonts.
