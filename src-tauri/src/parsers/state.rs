//! State-management detection shared by every language extractor.
//!
//! Mirrors `routes.rs`: a line scanner rather than an AST pass, so all
//! extractors can share one implementation regardless of whether they are
//! AST-based (JS/TS) or hand-rolled.
//!
//! Every rule is **gated on the framework actually being referenced in the
//! file**. Factory names like `create`, `atom` and `observable` are far too
//! generic to match blind — without the gate a `create()` helper in unrelated
//! code would be reported as a Zustand store.

/// A detected state container: `name` is the binding it was assigned to.
#[derive(Debug, Clone, PartialEq)]
pub struct StateStore {
    pub name: String,
    pub line: usize,
}

pub(crate) fn ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

/// Byte offset of `factory` used as a call (`factory(` or `factory<`), not as
/// a substring of a longer identifier.
fn find_call(line: &str, factory: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(rel) = line[from..].find(factory) {
        let at = from + rel;
        let prev_ok = line[..at].chars().next_back().is_none_or(|c| !ident_char(c));
        let next_ok = matches!(line[at + factory.len()..].chars().next(), Some('(') | Some('<'));
        if prev_ok && next_ok {
            return Some(at);
        }
        from = at + factory.len();
    }
    None
}

/// `export const useStore = create<T>()(...)` → `useStore`.
pub(crate) fn binding_for(line: &str, factory: &str) -> Option<String> {
    let call_at = find_call(line, factory)?;
    let lhs = line[..call_at].trim_end();
    let lhs = lhs.strip_suffix('=')?.trim_end();
    // Drop a type annotation (`const s: Store = ...`) before taking the name.
    let lhs = lhs.split(':').next().unwrap_or(lhs).trim_end();
    let name: String = lhs
        .chars()
        .rev()
        .take_while(|c| ident_char(*c))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    (!name.is_empty()).then_some(name)
}

fn push_unique(out: &mut Vec<StateStore>, name: String, line: usize) {
    if !out.iter().any(|s| s.name == name) {
        out.push(StateStore { name, line });
    }
}

/// Zustand, Redux, React Context, Pinia, Vuex, MobX, Recoil/Jotai.
pub fn scan_js_state(source: &str) -> Vec<StateStore> {
    let mentions = |needle: &str| source.contains(needle);

    // (factory, enabled) — gated on the library appearing in the file.
    let redux = mentions("redux") || mentions("@reduxjs/toolkit");
    let rules: [(&str, bool); 10] = [
        ("create", mentions("zustand")),
        ("createStore", mentions("zustand") || mentions("vuex") || redux),
        ("createSlice", redux),
        ("configureStore", redux),
        ("createContext", mentions("react")),
        ("defineStore", mentions("pinia")),
        ("makeAutoObservable", mentions("mobx")),
        ("observable", mentions("mobx")),
        ("atom", mentions("recoil") || mentions("jotai")),
        ("selector", mentions("recoil")),
    ];

    let mut out = Vec::new();
    for (idx, line) in source.lines().enumerate() {
        for (factory, enabled) in rules.iter() {
            if !enabled {
                continue;
            }
            if let Some(name) = binding_for(line, factory) {
                push_unique(&mut out, name, idx + 1);
            }
        }
    }
    out
}

/// Celery task workers plus module-level Celery/Redis singletons.
pub fn scan_python_state(source: &str) -> Vec<StateStore> {
    let mut out = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    for (idx, raw) in lines.iter().enumerate() {
        let line = raw.trim();

        // `@app.task` / `@shared_task` decorate the `def` that follows.
        let is_task = line.starts_with("@shared_task")
            || (line.starts_with('@') && line.contains(".task"));
        if is_task {
            for probe in lines.iter().skip(idx + 1).take(6) {
                let p = probe.trim_start();
                let after_def = p
                    .strip_prefix("async def ")
                    .or_else(|| p.strip_prefix("def "));
                if let Some(rest) = after_def {
                    let name: String = rest.chars().take_while(|c| ident_char(*c)).collect();
                    if !name.is_empty() {
                        push_unique(&mut out, name, idx + 1);
                    }
                    break;
                }
            }
            continue;
        }

        // `celery_app = Celery(...)`, `redis_client = Redis(...)`
        for factory in ["Celery", "Redis", "StrictRedis"] {
            if let Some(name) = binding_for(raw, factory) {
                push_unique(&mut out, name, idx + 1);
            }
        }
    }
    out
}

/// Tokio channels and process-wide statics.
pub fn scan_rust_state(source: &str) -> Vec<StateStore> {
    let mut out = Vec::new();
    for (idx, raw) in source.lines().enumerate() {
        let line = raw.trim();
        let line_no = idx + 1;

        // Comments and doc comments describe channels without declaring them.
        if line.starts_with("//") || line.starts_with("/*") || line.starts_with('*') {
            continue;
        }

        // `pub static PENDING_CHANGES: Mutex<...>` / `static CACHE: OnceLock<..>`
        let statics = line.strip_prefix("pub static ").or_else(|| line.strip_prefix("static "));
        if let Some(rest) = statics {
            let rest = rest.trim_start_matches("ref ");
            let name: String = rest.chars().take_while(|c| ident_char(*c)).collect();
            if !name.is_empty() && name.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                push_unique(&mut out, name, line_no);
                continue;
            }
        }

        // `let (tx, rx) = mpsc::channel(32);`
        if ["mpsc::channel", "broadcast::channel", "watch::channel", "mpsc::unbounded_channel"]
            .iter()
            .any(|c| line.contains(c))
        {
            if let Some(eq) = line.find('=') {
                // A string literal on the right means the channel call is
                // quoted sample text, not a real declaration.
                if line[eq + 1..].trim_start().starts_with(['"', '\'']) {
                    continue;
                }
                let lhs = line[..eq].trim().trim_start_matches("let ").trim();
                let lhs = lhs.trim_matches(|c| c == '(' || c == ')');
                // Destructured `(tx, rx)` declares two bindings, not one.
                for part in lhs.split(',') {
                    let name: String = part
                        .trim()
                        .trim_start_matches("mut ")
                        .chars()
                        .take_while(|c| ident_char(*c))
                        .collect();
                    if !name.is_empty() {
                        push_unique(&mut out, name, line_no);
                    }
                }
            }
        }
    }
    out
}

/// Go channels and package-level state vars.
pub fn scan_go_state(source: &str) -> Vec<StateStore> {
    let mut out = Vec::new();
    for (idx, raw) in source.lines().enumerate() {
        let line = raw.trim();
        let line_no = idx + 1;
        if line.starts_with("//") {
            continue;
        }

        // `var events = make(chan Event)` / `events := make(chan Event, 8)`
        if line.contains("make(chan ") || line.contains("make(chan\t") || line.contains("chan ") && line.contains(":=") {
            let split = line.find(":=").map(|i| (i, 2)).or_else(|| line.find('=').map(|i| (i, 1)));
            if let Some((at, _)) = split {
                if line[at..].contains("make(chan ") {
                    let lhs = line[..at].trim().trim_start_matches("var ").trim();
                    let name: String = lhs.chars().take_while(|c| ident_char(*c)).collect();
                    if !name.is_empty() {
                        push_unique(&mut out, name, line_no);
                    }
                }
            }
        }
    }
    out
}

/// Promote already-extracted symbols to `state_store`, preserving their real
/// AST line ranges. Only symbols the language extractor missed are appended,
/// so slices stay accurate instead of collapsing to a single line.
pub fn apply(stores: &[StateStore], symbols: &mut Vec<super::ExtractedSymbol>) {
    for store in stores {
        if let Some(existing) = symbols.iter_mut().find(|s| s.name == store.name) {
            existing.kind = "state_store".to_string();
        } else {
            symbols.push(super::ExtractedSymbol {
                name: store.name.clone(),
                kind: "state_store".to_string(),
                start_line: store.line.max(1),
                end_line: store.line.max(1),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(stores: &[StateStore]) -> Vec<&str> {
        stores.iter().map(|s| s.name.as_str()).collect()
    }

    #[test]
    fn zustand_redux_and_context_stores_are_detected() {
        let src = r#"
import { create } from 'zustand'
import { createSlice } from '@reduxjs/toolkit'
import { createContext } from 'react'

export const useGraphStore = create<GraphState>()((set) => ({ n: 0 }))
export const userSlice = createSlice({ name: 'user' })
const ThemeContext = createContext(null)
"#;
        let stores = scan_js_state(src);
        let found = names(&stores);
        assert!(found.contains(&"useGraphStore"), "got {found:?}");
        assert!(found.contains(&"userSlice"), "got {found:?}");
        assert!(found.contains(&"ThemeContext"), "got {found:?}");
    }

    #[test]
    fn pinia_and_vuex_stores_are_detected() {
        let src = "import { defineStore } from 'pinia'\nexport const useCart = defineStore('cart', {})\n";
        assert!(names(&scan_js_state(src)).contains(&"useCart"));
    }

    #[test]
    fn generic_factories_do_not_match_without_the_library() {
        // No zustand/recoil import → `create` and `atom` must not be claimed.
        let src = "const widget = create({ id: 1 })\nconst atomCount = atom(3)\n";
        assert!(scan_js_state(src).is_empty(), "false positives: {:?}", scan_js_state(src));
    }

    #[test]
    fn celery_tasks_and_singletons_are_detected() {
        let src = "from celery import Celery\n\napp = Celery('proj')\n\n@app.task\ndef send_email(to):\n    pass\n\n@shared_task\nasync def rebuild_index():\n    pass\n";
        let stores = scan_python_state(src);
        let found = names(&stores);
        assert!(found.contains(&"app"), "got {found:?}");
        assert!(found.contains(&"send_email"), "got {found:?}");
        assert!(found.contains(&"rebuild_index"), "got {found:?}");
    }

    #[test]
    fn rust_statics_and_channels_are_detected() {
        let src = "pub static PENDING_CHANGES: Mutex<Option<HashMap>> = Mutex::new(None);\nlet (tx, rx) = mpsc::channel::<Event>(32);\nlet local = compute();\n";
        let stores = scan_rust_state(src);
        let found = names(&stores);
        assert!(found.contains(&"PENDING_CHANGES"), "got {found:?}");
        // Destructured channel ends must be separate bindings, not "tx, rx".
        assert!(found.contains(&"tx"), "got {found:?}");
        assert!(found.contains(&"rx"), "got {found:?}");
        assert!(!found.contains(&"local"), "locals must not be stores: {found:?}");
    }

    /// Regression: scanning this very file previously reported a comment and a
    /// test string literal as Tokio channels.
    #[test]
    fn commented_or_quoted_channels_are_not_stores() {
        let src = "// let (tx, rx) = mpsc::channel(32);\nlet sample = \"let (a, b) = mpsc::channel(1);\";\n";
        let stores = scan_rust_state(src);
        assert!(stores.is_empty(), "false positives: {stores:?}");
    }

    #[test]
    fn go_channels_are_detected() {
        let src = "var events = make(chan Event)\nqueue := make(chan Job, 8)\nx := compute()\n";
        let stores = scan_go_state(src);
        let found = names(&stores);
        assert!(found.contains(&"events"), "got {found:?}");
        assert!(found.contains(&"queue"), "got {found:?}");
        assert!(!found.contains(&"x"), "got {found:?}");
    }

    #[test]
    fn apply_retags_existing_symbol_and_keeps_its_line_range() {
        let mut symbols = vec![super::super::ExtractedSymbol {
            name: "useGraphStore".to_string(),
            kind: "variable".to_string(),
            start_line: 10,
            end_line: 240,
        }];
        apply(&[StateStore { name: "useGraphStore".into(), line: 10 }], &mut symbols);
        assert_eq!(symbols[0].kind, "state_store");
        assert_eq!(symbols[0].end_line, 240, "real AST range must be preserved");
        assert_eq!(symbols.len(), 1, "must retag, not duplicate");
    }
}
