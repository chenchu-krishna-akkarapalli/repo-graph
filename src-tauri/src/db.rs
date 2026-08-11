use rusqlite::{params, Connection, Result};
use std::path::Path;
use crate::graph::{ParsedFile, Node, Edge, Warning, EdgeKind, Graph};
use serde_json;

pub fn init_db(db_path: &Path) -> Result<Connection> {
    let conn = Connection::open(db_path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;

    let has_exports = conn.prepare("SELECT exports FROM files LIMIT 1").is_ok();
    if !has_exports {
        let _ = conn.execute("DROP TABLE IF EXISTS edges", []);
        let _ = conn.execute("DROP TABLE IF EXISTS file_edges", []);
        let _ = conn.execute("DROP TABLE IF EXISTS external_dependencies", []);
        let _ = conn.execute("DROP TABLE IF EXISTS warnings", []);
        let _ = conn.execute("DROP TABLE IF EXISTS symbols", []);
        let _ = conn.execute("DROP TABLE IF EXISTS files", []);
    }

    conn.execute(
        "CREATE TABLE IF NOT EXISTS files (
            path TEXT PRIMARY KEY,
            size_bytes INTEGER NOT NULL,
            language TEXT NOT NULL,
            mtime INTEGER NOT NULL DEFAULT 0,
            exports TEXT NOT NULL DEFAULT '[]',
            routes TEXT NOT NULL DEFAULT '[]'
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS symbols (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            file_path TEXT NOT NULL,
            name TEXT NOT NULL,
            kind TEXT NOT NULL,
            start_line INTEGER NOT NULL,
            end_line INTEGER NOT NULL,
            FOREIGN KEY(file_path) REFERENCES files(path) ON DELETE CASCADE
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS edges (
            from_symbol_id INTEGER NOT NULL,
            to_symbol_id INTEGER NOT NULL,
            kind TEXT NOT NULL,
            provenance TEXT NOT NULL,
            wiring_site TEXT,
            FOREIGN KEY(from_symbol_id) REFERENCES symbols(id) ON DELETE CASCADE,
            FOREIGN KEY(to_symbol_id) REFERENCES symbols(id) ON DELETE CASCADE,
            PRIMARY KEY (from_symbol_id, to_symbol_id, kind)
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS file_edges (
            from_path TEXT NOT NULL,
            to_path TEXT NOT NULL,
            kind TEXT NOT NULL,
            PRIMARY KEY (from_path, to_path, kind),
            FOREIGN KEY(from_path) REFERENCES files(path) ON DELETE CASCADE,
            FOREIGN KEY(to_path) REFERENCES files(path) ON DELETE CASCADE
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS external_dependencies (
            file_path TEXT NOT NULL,
            name TEXT NOT NULL,
            PRIMARY KEY (file_path, name),
            FOREIGN KEY(file_path) REFERENCES files(path) ON DELETE CASCADE
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS warnings (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            path TEXT NOT NULL,
            kind TEXT NOT NULL,
            FOREIGN KEY(path) REFERENCES files(path) ON DELETE CASCADE
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS agent_queries (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            symbol TEXT NOT NULL,
            path TEXT NOT NULL,
            action TEXT NOT NULL,
            timestamp INTEGER NOT NULL
        )",
        [],
    )?;

    // Files edited but not yet re-indexed. This lives in the database rather
    // than a process-local static because the watcher runs inside the Tauri
    // app while the freshness warning is served by the standalone `mcp_server`
    // process — which is exactly where it matters. When it was a `static`, the
    // MCP server saw an always-empty map and reported "Synced" unconditionally.
    conn.execute(
        "CREATE TABLE IF NOT EXISTS pending_changes (
            path TEXT PRIMARY KEY,
            marked_at INTEGER NOT NULL
        )",
        [],
    )?;

    // `tokens` holds the sub-words of `name` (see `tokenize_symbol_name`) so a
    // query for "store" reaches `useGraphStore`. FTS5 matches token prefixes,
    // never substrings, so the split has to happen at write time.
    //
    // The column was added after the table shipped, and FTS5 has no ALTER: a
    // 3-column table is a stale schema and gets rebuilt. Safe to drop — it is
    // derived data, repopulated by the next `populate_db`.
    if fts_lacks_tokens_column(&conn) {
        let _ = conn.execute("DROP TABLE IF EXISTS symbols_fts", []);
    }
    let _ = conn.execute(
        "CREATE VIRTUAL TABLE IF NOT EXISTS symbols_fts USING fts5(name, file_path, content, tokens)",
        [],
    );

    conn.execute(
        "CREATE TRIGGER IF NOT EXISTS symbols_after_delete AFTER DELETE ON symbols BEGIN
            DELETE FROM symbols_fts WHERE name = old.name AND file_path = old.file_path;
        END;",
        [],
    )?;

    // Incremental re-index deletes a file's rows on every save; without these
    // each delete degrades into a full table scan as the graph grows.
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_symbols_file_path ON symbols(file_path)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_edges_to_symbol ON edges(to_symbol_id)",
        [],
    )?;

    Ok(conn)
}

/// True when `symbols_fts` exists but predates the `tokens` column.
fn fts_lacks_tokens_column(conn: &Connection) -> bool {
    let exists: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'symbols_fts'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);
    if !exists {
        return false;
    }
    // `PRAGMA table_info` works on FTS5 virtual tables and lists its columns.
    let mut stmt = match conn.prepare("PRAGMA table_info(symbols_fts)") {
        Ok(s) => s,
        Err(_) => return false,
    };
    let has_tokens = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map(|rows| rows.filter_map(|r| r.ok()).any(|c| c == "tokens"))
        .unwrap_or(false);
    !has_tokens
}

/// Sub-words of an identifier, so prefix-only FTS5 can answer mid-name queries.
///
/// `useGraphStore` → `["useGraphStore", "use", "Graph", "Store"]`
/// `user_controller` → `["user_controller", "user", "controller"]`
/// `HTTPServer` → `["HTTPServer", "HTTP", "Server"]` (acronym boundary)
///
/// The full name stays first so an exact query still ranks it.
pub fn tokenize_symbol_name(name: &str) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = name.chars().collect();

    for (i, &c) in chars.iter().enumerate() {
        if c == '_' || c == '-' || c == '.' || c == '$' || c == ' ' {
            if !current.is_empty() {
                parts.push(std::mem::take(&mut current));
            }
            continue;
        }
        // Split before an uppercase letter that starts a new word: either the
        // previous char is lowercase/digit (`useGraph`), or this is the last
        // capital of an acronym run followed by a lowercase (`HTTPServer`).
        let prev_lower = i > 0 && (chars[i - 1].is_lowercase() || chars[i - 1].is_ascii_digit());
        let acronym_end = i > 0
            && chars[i - 1].is_uppercase()
            && chars.get(i + 1).is_some_and(|n| n.is_lowercase());
        if c.is_uppercase() && (prev_lower || acronym_end) && !current.is_empty() {
            parts.push(std::mem::take(&mut current));
        }
        current.push(c);
    }
    if !current.is_empty() {
        parts.push(current);
    }

    let mut out = vec![name.to_string()];
    for p in parts {
        // Single characters are pure noise in an FTS index.
        if p.len() > 1 && !out.iter().any(|e| e.eq_ignore_ascii_case(&p)) {
            out.push(p);
        }
    }
    out
}

/// FTS5 MATCH expression for a user query: every term becomes a prefix match,
/// OR-joined so `search("auth service")` still finds `AuthService`.
pub fn build_fts_match_query(query: &str) -> String {
    let cleaned = query.replace(|c: char| !c.is_alphanumeric() && c != '_', " ");
    let mut terms: Vec<String> = Vec::new();
    for raw in cleaned.split_whitespace() {
        // Sub-words of the query itself, so `search("useGraphStore")` also
        // matches on `Graph` when the exact name is not indexed.
        for t in tokenize_symbol_name(raw) {
            let t = t.trim_matches('_');
            if t.len() > 1 && !terms.iter().any(|e| e.eq_ignore_ascii_case(t)) {
                terms.push(format!("\"{}\"*", t.replace('"', "")));
            }
        }
    }
    if terms.is_empty() {
        return String::new();
    }
    terms.join(" OR ")
}

/// `symbol_map` is keyed file-path-first so lookups borrow both keys —
/// no per-probe `(String, String)` tuple allocation on 10k+ file indexes.
fn lookup_symbol(
    symbol_map: &std::collections::HashMap<String, std::collections::HashMap<String, i64>>,
    file_path: &str,
    symbol_name: &str,
) -> Option<i64> {
    symbol_map.get(file_path).and_then(|inner| inner.get(symbol_name)).copied()
}

fn get_enclosing_symbol_for_file(
    file_symbols: &std::collections::HashMap<String, Vec<(i64, i64, i64)>>,
    symbol_map: &std::collections::HashMap<String, std::collections::HashMap<String, i64>>,
    path: &str,
    line: usize,
) -> i64 {
    if let Some(syms) = file_symbols.get(path) {
        let mut best_id = None;
        let mut best_span = i64::MAX;
        for &(id, start, end) in syms {
            let line_i = line as i64;
            if line_i >= start && line_i <= end {
                let span = end - start;
                if span < best_span {
                    best_span = span;
                    best_id = Some(id);
                }
            }
        }
        if let Some(id) = best_id {
            return id;
        }
    }
    lookup_symbol(symbol_map, path, path).unwrap_or(0)
}

pub fn populate_db(conn: &mut Connection, parsed_files: &[ParsedFile], root: &Path) -> Result<()> {
    let tx = conn.transaction()?;

    tx.execute("DELETE FROM files", [])?;
    tx.execute("DELETE FROM file_edges", [])?;
    tx.execute("DELETE FROM external_dependencies", [])?;
    tx.execute("DELETE FROM warnings", [])?;
    tx.execute("DELETE FROM symbols_fts", [])?;

    let aliases = crate::graph::load_tsconfig_aliases(root);
    let file_set: std::collections::BTreeSet<String> = parsed_files.iter().map(|p| p.entry.path.clone()).collect();

    for pf in parsed_files {
        let abs_path = root.join(&pf.entry.path);
        let mtime = std::fs::metadata(&abs_path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let exports_json = serde_json::to_string(&pf.extraction.exports).unwrap_or_else(|_| "[]".to_string());
        let mut routes = Vec::new();
        for ep in &pf.extraction.entry_points {
            if !routes.contains(&ep.route) {
                routes.push(ep.route.clone());
            }
        }
        let routes_json = serde_json::to_string(&routes).unwrap_or_else(|_| "[]".to_string());

        tx.execute(
            "INSERT OR REPLACE INTO files (path, size_bytes, language, mtime, exports, routes) VALUES (?, ?, ?, ?, ?, ?)",
            params![pf.entry.path, pf.entry.size_bytes as i64, pf.entry.language, mtime, exports_json, routes_json],
        )?;

        for warn in &pf.extraction.warnings {
            tx.execute(
                "INSERT INTO warnings (path, kind) VALUES (?, ?)",
                params![pf.entry.path, warn],
            )?;
        }

        tx.execute(
            "INSERT INTO symbols (file_path, name, kind, start_line, end_line) VALUES (?, ?, ?, ?, ?)",
            params![pf.entry.path, pf.entry.path, "file", 1, 999999],
        )?;

        // Must be the absolute path: `pf.entry.path` is repo-relative, so a
        // bare read resolves against the process CWD. An MCP server spawned by
        // a host almost never runs from the repo root, so every read failed and
        // silently wrote an empty `symbols_fts.content` for the whole index.
        //
        // Content is only consumed by the per-symbol FTS slices below, so
        // symbol-less files (metadata, assets) skip the disk read entirely.
        let lines: Vec<String> = if pf.extraction.symbols.is_empty() {
            Vec::new()
        } else {
            std::fs::read_to_string(&abs_path)
                .map(|c| c.lines().map(|s| s.to_string()).collect())
                .unwrap_or_default()
        };

        for sym in &pf.extraction.symbols {
            tx.execute(
                "INSERT INTO symbols (file_path, name, kind, start_line, end_line) VALUES (?, ?, ?, ?, ?)",
                params![pf.entry.path, sym.name, sym.kind.to_lowercase(), sym.start_line, sym.end_line],
            )?;

            let start = sym.start_line.max(1);
            let end = sym.end_line.min(lines.len());
            let sym_content = if start <= end && start <= lines.len() {
                lines[start - 1..end].join("\n")
            } else {
                String::new()
            };

            tx.execute(
                "INSERT INTO symbols_fts (name, file_path, content, tokens) VALUES (?, ?, ?, ?)",
                params![
                    sym.name,
                    pf.entry.path,
                    sym_content,
                    tokenize_symbol_name(&sym.name).join(" ")
                ],
            )?;
        }
    }

    let mut stmt = tx.prepare("SELECT id, file_path, name, start_line, end_line FROM symbols")?;
    // File-path-first nesting: resolution loops below probe this millions of
    // times on large repos, and borrowed-key gets beat tuple-key clones.
    let mut symbol_map: std::collections::HashMap<String, std::collections::HashMap<String, i64>> =
        std::collections::HashMap::new();
    let mut file_symbols = std::collections::HashMap::new();

    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, i64>(4)?,
        ))
    })?;

    for (id, file_path, name, start, end) in rows.flatten() {
        symbol_map.entry(file_path.clone()).or_default().insert(name, id);
        file_symbols.entry(file_path).or_insert_with(Vec::new).push((id, start, end));
    }
    drop(stmt);

    let mut import_map = std::collections::HashMap::new();
    for pf in parsed_files {
        let mut file_imports = Vec::new();
        for imp in &pf.extraction.imports {
            let resolved = imp.resolved_path.as_deref().and_then(|stem| {
                let mut stems = vec![stem.to_string()];
                let mapped = crate::graph::resolve_tsconfig_alias(&aliases, stem);
                stems.extend(mapped);

                stems.iter().find_map(|s| {
                    crate::graph::resolve_candidates(&pf.entry.language, s)
                        .into_iter()
                        .find(|c| file_set.contains(c))
                })
            });

            match resolved {
                Some(target) => {
                    if target != pf.entry.path {
                        file_imports.push(target.clone());
                        let kind_str = match imp.kind {
                            EdgeKind::Import => "imports",
                            EdgeKind::Require => "require",
                            EdgeKind::Mod => "contains",
                            EdgeKind::Use => "references",
                            EdgeKind::Route => "route",
                        };
                        tx.execute(
                            "INSERT OR IGNORE INTO file_edges (from_path, to_path, kind) VALUES (?, ?, ?)",
                            params![pf.entry.path, target, kind_str],
                        )?;
                    }
                }
                None if imp.external => {
                    let pkg_name = crate::graph::external_package_name(&pf.entry.language, &imp.raw_specifier);
                    tx.execute(
                        "INSERT OR IGNORE INTO external_dependencies (file_path, name) VALUES (?, ?)",
                        params![pf.entry.path, pkg_name],
                    )?;
                }
                None => {
                    if imp.kind != EdgeKind::Use {
                        tx.execute(
                            "INSERT INTO warnings (path, kind) VALUES (?, ?)",
                            params![pf.entry.path, format!("unresolved_import:{}", imp.raw_specifier)],
                        )?;
                    }
                }
            }
        }
        import_map.insert(pf.entry.path.clone(), file_imports);
    }

    // Component symbols per file, so the JSX-child heuristic below is a hash
    // lookup rather than a scan of every parsed file per reference.
    let component_names: std::collections::HashMap<&str, std::collections::HashSet<&str>> =
        parsed_files
            .iter()
            .map(|p| {
                let set = p
                    .extraction
                    .symbols
                    .iter()
                    .filter(|s| s.kind == "component")
                    .map(|s| s.name.as_str())
                    .collect();
                (p.entry.path.as_str(), set)
            })
            .collect();

    for pf in parsed_files {
        let path = &pf.entry.path;

        let get_enclosing_symbol = |line: usize| -> i64 {
            if let Some(syms) = file_symbols.get(path) {
                let mut best_id = None;
                let mut best_span = i64::MAX;
                for &(id, start, end) in syms {
                    let line_i = line as i64;
                    if line_i >= start && line_i <= end {
                        let span = end - start;
                        if span < best_span {
                            best_span = span;
                            best_id = Some(id);
                        }
                    }
                }
                if let Some(id) = best_id {
                    return id;
                }
            }
            lookup_symbol(&symbol_map, path, path).unwrap_or(0)
        };

        for ref_item in &pf.extraction.references {
            let from_id = get_enclosing_symbol(ref_item.line);
            if from_id == 0 {
                continue;
            }

            if ref_item.name.starts_with("set") && ref_item.name.len() > 3 {
                if let Some(c) = ref_item.name.chars().nth(3) {
                    if c.is_ascii_uppercase() {
                        let _ = tx.execute(
                            "INSERT OR IGNORE INTO edges (from_symbol_id, to_symbol_id, kind, provenance, wiring_site) VALUES (?, ?, ?, ?, ?)",
                            params![from_id, from_id, "references", "heuristic", "React State Render Sync"],
                        );
                    }
                }
            }

            if let Some(to_id) = lookup_symbol(&symbol_map, path, &ref_item.name) {
                if from_id != to_id {
                    let mut is_comp = false;
                    if let Some(sym) = pf.extraction.symbols.iter().find(|s| s.name == ref_item.name) {
                        if sym.kind == "component" {
                            is_comp = true;
                        }
                    }
                    let prov = if is_comp { "heuristic" } else { "ast" };
                    let site = if is_comp { Some("JSX Component Child") } else { None };

                    let _ = tx.execute(
                        "INSERT OR IGNORE INTO edges (from_symbol_id, to_symbol_id, kind, provenance, wiring_site) VALUES (?, ?, ?, ?, ?)",
                        params![from_id, to_id, "references", prov, site],
                    );
                }
                continue;
            }

            if let Some(targets) = import_map.get(path) {
                for target in targets {
                    if let Some(to_id) = lookup_symbol(&symbol_map, target, &ref_item.name) {
                        // Was `parsed_files.iter().find(...)` — a linear scan
                        // of every file in the repo, run once per resolved
                        // reference, i.e. O(references × files).
                        let is_comp = component_names
                            .get(target.as_str())
                            .is_some_and(|set| set.contains(ref_item.name.as_str()));

                        let kind = if is_comp { "references" } else { "calls" };
                        let prov = if is_comp { "heuristic" } else { "ast" };
                        let site = if is_comp { Some("JSX Component Child") } else { None };

                        let _ = tx.execute(
                            "INSERT OR IGNORE INTO edges (from_symbol_id, to_symbol_id, kind, provenance, wiring_site) VALUES (?, ?, ?, ?, ?)",
                            params![from_id, to_id, kind, prov, site],
                        );
                        break;
                    }
                }
            }
        }

        for ep in &pf.extraction.entry_points {
            // Route symbol: select-first so repeated registrations of the
            // same URL in one file dedupe (symbols has no unique index).
            let route_line = if ep.line > 0 { ep.line as i64 } else { 1 };
            let mut stmt = tx.prepare(
                "SELECT id FROM symbols WHERE file_path = ? AND name = ? AND kind = 'route' LIMIT 1",
            )?;
            let existing = stmt
                .query_row(params![pf.entry.path, ep.route], |r| r.get::<_, i64>(0))
                .ok();
            drop(stmt);
            let route_id = match existing {
                Some(id) => id,
                None => {
                    tx.execute(
                        "INSERT INTO symbols (file_path, name, kind, start_line, end_line) VALUES (?, ?, ?, ?, ?)",
                        params![pf.entry.path, ep.route, "route", route_line, route_line],
                    )?;
                    tx.last_insert_rowid()
                }
            };
            if route_id <= 0 {
                continue;
            }

            // Handler binding (Â§19): prefer the scanner's explicit handler
            // name â€” try it verbatim, then its last and first dot segments
            // (`UsersController.index` â†’ method, then controller class) in
            // this file first, then across resolved imports.
            let mut handler_id: Option<i64> = None;
            if let Some(h) = &ep.handler {
                let mut candidates: Vec<String> = vec![h.clone()];
                if let Some(last) = h.rsplit(['.', ':']).next() {
                    if last != h {
                        candidates.push(last.to_string());
                    }
                }
                if let Some(first) = h.split(['.', ':']).next() {
                    if first != h && !first.is_empty() {
                        candidates.push(first.to_string());
                    }
                }
                'outer: for cand in &candidates {
                    if let Some(id) = lookup_symbol(&symbol_map, &pf.entry.path, cand) {
                        handler_id = Some(id);
                        break;
                    }
                    if let Some(targets) = import_map.get(&pf.entry.path) {
                        for target in targets {
                            if let Some(id) = lookup_symbol(&symbol_map, target, cand) {
                                handler_id = Some(id);
                                break 'outer;
                            }
                        }
                    }
                }
            }

            // Fallback for file-based routes with no named handler.
            if handler_id.is_none() {
                if pf.entry.language == "javascript" {
                    if let Some(id) = lookup_symbol(&symbol_map, &pf.entry.path, "default") {
                        handler_id = Some(id);
                    } else {
                        for sym in &pf.extraction.symbols {
                            if sym.kind == "component" || sym.kind == "function" {
                                if let Some(id) = lookup_symbol(&symbol_map, &pf.entry.path, &sym.name) {
                                    handler_id = Some(id);
                                    break;
                                }
                            }
                        }
                    }
                } else if pf.entry.language == "python" {
                    for sym in &pf.extraction.symbols {
                        if sym.kind == "function" || sym.kind == "method" {
                            if let Some(id) = lookup_symbol(&symbol_map, &pf.entry.path, &sym.name) {
                                handler_id = Some(id);
                                break;
                            }
                        }
                    }
                }
            }

            if let Some(h_id) = handler_id {
                let _ = tx.execute(
                    "INSERT OR IGNORE INTO edges (from_symbol_id, to_symbol_id, kind, provenance, wiring_site) VALUES (?, ?, ?, ?, ?)",
                    params![route_id, h_id, "references", "ast", None::<String>],
                );
            }
        }

        for rel in &pf.extraction.inheritance {
            if let Some(class_id) = lookup_symbol(&symbol_map, path, &rel.class_name) {
                let mut resolved_parent_id = None;
                if let Some(to_id) = lookup_symbol(&symbol_map, path, &rel.parent_name) {
                    resolved_parent_id = Some(to_id);
                } else if let Some(targets) = import_map.get(path) {
                    for target in targets {
                        if let Some(to_id) = lookup_symbol(&symbol_map, target, &rel.parent_name) {
                            resolved_parent_id = Some(to_id);
                            break;
                        }
                    }
                }
                
                if let Some(parent_id) = resolved_parent_id {
                    let kind = if rel.is_implements { "implements" } else { "extends" };
                    let _ = tx.execute(
                        "INSERT OR IGNORE INTO edges (from_symbol_id, to_symbol_id, kind, provenance, wiring_site) VALUES (?, ?, ?, ?, ?)",
                        params![class_id, parent_id, kind, "ast", None::<String>],
                    );
                }
            }
        }

        let has_use_context = pf.extraction.references.iter().any(|r| r.name == "useContext");
        if has_use_context {
            for ref_item in &pf.extraction.references {
                if ref_item.name.ends_with("Context") {
                    let from_id = get_enclosing_symbol(ref_item.line);
                    if from_id == 0 {
                        continue;
                    }
                    if let Some(targets) = import_map.get(path) {
                        for target in targets {
                            if let Some(to_id) = lookup_symbol(&symbol_map, target, &ref_item.name) {
                                let _ = tx.execute(
                                    "INSERT OR IGNORE INTO edges (from_symbol_id, to_symbol_id, kind, provenance, wiring_site) VALUES (?, ?, ?, ?, ?)",
                                    params![from_id, to_id, "references", "heuristic", "useContext"],
                                );
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    // `.emit(` / `.on(` channel pairing. Two things used to make this the
    // most expensive step in the whole index: every file was re-read from
    // disk (already read during parsing), and the pairing was an all-pairs
    // `emitters × receivers` loop — quadratic on any JS codebase.
    let mut emitters = Vec::new();
    let mut receivers_by_channel: std::collections::HashMap<String, Vec<(String, String)>> =
        std::collections::HashMap::new();
    for pf in parsed_files {
        // Absolute path: `pf.entry.path` is repo-relative, so a bare read
        // resolves against the process CWD and silently found nothing.
        let content_opt = std::fs::read_to_string(root.join(&pf.entry.path)).ok();
        if let Some(content) = content_opt {
            for (idx, line) in content.lines().enumerate() {
                let line_num = idx + 1;
                if let Some(pos) = line.find(".emit(") {
                    let rest = &line[pos + 6..];
                    if let Some(quote) = rest.chars().next() {
                        if quote == '\'' || quote == '"' {
                            let rest = &rest[1..];
                            if let Some(end_pos) = rest.find(quote) {
                                let channel = &rest[..end_pos];
                                emitters.push((channel.to_string(), pf.entry.path.clone(), line_num));
                            }
                        }
                    }
                }
                if let Some(pos) = line.find(".on(") {
                    let rest = &line[pos + 4..];
                    if let Some(quote) = rest.chars().next() {
                        if quote == '\'' || quote == '"' {
                            let rest = &rest[1..];
                            if let Some(end_pos) = rest.find(quote) {
                                let channel = &rest[..end_pos];
                                let callback_rest = &rest[end_pos + 1..];
                                if let Some(comma_pos) = callback_rest.find(',') {
                                    let cb_part = &callback_rest[comma_pos + 1..];
                                    let cb_name = cb_part.trim()
                                        .trim_start_matches(|c: char| !c.is_alphanumeric() && c != '_')
                                        .split(|c: char| !c.is_alphanumeric() && c != '_')
                                        .next()
                                        .unwrap_or("")
                                        .to_string();
                                    if !cb_name.is_empty() {
                                        receivers_by_channel
                                            .entry(channel.to_string())
                                            .or_default()
                                            .push((pf.entry.path.clone(), cb_name));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    for (channel, emit_file, emit_line) in &emitters {
        // Hash lookup instead of a scan over every receiver in the repo.
        let Some(matches) = receivers_by_channel.get(channel) else { continue };
        let from_id = get_enclosing_symbol_for_file(&file_symbols, &symbol_map, emit_file, *emit_line);
        if from_id == 0 {
            continue;
        }
        for (rec_file, callback_name) in matches {
            let mut callback_id = lookup_symbol(&symbol_map, rec_file, callback_name);
            if callback_id.is_none() {
                if let Some(targets) = import_map.get(rec_file) {
                    for target in targets {
                        if let Some(id) = lookup_symbol(&symbol_map, target, callback_name) {
                            callback_id = Some(id);
                            break;
                        }
                    }
                }
            }
            if let Some(to_id) = callback_id {
                let label = format!("EventEmitter: {channel}");
                let _ = tx.execute(
                    "INSERT OR IGNORE INTO edges (from_symbol_id, to_symbol_id, kind, provenance, wiring_site) VALUES (?, ?, ?, ?, ?)",
                    params![from_id, to_id, "references", "heuristic", label],
                );
            }
        }
    }

    let mut dispatch_edges = Vec::new();
    {
        let mut stmt = tx.prepare(
            "SELECT e1.from_symbol_id, s_impl_method.id
             FROM edges e1
             JOIN symbols s_iface_method ON e1.to_symbol_id = s_iface_method.id
             JOIN symbols s_iface ON s_iface_method.file_path = s_iface.file_path AND s_iface.kind = 'interface'
             JOIN edges e_impl ON e_impl.to_symbol_id = s_iface.id AND e_impl.kind = 'implements'
             JOIN symbols s_class ON e_impl.from_symbol_id = s_class.id
             JOIN symbols s_impl_method ON s_class.file_path = s_impl_method.file_path AND s_impl_method.name = s_iface_method.name
             WHERE e1.kind = 'calls'"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })?;
        for (from, to) in rows.flatten() {
            dispatch_edges.push((from, to));
        }
    }
    for (from, to) in dispatch_edges {
        let _ = tx.execute(
            "INSERT OR IGNORE INTO edges (from_symbol_id, to_symbol_id, kind, provenance, wiring_site) VALUES (?, ?, ?, ?, ?)",
            params![from, to, "calls", "heuristic", "Interface -> Implementation Dispatch"],
        );
    }

    tx.commit()?;
    Ok(())
}

pub fn get_symbol_range(conn: &Connection, file_path: &str, name: &str) -> Result<Option<(usize, usize)>> {
    let mut stmt = conn.prepare("SELECT start_line, end_line FROM symbols WHERE file_path = ? AND name = ? LIMIT 1")?;
    let mut rows = stmt.query(params![file_path, name])?;
    if let Some(row) = rows.next()? {
        let start: i64 = row.get(0)?;
        let end: i64 = row.get(1)?;
        Ok(Some((start as usize, end as usize)))
    } else {
        Ok(None)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CallGraphNode {
    pub file_path: String,
    pub symbol_name: String,
    pub kind: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CallGraph {
    pub callers: Vec<CallGraphNode>,
    pub callees: Vec<CallGraphNode>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SymbolEdge {
    pub from_symbol: String,
    pub to_symbol: String,
    pub kind: String,
    pub provenance: String,
    pub wiring_site: Option<String>,
}

pub fn get_call_graph(
    conn: &Connection,
    file_path: &str,
    symbol_name: &str,
) -> Result<CallGraph> {
    use rusqlite::OptionalExtension;
    let mut stmt = conn.prepare("SELECT id FROM symbols WHERE file_path = ? AND name = ? LIMIT 1")?;
    let symbol_id: Option<i64> = stmt.query_row(params![file_path, symbol_name], |row| row.get(0)).optional()?;
    
    let mut callers = Vec::new();
    let mut callees = Vec::new();

    if let Some(sid) = symbol_id {
        let mut callers_stmt = conn.prepare(
            "SELECT s.file_path, s.name, s.kind
             FROM edges e
             JOIN symbols s ON e.from_symbol_id = s.id
             WHERE e.to_symbol_id = ?"
        )?;
        let caller_rows = callers_stmt.query_map(params![sid], |row| {
            Ok(CallGraphNode {
                file_path: row.get(0)?,
                symbol_name: row.get(1)?,
                kind: row.get(2)?,
            })
        })?;
        for node in caller_rows.flatten() {
            callers.push(node);
        }

        let mut callees_stmt = conn.prepare(
            "SELECT s.file_path, s.name, s.kind
             FROM edges e
             JOIN symbols s ON e.to_symbol_id = s.id
             WHERE e.from_symbol_id = ?"
        )?;
        let callee_rows = callees_stmt.query_map(params![sid], |row| {
            Ok(CallGraphNode {
                file_path: row.get(0)?,
                symbol_name: row.get(1)?,
                kind: row.get(2)?,
            })
        })?;
        for node in callee_rows.flatten() {
            callees.push(node);
        }
    }

    Ok(CallGraph { callers, callees })
}

pub fn get_all_symbol_edges(conn: &Connection) -> Result<Vec<SymbolEdge>> {
    let mut stmt = conn.prepare(
        "SELECT
            s1.file_path, s1.name,
            s2.file_path, s2.name,
            e.kind,
            e.provenance,
            e.wiring_site
         FROM edges e
         JOIN symbols s1 ON e.from_symbol_id = s1.id
         JOIN symbols s2 ON e.to_symbol_id = s2.id"
    )?;
    let rows = stmt.query_map([], |row| {
        let f_path: String = row.get(0)?;
        let f_name: String = row.get(1)?;
        let t_path: String = row.get(2)?;
        let t_name: String = row.get(3)?;
        let kind: String = row.get(4)?;
        let provenance: String = row.get(5)?;
        let wiring_site: Option<String> = row.get(6)?;
        Ok(SymbolEdge {
            from_symbol: format!("{f_path}#{f_name}"),
            to_symbol: format!("{t_path}#{t_name}"),
            kind,
            provenance,
            wiring_site,
        })
    })?;
    let mut edges = Vec::new();
    for e in rows.flatten() {
        edges.push(e);
    }
    Ok(edges)
}

pub fn get_all_symbols(conn: &Connection) -> Result<std::collections::HashMap<String, Vec<crate::parsers::ExtractedSymbol>>> {
    let mut stmt = conn.prepare("SELECT file_path, name, kind, start_line, end_line FROM symbols")?;
    let mut map = std::collections::HashMap::new();
    let rows = stmt.query_map([], |row| {
        let file_path: String = row.get(0)?;
        let name: String = row.get(1)?;
        let kind: String = row.get(2)?;
        let start_line: i64 = row.get(3)?;
        let end_line: i64 = row.get(4)?;
        Ok((file_path, crate::parsers::ExtractedSymbol {
            name,
            kind,
            start_line: start_line as usize,
            end_line: end_line as usize,
        }))
    })?;
    for (file_path, sym) in rows.flatten() {
        if sym.kind != "file" && sym.kind != "File" {
            map.entry(file_path).or_insert_with(Vec::new).push(sym);
        }
    }
    Ok(map)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SymbolSearchResult {
    pub name: String,
    pub file_path: String,
    pub content: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExploreSymbolBlock {
    pub name: String,
    pub kind: String,
    pub code: String,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExploreFilePayload {
    pub path: String,
    pub code_blocks: Vec<ExploreSymbolBlock>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExplorePathEdge {
    pub from_symbol: String,
    pub to_symbol: String,
    pub kind: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExplorePayload {
    pub files: Vec<ExploreFilePayload>,
    pub paths: Vec<ExplorePathEdge>,
}

pub fn reconcile_repo_startup(root: &Path) -> std::result::Result<(), String> {
    let db_path = root.join(".repograph/graph.db");
    if !db_path.exists() {
        return Ok(());
    }
    let conn = init_db(&db_path).map_err(|e| e.to_string())?;

    let mut stmt = conn.prepare("SELECT path, size_bytes, mtime FROM files").map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?))
    }).map_err(|e| e.to_string())?;

    let mut db_files = std::collections::HashMap::new();
    for (path, size, mtime) in rows.flatten() {
        db_files.insert(path, (size, mtime));
    }
    drop(stmt);

    let active_files = crate::walker::walk_repo(root);
    let mut files_to_reindex = Vec::new();

    for entry in active_files {
        let abs_path = root.join(&entry.path);
        let metadata = match std::fs::metadata(&abs_path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let current_size = metadata.len() as i64;
        let current_mtime = metadata.modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let needs_index = match db_files.get(&entry.path) {
            Some(&(db_size, db_mtime)) => current_size != db_size || current_mtime != db_mtime,
            None => true,
        };

        if needs_index {
            files_to_reindex.push(entry);
        }
    }

    if files_to_reindex.is_empty() {
        eprintln!("[reconcile] Index is up-to-date.");
        return Ok(());
    }

    // Deleted files first: they leave nodes, symbols and edges behind that no
    // amount of re-parsing removes.
    let on_disk: std::collections::HashSet<&String> =
        files_to_reindex.iter().map(|e| &e.path).collect();
    let deleted: Vec<String> = db_files
        .keys()
        .filter(|p| !on_disk.contains(p) && !root.join(p).is_file())
        .cloned()
        .collect();

    // A full walk + parse blocked the MCP handshake whenever a single file had
    // changed, because `McpServer::new` calls this. Above this share of the
    // repo a full index is genuinely cheaper than N single-file transactions.
    const FULL_REINDEX_FRACTION: f64 = 0.25;
    let changed = files_to_reindex.len() + deleted.len();
    let total = db_files.len().max(1);
    if changed as f64 / total as f64 > FULL_REINDEX_FRACTION {
        eprintln!("[reconcile] {changed}/{total} files changed — full re-index");
        let _ = crate::indexer::index_repo(root, None);
        return Ok(());
    }

    eprintln!("[reconcile] Catch-up indexing {changed} file(s) incrementally...");
    let aliases = crate::graph::load_tsconfig_aliases(root);
    let file_set: std::collections::BTreeSet<String> = db_files
        .keys()
        .cloned()
        .chain(files_to_reindex.iter().map(|e| e.path.clone()))
        .filter(|p| !deleted.contains(p))
        .collect();

    for path in &deleted {
        // Inbound edges must be rebuilt afterwards; see `inbound_dependents`.
        let dependents = inbound_dependents(&conn, path);
        let _ = conn.execute("DELETE FROM files WHERE path = ?", [path]);
        reindex_paths(&conn, root, &aliases, &file_set, dependents);
    }

    for entry in files_to_reindex {
        let dependents = inbound_dependents(&conn, &entry.path);
        let parsed = crate::indexer::parse_one(root, entry);
        if let Err(e) = populate_file_in_db(&conn, &parsed, root, &aliases, &file_set) {
            eprintln!("[reconcile] failed on {}: {e}", parsed.entry.path);
            continue;
        }
        reindex_paths(&conn, root, &aliases, &file_set, dependents);
    }

    // The desktop app and the browser dev bridge both read `graph.json`.
    if let Ok(graph) = load_graph_from_db(&conn) {
        let _ = graph.save_to_cache(root);
    }

    Ok(())
}

/// Re-parse `paths` so edges removed by a cascade are restored.
fn reindex_paths(
    conn: &Connection,
    root: &Path,
    aliases: &std::collections::HashMap<String, Vec<String>>,
    file_set: &std::collections::BTreeSet<String>,
    paths: Vec<String>,
) {
    for dep in paths {
        let abs = root.join(&dep);
        let Ok(source) = std::fs::read_to_string(&abs) else { continue };
        let entry = crate::walker::FileEntry {
            path: dep.clone(),
            size_bytes: source.len() as u64,
            extension: abs.extension().and_then(|s| s.to_str()).unwrap_or("").to_string(),
            language: crate::walker::language_for(&abs),
        };
        let parsed = crate::indexer::parse_one(root, entry);
        if let Err(e) = populate_file_in_db(conn, &parsed, root, aliases, file_set) {
            eprintln!("[reconcile] failed to restore edges from {dep}: {e}");
        }
    }
}

pub fn load_graph_from_db(conn: &Connection) -> Result<Graph> {
    let mut stmt = conn.prepare("SELECT path, size_bytes, language, exports, routes FROM files")?;
    let node_rows = stmt.query_map([], |row| {
        let path: String = row.get(0)?;
        let size_bytes: i64 = row.get(1)?;
        let language: String = row.get(2)?;
        let exports_json: String = row.get(3)?;
        let routes_json: String = row.get(4)?;
        
        let exports: Vec<String> = serde_json::from_str(&exports_json).unwrap_or_default();
        let routes: Vec<String> = serde_json::from_str(&routes_json).unwrap_or_default();
        
        Ok((path, size_bytes as u64, language, exports, routes))
    })?;
    
    let mut nodes_list = Vec::new();
    let mut symbols_map = get_all_symbols(conn)?;
    
    for (path, size_bytes, language, exports, routes) in node_rows.flatten() {
        let symbols = symbols_map.remove(&path).unwrap_or_default();
        nodes_list.push(Node {
            path,
            size_bytes,
            language,
            exports,
            routes,
            in_degree: 0,
            out_degree: 0,
            symbols,
            rank_score: 0.0,
            rank_order: 0,
        });
    }
    
    let mut stmt = conn.prepare("SELECT from_path, to_path, kind FROM file_edges")?;
    let edge_rows = stmt.query_map([], |row| {
        let from_path: String = row.get(0)?;
        let to_path: String = row.get(1)?;
        let kind_str: String = row.get(2)?;
        
        let kind = match kind_str.as_str() {
            "imports" | "import" => EdgeKind::Import,
            "require" => EdgeKind::Require,
            "contains" | "mod" => EdgeKind::Mod,
            "references" | "use" => EdgeKind::Use,
            "route" => EdgeKind::Route,
            _ => EdgeKind::Import,
        };
        
        Ok(Edge { from_path, to_path, kind })
    })?;
    
    let mut edges = Vec::new();
    let mut out_counts = std::collections::HashMap::new();
    let mut in_counts = std::collections::HashMap::new();
    
    for edge in edge_rows.flatten() {
        *out_counts.entry(edge.from_path.clone()).or_insert(0) += 1;
        *in_counts.entry(edge.to_path.clone()).or_insert(0) += 1;
        edges.push(edge);
    }
    
    for node in &mut nodes_list {
        node.out_degree = out_counts.get(&node.path).copied().unwrap_or(0);
        node.in_degree = in_counts.get(&node.path).copied().unwrap_or(0);
    }
    
    let mut stmt = conn.prepare("SELECT DISTINCT name FROM external_dependencies")?;
    let ext_rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut external_dependencies = Vec::new();
    for name in ext_rows.flatten() {
        external_dependencies.push(name);
    }
    
    let mut stmt = conn.prepare("SELECT path, kind FROM warnings")?;
    let warn_rows = stmt.query_map([], |row| {
        Ok(Warning {
            path: row.get(0)?,
            kind: row.get(1)?,
        })
    })?;
    let mut warnings = Vec::new();
    for w in warn_rows.flatten() {
        warnings.push(w);
    }
    
    let symbol_edges = get_all_symbol_edges(conn).unwrap_or_default();
    
    let mut graph = Graph {
        schema_version: 1,
        nodes: nodes_list,
        edges,
        external_dependencies,
        warnings,
        symbol_edges,
    };
    // The DB stores edges, not centrality — rank is derived, so it is computed
    // here rather than persisted and risked going stale after an incremental
    // re-index.
    crate::rank::compute_ranks(&mut graph);
    Ok(graph)
}

/// Files holding an edge that points **into** `rel_path`.
///
/// Re-indexing a file deletes its `files` row, and the `ON DELETE CASCADE`
/// chain (`files` → `symbols` → `edges`, plus `file_edges.to_path`) takes every
/// inbound edge with it — including edges owned by files that were never
/// touched. Only re-parsing those owners can restore them, so callers must
/// collect this list *before* the delete and re-index each entry afterwards.
pub fn inbound_dependents(conn: &Connection, rel_path: &str) -> Vec<String> {
    let Ok(mut stmt) = conn.prepare(
        "SELECT DISTINCT s_from.file_path
         FROM edges e
         JOIN symbols s_from ON e.from_symbol_id = s_from.id
         JOIN symbols s_to   ON e.to_symbol_id   = s_to.id
         WHERE s_to.file_path = ?1 AND s_from.file_path <> ?1
         UNION
         SELECT DISTINCT from_path FROM file_edges
         WHERE to_path = ?1 AND from_path <> ?1",
    ) else {
        return Vec::new();
    };
    let Ok(rows) = stmt.query_map(params![rel_path], |r| r.get::<_, String>(0)) else {
        return Vec::new();
    };
    rows.filter_map(|r| r.ok()).collect()
}

pub fn populate_file_in_db(
    conn: &Connection,
    pf: &ParsedFile,
    root: &Path,
    aliases: &std::collections::HashMap<String, Vec<String>>,
    file_set: &std::collections::BTreeSet<String>,
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    let path = &pf.entry.path;

    // Deleting the `files` row cascades to `symbols`, whose AFTER DELETE
    // trigger clears the matching `symbols_fts` rows â€” verified by
    // `repeated_incremental_reindex_does_not_grow_symbols_fts`.
    tx.execute("DELETE FROM files WHERE path = ?", [path])?;
    tx.execute("DELETE FROM file_edges WHERE from_path = ?", [path])?;
    tx.execute("DELETE FROM warnings WHERE path = ?", [path])?;
    tx.execute("DELETE FROM external_dependencies WHERE file_path = ?", [path])?;

    let abs_path = root.join(path);
    let mtime = std::fs::metadata(&abs_path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let exports_json = serde_json::to_string(&pf.extraction.exports).unwrap_or_else(|_| "[]".to_string());
    let mut routes = Vec::new();
    for ep in &pf.extraction.entry_points {
        if !routes.contains(&ep.route) {
            routes.push(ep.route.clone());
        }
    }
    let routes_json = serde_json::to_string(&routes).unwrap_or_else(|_| "[]".to_string());

    tx.execute(
        "INSERT INTO files (path, size_bytes, language, mtime, exports, routes) VALUES (?, ?, ?, ?, ?, ?)",
        params![path, pf.entry.size_bytes as i64, pf.entry.language, mtime, exports_json, routes_json],
    )?;

    tx.execute(
        "INSERT INTO symbols (file_path, name, kind, start_line, end_line) VALUES (?, ?, ?, ?, ?)",
        params![path, path, "file", 1, 999999],
    )?;

    let content_opt = std::fs::read_to_string(&abs_path).ok();
    let lines: Vec<String> = if let Some(ref c) = content_opt {
        c.lines().map(|s| s.to_string()).collect()
    } else {
        Vec::new()
    };

    let mut symbol_map = std::collections::HashMap::new();
    let mut file_symbols = Vec::new();

    let file_sym_id = tx.last_insert_rowid();
    symbol_map.insert(path.clone(), file_sym_id);

    for sym in &pf.extraction.symbols {
        tx.execute(
            "INSERT INTO symbols (file_path, name, kind, start_line, end_line) VALUES (?, ?, ?, ?, ?)",
            params![path, sym.name, sym.kind.to_lowercase(), sym.start_line, sym.end_line],
        )?;
        let sym_id = tx.last_insert_rowid();
        symbol_map.insert(sym.name.clone(), sym_id);
        file_symbols.push((sym_id, sym.start_line as i64, sym.end_line as i64));

        let start = sym.start_line.max(1);
        let end = sym.end_line.min(lines.len());
        let sym_content = if start <= end && start <= lines.len() {
            lines[start - 1..end].join("\n")
        } else {
            String::new()
        };
        tx.execute(
            "INSERT INTO symbols_fts (name, file_path, content, tokens) VALUES (?, ?, ?, ?)",
            params![
                sym.name,
                path,
                sym_content,
                tokenize_symbol_name(&sym.name).join(" ")
            ],
        )?;
    }

    let get_enclosing_symbol = |line: usize| -> i64 {
        let mut best_id = None;
        let mut best_span = i64::MAX;
        for &(id, start, end) in &file_symbols {
            let line_i = line as i64;
            if line_i >= start && line_i <= end {
                let span = end - start;
                if span < best_span {
                    best_span = span;
                    best_id = Some(id);
                }
            }
        }
        if let Some(id) = best_id {
            id
        } else {
            file_sym_id
        }
    };

    let mut resolved_imports = Vec::new();
    for import in &pf.extraction.imports {
        let resolved = import.resolved_path.as_deref().and_then(|stem| {
            let mut stems = vec![stem.to_string()];
            let mapped = crate::graph::resolve_tsconfig_alias(aliases, stem);
            stems.extend(mapped);

            stems.iter().find_map(|s| {
                crate::graph::resolve_candidates(&pf.entry.language, s)
                    .into_iter()
                    .find(|c| file_set.contains(c))
            })
        });

        match resolved {
            Some(target) => {
                if target != *path {
                    let kind_str = match import.kind {
                        EdgeKind::Import => "imports",
                        EdgeKind::Require => "require",
                        EdgeKind::Mod => "contains",
                        EdgeKind::Use => "references",
                        EdgeKind::Route => "route",
                    };
                    tx.execute(
                        "INSERT OR IGNORE INTO file_edges (from_path, to_path, kind) VALUES (?, ?, ?)",
                        params![path, target, kind_str],
                    )?;
                    resolved_imports.push(target);
                }
            }
            None if import.external => {
                let pkg_name = crate::graph::external_package_name(&pf.entry.language, &import.raw_specifier);
                tx.execute(
                    "INSERT OR IGNORE INTO external_dependencies (file_path, name) VALUES (?, ?)",
                    params![path, pkg_name],
                )?;
            }
            None => {
                if import.kind != EdgeKind::Use {
                    tx.execute(
                        "INSERT INTO warnings (path, kind) VALUES (?, ?)",
                        params![path, format!("unresolved_import:{}", import.raw_specifier)],
                    )?;
                }
            }
        }
    }

    for warn in &pf.extraction.warnings {
        tx.execute(
            "INSERT INTO warnings (path, kind) VALUES (?, ?)",
            params![path, warn],
        )?;
    }

    for ref_item in &pf.extraction.references {
        let from_id = get_enclosing_symbol(ref_item.line);
        if from_id == 0 {
            continue;
        }

        if ref_item.name.starts_with("set") && ref_item.name.len() > 3 {
            if let Some(c) = ref_item.name.chars().nth(3) {
                if c.is_ascii_uppercase() {
                    let _ = tx.execute(
                        "INSERT OR IGNORE INTO edges (from_symbol_id, to_symbol_id, kind, provenance, wiring_site) VALUES (?, ?, ?, ?, ?)",
                        params![from_id, from_id, "references", "heuristic", "React State Render Sync"],
                    );
                }
            }
        }

        if let Some(&to_id) = symbol_map.get(&ref_item.name) {
            if from_id != to_id {
                let mut is_comp = false;
                if let Some(sym) = pf.extraction.symbols.iter().find(|s| s.name == ref_item.name) {
                    if sym.kind == "component" {
                        is_comp = true;
                    }
                }
                let prov = if is_comp { "heuristic" } else { "ast" };
                let site = if is_comp { Some("JSX Component Child") } else { None };

                let _ = tx.execute(
                    "INSERT OR IGNORE INTO edges (from_symbol_id, to_symbol_id, kind, provenance, wiring_site) VALUES (?, ?, ?, ?, ?)",
                    params![from_id, to_id, "references", prov, site],
                );
            }
            continue;
        }

        // Prepared once outside the loop: this was recompiled per import, per
        // reference.
        let mut lookup = tx.prepare_cached(
            "SELECT id, kind FROM symbols WHERE file_path = ? AND name = ? LIMIT 1",
        )?;
        for target in &resolved_imports {
            let mut rows = lookup.query(params![target, ref_item.name])?;
            if let Some(row) = rows.next()? {
                let to_id: i64 = row.get(0)?;
                let sym_kind: String = row.get(1)?;
                
                let is_comp = sym_kind == "component";
                let kind = if is_comp { "references" } else { "calls" };
                let prov = if is_comp { "heuristic" } else { "ast" };
                let site = if is_comp { Some("JSX Component Child") } else { None };

                let _ = tx.execute(
                    "INSERT OR IGNORE INTO edges (from_symbol_id, to_symbol_id, kind, provenance, wiring_site) VALUES (?, ?, ?, ?, ?)",
                    params![from_id, to_id, kind, prov, site],
                );
                break;
            }
        }
    }

    for ep in &pf.extraction.entry_points {
        let route_line = if ep.line > 0 { ep.line as i64 } else { 1 };
        
        let mut stmt = tx.prepare(
            "SELECT id FROM symbols WHERE file_path = ? AND name = ? AND kind = 'route' LIMIT 1",
        )?;
        let existing = stmt
            .query_row(params![path, ep.route], |r| r.get::<_, i64>(0))
            .ok();
        drop(stmt);
        let route_id = match existing {
            Some(id) => id,
            None => {
                tx.execute(
                    "INSERT INTO symbols (file_path, name, kind, start_line, end_line) VALUES (?, ?, ?, ?, ?)",
                    params![path, ep.route, "route", route_line, route_line],
                )?;
                tx.last_insert_rowid()
            }
        };
        if route_id <= 0 {
            continue;
        }

        let mut handler_id: Option<i64> = None;
        if let Some(h) = &ep.handler {
            let mut candidates: Vec<String> = vec![h.clone()];
            if let Some(last) = h.rsplit(['.', ':']).next() {
                if last != h {
                    candidates.push(last.to_string());
                }
            }
            if let Some(first) = h.split(['.', ':']).next() {
                if first != h && !first.is_empty() {
                    candidates.push(first.to_string());
                }
            }
            'outer: for cand in &candidates {
                if let Some(&id) = symbol_map.get(cand) {
                    handler_id = Some(id);
                    break;
                }
                for target in &resolved_imports {
                    let mut stmt = tx.prepare_cached("SELECT id FROM symbols WHERE file_path = ? AND name = ? LIMIT 1")?;
                    if let Ok(id) = stmt.query_row(params![target, cand], |r| r.get::<_, i64>(0)) {
                        handler_id = Some(id);
                        break 'outer;
                    }
                }
            }
        }

        if handler_id.is_none() {
            if pf.entry.language == "javascript" {
                if let Some(&id) = symbol_map.get("default") {
                    handler_id = Some(id);
                } else {
                    for sym in &pf.extraction.symbols {
                        if sym.kind == "component" || sym.kind == "function" {
                            if let Some(&id) = symbol_map.get(&sym.name) {
                                handler_id = Some(id);
                                break;
                            }
                        }
                    }
                }
            } else if pf.entry.language == "python" {
                for sym in &pf.extraction.symbols {
                    if sym.kind == "function" || sym.kind == "method" {
                        if let Some(&id) = symbol_map.get(&sym.name) {
                            handler_id = Some(id);
                            break;
                        }
                    }
                }
            }
        }

        if let Some(h_id) = handler_id {
            let _ = tx.execute(
                "INSERT OR IGNORE INTO edges (from_symbol_id, to_symbol_id, kind, provenance, wiring_site) VALUES (?, ?, ?, ?, ?)",
                params![route_id, h_id, "references", "ast", None::<String>],
            );
        }
    }

    for rel in &pf.extraction.inheritance {
        if let Some(&class_id) = symbol_map.get(&rel.class_name) {
            let mut resolved_parent_id = None;
            if let Some(&to_id) = symbol_map.get(&rel.parent_name) {
                resolved_parent_id = Some(to_id);
            } else {
                for target in &resolved_imports {
                    let mut stmt = tx.prepare_cached("SELECT id FROM symbols WHERE file_path = ? AND name = ? LIMIT 1")?;
                    if let Ok(to_id) = stmt.query_row(params![target, rel.parent_name], |r| r.get::<_, i64>(0)) {
                        resolved_parent_id = Some(to_id);
                        break;
                    }
                }
            }
            
            if let Some(parent_id) = resolved_parent_id {
                let kind = if rel.is_implements { "implements" } else { "extends" };
                let _ = tx.execute(
                    "INSERT OR IGNORE INTO edges (from_symbol_id, to_symbol_id, kind, provenance, wiring_site) VALUES (?, ?, ?, ?, ?)",
                    params![class_id, parent_id, kind, "ast", None::<String>],
                );
            }
        }
    }

    tx.commit()?;
    Ok(())
}

/// Rows kept in `agent_queries`. It is a rolling telemetry buffer for the UI's
/// Agent Radar panel, not an audit log, and it used to grow without bound.
const AGENT_QUERY_LOG_LIMIT: i64 = 2_000;

/// Record one agent tool call against an already-open connection.
///
/// Takes a `&Connection` on purpose: this used to call `init_db` itself, which
/// re-runs ~10 DDL statements and a `PRAGMA table_info` probe — once per
/// logged query, and `explore` logs once per requested symbol.
pub fn log_agent_query(conn: &Connection, symbol: &str, path: &str, action: &str) {
    let _ = conn.execute(
        "INSERT INTO agent_queries (symbol, path, action, timestamp) VALUES (?, ?, ?, ?)",
        params![symbol, path, action, chrono::Utc::now().timestamp_millis()],
    );
    let _ = conn.execute(
        "DELETE FROM agent_queries WHERE id <= (
             SELECT MAX(id) - ? FROM agent_queries
         )",
        params![AGENT_QUERY_LOG_LIMIT],
    );
}

// ----- pending-change tracking (shared across processes) --------------------

/// Mark `rel_path` as edited-but-not-yet-reindexed.
pub fn mark_pending(conn: &Connection, rel_path: &str) {
    let _ = conn.execute(
        "INSERT OR REPLACE INTO pending_changes (path, marked_at) VALUES (?, ?)",
        params![rel_path, chrono::Utc::now().timestamp_millis()],
    );
}

pub fn clear_pending(conn: &Connection, rel_path: &str) {
    let _ = conn.execute("DELETE FROM pending_changes WHERE path = ?", params![rel_path]);
}

/// Every pending path with the epoch-millis timestamp it was marked at.
pub fn get_pending(conn: &Connection) -> Vec<(String, i64)> {
    let Ok(mut stmt) = conn.prepare("SELECT path, marked_at FROM pending_changes") else {
        return Vec::new();
    };
    let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))) else {
        return Vec::new();
    };
    rows.filter_map(|r| r.ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsers::{ExtractedSymbol, ExtractionResult};
    use crate::walker::FileEntry;

    #[test]
    fn camel_and_snake_names_split_into_searchable_subwords() {
        assert_eq!(
            tokenize_symbol_name("useGraphStore"),
            vec!["useGraphStore", "use", "Graph", "Store"]
        );
        assert_eq!(
            tokenize_symbol_name("user_controller"),
            vec!["user_controller", "user", "controller"]
        );
        // Acronym runs break before the last capital: HTTP + Server.
        assert_eq!(
            tokenize_symbol_name("HTTPServer"),
            vec!["HTTPServer", "HTTP", "Server"]
        );
        // Nothing to split — no duplicate of the name itself.
        assert_eq!(tokenize_symbol_name("router"), vec!["router"]);
    }

    #[test]
    fn match_query_turns_a_bare_word_into_prefix_terms() {
        assert_eq!(build_fts_match_query("store"), "\"store\"*");
        assert_eq!(build_fts_match_query("  "), "");
        // A camelCase query also matches on its own sub-words.
        let q = build_fts_match_query("AuthService");
        assert!(q.contains("\"AuthService\"*"), "got {q}");
        assert!(q.contains("\"Auth\"*") && q.contains("\"Service\"*"), "got {q}");
    }

    /// The bug this fixes: `search("store")` returned nothing while
    /// `useGraphStore` sat in the index, because FTS5 matches token prefixes.
    #[test]
    fn substring_query_finds_camelcase_symbol_through_fts() {
        let dir = std::env::temp_dir().join("repograph_fts_camelcase_search");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("graph.db");
        let conn = init_db(&db_path).unwrap();
        conn.execute(
            "INSERT INTO symbols_fts (name, file_path, content, tokens) VALUES (?, ?, ?, ?)",
            params![
                "useGraphStore",
                "src/store.ts",
                "export const useGraphStore = create(...)",
                tokenize_symbol_name("useGraphStore").join(" ")
            ],
        )
        .unwrap();

        let hit = |query: &str| -> Vec<String> {
            let m = build_fts_match_query(query);
            let mut stmt = conn
                .prepare("SELECT name FROM symbols_fts WHERE tokens MATCH ? LIMIT 10")
                .unwrap();
            let rows = stmt.query_map(params![m], |r| r.get::<_, String>(0)).unwrap();
            rows.filter_map(|r| r.ok()).collect()
        };

        assert_eq!(hit("store"), vec!["useGraphStore"], "lowercase sub-word");
        assert_eq!(hit("Store"), vec!["useGraphStore"], "case-insensitive");
        assert_eq!(hit("graph"), vec!["useGraphStore"], "middle sub-word");
        assert_eq!(hit("useGraphStore"), vec!["useGraphStore"], "exact name");
        assert!(hit("payment").is_empty(), "unrelated query must not match");
    }

    fn parsed_file(path: &str) -> ParsedFile {
        ParsedFile {
            entry: FileEntry {
                path: path.to_string(),
                size_bytes: 64,
                extension: "ts".to_string(),
                language: "javascript".to_string(),
            },
            extraction: ExtractionResult {
                symbols: vec![ExtractedSymbol {
                    name: "doThing".to_string(),
                    kind: "function".to_string(),
                    start_line: 1,
                    end_line: 2,
                }],
                ..Default::default()
            },
        }
    }


    /// Regression: `populate_db` read `pf.entry.path` (repo-relative) instead
    /// of the absolute path, so the content lookup resolved against the process
    /// CWD. An MCP server spawned by a host runs from an arbitrary directory,
    /// so every read failed and `symbols_fts.content` was written empty for the
    /// entire index — `repograph_search` returned symbol names with no code.
    /// The temp root here is deliberately never the CWD.
    #[test]
    fn full_index_stores_symbol_source_when_cwd_is_not_the_repo_root() {
        let dir = std::env::temp_dir().join("repograph_cwd_content_regression");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("src/a.ts"),
            "export function doThing() {\n  return 41 + 1\n}\n",
        )
        .unwrap();

        let mut pf = parsed_file("src/a.ts");
        pf.extraction.symbols[0].end_line = 3;

        let mut conn = init_db(&dir.join("graph.db")).unwrap();
        populate_db(&mut conn, &[pf], &dir).unwrap();

        let content: String = conn
            .query_row(
                "SELECT content FROM symbols_fts WHERE name = 'doThing'",
                [],
                |r| r.get(0),
            )
            .unwrap();

        assert!(
            content.contains("return 41 + 1"),
            "symbols_fts.content must hold real source, got {content:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The file watcher re-runs `populate_file_in_db` on every save, so this
    /// pins the invariant that re-indexing is idempotent: the `files` delete
    /// must cascade to `symbols` and fire the FTS cleanup trigger. Without it,
    /// `symbols_fts` — which stores each symbol's full source — would grow on
    /// every keystroke-triggered rebuild.
    #[test]
    fn repeated_incremental_reindex_does_not_grow_symbols_fts() {
        let dir = std::env::temp_dir().join("repograph_fts_growth_regression");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.ts"), "export function doThing() {}\n// tail\n").unwrap();

        let conn = init_db(&dir.join("graph.db")).unwrap();
        let pf = parsed_file("a.ts");
        let aliases = std::collections::HashMap::new();
        let file_set: std::collections::BTreeSet<String> =
            ["a.ts".to_string()].into_iter().collect();

        for _ in 0..5 {
            populate_file_in_db(&conn, &pf, &dir, &aliases, &file_set).unwrap();
        }

        let fts_rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM symbols_fts WHERE file_path = 'a.ts'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let symbol_rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM symbols WHERE file_path = 'a.ts'",
                [],
                |r| r.get(0),
            )
            .unwrap();

        assert_eq!(
            fts_rows, 1,
            "symbols_fts must hold exactly one row per symbol after 5 re-indexes, got {fts_rows}"
        );
        // The file pseudo-symbol plus `doThing`; proves the FK cascade from
        // `files` still clears `symbols` and nothing double-counts.
        assert_eq!(
            symbol_rows, 2,
            "expected the file node + doThing, got {symbol_rows}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
