use super::{normalize_relative, parent_dir, ExtractionResult, Extractor, ImportRef};
use crate::graph::EdgeKind;
use std::path::Path;

pub struct MarkdownExtractor;

impl Extractor for MarkdownExtractor {
    fn extract(&self, file_path: &Path, source: &str) -> ExtractionResult {
        let mut result = ExtractionResult::default();
        let base_dir = parent_dir(file_path);

        let mut current_idx = 0;
        while let Some(start_bracket) = source[current_idx..].find('[') {
            let abs_start = current_idx + start_bracket;
            if let Some(end_bracket) = source[abs_start..].find(']') {
                let abs_end_bracket = abs_start + end_bracket;
                if abs_end_bracket + 1 < source.len() && source[abs_end_bracket + 1..].starts_with('(') {
                    let link_start = abs_end_bracket + 2;
                    if let Some(link_end) = source[link_start..].find(')') {
                        let link_dest = &source[link_start..link_start + link_end];
                        let link_dest = link_dest.trim();
                        if !link_dest.is_empty() 
                            && !link_dest.starts_with("http://")
                            && !link_dest.starts_with("https://")
                            && !link_dest.starts_with("mailto:")
                            && !link_dest.starts_with('#') 
                        {
                            let resolved = normalize_relative(&base_dir, link_dest);
                            result.imports.push(ImportRef {
                                raw_specifier: link_dest.to_string(),
                                resolved_path: resolved,
                                kind: EdgeKind::Import,
                                external: false,
                            });
                        }
                        current_idx = link_start + link_end + 1;
                        continue;
                    }
                }
            }
            current_idx = abs_start + 1;
        }
        result
    }
}
