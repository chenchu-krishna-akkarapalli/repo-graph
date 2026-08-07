//! Swift extractor — tree-sitter backed. Previously emitted no symbols at
//! all, so `repograph_explore` returned nothing for any Swift codebase.

use super::{ts_engine, ts_langs, ExtractionResult, Extractor, ImportRef};
use crate::graph::EdgeKind;
use std::path::Path;

pub struct SwiftExtractor;

impl Extractor for SwiftExtractor {
    fn extract(&self, _file_path: &Path, source: &str) -> ExtractionResult {
        let mut result = ExtractionResult::default();
        ts_engine::apply_to(
            &ts_langs::spec_swift(),
            "swift",
            source,
            &mut result,
            |raw| {
                Some(ImportRef {
                    raw_specifier: raw.to_string(),
                    resolved_path: Some(raw.to_string()),
                    kind: EdgeKind::Import,
                    external: true,
                })
            },
        );
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GREETER: &str = "\
import Foundation

struct Greeter {
    func greet() -> String {
        return \"hi\"
    }
}
";

    #[test]
    fn types_and_functions_become_symbols_with_ranges() {
        let r = SwiftExtractor.extract(Path::new("App.swift"), GREETER);

        let g = r.symbols.iter().find(|s| s.name == "Greeter").expect("struct");
        assert_eq!(g.kind, "struct", "struct and class must stay distinguishable");
        assert_eq!((g.start_line, g.end_line), (3, 7));

        let f = r.symbols.iter().find(|s| s.name == "greet").expect("func");
        assert_eq!(f.kind, "function");
        assert_eq!((f.start_line, f.end_line), (4, 6));

        assert!(r.imports.iter().any(|i| i.raw_specifier == "Foundation"));
    }

    /// Swift routes `class`, `struct`, `enum` and `actor` through one grammar
    /// node, so the kinds only stay distinct because the query matches the
    /// keyword itself.
    #[test]
    fn class_protocol_and_enum_keep_their_own_kinds() {
        let r = SwiftExtractor.extract(
            Path::new("A.swift"),
            "class C {}\nprotocol P {}\nenum E {}\n",
        );
        let kind = |n: &str| {
            r.symbols
                .iter()
                .find(|s| s.name == n)
                .map(|s| s.kind.as_str())
                .unwrap_or("<missing>")
        };
        assert_eq!(kind("C"), "class");
        assert_eq!(kind("P"), "interface");
        assert_eq!(kind("E"), "enum");
    }

    /// The line scanner treated any line starting with `struct ` as a
    /// declaration, including inside a block comment.
    #[test]
    fn commented_declarations_are_ignored() {
        let r = SwiftExtractor.extract(
            Path::new("A.swift"),
            "/*\nstruct Ghost {}\n*/\nstruct Real {}\n",
        );
        let names: Vec<&str> = r.symbols.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["Real"], "got {names:?}");
    }
}
