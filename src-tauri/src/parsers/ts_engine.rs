//! Tree-sitter backed symbol/import extraction.
//!
//! Replaces the hand-written line scanners for every non-JS language. Those
//! scanners could not see through block comments reliably, guessed at symbol
//! end lines by counting braces, and matched declaration keywords inside
//! string literals. A real grammar removes that whole class of error.
//!
//! JS/TS deliberately stays on `swc` — it is already a full AST and
//! understands TypeScript's type syntax natively.
//!
//! ## How a language is wired up
//!
//! Each language supplies a [`LanguageSpec`]: its grammar, plus one
//! tree-sitter query whose captures name what to extract. Capture names are
//! the contract:
//!
//! - `@sym.<kind>` — a declaration. The captured node is the *whole* symbol
//!   (its span becomes `start_line`/`end_line`); `<kind>` is stored verbatim
//!   as the symbol kind.
//! - `@name` — the identifier naming the nearest enclosing `@sym.*`.
//! - `@import` — a module/include specifier (string literal or dotted path).
//! - `@extends` / `@implements` — a supertype reference.
//!
//! Writing a query beats writing a visitor per language: the query is
//! declarative, lives next to the language it describes, and cannot silently
//! fall out of step with the grammar the way an index-based walker can.

use super::{ExtractedSymbol, InheritanceRelation};
use std::collections::HashMap;
use tree_sitter::{Language, Node, Parser, Query, QueryCursor, StreamingIterator};

/// Everything needed to extract from one language.
pub struct LanguageSpec {
    pub language: Language,
    /// Tree-sitter query using the capture names documented above.
    pub query_source: &'static str,
}

/// What a single parse yielded. Route/schema/state scanning stays in the
/// per-language modules — those are framework heuristics, not syntax.
#[derive(Debug, Default)]
pub struct TsExtraction {
    pub symbols: Vec<ExtractedSymbol>,
    pub imports: Vec<String>,
    pub inheritance: Vec<InheritanceRelation>,
    /// Every identifier referenced, for the symbol-level call graph.
    pub references: Vec<(String, usize)>,
    /// True when tree-sitter marked the tree as containing errors. The result
    /// is still used — a partial tree beats no symbols — but the caller
    /// records a `partial_parse` warning so agents know the map has a gap.
    pub had_errors: bool,
}

/// Compiled queries are cached per language: `Query::new` is the expensive
/// part of a parse, and the indexer runs this once per file across a thread
/// pool. `OnceLock` keeps it lock-free after the first compile.
fn compiled_query(spec: &LanguageSpec, key: &'static str) -> Option<&'static Query> {
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<&'static str, &'static Query>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache.lock().ok()?;
    if let Some(q) = guard.get(key) {
        return Some(*q);
    }
    let query = Query::new(&spec.language, spec.query_source).ok()?;
    // Leaked deliberately: one per language for the process lifetime, which is
    // strictly less memory than recompiling per file.
    let leaked: &'static Query = Box::leak(Box::new(query));
    guard.insert(key, leaked);
    Some(leaked)
}

fn line_of(node: Node) -> usize {
    node.start_position().row + 1
}

fn end_line_of(node: Node) -> usize {
    node.end_position().row + 1
}

/// Strip the quoting a grammar keeps around string literals and includes.
///
/// The encoding prefixes (`L"x"`, `u8"x"`, `R"(x)"`, C#'s `@"x"`) are only
/// stripped when a quote actually follows. Stripping them unconditionally ate
/// the first letter of any bare identifier that happened to start with one —
/// `import LocalModule` became `ocalModule`.
fn unquote(raw: &str) -> String {
    let t = raw.trim();
    let body = ["u8", "L", "R", "@", "u", "U"]
        .iter()
        .find_map(|p| {
            t.strip_prefix(*p)
                .filter(|rest| rest.starts_with('"') || rest.starts_with('\''))
        })
        .unwrap_or(t);
    body.trim_matches(|c| c == '"' || c == '\'' || c == '<' || c == '>' || c == '`')
        .to_string()
}

/// Run a language spec and fold the result into an [`ExtractionResult`],
/// applying `resolve` to each raw import specifier.
///
/// Every migrated extractor funnels through here so the symbol/import/warning
/// handling stays identical across languages; only the query and the import
/// resolution differ.
pub fn apply_to(
    spec: &LanguageSpec,
    key: &'static str,
    source: &str,
    result: &mut super::ExtractionResult,
    mut resolve: impl FnMut(&str) -> Option<super::ImportRef>,
) {
    let Some(ts) = extract(spec, key, source) else {
        // The grammar could not be configured at all — treat as unparsed
        // rather than silently reporting an empty file.
        result.warnings.push("parse_error".to_string());
        return;
    };
    if ts.had_errors {
        // A recovered tree is still worth indexing; the warning tells agents
        // the map has a known gap here.
        result.warnings.push("partial_parse".to_string());
    }
    for raw in &ts.imports {
        if let Some(import) = resolve(raw) {
            result.imports.push(import);
        }
    }
    // Exported names are the declared top-level symbols. Visibility rules
    // differ per language, so the extractor narrows this where it matters.
    for sym in &ts.symbols {
        result.exports.push(sym.name.clone());
    }
    result.symbols.extend(ts.symbols);
    result.inheritance.extend(ts.inheritance);
    result
        .references
        .extend(ts.references.into_iter().map(|(name, line)| super::ExtractedReference { name, line }));
}

/// Replace an extractor's symbol table and reference list with the grammar's,
/// leaving imports, exports and entry points untouched.
///
/// Used by Go, Python and Rust, whose import resolution and route scanning are
/// language-specific, well covered by tests, and orthogonal to declaration
/// parsing. Only the part line scanning got wrong — symbol *ranges*, and not
/// mistaking commented or quoted code for a declaration — moves to the AST.
///
/// A no-op when the grammar cannot parse at all, so the existing line-scanned
/// symbols survive as a fallback rather than the file going dark.
pub fn replace_symbols(
    spec: &LanguageSpec,
    key: &'static str,
    source: &str,
    result: &mut super::ExtractionResult,
) {
    let Some(ts) = extract(spec, key, source) else { return };
    if ts.had_errors {
        result.warnings.push("partial_parse".to_string());
    }
    result.symbols = ts.symbols;
    result.references = ts
        .references
        .into_iter()
        .map(|(name, line)| super::ExtractedReference { name, line })
        .collect();
    result.inheritance.extend(ts.inheritance);
}

/// Parse `source` and run `spec`'s query over it.
///
/// Returns `None` only when the parser could not be configured at all; a
/// syntactically broken file still yields whatever the grammar recovered,
/// with `had_errors` set.
pub fn extract(spec: &LanguageSpec, key: &'static str, source: &str) -> Option<TsExtraction> {
    let mut parser = Parser::new();
    parser.set_language(&spec.language).ok()?;
    let tree = parser.parse(source, None)?;
    let query = compiled_query(spec, key)?;

    let mut out = TsExtraction {
        had_errors: tree.root_node().has_error(),
        ..Default::default()
    };

    let bytes = source.as_bytes();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, tree.root_node(), bytes);

    while let Some(m) = matches.next() {
        // Within one match, `@name` belongs to the `@sym.*` in the same match.
        let mut decl: Option<(Node, String)> = None;
        let mut name: Option<String> = None;
        let mut supertypes: Vec<(String, bool)> = Vec::new();

        for cap in m.captures {
            let cap_name = &query.capture_names()[cap.index as usize];
            let text = cap.node.utf8_text(bytes).unwrap_or("");

            if let Some(kind) = cap_name.strip_prefix("sym.") {
                decl = Some((cap.node, kind.to_string()));
            } else if *cap_name == "name" {
                name = Some(text.to_string());
            } else if *cap_name == "import" {
                let spec_text = unquote(text);
                if !spec_text.is_empty() {
                    out.imports.push(spec_text);
                }
            } else if *cap_name == "extends" {
                supertypes.push((text.to_string(), false));
            } else if *cap_name == "implements" {
                supertypes.push((text.to_string(), true));
            }
        }

        if let (Some((node, kind)), Some(name)) = (decl, name) {
            // A qualified name (`Class::method`, `pkg.Type`) keeps only its
            // last segment: that is what a caller writes at the call site.
            let short = name
                .rsplit(["::", "."].as_slice()[0])
                .next()
                .unwrap_or(&name)
                .to_string();
            for (parent, is_implements) in supertypes {
                out.inheritance.push(InheritanceRelation {
                    class_name: short.clone(),
                    parent_name: parent,
                    is_implements,
                    line: line_of(node),
                });
            }
            out.symbols.push(ExtractedSymbol {
                name: short,
                kind,
                start_line: line_of(node),
                end_line: end_line_of(node),
            });
        }
    }

    collect_references(tree.root_node(), bytes, &mut out.references);
    dedupe_symbols(&mut out.symbols);
    Some(out)
}

/// Every identifier in the file, for the symbol-level call graph.
///
/// Mirrors what the JS extractor gets from `visit_ident`. `db::populate_db`
/// resolves these against the symbol table and discards the ones that are not
/// real call-graph participants, so over-collecting here is safe.
fn collect_references(node: Node, bytes: &[u8], out: &mut Vec<(String, usize)>) {
    let mut cursor = node.walk();
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if n.kind().contains("identifier") && n.child_count() == 0 {
            if let Ok(text) = n.utf8_text(bytes) {
                if !text.is_empty() {
                    out.push((text.to_string(), n.start_position().row + 1));
                }
            }
        }
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }
}

/// One declaration can match several query patterns (a Java method is both a
/// `method_declaration` and, under some queries, a `_declaration`). Keep the
/// first, which is the most specific pattern in the query.
fn dedupe_symbols(symbols: &mut Vec<ExtractedSymbol>) {
    let mut seen = std::collections::HashSet::new();
    symbols.retain(|s| seen.insert((s.name.clone(), s.start_line)));
    symbols.sort_by_key(|s| (s.start_line, s.name.clone()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unquote_strips_the_shapes_grammars_keep() {
        assert_eq!(unquote("\"helper.h\""), "helper.h");
        assert_eq!(unquote("<vector>"), "vector");
        assert_eq!(unquote("'react'"), "react");
        assert_eq!(unquote("  \"a/b\"  "), "a/b");
        // Encoding prefixes only count when a quote follows.
        assert_eq!(unquote("L\"wide.h\""), "wide.h");
        assert_eq!(unquote("u8\"utf.h\""), "utf.h");
        assert_eq!(unquote("@\"verbatim\""), "verbatim");
    }

    /// Regression: stripping `L`/`R`/`U` unconditionally truncated any bare
    /// identifier starting with one — `import LocalModule` yielded
    /// `ocalModule`, which then failed to resolve as a dependency.
    #[test]
    fn bare_identifiers_starting_with_a_prefix_letter_survive() {
        assert_eq!(unquote("LocalModule"), "LocalModule");
        assert_eq!(unquote("Realm"), "Realm");
        assert_eq!(unquote("UIKit"), "UIKit");
        assert_eq!(unquote("u8string"), "u8string");
    }

    #[test]
    fn duplicate_matches_collapse_to_one_symbol() {
        let mut syms = vec![
            ExtractedSymbol { name: "a".into(), kind: "class".into(), start_line: 1, end_line: 9 },
            ExtractedSymbol { name: "a".into(), kind: "type".into(), start_line: 1, end_line: 9 },
            ExtractedSymbol { name: "b".into(), kind: "method".into(), start_line: 3, end_line: 4 },
        ];
        dedupe_symbols(&mut syms);
        assert_eq!(syms.len(), 2);
        assert_eq!(syms[0].kind, "class", "the first (most specific) match wins");
    }
}
