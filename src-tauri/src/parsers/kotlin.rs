//! Kotlin extractor — tree-sitter backed. Previously emitted symbols only
//! for route handlers, so ordinary classes and functions were invisible to
//! `repograph_explore`.

use super::{ts_engine, ts_langs, ExtractionResult, Extractor, ImportRef};
use crate::graph::EdgeKind;
use std::path::Path;

pub struct KotlinExtractor;

impl Extractor for KotlinExtractor {
    fn extract(&self, _file_path: &Path, source: &str) -> ExtractionResult {
        let mut result = ExtractionResult::default();
        ts_engine::apply_to(
            &ts_langs::spec_kotlin(),
            "kotlin",
            source,
            &mut result,
            |raw| {
                Some(ImportRef {
                    raw_specifier: raw.to_string(),
                    resolved_path: Some(raw.replace('.', "/")),
                    kind: EdgeKind::Import,
                    external: true,
                })
            },
        );

        if let Some(pkg) = source.lines().find_map(|l| {
            l.trim()
                .strip_prefix("package ")
                .map(|r| r.trim().to_string())
                .filter(|p| !p.is_empty())
        }) {
            result.exports.push(format!("package:{pkg}"));
        }

        // Micronaut/Quarkus annotations + Ktor routing DSL (§19.5). Both are
        // framework conventions rather than syntax, so they stay line-based.
        super::routes::scan_annotation_routes(source, &mut result.entry_points);
        scan_ktor_dsl(source, &mut result.entry_points);
        super::routes::ensure_handler_symbols(&result.entry_points, &mut result.symbols);
        result
    }
}

/// Ktor DSL: paths declared inside a `routing { ... }` block, e.g.
/// `get("/items") { ... }`. Nested `route("/base") { ... }` scopes prefix
/// their children; brace depth tracks scope lifetime.
fn scan_ktor_dsl(source: &str, entry_points: &mut Vec<super::EntryPoint>) {
    use super::routes::quoted_arg;
    const VERBS: &[&str] = &["get(", "post(", "put(", "delete(", "patch(", "head(", "options("];

    let mut routing_depth: Option<usize> = None;
    let mut depth: usize = 0;
    // (prefix, depth at which the scope opened)
    let mut prefix_stack: Vec<(String, usize)> = Vec::new();

    for (idx, raw_line) in source.lines().enumerate() {
        let line_num = idx + 1;
        let line = raw_line.trim();
        if line.starts_with("//") {
            continue;
        }

        if routing_depth.is_none() && (line.starts_with("routing {") || line.contains(" routing {")) {
            routing_depth = Some(depth);
        }

        if routing_depth.is_some() {
            // `route("/base") {` opens a prefix scope.
            if let Some(pos) = line.find("route(") {
                let preceded_ok = pos == 0
                    || !line.as_bytes()[pos - 1].is_ascii_alphanumeric();
                if preceded_ok && line.ends_with('{') {
                    if let Some((prefix, _)) = quoted_arg(&line[pos + "route(".len()..]) {
                        prefix_stack.push((prefix, depth));
                    }
                }
            }
            for verb in VERBS {
                let Some(pos) = line.find(verb) else { continue };
                let preceded_ok = pos == 0 || !line.as_bytes()[pos - 1].is_ascii_alphanumeric();
                if !preceded_ok {
                    continue;
                }
                if let Some((path, _)) = quoted_arg(&line[pos + verb.len()..]) {
                    if path.starts_with('/') {
                        let prefix: String =
                            prefix_stack.iter().map(|(p, _)| p.as_str()).collect();
                        entry_points.push(super::EntryPoint {
                            route: format!("{prefix}{path}"),
                            handler: None, // handler is the DSL lambda scope
                            line: line_num,
                        });
                        break;
                    }
                }
            }
        }

        depth += line.matches('{').count();
        let closes = line.matches('}').count();
        depth = depth.saturating_sub(closes);
        prefix_stack.retain(|(_, d)| *d < depth || closes == 0);
        if let Some(rd) = routing_depth {
            if depth <= rd && closes > 0 {
                routing_depth = None;
                prefix_stack.clear();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ktor_dsl_routes_with_nested_prefix() {
        let r = KotlinExtractor.extract(
            Path::new("src/Routing.kt"),
            "fun Application.configureRouting() {\n    routing {\n        get(\"/health\") { call.respondText(\"ok\") }\n        route(\"/api\") {\n            post(\"/items\") { }\n        }\n    }\n}\n",
        );
        let got: Vec<&str> = r.entry_points.iter().map(|e| e.route.as_str()).collect();
        assert_eq!(got, vec!["/health", "/api/items"]);
    }
}
