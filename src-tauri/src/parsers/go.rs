use super::{normalize_relative, parent_dir, EntryPoint, ExtractionResult, Extractor, ImportRef, ExtractedSymbol};
use crate::graph::EdgeKind;
use std::collections::HashMap;
use std::path::Path;

pub struct GoExtractor;

struct ActiveGoSymbol {
    name: String,
    kind: String,
    start_line: usize,
    start_depth: usize,
}

impl Extractor for GoExtractor {
    fn extract(&self, file_path: &Path, source: &str) -> ExtractionResult {
        let mut result = ExtractionResult::default();
        let mut in_import_block = false;
        let base_dir = parent_dir(file_path);

        let mut active_stack: Vec<ActiveGoSymbol> = Vec::new();
        let mut current_depth = 0;
        let lines: Vec<&str> = source.lines().collect();
        let total_lines = lines.len();

        for (idx, line) in lines.iter().enumerate() {
            let line_num = idx + 1;
            let trimmed = line.trim();
            if trimmed.starts_with("//") {
                continue;
            }

            if let Some(rest) = trimmed.strip_prefix("package ") {
                let pkg_name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if !pkg_name.is_empty() {
                    result.exports.push(format!("package:{}", pkg_name));
                }
            } else if trimmed == "import (" {
                in_import_block = true;
            } else if in_import_block && trimmed == ")" {
                in_import_block = false;
            } else if in_import_block {
                if let Some(spec) = extract_go_import_spec(trimmed) {
                    push_go_import(&mut result, &base_dir, spec);
                }
            } else if let Some(rest) = trimmed.strip_prefix("import ") {
                if let Some(spec) = extract_go_import_spec(rest) {
                    push_go_import(&mut result, &base_dir, spec);
                }
            }

            // Brace-based symbol boundaries
            if current_depth == 0 {
                if let Some((name, kind)) = parse_go_decl(trimmed) {
                    active_stack.push(ActiveGoSymbol {
                        name,
                        kind,
                        start_line: line_num,
                        start_depth: 0,
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
                if current_depth <= top.start_depth && (close_count > 0 || open_count > 0 || line.contains('}'))
                    && current_depth == 0 {
                        let top_pop = active_stack.pop().unwrap();
                        result.symbols.push(ExtractedSymbol {
                            name: top_pop.name,
                            kind: top_pop.kind,
                            start_line: top_pop.start_line,
                            end_line: line_num,
                        });
                    }
            }

            // Simple top-level declarations scan for exports
            if trimmed.starts_with("func ") {
                if let Some(rest) = trimmed.strip_prefix("func ") {
                    push_exported_name(&mut result.exports, rest);
                }
            } else if trimmed.starts_with("type ") {
                if let Some(rest) = trimmed.strip_prefix("type ") {
                    push_exported_name(&mut result.exports, rest);
                }
            } else if trimmed.starts_with("const ") {
                if let Some(rest) = trimmed.strip_prefix("const ") {
                    push_exported_name(&mut result.exports, rest);
                }
            } else if trimmed.starts_with("var ") {
                if let Some(rest) = trimmed.strip_prefix("var ") {
                    push_exported_name(&mut result.exports, rest);
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
            &super::ts_langs::spec_go(),
            "go",
            source,
            &mut result,
        );

        scan_go_routes(&lines, &mut result);
        super::state::apply(&super::state::scan_go_state(source), &mut result.symbols);
        super::schema::apply(&super::schema::scan_go(source), &mut result.symbols);
        result
    }
}

/// Playbook §19.3: Echo (`e.GET("/x", h)`), Fiber (`app.Get("/x", h)`),
/// `.Group("/prefix")` sub-routers, and Beego `// @router /x [get]`
/// comment decorators bound to the next `func`.
fn scan_go_routes(lines: &[&str], result: &mut ExtractionResult) {
    use super::routes::{handler_after_comma, method_call_route, quoted_arg};
    // Echo upper-case + Fiber title-case registration methods.
    const METHODS: &[&str] = &[
        "GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS", "Any",
        "Get", "Post", "Put", "Delete", "Patch", "Head", "Options", "All",
    ];
    let mut group_prefixes: HashMap<String, String> = HashMap::new();
    let mut pending_beego: Option<(String, usize)> = None;

    for (idx, line) in lines.iter().enumerate() {
        let line_num = idx + 1;
        let trimmed = line.trim();

        // Beego: `// @router /path [get]` binds to the next func.
        if let Some(rest) = trimmed.strip_prefix("//") {
            let rest = rest.trim();
            if let Some(route_part) = rest.strip_prefix("@router ") {
                let route = route_part
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_string();
                if route.starts_with('/') {
                    pending_beego = Some((route, line_num));
                }
            }
            continue;
        }
        if trimmed.starts_with("func ") {
            if let Some((route, decl_line)) = pending_beego.take() {
                if let Some((name, _)) = parse_go_decl(trimmed) {
                    result.entry_points.push(EntryPoint {
                        route,
                        handler: Some(name),
                        line: decl_line,
                    });
                }
            }
        } else if !trimmed.is_empty() {
            pending_beego = None;
        }

        // `g := e.Group("/api")` — remember the sub-router's prefix.
        if let Some(pos) = trimmed.find(".Group(") {
            if let Some((prefix, _)) = quoted_arg(&trimmed[pos + ".Group(".len()..]) {
                let var = trimmed
                    .split(":=")
                    .next()
                    .map(str::trim)
                    .unwrap_or("")
                    .to_string();
                if !var.is_empty() && var.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    group_prefixes.insert(var, prefix);
                }
            }
        }

        if let Some((path, rest, _)) = method_call_route(trimmed, None, METHODS) {
            // Prepend the receiver's group prefix when it's a known group var.
            let receiver = trimmed
                .split('.')
                .next()
                .map(str::trim)
                .unwrap_or("");
            let full = match group_prefixes.get(receiver) {
                Some(prefix) => format!("{prefix}{path}"),
                None => path,
            };
            result.entry_points.push(EntryPoint {
                route: full,
                handler: handler_after_comma(rest),
                line: line_num,
            });
        }
    }
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

fn parse_go_decl(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    let mut words = line.split_whitespace();
    let word = words.next()?;
    if word == "func" {
        let rest = words.collect::<Vec<_>>().join(" ");
        if rest.starts_with('(') {
            let parts: Vec<&str> = rest.split(')').collect();
            let after_receiver = parts.get(1)?;
            let mut name_words = after_receiver.split_whitespace();
            let name = name_words.next()?.split('(').next()?;
            return Some((name.to_string(), "method".to_string()));
        } else {
            let name = rest.split('(').next()?;
            return Some((name.to_string(), "function".to_string()));
        }
    }
    if word == "type" {
        let name = words.next()?;
        let kind_word = words.next()?;
        let kind = match kind_word {
            "struct" => "struct",
            "interface" => "interface",
            _ => "type_alias",
        };
        return Some((name.to_string(), kind.to_string()));
    }
    None
}

fn extract_go_import_spec(line: &str) -> Option<String> {
    let mut parts = line.split_whitespace();
    let last = parts.next_back()?;
    if last.starts_with('"') && last.ends_with('"') && last.len() > 2 {
        Some(last[1..last.len() - 1].to_string())
    } else {
        None
    }
}

fn push_go_import(result: &mut ExtractionResult, base_dir: &str, spec: String) {
    let (resolved, external) = if spec.starts_with('.') {
        (normalize_relative(base_dir, &spec), false)
    } else {
        (Some(spec.clone()), true)
    };
    result.imports.push(ImportRef {
        raw_specifier: spec,
        resolved_path: resolved,
        kind: EdgeKind::Import,
        external,
    });
}

fn push_exported_name(exports: &mut Vec<String>, decl_rest: &str) {
    let name: String = decl_rest
        .trim_start()
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if !name.is_empty() && name.chars().next().unwrap().is_uppercase() {
        exports.push(name);
    }
}
