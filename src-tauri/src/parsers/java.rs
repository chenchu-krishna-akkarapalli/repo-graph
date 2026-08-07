//! Java extractor — tree-sitter backed.
//!
//! This was a line scanner that matched `contains("class ")` anywhere on a
//! line, tracked no block-comment state, extracted no methods, and pushed
//! classes only to `exports` (never `symbols`), so `repograph_explore` could
//! not see a single Java type. The grammar removes all of that.

use super::ts_engine;
use super::ts_langs;
use super::{ExtractionResult, Extractor, ImportRef};
use crate::graph::EdgeKind;
use std::path::Path;

pub struct JavaExtractor;

impl Extractor for JavaExtractor {
    fn extract(&self, _file_path: &Path, source: &str) -> ExtractionResult {
        let mut result = ExtractionResult::default();

        ts_engine::apply_to(
            &ts_langs::spec_java(),
            "java",
            source,
            &mut result,
            |raw| {
                // `java.util.List` -> `java/util/List`. Marked external; the
                // graph builder promotes it to an edge if that path is indexed
                // and records it as a third-party dep otherwise.
                Some(ImportRef {
                    raw_specifier: raw.to_string(),
                    resolved_path: Some(raw.replace('.', "/")),
                    kind: EdgeKind::Import,
                    external: true,
                })
            },
        );

        // The package is not a symbol, but the manifest uses it to group files.
        if let Some(pkg) = package_of(source) {
            result.exports.push(format!("package:{pkg}"));
        }

        // Framework markers stay line-based: they are annotation *conventions*,
        // not syntax, and each is already covered by its own tests.
        super::routes::scan_annotation_routes(source, &mut result.entry_points);
        super::routes::ensure_handler_symbols(&result.entry_points, &mut result.symbols);
        super::schema::apply(&super::schema::scan_jvm_dotnet_php(source), &mut result.symbols);
        result
    }
}

/// `package com.example;` — the one declaration cheap enough to read directly.
fn package_of(source: &str) -> Option<String> {
    source.lines().find_map(|line| {
        let t = line.trim();
        t.strip_prefix("package ")
            .map(|rest| rest.trim_end_matches(';').trim().to_string())
            .filter(|p| !p.is_empty())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn micronaut_controller_routes_bind_methods() {
        let r = JavaExtractor.extract(
            Path::new("src/main/java/com/example/UserController.java"),
            "package com.example;\n\nimport io.micronaut.http.annotation.Controller;\n\n@Controller(\"/users\")\npublic class UserController {\n\n    @Get(\"/{id}\")\n    public HttpResponse<User> show(Long id) { return null; }\n\n    @Post\n    public HttpResponse<User> create(User u) { return null; }\n}\n",
        );
        let got: Vec<(&str, Option<&str>)> = r
            .entry_points
            .iter()
            .map(|e| (e.route.as_str(), e.handler.as_deref()))
            .collect();
        assert_eq!(
            got,
            vec![("/users/{id}", Some("show")), ("/users", Some("create"))]
        );
        assert!(r.symbols.iter().any(|s| s.name == "show"));
    }

    /// Java types reached `exports` but never `symbols`, so the one tool the
    /// MCP server exposes by default could not find any Java code at all.
    #[test]
    fn classes_interfaces_and_methods_become_symbols_with_ranges() {
        let r = JavaExtractor.extract(
            Path::new("src/main/java/com/example/Service.java"),
            "package com.example;\n\
             \n\
             public class Service implements Runnable {\n\
             \n\
             \x20   public void run() {\n\
             \x20       helper();\n\
             \x20   }\n\
             }\n",
        );

        let service = r.symbols.iter().find(|s| s.name == "Service").expect("class symbol");
        assert_eq!(service.kind, "class");
        assert_eq!((service.start_line, service.end_line), (3, 8));

        let run = r.symbols.iter().find(|s| s.name == "run").expect("method symbol");
        assert_eq!(run.kind, "method");
        assert_eq!((run.start_line, run.end_line), (5, 7));

        assert!(r
            .inheritance
            .iter()
            .any(|i| i.class_name == "Service" && i.parent_name == "Runnable" && i.is_implements));
    }

    /// The old scanner skipped a comment only when the line *began* with
    /// `//`, `/*` or `*`, and matched `contains("class ")` anywhere — so both
    /// of these produced phantom exports.
    #[test]
    fn commented_and_quoted_declarations_are_not_symbols() {
        let r = JavaExtractor.extract(
            Path::new("A.java"),
            "/*\nclass CommentedOut {}\n*/\n\
             public class Real {\n\
             \x20   String q = \"class Quoted {}\";\n\
             }\n",
        );
        let names: Vec<&str> = r.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"Real"), "{names:?}");
        assert!(!names.contains(&"CommentedOut"), "{names:?}");
        assert!(!names.contains(&"Quoted"), "{names:?}");
    }

    #[test]
    fn control_flow_and_calls_are_not_methods() {
        let r = JavaExtractor.extract(
            Path::new("A.java"),
            "class A {\n\
             \x20   void go() {\n\
             \x20       if (x) {\n\
             \x20       }\n\
             \x20       compute();\n\
             \x20       int n = compute();\n\
             \x20   }\n\
             }\n",
        );
        let methods: Vec<&str> = r
            .symbols
            .iter()
            .filter(|s| s.kind == "method")
            .map(|s| s.name.as_str())
            .collect();
        assert_eq!(methods, vec!["go"], "got {methods:?}");
    }
}
