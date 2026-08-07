use super::{ExtractionResult, Extractor, ImportRef};
use crate::graph::EdgeKind;
use std::path::Path;

pub struct SqlExtractor;

impl Extractor for SqlExtractor {
    fn extract(&self, _file_path: &Path, source: &str) -> ExtractionResult {
        let mut result = ExtractionResult::default();

        for line in source.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("--") {
                continue;
            }

            let lower = trimmed.to_lowercase();
            if let Some(idx) = lower.find("create table ") {
                let after = &trimmed[idx + "create table ".len()..];
                let table_name: String = after
                    .trim_start()
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '"' || *c == '`')
                    .filter(|c| *c != '"' && *c != '`')
                    .collect();
                if !table_name.is_empty() {
                    result.exports.push(table_name);
                }
            }

            if let Some(idx) = lower.find("references ") {
                let after = &trimmed[idx + "references ".len()..];
                let ref_table: String = after
                    .trim_start()
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '"' || *c == '`' || *c == '(')
                    .take_while(|c| *c != '(')
                    .filter(|c| *c != '"' && *c != '`')
                    .collect();
                let ref_table = ref_table.trim().to_string();
                if !ref_table.is_empty() {
                    result.imports.push(ImportRef {
                        raw_specifier: ref_table.clone(),
                        resolved_path: Some(ref_table),
                        kind: EdgeKind::Import,
                        external: false,
                    });
                }
            }
        }
        result
    }
}
