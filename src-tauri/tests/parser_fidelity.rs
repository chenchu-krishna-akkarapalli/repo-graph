//! Cross-language AST-fidelity tests.
//!
//! Every case here is something line scanning got wrong and a grammar gets
//! right. They live in one file, checked against every migrated language, so
//! a future extractor swapped back to line scanning fails loudly.

use repo_graph::parsers::{extractor_for, ExtractionResult};
use std::path::Path;

fn run(language: &str, path: &str, src: &str) -> ExtractionResult {
    let extractor = extractor_for(language).unwrap_or_else(|| panic!("no extractor for {language}"));
    extractor.extract(Path::new(path), src)
}

fn names(r: &ExtractionResult) -> Vec<&str> {
    r.symbols.iter().map(|s| s.name.as_str()).collect()
}

/// A declaration inside a block comment is not a declaration. The old
/// scanners skipped a comment only when the line *began* with `//`, `/*` or
/// `*`, so interior lines were read as code.
#[test]
fn block_comment_interiors_never_declare_symbols() {
    let cases: [(&str, &str, &str); 6] = [
        ("java", "A.java", "/*\nclass Ghost {}\n*/\nclass Real {}\n"),
        ("csharp", "A.cs", "/*\nclass Ghost { }\n*/\nclass Real { }\n"),
        ("cpp", "a.cpp", "/*\nclass Ghost {};\n*/\nclass Real {};\n"),
        ("swift", "A.swift", "/*\nstruct Ghost {}\n*/\nstruct Real {}\n"),
        ("kotlin", "A.kt", "/*\nclass Ghost\n*/\nclass Real\n"),
        ("rust", "a.rs", "/*\nstruct Ghost;\n*/\nstruct Real;\n"),
    ];
    for (lang, path, src) in cases {
        let r = run(lang, path, src);
        let got = names(&r);
        assert!(got.contains(&"Real"), "{lang}: expected Real, got {got:?}");
        assert!(!got.contains(&"Ghost"), "{lang}: phantom Ghost in {got:?}");
    }
}

/// A declaration keyword inside a string literal is not a declaration.
#[test]
fn string_literals_never_declare_symbols() {
    let cases: [(&str, &str, &str); 4] = [
        ("java", "A.java", "class Real { String s = \"class Quoted {}\"; }\n"),
        ("csharp", "A.cs", "class Real { string s = \"class Quoted { }\"; }\n"),
        ("go", "a.go", "package main\nvar s = \"func Quoted() {}\"\nfunc Real() {}\n"),
        ("python", "a.py", "x = \"def quoted(): pass\"\ndef real():\n    pass\n"),
    ];
    for (lang, path, src) in cases {
        let r = run(lang, path, src);
        let got = names(&r);
        assert!(
            !got.iter().any(|n| n.eq_ignore_ascii_case("quoted")),
            "{lang}: phantom symbol from a string literal in {got:?}"
        );
    }
}

/// Symbol ranges must cover the whole declaration. `repograph_node` slices
/// source by these lines, so an end line that stops at the first `}` on a
/// nested construct hands an agent a truncated function.
#[test]
fn symbol_ranges_span_the_full_declaration_including_nesting() {
    // Nested braces, a brace inside a string, and a closing brace on the
    // final line — all cases brace counting historically mishandled.
    let java = run(
        "java",
        "A.java",
        "class A {\n\
         \x20   void go() {\n\
         \x20       if (x) { }\n\
         \x20       String s = \"}\";\n\
         \x20   }\n\
         }\n",
    );
    let go = java.symbols.iter().find(|s| s.name == "go").expect("method go");
    assert_eq!(
        (go.start_line, go.end_line),
        (2, 5),
        "method range must cover the nested block and the quoted brace"
    );

    let py = run(
        "python",
        "a.py",
        "def outer():\n    def inner():\n        return 1\n    return inner\n\nX = 1\n",
    );
    let outer = py.symbols.iter().find(|s| s.name == "outer").expect("outer");
    assert_eq!((outer.start_line, outer.end_line), (1, 4));
}

/// Multi-line signatures are ordinary in every one of these languages and
/// were invisible to scanners keying off a single line.
#[test]
fn multi_line_signatures_are_still_declarations() {
    let java = run(
        "java",
        "A.java",
        "class A {\n\
         \x20   public void wrapped(\n\
         \x20       int a,\n\
         \x20       int b\n\
         \x20   ) {\n\
         \x20   }\n\
         }\n",
    );
    assert!(
        java.symbols.iter().any(|s| s.name == "wrapped"),
        "got {:?}",
        names(&java)
    );

    let rust = run(
        "rust",
        "a.rs",
        "pub fn wrapped(\n    a: usize,\n    b: usize,\n) -> usize {\n    a + b\n}\n",
    );
    assert!(
        rust.symbols.iter().any(|s| s.name == "wrapped"),
        "got {:?}",
        names(&rust)
    );
}

/// C++ methods are usually defined out of line. Line scanning could not
/// represent `Type::method` at all.
#[test]
fn cpp_out_of_line_definitions_are_found() {
    let r = run("cpp", "w.cpp", "int Widget::area() const {\n  return 4;\n}\n");
    assert!(
        r.symbols.iter().any(|s| s.name == "area"),
        "got {:?}",
        names(&r)
    );
}

/// A syntactically broken file must still yield whatever the grammar
/// recovered, flagged so agents know the map has a gap — never a hard failure
/// that drops the file from the graph.
#[test]
fn broken_syntax_degrades_to_a_warning_not_a_crash() {
    let r = run(
        "java",
        "Broken.java",
        "class Good { void ok() {} }\nclass Bad { void oops( { \n",
    );
    assert!(
        r.warnings.iter().any(|w| w == "partial_parse" || w == "parse_error"),
        "expected a recorded gap, got {:?}",
        r.warnings
    );
    assert!(
        r.symbols.iter().any(|s| s.name == "Good"),
        "recovered symbols must survive, got {:?}",
        names(&r)
    );
}

/// Every migrated language must produce real symbols — the GAP-06 condition
/// was that six of these emitted none at all and four only route handlers.
#[test]
fn every_migrated_language_emits_symbols() {
    let cases: [(&str, &str, &str, &str); 9] = [
        ("java", "A.java", "class Thing { void run() {} }\n", "Thing"),
        ("csharp", "A.cs", "class Thing { void Run() { } }\n", "Thing"),
        ("kotlin", "A.kt", "class Thing {\n  fun run() {}\n}\n", "Thing"),
        ("swift", "A.swift", "class Thing {\n  func run() {}\n}\n", "Thing"),
        ("c", "a.c", "int thing(void) { return 0; }\n", "thing"),
        ("cpp", "a.cpp", "class Thing { };\n", "Thing"),
        ("go", "a.go", "package main\nfunc Thing() {}\n", "Thing"),
        ("python", "a.py", "class Thing:\n    pass\n", "Thing"),
        ("rust", "a.rs", "pub struct Thing;\n", "Thing"),
    ];
    for (lang, path, src, expected) in cases {
        let r = run(lang, path, src);
        assert!(
            r.symbols.iter().any(|s| s.name == expected),
            "{lang}: expected symbol {expected}, got {:?}",
            names(&r)
        );
    }
}
