use super::{normalize_relative, parent_dir, ExtractionResult, Extractor, ImportRef};
use crate::graph::EdgeKind;
use std::path::Path;

pub struct DockerfileExtractor;

impl Extractor for DockerfileExtractor {
    fn extract(&self, file_path: &Path, source: &str) -> ExtractionResult {
        let mut result = ExtractionResult::default();
        let base_dir = parent_dir(file_path);

        for line in source.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('#') {
                continue;
            }

            if let Some(rest) = trimmed.strip_prefix("FROM ") {
                let img = rest.trim().to_string();
                if !img.is_empty() {
                    result.imports.push(ImportRef {
                        raw_specifier: img.clone(),
                        resolved_path: None,
                        kind: EdgeKind::Import,
                        external: true,
                    });
                }
            } else if let Some(rest) = trimmed.strip_prefix("COPY ") {
                let parts: Vec<&str> = rest.split_whitespace().collect();
                if let Some(src) = parts.iter().find(|p| !p.starts_with("--")) {
                    let spec = src.to_string();
                    if spec != "." && spec != "/" && !spec.is_empty() {
                        let resolved = normalize_relative(&base_dir, &spec);
                        result.imports.push(ImportRef {
                            raw_specifier: spec,
                            resolved_path: resolved,
                            kind: EdgeKind::Import,
                            external: false,
                        });
                    }
                }
            }
        }
        result
    }
}
