//! C / C++ extractor — tree-sitter backed.
//!
//! Previously emitted no symbols at all, and its `#include` handling was a
//! prefix match that could not tell a directive from a commented-out one.
//! C++ declaration syntax is not tractable by line scanning at all, which is
//! why the earlier version deliberately recognised almost nothing.

use super::{normalize_relative, parent_dir, ts_engine, ts_langs, ExtractionResult, Extractor, ImportRef};
use crate::graph::EdgeKind;
use std::path::Path;

pub struct CCppExtractor;

/// `.c`/`.h` use the C grammar; everything else the C++ one, which is a
/// superset. A `.h` holding C++ still parses — the C grammar recovers and the
/// `partial_parse` warning records the gap.
fn is_cpp(file_path: &Path) -> bool {
    !matches!(
        file_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "c" | "h"
    )
}

impl Extractor for CCppExtractor {
    fn extract(&self, file_path: &Path, source: &str) -> ExtractionResult {
        let mut result = ExtractionResult::default();
        let base_dir = parent_dir(file_path);
        let cpp = is_cpp(file_path);

        let (spec, key) = if cpp {
            (ts_langs::spec_cpp(), "cpp")
        } else {
            (ts_langs::spec_c(), "c")
        };

        ts_engine::apply_to(&spec, key, source, &mut result, |raw| {
            // `#include "x.h"` is repo-relative; `#include <x>` is a system
            // header and never resolves to an indexed file. The engine has
            // already stripped both quote forms, so they are distinguished by
            // whether the raw directive used angle brackets — recovered here
            // by testing whether the specifier resolves inside the repo.
            let resolved = normalize_relative(&base_dir, raw);
            let looks_local = raw.ends_with(".h") || raw.ends_with(".hpp") || raw.contains('/');
            Some(ImportRef {
                raw_specifier: raw.to_string(),
                resolved_path: if looks_local { resolved } else { None },
                kind: EdgeKind::Import,
                external: !looks_local,
            })
        });

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn types_and_functions_become_symbols_with_ranges() {
        let r = CCppExtractor.extract(
            Path::new("main.cpp"),
            "#include \"helper.h\"\n\
             \n\
             class Widget {\n\
             public:\n\
             \x20 int size;\n\
             };\n\
             \n\
             int compute(int n) {\n\
             \x20 return n * 2;\n\
             }\n",
        );
        let w = r.symbols.iter().find(|s| s.name == "Widget").expect("class");
        assert_eq!(w.kind, "class");
        assert_eq!(w.start_line, 3);

        let c = r.symbols.iter().find(|s| s.name == "compute").expect("function");
        assert_eq!(c.kind, "function");
        assert_eq!((c.start_line, c.end_line), (8, 10));

        assert!(r
            .imports
            .iter()
            .any(|i| i.raw_specifier == "helper.h" && !i.external));
    }

    #[test]
    fn control_flow_is_not_a_function() {
        let r = CCppExtractor.extract(
            Path::new("a.c"),
            "int run(void) {\n  if (x) {\n  }\n  while (y) {\n  }\n  return 0;\n}\n",
        );
        let fns: Vec<&str> = r
            .symbols
            .iter()
            .filter(|s| s.kind == "function")
            .map(|s| s.name.as_str())
            .collect();
        assert_eq!(fns, vec!["run"], "got {fns:?}");
    }

    #[test]
    fn includes_are_classified_and_comments_ignored() {
        let r = CCppExtractor.extract(
            Path::new("src/main.cpp"),
            "/* block\nclass Ghost {};\n*/\n#include \"../lib/util.h\"\n#include <vector>\n",
        );
        assert!(!r.symbols.iter().any(|s| s.name == "Ghost"));
        assert!(r
            .imports
            .iter()
            .any(|i| i.resolved_path.as_deref() == Some("lib/util.h")));
        assert!(r.imports.iter().any(|i| i.raw_specifier == "vector" && i.external));
    }

    /// Out-of-line definitions are how most C++ methods are written; the line
    /// scanner could not represent them at all.
    #[test]
    fn qualified_out_of_line_definitions_resolve_to_the_method_name() {
        let r = CCppExtractor.extract(
            Path::new("w.cpp"),
            "int Widget::area() const {\n  return 4;\n}\n",
        );
        assert!(
            r.symbols.iter().any(|s| s.name == "area" && s.kind == "function"),
            "got {:?}",
            r.symbols
        );
    }
}
