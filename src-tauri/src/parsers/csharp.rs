//! C# extractor — tree-sitter backed for declarations and `using` directives,
//! with line-based scanning retained for Minimal API endpoint registrations
//! (§19.6: `app.MapGet("/x", handler)`), which are a call convention rather
//! than a declaration form.
//!
//! Types previously reached `exports` but never `symbols`, so no C# type was
//! reachable through `repograph_explore`.

use super::routes::{entry, handler_after_comma, method_call_route};
use super::{ts_engine, ts_langs, ExtractionResult, Extractor, ImportRef};
use crate::graph::EdgeKind;
use std::path::Path;

pub struct CSharpExtractor;

impl Extractor for CSharpExtractor {
    fn extract(&self, _file_path: &Path, source: &str) -> ExtractionResult {
        let mut result = ExtractionResult::default();

        ts_engine::apply_to(
            &ts_langs::spec_csharp(),
            "csharp",
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

        scan_minimal_api(source, &mut result);
        super::routes::ensure_handler_symbols(&result.entry_points, &mut result.symbols);
        // Entity Framework `DbSet<T>` model registrations.
        super::schema::apply(&super::schema::scan_jvm_dotnet_php(source), &mut result.symbols);
        result
    }
}

/// `app.MapGet("/todos", GetTodos)` and friends. Deliberately still a line
/// scan: these are ordinary method calls, so a grammar cannot tell them from
/// any other invocation without resolving types.
fn scan_minimal_api(source: &str, result: &mut ExtractionResult) {
    const MAP_METHODS: &[&str] = &[
        "MapGet", "MapPost", "MapPut", "MapDelete", "MapPatch", "MapMethods", "Map",
    ];
    for (idx, raw_line) in source.lines().enumerate() {
        let line = raw_line.trim();
        if line.starts_with("//") {
            continue;
        }
        if let Some((path, rest, _)) = method_call_route(line, None, MAP_METHODS) {
            result
                .entry_points
                .push(entry(path, handler_after_comma(rest), idx + 1));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_api_map_endpoints() {
        let r = CSharpExtractor.extract(
            Path::new("Program.cs"),
            "using Microsoft.AspNetCore.Builder;\n\nvar app = WebApplication.Create(args);\napp.MapGet(\"/todos\", GetTodos);\napp.MapPost(\"/todos\", (Todo t) => Results.Ok(t));\napp.Run();\n",
        );
        let got: Vec<(&str, Option<&str>)> = r
            .entry_points
            .iter()
            .map(|e| (e.route.as_str(), e.handler.as_deref()))
            .collect();
        assert_eq!(got, vec![("/todos", Some("GetTodos")), ("/todos", None)]);
    }

    #[test]
    fn types_and_members_become_symbols_with_ranges() {
        let r = CSharpExtractor.extract(
            Path::new("Service.cs"),
            "using System.Text;\n\
             \n\
             namespace App {\n\
             \x20   public class Service {\n\
             \x20       public int Compute() {\n\
             \x20           return 1;\n\
             \x20       }\n\
             \x20   }\n\
             \x20   public interface IThing { }\n\
             }\n",
        );

        let svc = r.symbols.iter().find(|s| s.name == "Service").expect("class");
        assert_eq!(svc.kind, "class");
        assert_eq!((svc.start_line, svc.end_line), (4, 8));

        let m = r.symbols.iter().find(|s| s.name == "Compute").expect("method");
        assert_eq!(m.kind, "method");
        assert_eq!((m.start_line, m.end_line), (5, 7));

        assert!(r.symbols.iter().any(|s| s.name == "IThing" && s.kind == "interface"));
        assert!(r
            .imports
            .iter()
            .any(|i| i.raw_specifier == "System.Text" && i.external));
    }

    /// The old scanner matched `class ` anywhere on a line, including inside
    /// verbatim strings and block comments.
    #[test]
    fn commented_and_quoted_declarations_are_not_symbols() {
        let r = CSharpExtractor.extract(
            Path::new("A.cs"),
            "/*\nclass Ghost { }\n*/\nclass Real { string s = \"class Quoted { }\"; }\n",
        );
        let names: Vec<&str> = r.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"Real"), "{names:?}");
        assert!(!names.contains(&"Ghost"), "{names:?}");
        assert!(!names.contains(&"Quoted"), "{names:?}");
    }
}
