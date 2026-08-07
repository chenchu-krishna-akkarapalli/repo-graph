use super::{normalize_relative, parent_dir, ExtractionResult, Extractor, ImportRef};
use crate::graph::EdgeKind;
use std::path::Path;

pub struct HtmlExtractor;

impl Extractor for HtmlExtractor {
    fn extract(&self, file_path: &Path, source: &str) -> ExtractionResult {
        let mut result = ExtractionResult::default();
        let base_dir = parent_dir(file_path);

        for line in source.lines() {
            let trimmed = line.trim();

            let mut search_idx = 0;
            while let Some(id_idx) = trimmed[search_idx..].find("id=\"") {
                let start = search_idx + id_idx + 4;
                if let Some(end_offset) = trimmed[start..].find('"') {
                    let id_val = &trimmed[start..start + end_offset];
                    if !id_val.is_empty() {
                        result.exports.push(format!("id:{}", id_val));
                    }
                    search_idx = start + end_offset + 1;
                } else {
                    break;
                }
            }

            let mut search_idx = 0;
            while let Some(src_idx) = trimmed[search_idx..].find("src=\"") {
                let start = search_idx + src_idx + 5;
                if let Some(end_offset) = trimmed[start..].find('"') {
                    let val = &trimmed[start..start + end_offset];
                    if !val.is_empty() && !val.starts_with("http://") && !val.starts_with("https://") && !val.starts_with("//") {
                        let resolved = normalize_relative(&base_dir, val);
                        result.imports.push(ImportRef {
                            raw_specifier: val.to_string(),
                            resolved_path: resolved,
                            kind: EdgeKind::Import,
                            external: false,
                        });
                    }
                    search_idx = start + end_offset + 1;
                } else {
                    break;
                }
            }

            let mut search_idx = 0;
            while let Some(href_idx) = trimmed[search_idx..].find("href=\"") {
                let start = search_idx + href_idx + 6;
                if let Some(end_offset) = trimmed[start..].find('"') {
                    let val = &trimmed[start..start + end_offset];
                    if !val.is_empty() && !val.starts_with("http://") && !val.starts_with("https://") && !val.starts_with("//") {
                        let resolved = normalize_relative(&base_dir, val);
                        result.imports.push(ImportRef {
                            raw_specifier: val.to_string(),
                            resolved_path: resolved,
                            kind: EdgeKind::Import,
                            external: false,
                        });
                    }
                    search_idx = start + end_offset + 1;
                } else {
                    break;
                }
            }
        }
        result
    }
}
