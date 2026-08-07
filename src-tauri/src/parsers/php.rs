//! PHP extractor — tree-sitter backed for declarations and `use` imports,
//! with line scanning retained for the two framework conventions it also
//! recognises: Symfony `#[Route('/path')]` attributes bound to the following
//! declaration, and Slim `$app->get('/x', $handler)` registrations.
//!
//! Symbols previously existed only for route handlers and Doctrine entities.

use super::routes::{entry, handler_after_comma, method_call_route, quoted_arg};
use super::{ts_engine, ts_langs, EntryPoint, ExtractionResult, Extractor, ImportRef};
use crate::graph::EdgeKind;
use std::path::Path;

pub struct PhpExtractor;

impl Extractor for PhpExtractor {
    fn extract(&self, _file_path: &Path, source: &str) -> ExtractionResult {
        let mut result = ExtractionResult::default();

        ts_engine::apply_to(
            &ts_langs::spec_php(),
            "php",
            source,
            &mut result,
            |raw| {
                Some(ImportRef {
                    raw_specifier: raw.to_string(),
                    // `App\Models\User` → `App/Models/User`.
                    resolved_path: Some(raw.replace('\\', "/")),
                    kind: EdgeKind::Import,
                    external: true,
                })
            },
        );

        scan_php_routes(source, &mut result);
        super::routes::ensure_handler_symbols(&result.entry_points, &mut result.symbols);
        // Doctrine `@Entity` and Eloquent `extends Model` classes.
        super::schema::apply(&super::schema::scan_jvm_dotnet_php(source), &mut result.symbols);
        result
    }
}

/// Symfony attributes and Slim registrations. Both are conventions layered on
/// ordinary syntax, so the grammar cannot distinguish them from any other
/// attribute or method call.
fn scan_php_routes(source: &str, result: &mut ExtractionResult) {
    const SLIM_METHODS: &[&str] =
        &["get", "post", "put", "delete", "patch", "options", "any", "map"];

    // A `#[Route(...)]` attribute waiting for the declaration it annotates.
    let mut pending_route: Option<(String, usize)> = None;

    for (idx, raw_line) in source.lines().enumerate() {
        let line_num = idx + 1;
        let line = raw_line.trim();
        if line.starts_with("//") || line.starts_with('#') && !line.starts_with("#[") {
            continue;
        }

        if line.starts_with("#[Route(") || line.starts_with("#[Route (") {
            if let Some((path, _)) = quoted_arg(line) {
                pending_route = Some((path, line_num));
            }
            continue;
        }

        if let Some(name) = declared_name(line) {
            if let Some((route, decl_line)) = pending_route.take() {
                result.entry_points.push(EntryPoint {
                    route,
                    handler: Some(name),
                    line: decl_line,
                });
            }
        } else if !line.is_empty() && !line.starts_with("#[") {
            pending_route = None;
        }

        // Slim: `$app->get('/x', handler)` — `->` normalizes like `.`.
        if let Some((path, rest, _)) =
            method_call_route(line, Some(&["$app", "$group", "$router"]), SLIM_METHODS)
        {
            result
                .entry_points
                .push(entry(path, handler_after_comma(rest), line_num));
        }
    }
}

/// Name declared on this line, for binding a pending `#[Route]` attribute.
/// Only used for that binding — the symbol table itself comes from the AST.
fn declared_name(line: &str) -> Option<String> {
    for keyword in ["function ", "class "] {
        let Some(pos) = line.find(keyword) else { continue };
        let boundary_ok =
            pos == 0 || !line.as_bytes()[pos - 1].is_ascii_alphanumeric();
        if !boundary_ok {
            continue;
        }
        let name: String = line[pos + keyword.len()..]
            .trim_start()
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() {
            return Some(name);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symfony_attribute_and_slim_routes() {
        let r = PhpExtractor.extract(
            Path::new("src/Controller/UserController.php"),
            "<?php\nuse App\\Service\\Users;\n\nclass UserController {\n    #[Route('/users/{id}')]\n    public function show(int $id) {}\n}\n\n$app->get('/health', healthCheck);\n",
        );
        let got: Vec<(&str, Option<&str>)> = r
            .entry_points
            .iter()
            .map(|e| (e.route.as_str(), e.handler.as_deref()))
            .collect();
        assert_eq!(
            got,
            vec![
                ("/users/{id}", Some("show")),
                ("/health", Some("healthCheck")),
            ]
        );
        assert!(r.exports.contains(&"UserController".to_string()));
    }
}
