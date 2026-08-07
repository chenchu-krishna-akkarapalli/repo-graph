//! Rust extractor — `Cargo.toml` dependency names become
//! `external_dependencies`; `mod x;` declarations and `use crate::` /
//! `use super::` / `use self::` paths become internal edge candidates.
//! External-crate `use` lines are ignored entirely (Cargo.toml is the
//! source of truth for externals; prefer false negatives over wrong edges).

use super::{normalize_relative, parent_dir, ExtractionResult, Extractor, ImportRef, ExtractedSymbol};
use crate::graph::EdgeKind;
use std::path::Path;

pub struct RustExtractor;

impl Extractor for RustExtractor {
    fn extract(&self, file_path: &Path, source: &str) -> ExtractionResult {
        if file_path.file_name().is_some_and(|n| n == "Cargo.toml") {
            return extract_cargo_toml(source);
        }
        extract_rust_source(file_path, source)
    }
}

fn extract_cargo_toml(source: &str) -> ExtractionResult {
    let mut result = ExtractionResult::default();
    let parsed: toml::Table = match source.trim_start_matches('\u{feff}').parse() {
        Ok(v) => v,
        Err(_) => {
            result.warnings.push("parse_error".to_string());
            return result;
        }
    };
    let mut push_deps = |table: Option<&toml::Value>| {
        if let Some(toml::Value::Table(deps)) = table {
            for name in deps.keys() {
                result.imports.push(ImportRef {
                    raw_specifier: name.clone(),
                    resolved_path: None,
                    kind: EdgeKind::Use,
                    external: true,
                });
            }
        }
    };
    push_deps(parsed.get("dependencies"));
    push_deps(parsed.get("dev-dependencies"));
    push_deps(parsed.get("build-dependencies"));
    push_deps(parsed.get("workspace").and_then(|w| w.get("dependencies")));
    result
}

struct ActiveRustSymbol {
    name: String,
    kind: String,
    start_line: usize,
    start_depth: usize,
}

fn clean_line_braces(line: &str) -> String {
    let mut cleaned = String::new();
    let mut in_str = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if in_str {
            if c == '"' {
                in_str = false;
            }
        } else {
            if c == '"' {
                in_str = true;
            } else if c == '/' && chars.peek() == Some(&'/') {
                break;
            } else {
                cleaned.push(c);
            }
        }
    }
    cleaned
}

fn parse_rust_decl(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    let decl = line.strip_prefix("pub ").unwrap_or(line);
    let decl = decl.strip_prefix("pub(crate) ").unwrap_or(decl);
    let decl = decl.strip_prefix("async ").unwrap_or(decl);
    let decl = decl.strip_prefix("const ").unwrap_or(decl);
    let decl = decl.strip_prefix("unsafe ").unwrap_or(decl);
    
    let mut words = decl.split_whitespace();
    let word = words.next()?;
    if word == "fn" {
        let name = words.next()?.split('(').next()?.split('<').next()?.trim_matches(':');
        return Some((name.to_string(), "function".to_string()));
    }
    if word == "struct" {
        let name = words.next()?.split('(').next()?.split('{').next()?.split('<').next()?.trim_matches(';');
        return Some((name.to_string(), "struct".to_string()));
    }
    if word == "enum" {
        let name = words.next()?.split('{').next()?.split('<').next()?;
        return Some((name.to_string(), "enum".to_string()));
    }
    if word == "trait" {
        let name = words.next()?.split('{').next()?.split(':').next()?.split('<').next()?;
        return Some((name.to_string(), "trait".to_string()));
    }
    if word == "impl" {
        let target = words.collect::<Vec<_>>().join(" ");
        let name = target.split('<').next()?.split('{').next()?.trim().to_string();
        return Some((name, "class".to_string()));
    }
    None
}

fn extract_rust_source(file_path: &Path, source: &str) -> ExtractionResult {
    let mut result = ExtractionResult::default();
    let dir = parent_dir(file_path);
    let stem = file_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let module_base = if matches!(stem.as_str(), "lib" | "main" | "mod") {
        dir.clone()
    } else if dir.is_empty() {
        stem.clone()
    } else {
        format!("{dir}/{stem}")
    };
    let crate_root = dir
        .rfind("src")
        .filter(|&i| {
            let before_ok = i == 0 || dir.as_bytes()[i - 1] == b'/';
            let after = &dir[i + 3..];
            before_ok && (after.is_empty() || after.starts_with('/'))
        })
        .map(|i| dir[..i + 3].to_string())
        .unwrap_or_default();

    let mut active_stack: Vec<ActiveRustSymbol> = Vec::new();
    let mut current_depth = 0;
    let lines: Vec<&str> = source.lines().collect();
    let total_lines = lines.len();

    for (idx, line) in lines.iter().enumerate() {
        let line = *line;
        let line_num = idx + 1;
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*') {
            continue;
        }

        let is_class_member = current_depth == 1 && active_stack.last().is_some_and(|t| t.kind == "class");
        if current_depth == 0 || is_class_member {
            if let Some((name, mut kind)) = parse_rust_decl(trimmed) {
                if is_class_member && kind == "function" {
                    kind = "method".to_string();
                }
                active_stack.push(ActiveRustSymbol {
                    name,
                    kind,
                    start_line: line_num,
                    start_depth: current_depth,
                });
            }
        }

        let cleaned = clean_line_braces(line);
        let open_count = cleaned.matches('{').count();
        let close_count = cleaned.matches('}').count();

        current_depth += open_count;
        if close_count >= current_depth {
            current_depth = 0;
        } else {
            current_depth -= close_count;
        }

        if let Some(top) = active_stack.last() {
            let is_single_line_decl = top.start_line == line_num && (trimmed.ends_with(';') || trimmed.contains(';'));
            if (is_single_line_decl || (current_depth <= top.start_depth && (close_count > 0 || open_count > 0 || line.contains('}'))))
                && (current_depth == 0 || is_single_line_decl) {
                    let top_pop = active_stack.pop().unwrap();
                    result.symbols.push(ExtractedSymbol {
                        name: top_pop.name,
                        kind: top_pop.kind,
                        start_line: top_pop.start_line,
                        end_line: line_num,
                    });
                }
        }

        if line.starts_with(char::is_whitespace) {
            continue; // module level only
        }

        if let Some(rest) = mod_decl(line) {
            result.imports.push(ImportRef {
                raw_specifier: format!("mod {rest}"),
                resolved_path: normalize_relative(&module_base, rest),
                kind: EdgeKind::Mod,
                external: false,
            });
            continue;
        }

        if let Some(rest) = line.trim().strip_prefix("use ") {
            let path = rest.trim_end_matches(';').trim();
            let segments: Vec<&str> = path
                .split("::")
                .map(|s| s.trim())
                .take_while(|s| !s.contains('{') && !s.contains('*') && !s.contains(" as "))
                .collect();
            let (first, rest_segs) = match segments.split_first() {
                Some(pair) => pair,
                None => continue,
            };
            let base = match *first {
                "crate" => crate_root.clone(),
                "super" => {
                    let p = dir.clone();
                    if p.is_empty() || !p.contains('/') {
                        String::new()
                    } else {
                        if let Some(idx) = p.rfind('/') {
                            p[..idx].to_string()
                        } else {
                            String::new()
                        }
                    }
                }
                "self" => module_base.clone(),
                _ => continue, // external / std crate: no edge
            };
            let joined = rest_segs.join("/");
            let candidate = normalize_relative(&base, &joined);
            result.imports.push(ImportRef {
                raw_specifier: path.to_string(),
                resolved_path: candidate.filter(|c| !c.is_empty()),
                kind: EdgeKind::Use,
                external: false,
            });
            continue;
        }

        let decl = line.strip_prefix("pub ").unwrap_or(line);
        let decl = decl.strip_prefix("pub(crate) ").unwrap_or(decl);
        let decl = decl.strip_prefix("async ").unwrap_or(decl);
        let decl = decl.strip_prefix("const ").unwrap_or(decl);
        let decl = decl.strip_prefix("unsafe ").unwrap_or(decl);

        for prefix in ["fn ", "struct ", "enum ", "trait ", "type ", "const ", "static "] {
            if line.starts_with("pub ") {
                if let Some(rest) = decl.strip_prefix(prefix) {
                    let name: String = rest
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect();
                    if !name.is_empty() {
                        result.exports.push(name);
                    }
                    break;
                }
            }
        }
    }

    while let Some(top) = active_stack.pop() {
        result.symbols.push(ExtractedSymbol {
            name: top.name,
            kind: top.kind,
            start_line: top.start_line,
            end_line: total_lines,
        });
    }

    // Extract identifier references
    for (idx, line) in lines.iter().enumerate() {
        let line_num = idx + 1;
        let mut current_word = String::new();
        for c in line.chars() {
            if c.is_alphanumeric() || c == '_' {
                current_word.push(c);
            } else {
                if !current_word.is_empty() {
                    let first = current_word.chars().next().unwrap();
                    if (first.is_alphabetic() || first == '_') && current_word.len() > 1 {
                        result.references.push(super::ExtractedReference {
                            name: current_word.clone(),
                            line: line_num,
                        });
                    }
                    current_word.clear();
                }
            }
        }
        if !current_word.is_empty() {
            let first = current_word.chars().next().unwrap();
            if (first.is_alphabetic() || first == '_') && current_word.len() > 1 {
                result.references.push(super::ExtractedReference {
                    name: current_word.clone(),
                    line: line_num,
                });
            }
        }
    }

    super::ts_engine::replace_symbols(
        &super::ts_langs::spec_rust(),
        "rust",
        source,
        &mut result,
    );

    scan_rust_routes(&lines, &mut result);
    super::state::apply(&super::state::scan_rust_state(source), &mut result.symbols);
    super::schema::apply(&super::schema::scan_rust(source), &mut result.symbols);
    result
}

/// Playbook §19.4: Axum `Router::new().route("/x", get(handler))` and Poem
/// `Route::new().at("/x", get(handler))` builder chains — each `.route(` /
/// `.at(` call on any line yields one entry point bound to the identifier
/// inside the method wrapper.
fn scan_rust_routes(lines: &[&str], result: &mut ExtractionResult) {
    use super::routes::{quoted_arg, wrapped_handler_after_comma};
    for (idx, line) in lines.iter().enumerate() {
        let line_num = idx + 1;
        let trimmed = line.trim();
        if trimmed.starts_with("//") {
            continue;
        }
        for needle in [".route(", ".at("] {
            let mut search_from = 0;
            while let Some(pos) = trimmed[search_from..].find(needle) {
                let abs = search_from + pos;
                let rest = &trimmed[abs + needle.len()..];
                if let Some((path, after_path)) = quoted_arg(rest) {
                    if path.starts_with('/') {
                        result.entry_points.push(super::EntryPoint {
                            route: path,
                            handler: wrapped_handler_after_comma(after_path),
                            line: line_num,
                        });
                    }
                }
                search_from = abs + needle.len();
            }
        }
    }
}

fn mod_decl(line: &str) -> Option<&str> {
    let decl = line.trim().strip_prefix("pub ").unwrap_or(line.trim());
    let rest = decl.strip_prefix("mod ")?;
    let rest = rest.trim();
    // Declaration only (`mod x;`), not inline module bodies (`mod x {`).
    rest.strip_suffix(';')
        .map(str::trim)
        .filter(|n| n.chars().all(|c| c.is_alphanumeric() || c == '_'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_toml_dependencies_are_external() {
        let r = RustExtractor.extract(
            Path::new("src-tauri/Cargo.toml"),
            "[package]\nname = \"x\"\n[dependencies]\nserde = \"1\"\nignore = \"0.4\"\n",
        );
        let mut names: Vec<&str> = r.imports.iter().map(|i| i.raw_specifier.as_str()).collect();
        names.sort();
        assert_eq!(names, vec!["ignore", "serde"]);
        assert!(r.imports.iter().all(|i| i.external));
    }

    #[test]
    fn mod_and_use_edges() {
        let r = RustExtractor.extract(
            Path::new("src-tauri/src/lib.rs"),
            "pub mod graph;\npub mod walker;\nuse crate::graph::EdgeKind;\nuse serde::Serialize;\nmod inline { }\n",
        );
        let internal: Vec<(&str, EdgeKind)> = r
            .imports
            .iter()
            .filter_map(|i| i.resolved_path.as_deref().map(|p| (p, i.kind)))
            .collect();
        assert_eq!(
            internal,
            vec![
                ("src-tauri/src/graph", EdgeKind::Mod),
                ("src-tauri/src/walker", EdgeKind::Mod),
                ("src-tauri/src/graph/EdgeKind", EdgeKind::Use),
            ]
        );
        // `use serde::...` (external) produced no edge candidate at all.
        assert_eq!(r.imports.len(), 3);
    }

    #[test]
    fn submodule_mod_base_and_pub_exports() {
        let r = RustExtractor.extract(
            Path::new("src/parsers.rs"),
            "mod javascript;\npub fn extract_all() {}\npub struct Registry;\nfn private() {}\n",
        );
        assert_eq!(
            r.imports[0].resolved_path.as_deref(),
            Some("src/parsers/javascript")
        );
        assert_eq!(r.exports, vec!["extract_all", "Registry"]);
    }
}
