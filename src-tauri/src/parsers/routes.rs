//! Shared line-scan helpers for framework route extraction (Playbook §19).
//! Deliberately regex-free and allocation-light so full-repo indexing stays
//! sub-second: every helper is a single forward pass over one line.

use super::EntryPoint;

/// First quoted string literal (`"…"` or `'…'`) in `s`, plus the remainder
/// after the closing quote. Tolerates a leading `r`/`r#` raw prefix (Rust,
/// Tornado `r"..."` patterns).
pub fn quoted_arg(s: &str) -> Option<(String, &str)> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c == '"' || c == '\'' {
            let quote = c;
            let rest = &s[i + 1..];
            let end = rest.find(quote)?;
            return Some((rest[..end].to_string(), &rest[end + 1..]));
        }
        // Stop scanning at a closing paren before any quote: no literal arg.
        if c == ')' {
            return None;
        }
        i += 1;
    }
    None
}

/// First identifier chain after the next comma: `foo`, `Controller.method`,
/// `handlers::login`, `Handler`. Returns `None` for inline closures and
/// object/array literals (we'd rather miss than mis-bind).
pub fn handler_after_comma(rest: &str) -> Option<String> {
    let after = rest.split_once(',')?.1.trim_start();
    let first = after.chars().next()?;
    if !(first.is_alphabetic() || first == '_' || first == '$') {
        return None;
    }
    let ident: String = after
        .chars()
        .take_while(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | ':' | '$'))
        .collect();
    let after_ident = &after[ident.len()..];
    // `(x) =>`-style or `func(...)`-style inline values are not names; but
    // wrapper calls like `get(handler)` are handled by callers, and a plain
    // name followed by `)` or `,` is what we want.
    if after_ident.trim_start().starts_with("=>") {
        return None;
    }
    let cleaned = ident.trim_matches(|c: char| c == '.' || c == ':').to_string();
    // Filter obvious non-handlers.
    if cleaned.is_empty() || cleaned == "async" || cleaned == "function" || cleaned == "func" {
        return None;
    }
    Some(cleaned)
}

/// Quoted handler binding (`Route.get('/x', 'Controller.method')`).
pub fn quoted_handler_after_comma(rest: &str) -> Option<String> {
    let after = rest.split_once(',')?.1;
    quoted_arg(after).map(|(h, _)| h)
}

/// For `.route("/x", get(handler))` shapes: identifier inside the first
/// call-wrapper after the comma (`get(handler)` → `handler`).
pub fn wrapped_handler_after_comma(rest: &str) -> Option<String> {
    let after = rest.split_once(',')?.1;
    let open = after.find('(')?;
    let inner = &after[open + 1..];
    let ident: String = inner
        .trim_start()
        .chars()
        .take_while(|c| c.is_alphanumeric() || matches!(c, '_' | ':' | '.'))
        .collect();
    let cleaned = ident.trim_matches(|c: char| c == '.' || c == ':').to_string();
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

/// Scan one line for `<recv><method>(` where `recv` ends with `.`/`->` and
/// method is in `methods` (e.g. receiver "app"/"router"/any, methods
/// ["get", "post"]). Returns (path, rest-after-path) when the first
/// argument is a string literal.
pub fn method_call_route<'a>(
    line: &'a str,
    receivers: Option<&[&str]>,
    methods: &[&str],
) -> Option<(String, &'a str, String)> {
    for method in methods {
        let mut search_from = 0;
        while let Some(pos) = line[search_from..].find(&format!("{method}(")) {
            let abs = search_from + pos;
            // Must be a call on something: preceded by `.`, `->`, or a
            // receiver name at word boundary.
            let before = &line[..abs];
            let dotted = before.ends_with('.') || before.ends_with("->");
            let receiver_ok = match receivers {
                None => dotted,
                Some(rs) => {
                    dotted
                        && rs.iter().any(|r| {
                            let stripped =
                                before.trim_end_matches("->").trim_end_matches('.');
                            stripped.ends_with(r)
                                && !stripped
                                    [..stripped.len() - r.len()]
                                    .chars()
                                    .next_back()
                                    .is_some_and(|c| c.is_alphanumeric() || c == '_')
                        })
                }
            };
            if receiver_ok {
                let rest = &line[abs + method.len() + 1..];
                if let Some((path, after_path)) = quoted_arg(rest) {
                    if path.starts_with('/') || path.is_empty() {
                        return Some((path, after_path, (*method).to_string()));
                    }
                }
            }
            search_from = abs + method.len() + 1;
        }
    }
    None
}

/// Micronaut / Quarkus / JAX-RS annotation routing for Java and Kotlin
/// (§19.5): `@Controller("/base")` / `@Path("/base")` on the class sets a
/// prefix; `@Get("/x")`, `@Post`, `@Route("/x")`, `@GET` + `@Path("/x")`
/// on methods produce endpoints bound to the following method name.
pub fn scan_annotation_routes(source: &str, entry_points: &mut Vec<EntryPoint>) {
    const CLASS_ANNOTATIONS: &[&str] = &["@Controller", "@RequestMapping"];
    const VERB_ANNOTATIONS: &[&str] = &[
        "@Get", "@Post", "@Put", "@Delete", "@Patch", "@Head", "@Options", "@Route",
        "@GET", "@POST", "@PUT", "@DELETE", "@PATCH", "@HEAD", "@OPTIONS",
    ];

    let mut class_base = String::new();
    // Pending method annotations: (path-or-empty, is_verb, line).
    let mut pending: Vec<(String, bool, usize)> = Vec::new();

    for (idx, raw_line) in source.lines().enumerate() {
        let line_num = idx + 1;
        let line = raw_line.trim();
        if line.starts_with("//") || line.starts_with('*') || line.starts_with("/*") {
            continue;
        }

        if line.starts_with('@') {
            let path = quoted_arg(line).map(|(p, _)| p).unwrap_or_default();
            if CLASS_ANNOTATIONS.iter().any(|a| annotation_is(line, a)) {
                class_base = path;
            } else if VERB_ANNOTATIONS.iter().any(|a| annotation_is(line, a)) {
                pending.push((path, true, line_num));
            } else if annotation_is(line, "@Path") {
                // Class-level or method-level @Path — decided by what follows.
                pending.push((path, false, line_num));
            }
            continue;
        }

        let is_class_decl = line.split_whitespace().any(|w| w == "class");
        if is_class_decl {
            // A pending @Path directly above a class is the class base.
            if let Some((p, false, _)) = pending.iter().find(|(_, v, _)| !v).cloned() {
                if class_base.is_empty() {
                    class_base = p;
                }
            }
            pending.clear();
            continue;
        }

        if pending.is_empty() {
            continue;
        }

        if let Some(method) = method_decl_name(line) {
            let has_verb = pending.iter().any(|(_, v, _)| *v);
            if has_verb {
                let method_path = pending
                    .iter()
                    .map(|(p, _, _)| p.as_str())
                    .find(|p| !p.is_empty())
                    .unwrap_or("");
                let first_line = pending.first().map(|(_, _, l)| *l).unwrap_or(line_num);
                let route = format!("{class_base}{method_path}");
                if !route.is_empty() {
                    entry_points.push(entry(route, Some(method), first_line));
                }
            }
            pending.clear();
        } else if !line.is_empty() {
            pending.clear();
        }
    }
}

fn annotation_is(line: &str, name: &str) -> bool {
    line.strip_prefix(name).is_some_and(|rest| {
        rest.is_empty() || rest.starts_with('(') || rest.starts_with(char::is_whitespace)
    })
}

/// Method name from a Java (`public HttpResponse index(...)`) or Kotlin
/// (`fun index(...)`) declaration line.
fn method_decl_name(line: &str) -> Option<String> {
    let paren = line.find('(')?;
    let before = &line[..paren];
    if let Some(pos) = before.find("fun ") {
        let name: String = before[pos + 4..]
            .trim()
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        return if name.is_empty() { None } else { Some(name) };
    }
    let name = before.split_whitespace().next_back()?;
    let cleaned: String = name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    // Skip control-flow keywords that look like calls.
    if cleaned.is_empty() || matches!(cleaned.as_str(), "if" | "for" | "while" | "switch" | "return" | "catch" | "new") {
        None
    } else {
        Some(cleaned)
    }
}

/// For languages whose extractors don't build full symbol tables
/// (Java/Kotlin/PHP/C#): materialize a symbol for each bound handler so the
/// DB layer can attach the route's `references` edge to something real.
pub fn ensure_handler_symbols(
    entry_points: &[EntryPoint],
    symbols: &mut Vec<super::ExtractedSymbol>,
) {
    for ep in entry_points {
        let Some(h) = &ep.handler else { continue };
        let name = h.rsplit(['.', ':']).next().unwrap_or(h);
        if name.is_empty() || symbols.iter().any(|s| s.name == name) {
            continue;
        }
        symbols.push(super::ExtractedSymbol {
            name: name.to_string(),
            kind: "function".to_string(),
            start_line: ep.line.max(1),
            end_line: ep.line.max(1),
        });
    }
}

/// Convenience constructor.
pub fn entry(route: impl Into<String>, handler: Option<String>, line: usize) -> EntryPoint {
    EntryPoint {
        route: route.into(),
        handler,
        line,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoted_arg_handles_both_quote_styles_and_raw() {
        assert_eq!(quoted_arg(r#""/users", handler)"#).unwrap().0, "/users");
        assert_eq!(quoted_arg("'/items')").unwrap().0, "/items");
        assert_eq!(quoted_arg(r#"r"/regex/(\d+)", H)"#).unwrap().0, "/regex/(\\d+)");
        assert!(quoted_arg("handler)").is_none());
    }

    #[test]
    fn handler_extraction_variants() {
        assert_eq!(handler_after_comma(", listUsers)").as_deref(), Some("listUsers"));
        assert_eq!(
            handler_after_comma(", handlers.createUser)").as_deref(),
            Some("handlers.createUser")
        );
        assert_eq!(handler_after_comma(", (c) => c.text('hi'))"), None);
        assert_eq!(
            quoted_handler_after_comma(", 'UsersController.index')").as_deref(),
            Some("UsersController.index")
        );
        assert_eq!(
            wrapped_handler_after_comma(", get(list_users))").as_deref(),
            Some("list_users")
        );
    }

    #[test]
    fn method_call_route_respects_receivers() {
        let (path, _, m) =
            method_call_route("app.get(\"/users\", listUsers)", Some(&["app", "router"]), &["get"])
                .unwrap();
        assert_eq!(path, "/users");
        assert_eq!(m, "get");
        assert!(method_call_route("widget.get(\"/x\")", Some(&["app"]), &["get"]).is_none());
        // Any-receiver mode still requires a call site, not a bare word.
        assert!(method_call_route("get(\"/x\")", None, &["get"]).is_none());
        assert!(method_call_route("e.GET(\"/x\", h)", None, &["GET"]).is_some());
    }
}
