use super::{ExtractionResult, Extractor, ImportRef};
use crate::graph::EdgeKind;
use std::path::Path;

pub struct PrismaExtractor;

impl Extractor for PrismaExtractor {
    fn extract(&self, _file_path: &Path, source: &str) -> ExtractionResult {
        let mut result = ExtractionResult::default();

        for (idx, line) in source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('#') {
                continue;
            }

            if let Some(rest) = trimmed.strip_prefix("model ") {
                let model_name: String = rest
                    .trim_start()
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if !model_name.is_empty() {
                    // A model is a symbol, not just an export — without this it
                    // never reaches the graph and cannot be explored or themed.
                    result.symbols.push(super::ExtractedSymbol {
                        name: model_name.clone(),
                        kind: super::schema::DATABASE_SCHEMA.to_string(),
                        start_line: idx + 1,
                        end_line: idx + 1,
                    });
                    result.exports.push(model_name);
                }
            }

            if trimmed.contains("@relation") {
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 2 {
                    let model_type = parts[1];
                    if model_type.chars().next().unwrap_or(' ').is_uppercase() {
                        let clean_type: String = model_type
                            .chars()
                            .take_while(|c| c.is_alphanumeric() || *c == '_')
                            .collect();
                        if !clean_type.is_empty() {
                            result.imports.push(ImportRef {
                                raw_specifier: clean_type.clone(),
                                resolved_path: Some(clean_type),
                                kind: EdgeKind::Import,
                                external: false,
                            });
                        }
                    }
                }
            }
        }
        result
    }
}
