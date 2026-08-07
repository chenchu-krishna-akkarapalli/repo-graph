//! Vue / Svelte single-file component (SFC) extraction.
//!
//! An SFC is an HTML-shaped document whose `<script>` blocks hold the real
//! module graph (imports, exports, symbols). Feeding the whole file to the
//! JS/TS parser fails on the template markup, so this extractor lifts each
//! script block out, delegates to the JS/TS extractor, and shifts every
//! reported line back into the original file's coordinate space — otherwise
//! `repograph_node` would slice the wrong lines.

use super::javascript::JsExtractor;
use super::{ExtractionResult, Extractor};
use std::path::Path;

pub struct SfcExtractor;

/// Every `<script>` block as `(line_offset, body)`, where `line_offset` is the
/// count of newlines preceding the block body in the original source.
fn script_blocks(source: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut cursor = 0usize;
    while let Some(rel) = source[cursor..].find("<script") {
        let tag_start = cursor + rel;
        // Reject `<scriptfoo`; a real tag is followed by whitespace or `>`.
        let after = source[tag_start + "<script".len()..].chars().next();
        if !matches!(after, Some(c) if c.is_whitespace() || c == '>') {
            cursor = tag_start + "<script".len();
            continue;
        }
        let Some(gt_rel) = source[tag_start..].find('>') else { break };
        let body_start = tag_start + gt_rel + 1;
        let Some(close_rel) = source[body_start..].find("</script") else { break };
        let body_end = body_start + close_rel;
        out.push((source[..body_start].matches('\n').count(), &source[body_start..body_end]));
        cursor = body_end;
    }
    out
}

impl Extractor for SfcExtractor {
    fn extract(&self, file_path: &Path, source: &str) -> ExtractionResult {
        let mut merged = ExtractionResult::default();
        // Vue allows `<script setup>` alongside `<script>`; Svelte allows
        // `<script context="module">`. Merge every block.
        for (line_offset, body) in script_blocks(source) {
            let mut r = JsExtractor.extract(file_path, body);
            for s in &mut r.symbols {
                s.start_line += line_offset;
                s.end_line += line_offset;
            }
            for reference in &mut r.references {
                reference.line += line_offset;
            }
            for inherit in &mut r.inheritance {
                inherit.line += line_offset;
            }
            for ep in &mut r.entry_points {
                // 0 means file-level/unknown — leave it alone.
                if ep.line > 0 {
                    ep.line += line_offset;
                }
            }
            merged.exports.extend(r.exports);
            merged.imports.extend(r.imports);
            merged.entry_points.extend(r.entry_points);
            merged.warnings.extend(r.warnings);
            merged.symbols.extend(r.symbols);
            merged.references.extend(r.references);
            merged.inheritance.extend(r.inheritance);
        }
        merged
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vue_script_setup_imports_resolve_with_shifted_lines() {
        let src = "<template>\n  <Button />\n</template>\n\n<script setup lang=\"ts\">\nimport Button from './Button.vue'\nconst label: string = 'hi'\n</script>\n\n<style>\n.a { color: red; }\n</style>\n";
        let res = SfcExtractor.extract(Path::new("src/App.vue"), src);

        assert!(
            res.imports.iter().any(|i| i.raw_specifier == "./Button.vue"),
            "expected the SFC import to be extracted, got {:?}",
            res.imports.iter().map(|i| &i.raw_specifier).collect::<Vec<_>>()
        );
        // The import sits on line 6 of the file; inside the isolated block it
        // is line 2, so the offset must have been applied.
        if let Some(sym) = res.symbols.iter().find(|s| s.name == "label") {
            assert!(sym.start_line >= 5, "line offset not applied: {sym:?}");
        }
    }

    #[test]
    fn template_only_component_yields_nothing_and_does_not_panic() {
        let res = SfcExtractor.extract(Path::new("src/Plain.vue"), "<template>\n  <p>hi</p>\n</template>\n");
        assert!(res.symbols.is_empty());
        assert!(res.imports.is_empty());
    }

    #[test]
    fn svelte_module_and_instance_blocks_both_parsed() {
        let src = "<script context=\"module\">\nexport const prerender = true\n</script>\n\n<script>\nimport { onMount } from 'svelte'\n</script>\n\n<h1>hi</h1>\n";
        let res = SfcExtractor.extract(Path::new("src/routes/Page.svelte"), src);
        assert_eq!(script_blocks(src).len(), 2, "both script blocks should be found");
        assert!(res.imports.iter().any(|i| i.raw_specifier == "svelte"));
    }
}
