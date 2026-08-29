use super::{normalize_relative, parent_dir, EntryPoint, ExtractionResult, Extractor, ImportRef, ExtractedSymbol};
use crate::graph::EdgeKind;
use std::path::Path;

pub struct PythonExtractor;

struct ActivePySymbol {
    name: String,
    kind: String,
    start_line: usize,
    indent: usize,
}

impl Extractor for PythonExtractor {
    fn extract(&self, file_path: &Path, source: &str) -> ExtractionResult {
        let mut result = ExtractionResult::default();
        let base_dir = parent_dir(file_path);

        let mut active_stack: Vec<ActivePySymbol> = Vec::new();
        let lines: Vec<&str> = source.lines().collect();
        let total_lines = lines.len();

        // Route decorators seen above the next `def` (decorators stack).
        let mut pending_routes: Vec<(String, usize)> = Vec::new();

        for (idx, line) in lines.iter().enumerate() {
            let line_num = idx + 1;
            let trimmed = line.trim_start();

            if let Some(rest) = trimmed.strip_prefix("import ") {
                // `import a.b, c as d` — one module per comma group.
                for group in rest.split(',') {
                    let module = group.split_whitespace().next().unwrap_or("");
                    if !module.is_empty() {
                        push_module(&mut result, &base_dir, module);
                    }
                }
            } else if let Some(rest) = trimmed.strip_prefix("from ") {
                let parts: Vec<&str> = rest.split_whitespace().collect();
                if let Some(&module) = parts.first() {
                    if (module == "." || module == "..") && parts.len() >= 3 && parts[1] == "import" {
                        let imported_names = parts[2..].join(" ");
                        for sym in imported_names.split(',') {
                            let clean_sym = sym.trim();
                            if !clean_sym.is_empty() {
                                let sub_mod = format!("{module}{clean_sym}");
                                push_module(&mut result, &base_dir, &sub_mod);
                            }
                        }
                    } else {
                        push_module(&mut result, &base_dir, module);
                    }
                }
            } else if trimmed.starts_with('@') {
                if let Some(route) = decorator_route(trimmed) {
                    pending_routes.push((route, line_num));
                }
            } else if trimmed.starts_with("def ") || trimmed.starts_with("async def ") {
                // Bind stacked route decorators to this handler.
                let rest = trimmed
                    .strip_prefix("async def ")
                    .or_else(|| trimmed.strip_prefix("def "))
                    .unwrap_or("");
                if let Some(name) = get_decl_name(rest) {
                    for (route, decl_line) in pending_routes.drain(..) {
                        result.entry_points.push(EntryPoint {
                            route,
                            handler: Some(name.clone()),
                            line: decl_line,
                        });
                    }
                }
            } else if !trimmed.is_empty() && !trimmed.starts_with('#') {
                // Sanic class-based views: app.add_route(Handler.as_view(), "/p")
                if let Some(ep) = sanic_add_route(trimmed, line_num) {
                    result.entry_points.push(ep);
                } else if let Some(ep) = tornado_route_tuple(trimmed, line_num) {
                    result.entry_points.push(ep);
                }
                // Any other statement breaks a decorator → def sequence.
                pending_routes.clear();
            }

            // Indentation-based symbol boundaries
            if let Some(indent) = get_indent(line) {
                while let Some(top) = active_stack.last() {
                    if top.indent >= indent {
                        let top_pop = active_stack.pop().unwrap();
                        result.symbols.push(ExtractedSymbol {
                            name: top_pop.name,
                            kind: top_pop.kind,
                            start_line: top_pop.start_line,
                            end_line: line_num - 1,
                        });
                    } else {
                        break;
                    }
                }

                if let Some((name, mut kind)) = parse_python_decl(trimmed) {
                    if kind == "function" && indent > 0 {
                        kind = "method".to_string();
                    }
                    active_stack.push(ActivePySymbol {
                        name,
                        kind,
                        start_line: line_num,
                        indent,
                    });
                }
            }

            // Module-level defs/classes only (no indentation).
            if !line.starts_with(char::is_whitespace) {
                if let Some(rest) = line.strip_prefix("def ") {
                    push_name(&mut result.exports, rest);
                } else if let Some(rest) = line.strip_prefix("class ") {
                    push_name(&mut result.exports, rest);
                } else if let Some(rest) = line.strip_prefix("async def ") {
                    push_name(&mut result.exports, rest);
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

        // Declarations come from the grammar: indentation-based range
        // tracking mis-sized decorated and multi-line signatures, and read
        // `def` inside docstrings as a definition. Imports and route
        // decorators above stay line-based — both are conventions the
        // grammar cannot distinguish from ordinary syntax.
        super::ts_engine::replace_symbols(
            &super::ts_langs::spec_python(),
            "python",
            source,
            &mut result,
        );

        super::state::apply(&super::state::scan_python_state(source), &mut result.symbols);
        super::schema::apply(&super::schema::scan_python(source), &mut result.symbols);
        result
    }
}

fn get_indent(line: &str) -> Option<usize> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    Some(line.len() - line.trim_start().len())
}

fn parse_python_decl(trimmed: &str) -> Option<(String, String)> {
    if let Some(rest) = trimmed.strip_prefix("def ") {
        let name = get_decl_name(rest)?;
        return Some((name, "function".to_string()));
    }
    if let Some(rest) = trimmed.strip_prefix("async def ") {
        let name = get_decl_name(rest)?;
        return Some((name, "function".to_string()));
    }
    if let Some(rest) = trimmed.strip_prefix("class ") {
        let name = get_decl_name(rest)?;
        return Some((name, "class".to_string()));
    }
    None
}

fn get_decl_name(rest: &str) -> Option<String> {
    let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
    if name.is_empty() { None } else { Some(name) }
}

fn push_name(exports: &mut Vec<String>, decl_rest: &str) {
    let name: String = decl_rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if !name.is_empty() && !name.starts_with('_') {
        exports.push(name);
    }
}

fn push_module(result: &mut ExtractionResult, base_dir: &str, module: &str) {
    let dots = module.chars().take_while(|c| *c == '.').count();
    let remainder = &module[dots..];
    let (resolved, external) = if dots > 0 {
        // `.` = current package, each extra dot = one package up.
        let ups = "../".repeat(dots - 1);
        let rel = format!("{ups}{}", remainder.replace('.', "/"));
        (normalize_relative(base_dir, &rel), false)
    } else {
        // Absolute module path: candidate top-level path; the builder marks
        // it external if nothing in the repo matches.
        (Some(remainder.replace('.', "/")), true)
    };
    result.imports.push(ImportRef {
        raw_specifier: module.to_string(),
        resolved_path: resolved.filter(|r| !r.is_empty()),
        kind: EdgeKind::Import,
        external,
    });
}

/// `@app.get("/users/{id}")`, `@router.post('/x')`, `@app.route("/x")` →
/// the route string. Conservative: literal string argument only.
fn decorator_route(line: &str) -> Option<String> {
    const DOTTED: &[&str] = &[
        ".get(", ".post(", ".put(", ".delete(", ".patch(", ".head(", ".options(", ".route(",
        ".websocket(",
    ];
    // Litestar / Sanic bare decorators.
    const BARE: &[&str] = &[
        "@get(", "@post(", "@put(", "@delete(", "@patch(", "@head(", "@options(", "@route(",
        "@websocket(",
    ];
    let dotted = DOTTED.iter().find_map(|m| {
        let idx = line.find(m)?;
        Some(&line[idx + m.len()..])
    });
    let bare = BARE.iter().find_map(|m| line.strip_prefix(m));
    let after = dotted.or(bare)?;
    let (route, _) = super::routes::quoted_arg(after)?;
    Some(route)
}

/// Sanic class-based views: `app.add_route(Handler.as_view(), "/path")`.
fn sanic_add_route(line: &str, line_num: usize) -> Option<EntryPoint> {
    let idx = line.find(".add_route(")?;
    let inner = &line[idx + ".add_route(".len()..];
    let handler: String = inner
        .trim_start()
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    // Route is the argument after the handler expression (which may itself
    // contain parens, e.g. `Handler.as_view()`).
    let after_handler = inner.split_once(',').map(|(_, r)| r)?;
    let (route, _) = super::routes::quoted_arg(after_handler)?;
    if handler.is_empty() || !route.starts_with('/') {
        return None;
    }
    Some(EntryPoint {
        route,
        handler: Some(handler),
        line: line_num,
    })
}

/// Tornado routing tables: tuples like `(r"/users/(\d+)", UserHandler),`
/// inside `tornado.web.Application([...])`. Heuristic: a parenthesized
/// pair of (quoted absolute path, ClassName) — class names must start
/// uppercase to avoid false positives.
fn tornado_route_tuple(line: &str, line_num: usize) -> Option<EntryPoint> {
    let open = line.find('(')?;
    let inner = &line[open + 1..];
    let lead = inner.trim_start();
    if !(lead.starts_with('"')
        || lead.starts_with('\'')
        || lead.starts_with("r\"")
        || lead.starts_with("r'"))
    {
        return None;
    }
    let (route, rest) = super::routes::quoted_arg(inner)?;
    if !route.starts_with('/') {
        return None;
    }
    let handler = super::routes::handler_after_comma(rest)?;
    if !handler.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
        return None;
    }
    Some(EntryPoint {
        route,
        handler: Some(handler),
        line: line_num,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(path: &str, src: &str) -> ExtractionResult {
        PythonExtractor.extract(Path::new(path), src)
    }

    #[test]
    fn absolute_and_relative_imports() {
        let r = run(
            "pkg/api/users.py",
            "import os\nfrom fastapi import APIRouter\nfrom .schemas import User\nfrom . import models\nfrom ..core.db import session\n",
        );
        let stems: Vec<(&str, bool)> = r
            .imports
            .iter()
            .map(|i| (i.resolved_path.as_deref().unwrap_or("-"), i.external))
            .collect();
        assert_eq!(
            stems,
            vec![
                ("os", true),
                ("fastapi", true),
                ("pkg/api/schemas", false),
                ("pkg/api/models", false),
                ("pkg/core/db", false),
            ]
        );
    }

    #[test]
    fn route_decorators_and_exports() {
        let r = run(
            "app/main.py",
            "@app.get(\"/users/{id}\")\ndef read_user(id):\n    pass\n\n@router.post('/items')\nasync def create_item():\n    pass\n\nclass Service:\n    def _hidden(self):\n        pass\n",
        );
        assert_eq!(
            r.entry_points,
            vec![
                EntryPoint {
                    route: "/users/{id}".into(),
                    handler: Some("read_user".into()),
                    line: 1
                },
                EntryPoint {
                    route: "/items".into(),
                    handler: Some("create_item".into()),
                    line: 5
                }
            ]
        );
        assert_eq!(r.exports, vec!["read_user", "create_item", "Service"]);
    }

    #[test]
    fn litestar_sanic_and_tornado_routes() {
        let r = run(
            "srv/app.py",
            "@get(\"/books\")\nasync def list_books():\n    pass\n\napp.add_route(UserView.as_view(), \"/users\")\n\napplication = tornado.web.Application([\n    (r\"/posts/(\\d+)\", PostHandler),\n])\n",
        );
        let got: Vec<(&str, Option<&str>)> = r
            .entry_points
            .iter()
            .map(|e| (e.route.as_str(), e.handler.as_deref()))
            .collect();
        assert_eq!(
            got,
            vec![
                ("/books", Some("list_books")),
                ("/users", Some("UserView")),
                ("/posts/(\\d+)", Some("PostHandler")),
            ]
        );
    }
}
