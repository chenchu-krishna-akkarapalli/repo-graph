//! JavaScript / TypeScript extractor — AST-based via `swc_ecma_parser`
//! (regex breaks on multi-line and aliased imports). Also tags Next.js App
//! Router files (`app/**/page.tsx`, `route.ts`, `layout.tsx`) as entry
//! points with the route derived from the directory path.

use super::{normalize_relative, parent_dir, EntryPoint, ExtractionResult, Extractor, ImportRef, ExtractedSymbol, ExtractedReference};
use crate::graph::EdgeKind;
use std::path::Path;
use swc_common::input::StringInput;
use swc_common::sync::Lrc;
use swc_common::{FileName, SourceMap};
use swc_ecma_ast::{
    Callee, Decl, ExportSpecifier, Expr, Lit, Module, ModuleDecl, ModuleExportName, ModuleItem,
    Pat,
};
use swc_ecma_parser::{Parser, Syntax, TsSyntax};
use swc_ecma_visit::{Visit, VisitWith};

pub struct JsExtractor;

struct SymbolVisitor {
    cm: Lrc<SourceMap>,
    symbols: Vec<ExtractedSymbol>,
    references: Vec<ExtractedReference>,
    inheritance: Vec<super::InheritanceRelation>,
    file_path: std::path::PathBuf,
}

impl SymbolVisitor {
    fn get_lines(&self, span: swc_common::Span) -> (usize, usize) {
        let start_pos = self.cm.lookup_char_pos(span.lo);
        let end_pos = self.cm.lookup_char_pos(span.hi);
        (start_pos.line, end_pos.line)
    }

    fn map_kind(&self, name: &str, default_kind: &str) -> String {
        if (default_kind == "Function" || default_kind == "Variable")
            && name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
                && self.file_path.to_string_lossy().ends_with('x') {
                    return "component".to_string();
                }
        match default_kind {
            "Class" => "class".to_string(),
            "Function" => "function".to_string(),
            "Interface" => "interface".to_string(),
            "Type" => "type_alias".to_string(),
            "Enum" => "enum".to_string(),
            "Variable" => "variable".to_string(),
            other => other.to_lowercase(),
        }
    }
}

impl Visit for SymbolVisitor {
    fn visit_ident(&mut self, n: &swc_ecma_ast::Ident) {
        let name = n.sym.to_string();
        let line = self.cm.lookup_char_pos(n.span.lo).line;
        self.references.push(ExtractedReference { name, line });
    }

    fn visit_decl(&mut self, n: &Decl) {
        n.visit_children_with(self);
        match n {
            Decl::Fn(f) => {
                let name = f.ident.sym.to_string();
                let (start, end) = self.get_lines(f.function.span);
                let kind = self.map_kind(&name, "Function");
                self.symbols.push(ExtractedSymbol {
                    name,
                    kind,
                    start_line: start,
                    end_line: end,
                });
            }
            Decl::Class(c) => {
                let name = c.ident.sym.to_string();
                let (start, end) = self.get_lines(c.class.span);
                let kind = self.map_kind(&name, "Class");
                self.symbols.push(ExtractedSymbol {
                    name: name.clone(),
                    kind,
                    start_line: start,
                    end_line: end,
                });

                if let Some(super_class) = &c.class.super_class {
                    if let Expr::Ident(ident) = &**super_class {
                        let parent_name = ident.sym.to_string();
                        let line = self.cm.lookup_char_pos(ident.span.lo).line;
                        self.inheritance.push(super::InheritanceRelation {
                            class_name: name.clone(),
                            parent_name,
                            is_implements: false,
                            line,
                        });
                    }
                }

                for impl_item in &c.class.implements {
                    if let Expr::Ident(ident) = &*impl_item.expr {
                        let parent_name = ident.sym.to_string();
                        let line = self.cm.lookup_char_pos(ident.span.lo).line;
                        self.inheritance.push(super::InheritanceRelation {
                            class_name: name.clone(),
                            parent_name,
                            is_implements: true,
                            line,
                        });
                    }
                }
            }
            Decl::TsInterface(i) => {
                let name = i.id.sym.to_string();
                let (start, end) = self.get_lines(i.span);
                let kind = self.map_kind(&name, "Interface");
                self.symbols.push(ExtractedSymbol {
                    name,
                    kind,
                    start_line: start,
                    end_line: end,
                });
            }
            Decl::TsTypeAlias(t) => {
                let name = t.id.sym.to_string();
                let (start, end) = self.get_lines(t.span);
                let kind = self.map_kind(&name, "Type");
                self.symbols.push(ExtractedSymbol {
                    name,
                    kind,
                    start_line: start,
                    end_line: end,
                });
            }
            Decl::TsEnum(e) => {
                let name = e.id.sym.to_string();
                let (start, end) = self.get_lines(e.span);
                let kind = self.map_kind(&name, "Enum");
                self.symbols.push(ExtractedSymbol {
                    name,
                    kind,
                    start_line: start,
                    end_line: end,
                });
            }
            Decl::Var(v) => {
                for decl in &v.decls {
                    if let Pat::Ident(bi) = &decl.name {
                        let name = bi.id.sym.to_string();
                        let (start, end) = self.get_lines(decl.span);
                        let raw_kind = if let Some(init) = &decl.init {
                            match &**init {
                                Expr::Arrow(_) | Expr::Fn(_) => "Function",
                                _ => "Variable",
                            }
                        } else {
                            "Variable"
                        };
                        let kind = self.map_kind(&name, raw_kind);
                        self.symbols.push(ExtractedSymbol {
                            name,
                            kind,
                            start_line: start,
                            end_line: end,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    fn visit_export_default_decl(&mut self, n: &swc_ecma_ast::ExportDefaultDecl) {
        n.visit_children_with(self);
        match &n.decl {
            swc_ecma_ast::DefaultDecl::Fn(f) => {
                let name = f.ident.as_ref().map(|i| i.sym.to_string()).unwrap_or_else(|| "default".to_string());
                let (start, end) = self.get_lines(n.span);
                let kind = self.map_kind(&name, "Function");
                self.symbols.push(ExtractedSymbol {
                    name,
                    kind,
                    start_line: start,
                    end_line: end,
                });
            }
            swc_ecma_ast::DefaultDecl::Class(c) => {
                let name = c.ident.as_ref().map(|i| i.sym.to_string()).unwrap_or_else(|| "default".to_string());
                let (start, end) = self.get_lines(n.span);
                let kind = self.map_kind(&name, "Class");
                self.symbols.push(ExtractedSymbol {
                    name: name.clone(),
                    kind,
                    start_line: start,
                    end_line: end,
                });

                if let Some(super_class) = &c.class.super_class {
                    if let Expr::Ident(ident) = &**super_class {
                        let parent_name = ident.sym.to_string();
                        let line = self.cm.lookup_char_pos(ident.span.lo).line;
                        self.inheritance.push(super::InheritanceRelation {
                            class_name: name.clone(),
                            parent_name,
                            is_implements: false,
                            line,
                        });
                    }
                }

                for impl_item in &c.class.implements {
                    if let Expr::Ident(ident) = &*impl_item.expr {
                        let parent_name = ident.sym.to_string();
                        let line = self.cm.lookup_char_pos(ident.span.lo).line;
                        self.inheritance.push(super::InheritanceRelation {
                            class_name: name.clone(),
                            parent_name,
                            is_implements: true,
                            line,
                        });
                    }
                }
            }
            _ => {}
        }
    }
}

impl Extractor for JsExtractor {
    fn extract(&self, file_path: &Path, source: &str) -> ExtractionResult {
        let mut result = ExtractionResult::default();

        if let Some(route) = next_app_router_route(file_path) {
            result.entry_points.push(EntryPoint {
                route,
                handler: None,
                line: 0,
            });
        }

        let ParseOutcome { cm, module, recovered } = match parse(file_path, source) {
            Ok(outcome) => outcome,
            Err(_) => {
                result.warnings.push("parse_error".to_string());
                return result; // never fatal: file stays in the graph, unmapped
            }
        };
        if recovered {
            // The map is usable but incomplete here — say so rather than
            // letting an agent trust it silently.
            result.warnings.push("partial_parse".to_string());
        }

        let mut visitor = SymbolVisitor {
            cm: cm.clone(),
            symbols: Vec::new(),
            references: Vec::new(),
            inheritance: Vec::new(),
            file_path: file_path.to_path_buf(),
        };
        module.visit_with(&mut visitor);
        result.symbols = visitor.symbols;
        result.references = visitor.references;
        result.inheritance = visitor.inheritance;

        let base_dir = parent_dir(file_path);
        for item in &module.body {
            let ModuleItem::ModuleDecl(decl) = item else {
                continue;
            };
            match decl {
                ModuleDecl::Import(imp) => {
                    push_import(&mut result, &base_dir, &str_value(&imp.src), EdgeKind::Import);
                }
                ModuleDecl::ExportDecl(ed) => collect_decl_exports(&ed.decl, &mut result.exports),
                ModuleDecl::ExportDefaultDecl(_) | ModuleDecl::ExportDefaultExpr(_) => {
                    result.exports.push("default".to_string());
                }
                ModuleDecl::ExportNamed(ne) => {
                    for spec in &ne.specifiers {
                        if let ExportSpecifier::Named(named) = spec {
                            let name = named.exported.as_ref().unwrap_or(&named.orig);
                            result.exports.push(export_name(name));
                        }
                    }
                    if let Some(src) = &ne.src {
                        push_import(&mut result, &base_dir, &str_value(src), EdgeKind::Import);
                    }
                }
                ModuleDecl::ExportAll(ea) => {
                    push_import(&mut result, &base_dir, &str_value(&ea.src), EdgeKind::Import);
                }
                _ => {}
            }
        }

        // require(...) and dynamic import(...) live in expression position.
        let mut calls = CallScanner::default();
        module.visit_with(&mut calls);
        for spec in calls.requires {
            push_import(&mut result, &base_dir, &spec, EdgeKind::Require);
        }
        for spec in calls.dynamic_literals {
            push_import(&mut result, &base_dir, &spec, EdgeKind::Import);
        }
        if calls.unresolved_dynamic {
            result.warnings.push("unresolved_dynamic".to_string());
        }

        // Playbook §19.1 route scanners (line-scan, cheap by design).
        remix_file_routes(file_path, &result.exports, &mut result.entry_points);
        scan_router_calls(source, &mut result.entry_points);
        // Zustand / Redux / Context / Pinia / Vuex / MobX containers. Runs
        // after the AST pass so it can retag symbols and keep their ranges.
        super::state::apply(&super::state::scan_js_state(source), &mut result.symbols);
        super::schema::apply(&super::schema::scan_js(source), &mut result.symbols);
        result
    }
}

/// Remix file-based routes: `app/routes/**` (or top-level `routes/**`).
/// Flat-file convention: `routes/posts.$id.tsx` → `/posts/:id`,
/// `routes/_index.tsx` → `/`. Nested directories append segments. Route
/// entries bind to the `loader` / `action` exports (and the default page
/// component) when present.
fn remix_file_routes(
    file_path: &Path,
    exports: &[String],
    entry_points: &mut Vec<super::EntryPoint>,
) {
    let rel = file_path.to_string_lossy().replace('\\', "/");
    let segments: Vec<&str> = rel.split('/').collect();
    let routes_idx = segments.iter().position(|s| *s == "routes").filter(|&i| {
        i == 0 || segments[i - 1] == "app" || segments[i - 1] == "src"
    });
    let Some(idx) = routes_idx else { return };
    let inner = &segments[idx + 1..];
    if inner.is_empty() {
        return;
    }
    let file = inner[inner.len() - 1];
    let stem = file
        .trim_end_matches(".tsx")
        .trim_end_matches(".jsx")
        .trim_end_matches(".ts")
        .trim_end_matches(".js");
    if stem == file {
        return; // not a JS/TS route module
    }

    let mut route = String::new();
    // Directory segments + flat-file dot segments share the same rules.
    let dir_parts = inner[..inner.len() - 1].iter().copied();
    let flat_parts = stem.split('.');
    for part in dir_parts.chain(flat_parts) {
        match part {
            "_index" | "index" | "route" => continue,
            p if p.starts_with('_') => continue, // pathless layout segment
            p => {
                let seg = if let Some(param) = p.strip_prefix("$") {
                    if param.is_empty() {
                        "*".to_string() // splat: `$.tsx`
                    } else {
                        format!(":{param}")
                    }
                } else {
                    p.to_string()
                };
                route.push('/');
                route.push_str(&seg);
            }
        }
    }
    if route.is_empty() {
        route.push('/');
    }

    let mut handlers: Vec<&str> = exports
        .iter()
        .filter(|e| *e == "loader" || *e == "action")
        .map(String::as_str)
        .collect();
    if handlers.is_empty() && exports.iter().any(|e| e == "default") {
        handlers.push("default");
    }
    if handlers.is_empty() {
        entry_points.push(super::routes::entry(&route, None, 0));
    } else {
        for h in handlers {
            entry_points.push(super::routes::entry(&route, Some(h.to_string()), 0));
        }
    }
}

/// Hono/Koa (`app.get('/x', handler)`, `router.post(...)`) and AdonisJS
/// (`Route.get('/x', 'Controller.method')`) registrations.
fn scan_router_calls(source: &str, entry_points: &mut Vec<super::EntryPoint>) {
    use super::routes::{handler_after_comma, method_call_route, quoted_handler_after_comma};
    const METHODS: &[&str] = &["get", "post", "put", "delete", "patch", "options", "all"];
    for (idx, line) in source.lines().enumerate() {
        let line_num = idx + 1;
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") || trimmed.starts_with('*') {
            continue;
        }
        if let Some((path, rest, _)) =
            method_call_route(line, Some(&["app", "router", "Route"]), METHODS)
        {
            // Adonis string-bound controllers take precedence over idents.
            let handler = quoted_handler_after_comma(rest).or_else(|| handler_after_comma(rest));
            entry_points.push(super::routes::entry(path, handler, line_num));
        }
    }
}

/// Outcome of parsing one file. `recovered` marks a module swc produced
/// despite emitting recoverable diagnostics — usable, but flagged.
struct ParseOutcome {
    cm: Lrc<SourceMap>,
    module: Module,
    recovered: bool,
}

/// `tsx` has to follow the extension. Forced on, a plain `.ts` file's generic
/// arrows (`const f = <T,>(x: T) => x`) and angle-bracket casts
/// (`<string>value`) are read as JSX and fail; forced off, every `.tsx`
/// component fails instead.
fn syntax_for(file_path: &Path) -> Syntax {
    let ext = file_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    Syntax::Typescript(TsSyntax {
        tsx: matches!(ext.as_str(), "tsx" | "jsx"),
        decorators: true,
        ..Default::default()
    })
}

fn parse(file_path: &Path, source: &str) -> Result<ParseOutcome, ()> {
    let cm: Lrc<SourceMap> = Default::default();
    let fm = cm.new_source_file(
        Lrc::new(FileName::Custom(file_path.display().to_string())),
        source.to_string(),
    );
    let mut parser = Parser::new(syntax_for(file_path), StringInput::from(&*fm), None);
    let module = parser.parse_module().map_err(|_| ())?;
    // `take_errors` returns *recoverable* diagnostics — swc still produced a
    // usable module. Discarding it threw away every symbol, import and edge in
    // an otherwise fine file; the file is now kept and the gap is reported as
    // a `partial_parse` warning instead.
    let recovered = !parser.take_errors().is_empty();
    Ok(ParseOutcome { cm, module, recovered })
}

fn push_import(result: &mut ExtractionResult, base_dir: &str, specifier: &str, kind: EdgeKind) {
    let is_relative = specifier.starts_with("./") || specifier.starts_with("../");
    let resolved = if is_relative {
        normalize_relative(base_dir, specifier)
    } else {
        None
    };
    result.imports.push(ImportRef {
        raw_specifier: specifier.to_string(),
        external: !is_relative,
        resolved_path: resolved,
        kind,
    });
}

fn collect_decl_exports(decl: &Decl, exports: &mut Vec<String>) {
    match decl {
        Decl::Fn(f) => exports.push(f.ident.sym.to_string()),
        Decl::Class(c) => exports.push(c.ident.sym.to_string()),
        Decl::Var(v) => {
            for d in &v.decls {
                if let Pat::Ident(bi) = &d.name {
                    exports.push(bi.id.sym.to_string());
                }
            }
        }
        Decl::TsInterface(i) => exports.push(i.id.sym.to_string()),
        Decl::TsTypeAlias(t) => exports.push(t.id.sym.to_string()),
        Decl::TsEnum(e) => exports.push(e.id.sym.to_string()),
        _ => {}
    }
}

fn export_name(name: &ModuleExportName) -> String {
    match name {
        ModuleExportName::Ident(i) => i.sym.to_string(),
        ModuleExportName::Str(s) => str_value(s),
    }
}

/// String literal value (swc stores these as WTF-8 atoms).
fn str_value(s: &swc_ecma_ast::Str) -> String {
    s.value.to_string_lossy().into_owned()
}

#[derive(Default)]
struct CallScanner {
    requires: Vec<String>,
    dynamic_literals: Vec<String>,
    unresolved_dynamic: bool,
}

impl Visit for CallScanner {
    fn visit_call_expr(&mut self, n: &swc_ecma_ast::CallExpr) {
        n.visit_children_with(self);
        match &n.callee {
            Callee::Import(_) => match n.args.first().map(|a| &*a.expr) {
                Some(Expr::Lit(Lit::Str(s))) => self.dynamic_literals.push(str_value(s)),
                _ => self.unresolved_dynamic = true,
            },
            Callee::Expr(e) => {
                if let Expr::Ident(ident) = &**e {
                    if ident.sym.as_ref() == "require" {
                        match n.args.first().map(|a| &*a.expr) {
                            Some(Expr::Lit(Lit::Str(s))) => {
                                self.requires.push(str_value(s))
                            }
                            _ => self.unresolved_dynamic = true,
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// `app/**/page.tsx`, `app/**/route.ts`, `app/**/layout.tsx` (optionally
/// under `src/`) → route string. `[param]` → `:param`; `(group)` segments
/// are elided, matching Next.js semantics.
fn next_app_router_route(file_path: &Path) -> Option<String> {
    let rel = file_path.to_string_lossy().replace('\\', "/");
    let segments: Vec<&str> = rel.split('/').collect();
    let file = segments.last()?;
    let stem = file.rsplit_once('.').map(|(s, _)| s).unwrap_or(file);
    if !matches!(stem, "page" | "route" | "layout") {
        return None;
    }
    let app_idx = segments
        .iter()
        .position(|s| *s == "app")
        .filter(|&i| i == 0 || (i == 1 && segments[0] == "src"))?;
    let mut route = String::new();
    for seg in &segments[app_idx + 1..segments.len() - 1] {
        if seg.starts_with('(') && seg.ends_with(')') {
            continue;
        }
        let part = if seg.starts_with('[') && seg.ends_with(']') {
            format!(":{}", &seg[1..seg.len() - 1])
        } else {
            (*seg).to_string()
        };
        route.push('/');
        route.push_str(&part);
    }
    Some(if route.is_empty() { "/".to_string() } else { route })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(path: &str, src: &str) -> ExtractionResult {
        JsExtractor.extract(Path::new(path), src)
    }

    #[test]
    fn extracts_imports_exports_and_aliases() {
        let r = run(
            "src/components/Button.tsx",
            r#"
import { useTheme, colors as palette } from '../hooks/useTheme';
import React from 'react';
export function Button() { return null; }
export const SIZE = 2;
export default Button;
"#,
        );
        assert_eq!(r.exports, vec!["Button", "SIZE", "default"]);
        let internal: Vec<_> = r.imports.iter().filter(|i| !i.external).collect();
        assert_eq!(internal.len(), 1);
        assert_eq!(
            internal[0].resolved_path.as_deref(),
            Some("src/hooks/useTheme")
        );
        assert!(r.imports.iter().any(|i| i.external && i.raw_specifier == "react"));
        assert!(r.warnings.is_empty());
    }

    #[test]
    fn require_and_dynamic_import_handling() {
        let r = run(
            "src/a.ts",
            r#"
const x = require('./legacy');
const y = import('./lazy');
const z = import(someVariable);
"#,
        );
        assert!(r
            .imports
            .iter()
            .any(|i| i.kind == EdgeKind::Require && i.resolved_path.as_deref() == Some("src/legacy")));
        assert!(r.imports.iter().any(|i| i.resolved_path.as_deref() == Some("src/lazy")));
        assert_eq!(r.warnings, vec!["unresolved_dynamic"]);
    }

    #[test]
    fn re_exports_create_edges() {
        let r = run("src/index.ts", "export { Button } from './components/Button';\nexport * from './hooks/useTheme';");
        assert_eq!(r.exports, vec!["Button"]);
        assert_eq!(r.imports.len(), 2);
        assert!(r.imports.iter().all(|i| !i.external));
    }

    /// `tsx: true` was forced on every file, so ordinary `.ts` type syntax
    /// was read as JSX and the whole file was reported as a parse error.
    #[test]
    fn plain_ts_generics_and_casts_are_not_mistaken_for_jsx() {
        let r = run(
            "src/util.ts",
            "export const identity = <T,>(x: T): T => x;\nexport const n = <number>someValue;\n",
        );
        assert!(
            !r.warnings.iter().any(|w| w == "parse_error"),
            "`.ts` generic/cast syntax must parse, got {:?}",
            r.warnings
        );
        assert!(r.exports.contains(&"identity".to_string()), "{:?}", r.exports);
    }

    /// The other half of the same rule: `.tsx` still needs JSX enabled.
    #[test]
    fn tsx_files_still_parse_jsx() {
        let r = run(
            "src/Button.tsx",
            "export const Button = () => <div className=\"x\">hi</div>;\n",
        );
        assert!(!r.warnings.iter().any(|w| w == "parse_error"), "{:?}", r.warnings);
        assert!(r.exports.contains(&"Button".to_string()));
    }

    #[test]
    fn syntax_follows_the_file_extension() {
        let tsx = |p: &str| match syntax_for(Path::new(p)) {
            Syntax::Typescript(t) => t.tsx,
            _ => unreachable!("always TypeScript syntax"),
        };
        assert!(tsx("a/b.tsx"));
        assert!(tsx("a/b.jsx"));
        assert!(!tsx("a/b.ts"));
        assert!(!tsx("a/b.js"));
        assert!(!tsx("a/b.mts"));
    }

    #[test]
    fn parse_error_is_warning_not_crash() {
        let r = run("src/broken.ts", "import { from ]]]");
        assert_eq!(r.warnings, vec!["parse_error"]);
        assert!(r.imports.is_empty());
    }

    #[test]
    fn next_app_router_routes() {
        assert_eq!(
            next_app_router_route(Path::new("src/app/dashboard/[id]/page.tsx")),
            Some("/dashboard/:id".to_string())
        );
        assert_eq!(
            next_app_router_route(Path::new("app/(marketing)/about/page.tsx")),
            Some("/about".to_string())
        );
        assert_eq!(
            next_app_router_route(Path::new("app/api/login/route.ts")),
            Some("/api/login".to_string())
        );
        assert_eq!(next_app_router_route(Path::new("src/app/page.tsx")), Some("/".to_string()));
        assert_eq!(next_app_router_route(Path::new("src/components/page-header.tsx")), None);
        assert_eq!(next_app_router_route(Path::new("lib/app/page.tsx")), None);
    }

    fn routes_of(r: &ExtractionResult) -> Vec<(String, Option<String>)> {
        r.entry_points
            .iter()
            .map(|e| (e.route.clone(), e.handler.clone()))
            .collect()
    }

    #[test]
    fn hono_koa_and_adonis_registrations() {
        let r = run(
            "src/server.ts",
            r#"
import { Hono } from 'hono';
const app = new Hono();
export function listUsers(c) { return c.json([]); }
app.get('/users', listUsers);
app.post('/users', (c) => c.text('inline'));
router.delete('/sessions/:id', destroySession);
Route.get('/admin', 'AdminController.index');
"#,
        );
        assert_eq!(
            routes_of(&r),
            vec![
                ("/users".to_string(), Some("listUsers".to_string())),
                ("/users".to_string(), None), // inline closure: no mis-binding
                ("/sessions/:id".to_string(), Some("destroySession".to_string())),
                ("/admin".to_string(), Some("AdminController.index".to_string())),
            ]
        );
    }

    #[test]
    fn remix_flat_file_routes_bind_loader_and_action() {
        let r = run(
            "app/routes/posts.$id.tsx",
            "export async function loader() { return null; }\nexport async function action() { return null; }\nexport default function Post() { return null; }\n",
        );
        assert_eq!(
            routes_of(&r),
            vec![
                ("/posts/:id".to_string(), Some("loader".to_string())),
                ("/posts/:id".to_string(), Some("action".to_string())),
            ]
        );

        let index = run(
            "app/routes/_index.tsx",
            "export default function Home() { return null; }\n",
        );
        assert_eq!(
            routes_of(&index),
            vec![("/".to_string(), Some("default".to_string()))]
        );
    }
}
