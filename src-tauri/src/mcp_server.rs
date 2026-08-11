//! Read-only MCP server (stdio transport, JSON-RPC 2.0, newline-delimited).
//!
//! Exposes `repograph_files`, `repograph_node`, `repograph_explore` and the
//! call-graph tools over the graph cache produced by the walker. Hard
//! constraints (see `docs/logic/agent-api-logic.md`):
//! - never executes project code, and never modifies a project *source* file
//!   (it does write its own index under `.repograph/`: the agent-query log and
//!   pending-change table both live in `graph.db`)
//! - every path argument is canonicalized and checked against the repo root
//! - credential-bearing files are refused even when present in the index

use crate::graph::{Adjacency, Graph, GraphLoadError, GRAPH_CACHE_RELATIVE_PATH};
use crate::manifest::{
    build_manifest_with, render_markdown, ManifestOptions, SortBy, DEFAULT_LINE_BUDGET,
};
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub struct McpServer {
    /// Canonicalized repo root; the confinement boundary for all file access.
    root: PathBuf,
    /// Display form of the root (no `\\?\` prefix noise on Windows).
    root_display: String,
    cache_path: PathBuf,
    graph: Option<Graph>,
    /// Adjacency index for `graph`, rebuilt on the same mtime check. Without
    /// it every manifest render was O(nodes × edges).
    adjacency: Option<Adjacency>,
    graph_mtime: Option<SystemTime>,
    sync_warning: Option<String>,
    /// One long-lived SQLite handle. `init_db` runs ~10 DDL statements plus a
    /// `PRAGMA table_info` probe, and this used to be re-run per logged query
    /// and once more per explored symbol.
    conn: Option<rusqlite::Connection>,
}

#[derive(Debug, PartialEq)]
pub struct ToolError {
    pub code: &'static str,
    pub message: String,
}

impl ToolError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        ToolError {
            code,
            message: message.into(),
        }
    }
}

fn sanitize_windows_path(raw: &str) -> PathBuf {
    let mut cleaned = raw.trim();
    if cleaned.starts_with("//?/") || cleaned.starts_with("\\\\?\\") {
        cleaned = &cleaned[4..];
    } else if cleaned.starts_with("//?") || cleaned.starts_with("\\\\?") {
        // `starts_with` on a 3-byte ASCII prefix already guarantees the length.
        cleaned = &cleaned[3..];
    }
    PathBuf::from(cleaned.replace('/', std::path::MAIN_SEPARATOR_STR))
}

fn resolve_mcp_root(root: &Path) -> Result<(PathBuf, String, Option<String>), String> {
    let mut warning_msg = None;
    let path_str = root.to_string_lossy();
    let target_root = if path_str == "auto" || path_str.is_empty() {
        // The registry is a *hint*, not an authority: this value becomes the
        // confinement boundary every `resolve_inside_root` check is measured
        // against, and the file lives at a predictable, user-writable path.
        // Only accept a root that already looks like an indexed repo.
        let loaded_path = crate::paths::active_project_registry()
            .filter(|p| p.is_file())
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
            .and_then(|json| {
                json.get("active_project_root")
                    .and_then(|v| v.as_str())
                    .map(sanitize_windows_path)
            })
            .filter(|p| {
                let ok = p.join(".repograph").is_dir();
                if !ok {
                    eprintln!(
                        "[mcp] ignoring registry root {} — no .repograph directory",
                        p.display()
                    );
                }
                ok
            });

        if let Some(lp) = loaded_path {
            lp
        } else {
            warning_msg = Some(
                "Warning: no valid active project registry found. Defaulting to the working directory."
                    .to_string(),
            );
            std::env::current_dir().map_err(|e| format!("failed to get current directory: {e}"))?
        }
    } else {
        sanitize_windows_path(&path_str)
    };

    let canonical = dunce::canonicalize(&target_root)
        .or_else(|_| target_root.canonicalize())
        .map_err(|e| format!("cannot canonicalize repo root {}: {e}", target_root.display()))?;

    let root_display = dunce::canonicalize(&canonical)
        .unwrap_or_else(|_| canonical.clone())
        .to_string_lossy()
        .to_string();
    Ok((canonical, root_display, warning_msg))
}

/// Longest declaration a signature may span before we stop looking for the
/// body opener — covers multi-line generic/parameter lists without ever
/// dragging in a body when the opener is missing.
const MAX_SIGNATURE_LINES: usize = 6;

/// The declaration head of a symbol: everything from its first line up to and
/// including the line that opens the body, with the opening brace stripped.
///
/// `lines` is the whole file; `start`/`end` are 1-based and inclusive.
///
/// ```text
/// pub fn init_db(db_path: &Path) -> Result<Connection> {   →  pub fn init_db(db_path: &Path) -> Result<Connection>
///     ...body...
/// }
/// ```
///
/// Language-agnostic on purpose: it keys off the punctuation that opens a
/// block (`{`, `:`, `=>`) or ends a declaration (`;`), which covers every
/// language this indexer parses without a per-language rule.
pub fn signature_block(lines: &[&str], start: usize, end: usize) -> String {
    if start == 0 || start > lines.len() {
        return String::new();
    }
    let last = end.min(lines.len()).max(start);
    let mut collected: Vec<&str> = Vec::new();

    for line in &lines[start - 1..last] {
        collected.push(line);
        let trimmed = line.trim_end();
        let opens_body = trimmed.ends_with('{')
            || trimmed.ends_with(':')
            || trimmed.ends_with("=>")
            || trimmed.ends_with(';')
            || trimmed.ends_with("({");
        if opens_body || collected.len() >= MAX_SIGNATURE_LINES {
            break;
        }
    }

    // The brace carries no information once the body is gone.
    let mut out = collected.join("\n");
    while out.ends_with('{') || out.ends_with(char::is_whitespace) {
        out.pop();
    }
    out
}

/// One call-graph edge as a single arrow string: `caller -kind-> callee`.
///
/// The `kind` stays inside the arrow rather than being dropped: it is what
/// separates a named caller (`calls`) from a whole file whose references were
/// all local bindings (`references`), which is the entire point of
/// [`collapse_endpoints`]. Keeping it costs ~10 chars per edge and preserves
/// the distinction; dropping it would save bytes by deleting information.
pub fn compact_edge(edge: &crate::db::ExplorePathEdge) -> String {
    format!("{} -{}-> {}", edge.from_symbol, edge.kind, edge.to_symbol)
}

/// One end of a call-graph edge after collapsing (see `collapse_endpoints`).
#[derive(Debug, PartialEq)]
pub enum Endpoint {
    /// A named symbol that genuinely participates in the call graph.
    Symbol { file: String, name: String },
    /// A whole file whose only references were in-function locals — reported
    /// once instead of once per local binding.
    File { file: String },
}

impl Endpoint {
    fn reference(&self) -> String {
        match self {
            Endpoint::Symbol { file, name } => format!("{file}#{name}"),
            Endpoint::File { file } => file.clone(),
        }
    }

    fn edge_kind(&self) -> &'static str {
        match self {
            Endpoint::Symbol { .. } => "calls",
            Endpoint::File { .. } => "references",
        }
    }
}

/// Kinds that are storage for a call result rather than participants in the
/// call graph. `const x = useGraphStore(...)` indexes `x` as a `variable`;
/// reporting it tells an agent nothing it could not infer.
fn is_noise_kind(kind: &str) -> bool {
    matches!(kind, "variable" | "file" | "constant" | "field" | "property")
}

/// Collapses raw `(file, name, kind)` endpoints into the smallest set that
/// still answers "who touches this symbol".
///
/// Measured against `useGraphStore` in this repo, the raw edge set was 138
/// endpoints (16,087 chars of `paths`) of which 111 were in-function locals
/// (`#load`, `#activeProjectRoot`) and 12 were `file#file` self-descriptions.
///
/// Rules, per source file:
/// 1. Named symbols (anything not in `is_noise_kind`) are kept and deduped.
/// 2. A file contributing only noise collapses to a single `references` edge —
///    dropping it entirely would lose the answer, since for a React store
///    *every* subscriber appears only as a local binding.
/// 3. Self-references inside the anchor's own file are dropped unless they are
///    named symbols; a store's own actions referencing it are not information.
pub fn collapse_endpoints(raw: Vec<(String, String, String)>, anchor_file: &str) -> Vec<Endpoint> {
    use std::collections::BTreeMap;

    let mut by_file: BTreeMap<String, (Vec<String>, bool)> = BTreeMap::new();
    for (file, name, kind) in raw {
        let entry = by_file.entry(file.clone()).or_insert_with(|| (Vec::new(), false));
        // `file#file` rows are the file's own pseudo-symbol, never a caller.
        let is_file_pseudo = name == file;
        if !is_file_pseudo && !is_noise_kind(&kind) {
            if !entry.0.contains(&name) {
                entry.0.push(name);
            }
        } else {
            entry.1 = true;
        }
    }

    let mut out = Vec::new();
    for (file, (named, had_noise)) in by_file {
        if !named.is_empty() {
            for name in named {
                out.push(Endpoint::Symbol { file: file.clone(), name });
            }
        } else if had_noise && file != anchor_file {
            out.push(Endpoint::File { file });
        }
    }
    out
}

impl McpServer {
    pub fn new(root: &Path) -> Result<McpServer, String> {
        let (canonical, root_display, sync_warning) = resolve_mcp_root(root)?;
        let _ = crate::db::reconcile_repo_startup(&canonical);
        Ok(McpServer {
            root_display,
            cache_path: canonical.join(GRAPH_CACHE_RELATIVE_PATH),
            root: canonical,
            graph: None,
            adjacency: None,
            graph_mtime: None,
            sync_warning,
            conn: None,
        })
    }

    /// The shared SQLite handle, opened on first use.
    fn db(&mut self) -> Result<&rusqlite::Connection, ToolError> {
        if self.conn.is_none() {
            let db_path = self.root.join(".repograph/graph.db");
            let conn = crate::db::init_db(&db_path)
                .map_err(|e| ToolError::new("db_error", format!("failed to open database: {e}")))?;
            self.conn = Some(conn);
        }
        Ok(self.conn.as_ref().expect("opened above"))
    }

    /// Blocking serve loop over stdin/stdout. Returns when stdin closes.
    pub fn serve_stdio(&mut self) -> std::io::Result<()> {
        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout();
        for line in stdin.lock().lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            if let Some(response) = self.handle_message(&line) {
                stdout.write_all(response.to_string().as_bytes())?;
                stdout.write_all(b"\n")?;
                stdout.flush()?;
            }
        }
        Ok(())
    }

    /// Handle one raw JSON-RPC message; `None` for notifications (no reply).
    pub fn handle_message(&mut self, raw: &str) -> Option<Value> {
        // Strip a possible UTF-8 BOM (Windows shells prepend one to piped input).
        let raw = raw.trim_start_matches('\u{feff}');
        let msg: Value = match serde_json::from_str(raw) {
            Ok(v) => v,
            Err(e) => {
                return Some(jsonrpc_error(Value::Null, -32700, &format!("parse error: {e}")))
            }
        };
        let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
        let id = msg.get("id").cloned();
        // Notifications (no id) get no response.
        let id = match id {
            Some(id) if !id.is_null() => id,
            _ => return None,
        };
        let params = msg.get("params").cloned().unwrap_or(Value::Null);

        let result = match method {
            "initialize" => Ok(self.initialize_result(&params)),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(tools_list_result()),
            "tools/call" => self.tools_call(&params),
            _ => Err((-32601, format!("method not found: {method}"))),
        };
        Some(match result {
            Ok(value) => json!({"jsonrpc": "2.0", "id": id, "result": value}),
            Err((code, message)) => jsonrpc_error(id, code, &message),
        })
    }

    fn initialize_result(&self, params: &Value) -> Value {
        let requested = params
            .get("protocolVersion")
            .and_then(Value::as_str)
            .unwrap_or("2025-06-18");
        json!({
            "protocolVersion": requested,
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "repo-graph", "version": env!("CARGO_PKG_VERSION") },
            "instructions": "The 'repograph_explore' tool is the primary, compound tool. When researching or editing code in this workspace, always prioritize calling the 'repograph_explore' tool. It retrieves exact symbol slices and call graphs in a single query, minimizing token ingestion and context costs."
        })
    }

    fn tools_call(&mut self, params: &Value) -> Result<Value, (i64, String)> {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or((-32602, "missing tool name".to_string()))?;
        // The allowlist has to gate dispatch too. Filtering `tools/list` alone
        // only hid names that are published in this repo's own docs and in the
        // config snippet the desktop app generates — any client could still
        // call them.
        if !tool_is_enabled(name) {
            return Err((
                -32601,
                format!("tool not enabled: {name} (see REPOGRAPH_MCP_TOOLS)"),
            ));
        }
        let args = params.get("arguments").cloned().unwrap_or(json!({}));

        let outcome = match name {
            "repograph_files" => {
                let options = ManifestOptions {
                    scope: args.get("scope").and_then(Value::as_str),
                    // `as_u64` rejects a float or a negative, which is the
                    // behaviour we want — a bad `top_k` should be ignored, not
                    // silently rounded into a different query.
                    top_k: args
                        .get("top_k")
                        .and_then(Value::as_u64)
                        .map(|k| k as usize),
                    min_rank: args.get("min_rank").and_then(Value::as_f64),
                    sort_by: args
                        .get("sort_by")
                        .and_then(Value::as_str)
                        .map(SortBy::parse)
                        .unwrap_or_default(),
                };
                self.get_manifest_with(&options)
            }
            "repograph_node" => {
                let path = args.get("path").and_then(Value::as_str).ok_or((-32602, "repograph_node requires 'path'".to_string()))?;
                let symbol = args.get("symbol").and_then(Value::as_str);
                if let Some(s) = symbol {
                    self.read_symbol(path, s)
                } else {
                    self.read_file(path)
                }
            }
            "repograph_callers" => {
                let path = args.get("path").and_then(Value::as_str);
                let symbol = args.get("symbol").and_then(Value::as_str);
                self.repograph_callers(path, symbol)
            }
            "repograph_callees" => {
                let path = args.get("path").and_then(Value::as_str);
                let symbol = args.get("symbol").and_then(Value::as_str);
                self.repograph_callees(path, symbol)
            }
            "repograph_impact" => {
                let path = args.get("path").and_then(Value::as_str);
                let symbol = args.get("symbol").and_then(Value::as_str);
                self.repograph_impact(path, symbol)
            }
            "repograph_search" => {
                let query = args.get("query").and_then(Value::as_str).ok_or((-32602, "repograph_search requires 'query'".to_string()))?;
                self.search_symbols(query)
            }
            "repograph_explore" => {
                let symbols_val = args.get("symbols").ok_or((-32602, "repograph_explore requires 'symbols'".to_string()))?;
                let symbols_arr = symbols_val.as_array().ok_or((-32602, "repograph_explore 'symbols' must be an array".to_string()))?;
                let mut symbols = Vec::new();
                for val in symbols_arr {
                    if let Some(s) = val.as_str() {
                        symbols.push(s.to_string());
                    }
                }
                let signature_only = args
                    .get("signature_only")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                // Compact edges default to on under `signature_only`: an agent
                // asking for structure wants the smallest faithful shape.
                let compact_edges = args
                    .get("compact_edges")
                    .and_then(Value::as_bool)
                    .unwrap_or(signature_only);
                self.explore(symbols, signature_only, compact_edges)
            }
            "repograph_status" => {
                self.codegraph_status()
            }
            other => return Err((-32602, format!("unknown tool: {other}"))),
        };

        // Tool-level failures are structured MCP tool results (isError),
        // not JSON-RPC protocol errors, so the calling agent can react.
        Ok(match outcome {
            Ok(text) => json!({
                "content": [{"type": "text", "text": text}],
                "isError": false
            }),
            Err(e) => json!({
                "content": [{"type": "text", "text":
                    format!("error [{}]: {}", e.code, e.message)}],
                "isError": true
            }),
        })
    }

    // ----- tools -----------------------------------------------------------

    pub fn get_manifest(&mut self, scope: Option<&str>) -> Result<String, ToolError> {
        self.get_manifest_with(&ManifestOptions::scoped(scope))
    }

    pub fn get_manifest_with(
        &mut self,
        options: &ManifestOptions<'_>,
    ) -> Result<String, ToolError> {
        let root_display = self.root_display.clone();
        let (graph, adjacency) = self.graph_and_adjacency()?;
        let manifest = build_manifest_with(graph, adjacency, &root_display, options);
        if manifest.files.is_empty() {
            if let Some(s) = options.scope {
                return Err(ToolError::new(
                    "empty_scope",
                    format!("no indexed files match scope '{s}'"),
                ));
            }
            // An over-strict `min_rank` returns nothing from a healthy graph,
            // which is indistinguishable from "the repo is empty" unless we
            // say so.
            if let Some(min) = options.min_rank {
                if manifest.total_files > 0 {
                    return Err(ToolError::new(
                        "empty_rank_filter",
                        format!(
                            "no files scored at or above min_rank {min}; \
                             ranks are normalized so the top file is 1.0"
                        ),
                    ));
                }
            }
        }
        let mut md = render_markdown(&manifest, DEFAULT_LINE_BUDGET);
        
        let ref_files: Vec<&str> = manifest.files.iter().map(|f| f.path.as_str()).collect();
        let (header, footer) = self.check_staleness(&ref_files);

        if let Some(h) = header {
            md = format!("{}{}", h, md);
        }
        if let Some(f) = footer {
            md = format!("{}{}", md, f);
        }

        if let Some(ref warning) = self.sync_warning {
            md = format!("{}\n\n{}", warning, md);
        }
        Ok(md)
    }

    pub fn read_file(&self, path: &str) -> Result<String, ToolError> {
        let resolved = self.resolve_inside_root(path)?;
        // Enforced here as well as in the walker: a `.repograph` cache built
        // before the walker learned this denylist still lists `.env` as a
        // node, and would otherwise keep serving it verbatim.
        self.reject_secret(&resolved, path)?;
        let mut content = std::fs::read_to_string(&resolved).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => {
                ToolError::new("file_not_found", format!("no such file: {path}"))
            }
            std::io::ErrorKind::InvalidData => ToolError::new(
                "not_utf8",
                format!("{path} is not valid UTF-8 (binary file?)"),
            ),
            _ => ToolError::new("io_error", format!("failed to read {path}: {e}")),
        })?;

        let (header, footer) = self.check_staleness(&[path]);
        if let Some(h) = header {
            content = format!("{}{}", h, content);
        }
        if let Some(f) = footer {
            content = format!("{}{}", content, f);
        }

        Ok(content)
    }

    pub fn read_symbol(&mut self, path: &str, symbol: &str) -> Result<String, ToolError> {
        let resolved = self.resolve_inside_root(path)?;
        self.reject_secret(&resolved, path)?;

        let repo_rel = resolved
            .strip_prefix(&self.root)
            .map_err(|_| ToolError::new("path_error", "failed to strip prefix"))?
            .to_string_lossy()
            .replace('\\', "/");

        let conn = self.db()?;
        crate::db::log_agent_query(conn, symbol, path, "read_symbol");

        let range = crate::db::get_symbol_range(conn, &repo_rel, symbol)
            .map_err(|e| ToolError::new("db_error", format!("database error: {e}")))?
            .ok_or_else(|| {
                ToolError::new("symbol_not_found", format!("symbol '{symbol}' not found in file '{path}'"))
            })?;

        let content = std::fs::read_to_string(&resolved).map_err(|e| {
            ToolError::new("io_error", format!("failed to read file: {e}"))
        })?;
        let lines: Vec<&str> = content.lines().collect();

        let start = range.0.max(1);
        let end = range.1.min(lines.len());
        if start > end || start > lines.len() {
            return Err(ToolError::new("invalid_range", "invalid line range in database"));
        }

        let slice = lines[start - 1..end].join("\n");
        Ok(slice)
    }

    pub fn find_dependents(&mut self, path: &str) -> Result<String, ToolError> {
        let normalized = path.replace('\\', "/");
        let normalized = normalized.trim_start_matches("./").to_string();
        let (graph, adjacency) = self.graph_and_adjacency()?;
        if graph.node(&normalized).is_none() {
            return Err(ToolError::new(
                "unknown_path",
                format!("'{normalized}' is not in the indexed graph"),
            ));
        }
        let dependents = adjacency.dependents_of(&normalized);
        if dependents.is_empty() {
            return Ok(format!("No indexed files depend on /{normalized}."));
        }
        let mut out = format!("Files that depend on /{normalized}:\n");
        for d in dependents {
            out.push_str(&format!("- /{d}\n"));
        }
        Ok(out)
    }

    pub fn search_symbols(&mut self, query: &str) -> Result<String, ToolError> {
        // Matches sub-word tokens, so "store" finds `useGraphStore` — the plain
        // `name MATCH 'store*'` this replaced returned nothing for that query.
        let match_query = crate::db::build_fts_match_query(query);
        if match_query.is_empty() {
            return Ok("[]".to_string());
        }

        let conn = self.db()?;
        crate::db::log_agent_query(conn, "", query, "search_symbols");

        let mut stmt = conn
            .prepare(
                "SELECT name, file_path, content
                 FROM symbols_fts
                 WHERE tokens MATCH ?
                 ORDER BY rank
                 LIMIT 50"
            )
            .map_err(|e| ToolError::new("db_error", format!("failed to prepare statement: {e}")))?;

        let rows = stmt
            .query_map(rusqlite::params![match_query], |row| {
                Ok(crate::db::SymbolSearchResult {
                    name: row.get(0)?,
                    file_path: row.get(1)?,
                    content: row.get(2)?,
                })
            })
            .map_err(|e| ToolError::new("db_error", format!("query failed: {e}")))?;

        let mut results = Vec::new();
        for res in rows.flatten() {
            results.push(res);
        }

        serde_json::to_string(&results)
            .map_err(|e| ToolError::new("serialization_error", e.to_string()))
    }

    /// `signature_only` returns each symbol's declaration head instead of its
    /// body. The call-graph edges are identical either way — for "what talks to
    /// what" questions the bodies are the entire cost and none of the answer.
    ///
    /// `compact_edges` drops the per-edge JSON keys (see [`compact_edge`]).
    pub fn explore(
        &mut self,
        symbols: Vec<String>,
        signature_only: bool,
        compact_edges: bool,
    ) -> Result<String, ToolError> {
        let root = self.root.clone();
        let conn = self.db()?;

        let mut resolved_symbols = Vec::new();
        for sym_str in &symbols {
            let (f, s) = match sym_str.split_once('#') {
                Some(pair) => (pair.0.to_string(), pair.1.to_string()),
                None => {
                    let mut stmt = conn.prepare("SELECT file_path, name FROM symbols WHERE name = ? LIMIT 1").map_err(|e| {
                        ToolError::new("db_error", format!("failed to prepare statement: {e}"))
                    })?;
                    let res: Option<(String, String)> = stmt.query_row(rusqlite::params![sym_str], |r| Ok((r.get(0)?, r.get(1)?))).ok();
                    match res {
                        Some(pair) => pair,
                        None => continue,
                    }
                }
            };
            
            crate::db::log_agent_query(conn, &s, &f, "explore");

            let mut stmt = conn.prepare("SELECT id, kind, start_line, end_line FROM symbols WHERE file_path = ? AND name = ? LIMIT 1").map_err(|e| {
                ToolError::new("db_error", format!("failed to prepare statement: {e}"))
            })?;
            let details = stmt.query_row(rusqlite::params![f, s], |r| Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)? as usize,
                r.get::<_, i64>(3)? as usize,
            ))).ok();

            if let Some((id, kind, start, end)) = details {
                resolved_symbols.push((f, s, id, kind, start, end));
            }
        }

        let mut file_map = std::collections::HashMap::new();
        let mut paths = Vec::new();
        let mut seen_edges = std::collections::HashSet::new();

        for &(ref f, ref s, id, ref kind, start, end) in &resolved_symbols {
            let anchor = format!("{f}#{s}");

            let mut callers_stmt = conn.prepare(
                "SELECT s.file_path, s.name, s.kind
                 FROM edges e
                 JOIN symbols s ON e.from_symbol_id = s.id
                 WHERE e.to_symbol_id = ?"
            ).map_err(|e| ToolError::new("db_error", format!("failed to prepare statement: {e}")))?;
            let caller_rows = callers_stmt.query_map(rusqlite::params![id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
            }).map_err(|e| ToolError::new("db_error", format!("query failed: {e}")))?;
            let raw_callers: Vec<(String, String, String)> = caller_rows.filter_map(|r| r.ok()).collect();

            for endpoint in collapse_endpoints(raw_callers, f) {
                let from_sym = endpoint.reference();
                if from_sym == anchor {
                    continue;
                }
                let edge_key = format!("{from_sym}->{anchor}");
                if seen_edges.insert(edge_key) {
                    paths.push(crate::db::ExplorePathEdge {
                        from_symbol: from_sym,
                        to_symbol: anchor.clone(),
                        kind: endpoint.edge_kind().to_string(),
                    });
                }
            }

            let mut callees_stmt = conn.prepare(
                "SELECT s.file_path, s.name, s.kind
                 FROM edges e
                 JOIN symbols s ON e.to_symbol_id = s.id
                 WHERE e.from_symbol_id = ?"
            ).map_err(|e| ToolError::new("db_error", format!("failed to prepare statement: {e}")))?;
            let callee_rows = callees_stmt.query_map(rusqlite::params![id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
            }).map_err(|e| ToolError::new("db_error", format!("query failed: {e}")))?;
            let raw_callees: Vec<(String, String, String)> = callee_rows.filter_map(|r| r.ok()).collect();

            for endpoint in collapse_endpoints(raw_callees, f) {
                let to_sym = endpoint.reference();
                if to_sym == anchor {
                    continue;
                }
                let edge_key = format!("{anchor}->{to_sym}");
                if seen_edges.insert(edge_key) {
                    paths.push(crate::db::ExplorePathEdge {
                        from_symbol: anchor.clone(),
                        to_symbol: to_sym,
                        kind: endpoint.edge_kind().to_string(),
                    });
                }
            }

            let clean_relative = f.trim_start_matches(['/', '\\']);
            let abs_path = root.join(clean_relative);
            let canonical = match abs_path.canonicalize() {
                Ok(c) => c,
                Err(_) => continue,
            };
            if !canonical.starts_with(&root) {
                continue;
            }
            if canonical
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(crate::walker::is_secret_file)
            {
                continue;
            }

            let content = match std::fs::read_to_string(&canonical) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let lines: Vec<&str> = content.lines().collect();
            let final_start = start.max(1);
            let final_end = end.min(lines.len());
            if final_start <= final_end && final_start <= lines.len() {
                let code = if signature_only {
                    signature_block(&lines, final_start, final_end)
                } else {
                    lines[final_start - 1..final_end].join("\n")
                };
                file_map.entry(f.clone()).or_insert_with(Vec::new).push(crate::db::ExploreSymbolBlock {
                    name: s.clone(),
                    kind: kind.clone(),
                    code,
                    start_line: final_start,
                    end_line: final_end,
                });
            }
        }

        let files = file_map
            .into_iter()
            .map(|(path, code_blocks)| crate::db::ExploreFilePayload { path, code_blocks })
            .collect();

        if compact_edges {
            // `{"from_symbol":"a","to_symbol":"b","kind":"calls"}` spends 44
            // chars per edge on keys that are identical every time — measured
            // at 748 of the 1,875 `paths` chars for `useGraphStore`.
            let compact: Vec<String> = paths.iter().map(compact_edge).collect();
            return serde_json::to_string(&json!({ "files": files, "paths": compact }))
                .map_err(|e| ToolError::new("serialization_error", e.to_string()));
        }

        let payload = crate::db::ExplorePayload { files, paths };
        serde_json::to_string(&payload)
            .map_err(|e| ToolError::new("serialization_error", e.to_string()))
    }

    fn check_staleness(&self, referenced_files: &[&str]) -> (Option<String>, Option<String>) {
        let pending_map = crate::watcher::get_pending_map(&self.root);
        if pending_map.is_empty() {
            return (None, None);
        }

        let mut inside_response = Vec::new();
        let mut outside_response = Vec::new();
        let now = chrono::Utc::now().timestamp_millis();

        for (rel_path, marked_at) in pending_map {
            let elapsed_ms = now.saturating_sub(marked_at).max(0);

            let is_referenced = referenced_files.iter().any(|&f| f == rel_path);
            if is_referenced {
                inside_response.push(format!("  - {} (edited {}ms ago, pending sync)", rel_path, elapsed_ms));
            } else {
                outside_response.push(rel_path);
            }
        }

        let header = if !inside_response.is_empty() {
            let mut h = String::new();
            h.push_str("⚠️ Some files referenced below were edited since the last index sync —\n");
            h.push_str("their graph entries may be stale:\n");
            for item in inside_response {
                h.push_str(&item);
                h.push('\n');
            }
            h.push_str("For accurate content of those specific files, Read them directly.\n");
            h.push_str("The rest of this response is fresh.\n");
            Some(h)
        } else {
            None
        };

        let footer = if !outside_response.is_empty() {
            let joined = outside_response.join(", ");
            Some(format!(
                "\n* (Note: {} file(s) elsewhere in this project are pending index sync but were not referenced above: {})",
                outside_response.len(),
                joined
            ))
        } else {
            None
        };

        (header, footer)
    }

    pub fn codegraph_status(&self) -> Result<String, ToolError> {
        let pending_map = crate::watcher::get_pending_map(&self.root);
        let sync_state = if pending_map.is_empty() { "Synced" } else { "Pending" };

        let mut md = String::new();
        md.push_str("## CodeGraph Status\n\n");
        md.push_str(&format!("- **Active Project Root:** `{}`\n", self.root.display()));
        md.push_str(&format!("- **Sync State:** {}\n\n", sync_state));

        if pending_map.is_empty() {
            md.push_str("All files are fully indexed. Graph is up-to-date.\n");
        } else {
            md.push_str("### Pending Files:\n");
            let now = chrono::Utc::now().timestamp_millis();
            let mut rows: Vec<(String, i64)> = pending_map.into_iter().collect();
            rows.sort();
            for (rel_path, marked_at) in rows {
                let elapsed_ms = now.saturating_sub(marked_at).max(0);
                md.push_str(&format!("- `{rel_path}` (edited {elapsed_ms}ms ago, pending sync)\n"));
            }
        }

        Ok(md)
    }

    pub fn repograph_callers(&mut self, path: Option<&str>, symbol: Option<&str>) -> Result<String, ToolError> {
        // Resolved before the connection is borrowed: `self.db()` needs
        // `&mut self`, so no `&self` method may be called while it is alive.
        let scoped_path = match path {
            Some(p) => Some(self.repo_relative(p)?),
            None => None,
        };
        let conn = self.db()?;

        if let Some(sym_name) = symbol {
            let sid_opt: Option<i64>;
            if let Some(repo_rel) = scoped_path.as_deref() {
                let mut s = conn.prepare("SELECT id FROM symbols WHERE file_path = ? AND name = ? LIMIT 1").map_err(|e| ToolError::new("db_error", e.to_string()))?;
                sid_opt = s.query_row(rusqlite::params![repo_rel, sym_name], |r| r.get(0)).ok();
            } else {
                let mut s = conn.prepare("SELECT id FROM symbols WHERE name = ? LIMIT 1").map_err(|e| ToolError::new("db_error", e.to_string()))?;
                sid_opt = s.query_row(rusqlite::params![sym_name], |r| r.get(0)).ok();
            }

            if let Some(sid) = sid_opt {
                let mut callers_stmt = conn.prepare(
                    "SELECT s.file_path, s.name, s.kind
                     FROM edges e
                     JOIN symbols s ON e.from_symbol_id = s.id
                     WHERE e.to_symbol_id = ?"
                ).map_err(|e| ToolError::new("db_error", e.to_string()))?;
                let rows = callers_stmt.query_map(rusqlite::params![sid], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
                }).map_err(|e| ToolError::new("db_error", e.to_string()))?;

                let mut out = format!("Callers of symbol '{sym_name}':\n");
                let mut count = 0;
                for (cf, cs, ck) in rows.flatten() {
                    out.push_str(&format!("- {}#{} ({ck})\n", cf, cs));
                    count += 1;
                }
                if count == 0 {
                    return Ok(format!("No callers found for symbol '{sym_name}'."));
                }
                return Ok(out);
            }
            Err(ToolError::new("symbol_not_found", format!("Symbol '{sym_name}' not found")))
        } else if let Some(p) = path {
            self.find_dependents(p)
        } else {
            Err(ToolError::new("invalid_params", "repograph_callers requires either 'symbol' or 'path'"))
        }
    }

    pub fn repograph_callees(&mut self, path: Option<&str>, symbol: Option<&str>) -> Result<String, ToolError> {
        // Resolved before the connection is borrowed: `self.db()` needs
        // `&mut self`, so no `&self` method may be called while it is alive.
        let scoped_path = match path {
            Some(p) => Some(self.repo_relative(p)?),
            None => None,
        };
        let conn = self.db()?;

        if let Some(sym_name) = symbol {
            let sid_opt: Option<i64>;
            if let Some(repo_rel) = scoped_path.as_deref() {
                let mut s = conn.prepare("SELECT id FROM symbols WHERE file_path = ? AND name = ? LIMIT 1").map_err(|e| ToolError::new("db_error", e.to_string()))?;
                sid_opt = s.query_row(rusqlite::params![repo_rel, sym_name], |r| r.get(0)).ok();
            } else {
                let mut s = conn.prepare("SELECT id FROM symbols WHERE name = ? LIMIT 1").map_err(|e| ToolError::new("db_error", e.to_string()))?;
                sid_opt = s.query_row(rusqlite::params![sym_name], |r| r.get(0)).ok();
            }

            if let Some(sid) = sid_opt {
                let mut callees_stmt = conn.prepare(
                    "SELECT s.file_path, s.name, s.kind
                     FROM edges e
                     JOIN symbols s ON e.to_symbol_id = s.id
                     WHERE e.from_symbol_id = ?"
                ).map_err(|e| ToolError::new("db_error", e.to_string()))?;
                let rows = callees_stmt.query_map(rusqlite::params![sid], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
                }).map_err(|e| ToolError::new("db_error", e.to_string()))?;

                let mut out = format!("Callees of symbol '{sym_name}':\n");
                let mut count = 0;
                for (tf, ts, tk) in rows.flatten() {
                    out.push_str(&format!("- {}#{} ({tk})\n", tf, ts));
                    count += 1;
                }
                if count == 0 {
                    return Ok(format!("No callees found for symbol '{sym_name}'."));
                }
                return Ok(out);
            }
            Err(ToolError::new("symbol_not_found", format!("Symbol '{sym_name}' not found")))
        } else if let Some(p) = path {
            let normalized = p.replace('\\', "/");
            let normalized = normalized.trim_start_matches("./").to_string();
            let (graph, adjacency) = self.graph_and_adjacency()?;
            if graph.node(&normalized).is_none() {
                return Err(ToolError::new("unknown_path", format!("'{normalized}' is not in the indexed graph")));
            }
            let dependencies = adjacency.dependencies_of(&normalized);
            if dependencies.is_empty() {
                return Ok(format!("No file dependencies found for /{normalized}."));
            }
            let mut out = format!("File dependencies of /{normalized}:\n");
            for d in &dependencies {
                out.push_str(&format!("- /{d}\n"));
            }
            Ok(out)
        } else {
            Err(ToolError::new("invalid_params", "repograph_callees requires either 'symbol' or 'path'"))
        }
    }

    pub fn repograph_impact(&mut self, path: Option<&str>, symbol: Option<&str>) -> Result<String, ToolError> {
        // Resolved before the connection is borrowed: `self.db()` needs
        // `&mut self`, so no `&self` method may be called while it is alive.
        let scoped_path = match path {
            Some(p) => Some(self.repo_relative(p)?),
            None => None,
        };
        let conn = self.db()?;

        if let Some(sym_name) = symbol {
            let sid_opt: Option<i64>;
            if let Some(repo_rel) = scoped_path.as_deref() {
                let mut s = conn.prepare("SELECT id FROM symbols WHERE file_path = ? AND name = ? LIMIT 1").map_err(|e| ToolError::new("db_error", e.to_string()))?;
                sid_opt = s.query_row(rusqlite::params![repo_rel, sym_name], |r| r.get(0)).ok();
            } else {
                let mut s = conn.prepare("SELECT id FROM symbols WHERE name = ? LIMIT 1").map_err(|e| ToolError::new("db_error", e.to_string()))?;
                sid_opt = s.query_row(rusqlite::params![sym_name], |r| r.get(0)).ok();
            }

            if let Some(sid) = sid_opt {
                let mut affected = std::collections::HashSet::new();
                let mut queue = vec![sid];
                
                while let Some(current_id) = queue.pop() {
                    let mut callers_stmt = conn.prepare(
                        "SELECT s.id, s.file_path, s.name
                         FROM edges e
                         JOIN symbols s ON e.from_symbol_id = s.id
                         WHERE e.to_symbol_id = ?"
                    ).map_err(|e| ToolError::new("db_error", e.to_string()))?;
                    let rows = callers_stmt.query_map(rusqlite::params![current_id], |row| {
                        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
                    }).map_err(|e| ToolError::new("db_error", e.to_string()))?;

                    for (id, cf, cs) in rows.flatten() {
                        let key = format!("{cf}#{cs}");
                        if !affected.contains(&key) {
                            affected.insert(key);
                            queue.push(id);
                        }
                    }
                }

                let mut out = format!("Blast radius / Impact of changing symbol '{sym_name}':\n");
                if affected.is_empty() {
                    out.push_str("No other symbols are affected.\n");
                } else {
                    for key in affected {
                        out.push_str(&format!("- {key}\n"));
                    }
                }
                return Ok(out);
            }
            Err(ToolError::new("symbol_not_found", format!("Symbol '{sym_name}' not found")))
        } else if let Some(p) = path {
            let normalized = p.replace('\\', "/");
            let normalized = normalized.trim_start_matches("./").to_string();
            let (graph, adjacency) = self.graph_and_adjacency()?;
            if graph.node(&normalized).is_none() {
                return Err(ToolError::new(
                    "unknown_path",
                    format!("'{normalized}' is not in the indexed graph"),
                ));
            }

            // Was a BFS whose every step re-scanned the whole edge list.
            let affected = adjacency.transitive_dependents(&normalized);

            let mut out = format!("Blast radius / Impact of changing file /{normalized}:\n");
            if affected.is_empty() {
                out.push_str("No other files are affected.\n");
            } else {
                for key in affected {
                    out.push_str(&format!("- /{key}\n"));
                }
            }
            Ok(out)
        } else {
            Err(ToolError::new("invalid_params", "repograph_impact requires either 'symbol' or 'path'"))
        }
    }

    // ----- internals -------------------------------------------------------

    /// Root-confined path as the repo-relative, forward-slashed key the
    /// `symbols`/`files` tables use.
    fn repo_relative(&self, path: &str) -> Result<String, ToolError> {
        let resolved = self.resolve_inside_root(path)?;
        Ok(resolved
            .strip_prefix(&self.root)
            .map_err(|_| ToolError::new("path_error", "resolved path escaped the repo root"))?
            .to_string_lossy()
            .replace('\\', "/"))
    }

    /// Refuse to serve credential-bearing files regardless of how they got
    /// into the index. See `walker::is_secret_file`.
    fn reject_secret(&self, resolved: &Path, requested: &str) -> Result<(), ToolError> {
        let name = resolved.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if crate::walker::is_secret_file(name) {
            return Err(ToolError::new(
                "secret_file_blocked",
                format!("'{requested}' looks like a credentials file and is never served"),
            ));
        }
        Ok(())
    }

    /// Canonicalize `path` relative to the repo root and reject anything that
    /// resolves outside it (traversal, absolute paths, symlink escapes).
    fn resolve_inside_root(&self, path: &str) -> Result<PathBuf, ToolError> {
        let sanitized = sanitize_windows_path(path);
        let joined = if sanitized.is_absolute() {
            sanitized
        } else {
            self.root.join(sanitized)
        };
        let resolved = dunce::canonicalize(&joined)
            .or_else(|_| joined.canonicalize())
            .map_err(|_| {
                ToolError::new("file_not_found", format!("cannot resolve path: {path}"))
            })?;
        if !resolved.starts_with(&self.root) {
            return Err(ToolError::new(
                "path_outside_root",
                format!("'{path}' resolves outside the repository root"),
            ));
        }
        Ok(resolved)
    }

    /// Serve the graph from memory; reload only when the cache file's mtime
    /// changes (keeps steady-state latency well under 100 ms). The adjacency
    /// index is rebuilt on the same trigger, so it can never go stale
    /// independently of the graph it indexes.
    fn load_graph(&mut self) -> Result<(), ToolError> {
        let mtime = std::fs::metadata(&self.cache_path)
            .and_then(|m| m.modified())
            .ok();
        let stale = self.graph.is_none() || mtime != self.graph_mtime;
        if stale {
            let graph = Graph::load_from_cache(&self.cache_path).map_err(|e| match e {
                GraphLoadError::CacheMissing(_) => ToolError::new("graph_cache_missing", e.to_string()),
                GraphLoadError::Malformed(_) => ToolError::new("graph_cache_malformed", e.to_string()),
                GraphLoadError::Io(_) => ToolError::new("io_error", e.to_string()),
            })?;
            self.adjacency = Some(graph.adjacency());
            self.graph = Some(graph);
            self.graph_mtime = mtime;
        }
        Ok(())
    }

    /// Graph plus its adjacency index, both guaranteed fresh.
    fn graph_and_adjacency(&mut self) -> Result<(&Graph, &Adjacency), ToolError> {
        self.load_graph()?;
        Ok((
            self.graph.as_ref().expect("graph loaded above"),
            self.adjacency.as_ref().expect("adjacency built above"),
        ))
    }
}

fn jsonrpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

/// Tool short-names enabled for this process. `REPOGRAPH_MCP_TOOLS` is a
/// comma-separated list of suffixes (`explore,search,node`); the default keeps
/// the agent's menu to the one compound tool.
fn allowed_tools() -> Vec<String> {
    match std::env::var("REPOGRAPH_MCP_TOOLS") {
        Ok(env_val) => env_val
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect(),
        Err(_) => vec!["explore".to_string()],
    }
}

/// Single source of truth shared by `tools/list` and `tools/call`.
fn tool_is_enabled(full_name: &str) -> bool {
    let Some(short) = full_name.strip_prefix("repograph_") else {
        return false;
    };
    allowed_tools().iter().any(|a| a == short)
}

fn tools_list_result() -> Value {
    let allowed = allowed_tools();

    let mut tools = Vec::new();

    if allowed.contains(&"status".to_string()) {
        tools.push(json!({
            "name": "repograph_status",
            "description": "Returns the active project root, connection sync state, and pending file details.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }));
    }

    if allowed.contains(&"files".to_string()) {
        tools.push(json!({
            "name": "repograph_files",
            "description": "Compressed project architecture map (one line per file: rank, exports, routes, dependency counts). Ordered by dependency-graph centrality, most important first. Check this before requesting full file reads. Use top_k to read only the core of a large repo.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "scope": {
                        "type": "string",
                        "description": "Optional path prefix to scope the manifest, e.g. 'src/api/**'"
                    },
                    "top_k": {
                        "type": "integer",
                        "minimum": 1,
                        "description": "Return only the K highest-ranked files. Start here on an unfamiliar repo: top_k 20-30 usually covers the architectural core."
                    },
                    "min_rank": {
                        "type": "number",
                        "minimum": 0,
                        "maximum": 1,
                        "description": "Drop files scoring below this centrality. Scores are normalized so the most central file is 1.0; 0.1 is a reasonable 'core only' cut."
                    },
                    "sort_by": {
                        "type": "string",
                        "enum": ["rank", "alphabetical", "depth"],
                        "description": "Presentation order only (default 'rank'). top_k and min_rank always select by rank."
                    }
                }
            }
        }));
    }

    if allowed.contains(&"node".to_string()) {
        tools.push(json!({
            "name": "repograph_node",
            "description": "Reads either the raw contents of a file or the exact source block of a symbol inside that file.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Repo-relative file path, e.g. 'src/components/Button.tsx'"
                    },
                    "symbol": {
                        "type": "string",
                        "description": "Optional name of the symbol to extract. If absent, the full file is read."
                    }
                },
                "required": ["path"]
            }
        }));
    }

    if allowed.contains(&"callers".to_string()) {
        tools.push(json!({
            "name": "repograph_callers",
            "description": "Finds incoming callers/dependents for a symbol or file in the repository.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Optional repo-relative file path."
                    },
                    "symbol": {
                        "type": "string",
                        "description": "Optional name of the symbol to trace callers for."
                    }
                }
            }
        }));
    }

    if allowed.contains(&"callees".to_string()) {
        tools.push(json!({
            "name": "repograph_callees",
            "description": "Finds outgoing dependencies/callees for a symbol or file in the repository.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Optional repo-relative file path."
                    },
                    "symbol": {
                        "type": "string",
                        "description": "Optional name of the symbol to trace dependencies for."
                    }
                }
            }
        }));
    }

    if allowed.contains(&"impact".to_string()) {
        tools.push(json!({
            "name": "repograph_impact",
            "description": "Computes transitive blast radius (dependents chain) for a modified symbol or file.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Optional repo-relative file path."
                    },
                    "symbol": {
                        "type": "string",
                        "description": "Optional name of the symbol."
                    }
                }
            }
        }));
    }

    if allowed.contains(&"search".to_string()) {
        tools.push(json!({
            "name": "repograph_search",
            "description": "Searches for matching symbols in the repository index via SQLite FTS5 fuzzy match.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Fuzzy symbol name query to match against, e.g. 'open_in_editor'"
                    }
                },
                "required": ["query"]
            }
        }));
    }

    if allowed.contains(&"explore".to_string()) {
        tools.push(json!({
            "name": "repograph_explore",
            "description": "Explores call graphs and groups code blocks for multiple symbols in a single payload. Provide an array of symbol names or 'path#name' references. This is the primary and most token-efficient search/read tool. Set signature_only for architecture-level questions where the call graph matters and the bodies do not.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "symbols": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Array of symbol references or names, e.g. ['open_in_editor', 'read_file']"
                    },
                    "signature_only": {
                        "type": "boolean",
                        "description": "Return only each symbol's declaration line(s) instead of its full body, alongside the same call-graph edges. Use when mapping structure; call again without it to read an implementation. Default false."
                    },
                    "compact_edges": {
                        "type": "boolean",
                        "description": "Serialize 'paths' as arrow strings ('caller -kind-> callee') instead of objects, removing the repeated JSON keys. Defaults to the value of signature_only. Payload format v1.1.0."
                    }
                },
                "required": ["symbols"]
            }
        }));
    }

    json!({
        "tools": tools
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(entries: &[(&str, &str, &str)]) -> Vec<(String, String, String)> {
        entries
            .iter()
            .map(|(f, n, k)| (f.to_string(), n.to_string(), k.to_string()))
            .collect()
    }

    /// A React store's subscribers only ever appear as local bindings
    /// (`const graph = useGraphStore(...)`). Dropping them outright would
    /// erase the answer to "who uses this", so each file collapses to one edge.
    #[test]
    fn local_variable_callers_collapse_to_one_edge_per_file() {
        let endpoints = collapse_endpoints(
            raw(&[
                ("src/App.tsx", "src/App.tsx", "file"),
                ("src/App.tsx", "load", "variable"),
                ("src/App.tsx", "activeProjectRoot", "variable"),
                ("src/GraphCanvas.tsx", "graph", "variable"),
                ("src/GraphCanvas.tsx", "setHovered", "variable"),
            ]),
            "src/store.ts",
        );
        assert_eq!(
            endpoints,
            vec![
                Endpoint::File { file: "src/App.tsx".to_string() },
                Endpoint::File { file: "src/GraphCanvas.tsx".to_string() },
            ]
        );
    }

    #[test]
    fn named_symbols_survive_and_suppress_the_file_level_edge() {
        let endpoints = collapse_endpoints(
            raw(&[
                ("src/nodes/CustomFileNode.tsx", "src/nodes/CustomFileNode.tsx", "file"),
                ("src/nodes/CustomFileNode.tsx", "CustomFileNode", "component"),
                ("src/nodes/CustomFileNode.tsx", "isDimmed", "variable"),
            ]),
            "src/store.ts",
        );
        assert_eq!(
            endpoints,
            vec![Endpoint::Symbol {
                file: "src/nodes/CustomFileNode.tsx".to_string(),
                name: "CustomFileNode".to_string(),
            }]
        );
    }

    #[test]
    fn noise_inside_the_anchors_own_file_is_dropped_but_real_symbols_are_kept() {
        let noise_only = collapse_endpoints(
            raw(&[
                ("src/store.ts", "src/store.ts", "file"),
                ("src/store.ts", "next", "variable"),
            ]),
            "src/store.ts",
        );
        assert!(noise_only.is_empty(), "self-referential noise, got {noise_only:?}");

        // A genuine same-file caller is signal — common in Rust modules, where
        // most callers live beside the callee.
        let real = collapse_endpoints(
            raw(&[("src/db.rs", "populate_db", "function")]),
            "src/db.rs",
        );
        assert_eq!(
            real,
            vec![Endpoint::Symbol { file: "src/db.rs".to_string(), name: "populate_db".to_string() }]
        );
    }

    #[test]
    fn compact_edges_drop_the_keys_but_keep_the_edge_kind() {
        let edge = crate::db::ExplorePathEdge {
            from_symbol: "src/App.tsx".to_string(),
            to_symbol: "src/store.ts#useGraphStore".to_string(),
            kind: "references".to_string(),
        };
        assert_eq!(
            compact_edge(&edge),
            "src/App.tsx -references-> src/store.ts#useGraphStore"
        );

        // The `calls`/`references` distinction survives compaction — it is the
        // difference between a named caller and a collapsed file.
        let named = crate::db::ExplorePathEdge {
            from_symbol: "src/nodes/CustomFileNode.tsx#CustomFileNode".to_string(),
            to_symbol: "src/store.ts#useGraphStore".to_string(),
            kind: "calls".to_string(),
        };
        assert!(compact_edge(&named).contains(" -calls-> "));

        // Compact form must be strictly smaller than the object form.
        let verbose = serde_json::to_string(&edge).unwrap();
        assert!(
            compact_edge(&edge).len() + 2 < verbose.len(),
            "compact {} vs verbose {}",
            compact_edge(&edge).len(),
            verbose.len()
        );
    }

    #[test]
    fn manifest_schema_version_is_semver_and_separate_from_the_cache_id() {
        assert_eq!(crate::manifest::MANIFEST_SCHEMA_VERSION, "1.2.0");
        // The on-disk cache layout is unchanged; conflating the two would
        // invalidate every existing `.repograph/graph.json`.
        let graph: crate::graph::Graph =
            serde_json::from_str(r#"{"schema_version": 1, "nodes": [], "edges": []}"#).unwrap();
        assert_eq!(graph.schema_version, 1);
    }

    #[test]
    fn signature_block_keeps_the_declaration_and_drops_the_body() {
        let rust: Vec<&str> = vec![
            "pub fn init_db(db_path: &Path) -> Result<Connection> {",
            "    let conn = Connection::open(db_path)?;",
            "    Ok(conn)",
            "}",
        ];
        assert_eq!(
            signature_block(&rust, 1, 4),
            "pub fn init_db(db_path: &Path) -> Result<Connection>"
        );

        let py: Vec<&str> = vec!["def handler(request):", "    return 1"];
        assert_eq!(signature_block(&py, 1, 2), "def handler(request):");

        let ts: Vec<&str> = vec![
            "export const useGraphStore = create<GraphState>((set, get) => ({",
            "  graph: null,",
            "}))",
        ];
        assert_eq!(
            signature_block(&ts, 1, 3),
            "export const useGraphStore = create<GraphState>((set, get) => ("
        );
    }

    #[test]
    fn multi_line_signatures_are_kept_whole_but_bounded() {
        let wrapped: Vec<&str> = vec![
            "pub fn collapse_endpoints(",
            "    raw: Vec<(String, String, String)>,",
            "    anchor_file: &str,",
            ") -> Vec<Endpoint> {",
            "    let mut out = Vec::new();",
        ];
        assert_eq!(
            signature_block(&wrapped, 1, 5),
            "pub fn collapse_endpoints(\n    raw: Vec<(String, String, String)>,\n    anchor_file: &str,\n) -> Vec<Endpoint>"
        );

        // No body opener anywhere: stop at the cap rather than return the file.
        let runaway: Vec<&str> = (0..40).map(|_| "value").collect();
        assert_eq!(signature_block(&runaway, 1, 40).lines().count(), MAX_SIGNATURE_LINES);
    }

    #[test]
    fn signature_block_tolerates_out_of_range_line_numbers() {
        let lines: Vec<&str> = vec!["fn a() {}"];
        assert_eq!(signature_block(&lines, 0, 1), "");
        assert_eq!(signature_block(&lines, 5, 9), "");
        // `end` past EOF clamps instead of panicking.
        assert_eq!(signature_block(&lines, 1, 99), "fn a() {}");
    }

    /// Build a throwaway repo with a graph cache and some files.
    fn temp_repo() -> (tempdir::TempDirGuard, McpServer) {
        let dir = tempdir::TempDirGuard::new("repo-graph-mcp-test");
        let root = dir.path().to_path_buf();
        std::fs::create_dir_all(root.join(".repograph")).unwrap();
        std::fs::create_dir_all(root.join("src/hooks")).unwrap();
        std::fs::create_dir_all(root.join("src/components")).unwrap();
        std::fs::write(
            root.join("src/hooks/useTheme.ts"),
            "export const useTheme = () => {};\n",
        )
        .unwrap();
        std::fs::write(
            root.join("src/components/Button.tsx"),
            "import { useTheme } from '../hooks/useTheme';\nexport const Button = 1;\n",
        )
        .unwrap();
        std::fs::write(
            root.join(".repograph/graph.json"),
            r#"{
              "schema_version": 1,
              "nodes": [
                {"path": "src/components/Button.tsx", "exports": ["Button"]},
                {"path": "src/hooks/useTheme.ts", "exports": ["useTheme"]}
              ],
              "edges": [
                {"from_path": "src/components/Button.tsx",
                 "to_path": "src/hooks/useTheme.ts", "kind": "import"}
              ]
            }"#,
        )
        .unwrap();
        let server = McpServer::new(&root).unwrap();
        (dir, server)
    }

    /// Minimal RAII temp dir so we don't need the `tempfile` crate.
    mod tempdir {
        use std::path::{Path, PathBuf};

        pub struct TempDirGuard(PathBuf);

        impl TempDirGuard {
            pub fn new(prefix: &str) -> TempDirGuard {
                let nanos = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos();
                let p = std::env::temp_dir().join(format!("{prefix}-{nanos}"));
                std::fs::create_dir_all(&p).unwrap();
                TempDirGuard(p)
            }
            pub fn path(&self) -> &Path {
                &self.0
            }
        }

        impl Drop for TempDirGuard {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }

    #[test]
    fn read_file_serves_in_repo_files() {
        let (_g, server) = temp_repo();
        let content = server.read_file("src/hooks/useTheme.ts").unwrap();
        assert!(content.contains("useTheme"));
    }

    #[test]
    fn read_file_rejects_path_traversal() {
        let (_g, server) = temp_repo();
        // A file that definitely exists outside the repo root.
        let outside = std::env::temp_dir().join("repo-graph-outside.txt");
        std::fs::write(&outside, "secret").unwrap();
        for attempt in [
            "../repo-graph-outside.txt".to_string(),
            "..\\repo-graph-outside.txt".to_string(),
            "src/../../repo-graph-outside.txt".to_string(),
            outside.display().to_string(), // absolute path
        ] {
            let err = server.read_file(&attempt).unwrap_err();
            assert!(
                err.code == "path_outside_root" || err.code == "file_not_found",
                "attempt {attempt:?} produced unexpected error {err:?}"
            );
        }
        let _ = std::fs::remove_file(outside);
    }

    #[test]
    fn find_dependents_reports_incoming_edges() {
        let (_g, mut server) = temp_repo();
        let out = server.find_dependents("src/hooks/useTheme.ts").unwrap();
        assert!(out.contains("- /src/components/Button.tsx"));
        let err = server.find_dependents("src/does-not-exist.ts").unwrap_err();
        assert_eq!(err.code, "unknown_path");
    }

    #[test]
    fn get_manifest_renders_markdown_and_scopes() {
        let (_g, mut server) = temp_repo();
        let md = server.get_manifest(None).unwrap();
        assert!(md.starts_with("## Project Architecture Map"));
        assert!(
            md.contains(
                "/src/components/Button.tsx (Exports: Button) (Depends on: /src/hooks/useTheme.ts)"
            ),
            "{md}"
        );
        let scoped = server.get_manifest(Some("src/hooks/**")).unwrap();
        assert!(scoped.contains("/src/hooks/useTheme.ts"));
        assert!(!scoped.contains("Button.tsx"));
        let err = server.get_manifest(Some("nope/**")).unwrap_err();
        assert_eq!(err.code, "empty_scope");
    }

    /// The ranking filters have to be reachable through the JSON-RPC surface,
    /// not just the Rust API — that is the only path an agent has.
    #[test]
    fn repograph_files_accepts_ranking_filters_over_the_wire() {
        let (_g, mut server) = temp_repo();
        let opts = ManifestOptions {
            top_k: Some(1),
            ..Default::default()
        };
        let md = server.get_manifest_with(&opts).unwrap();
        assert_eq!(md.matches("\n- /").count(), 1, "top_k=1 kept extra files:\n{md}");
        assert!(
            md.contains("Showing 1 of 2 files"),
            "truncation must be announced:\n{md}"
        );

        // A threshold nothing can satisfy is a distinct, explained failure —
        // not an empty map the agent would read as an empty repo.
        let err = server
            .get_manifest_with(&ManifestOptions {
                min_rank: Some(1.5),
                ..Default::default()
            })
            .unwrap_err();
        assert_eq!(err.code, "empty_rank_filter");
    }

    #[test]
    fn missing_cache_is_a_structured_error() {
        let dir = tempdir::TempDirGuard::new("repo-graph-nocache");
        let mut server = McpServer::new(dir.path()).unwrap();
        let err = server.get_manifest(None).unwrap_err();
        assert_eq!(err.code, "graph_cache_missing");
    }

    #[test]
    fn jsonrpc_initialize_list_and_call_roundtrip() {
        let (_g, mut server) = temp_repo();
        let init = server
            .handle_message(
                r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
            )
            .unwrap();
        assert_eq!(init["result"]["serverInfo"]["name"], "repo-graph");
        assert!(init["result"]["instructions"].as_str().unwrap().contains("repograph_explore"));

        // Notification → no response.
        assert!(server
            .handle_message(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
            .is_none());

        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("REPOGRAPH_MCP_TOOLS", "explore,files,node");

        let call = server
            .handle_message(
                r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"repograph_files","arguments":{}}}"#,
            )
            .unwrap();
        assert_eq!(call["result"]["isError"], false);
        assert!(call["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Project Architecture Map"));

        // Traversal through the JSON-RPC surface is a tool error, not a panic.
        let bad = server
            .handle_message(
                r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"repograph_node","arguments":{"path":"../../etc/passwd"}}}"#,
            )
            .unwrap();
        assert_eq!(bad["result"]["isError"], true);

        std::env::remove_var("REPOGRAPH_MCP_TOOLS");
    }

    /// `REPOGRAPH_MCP_TOOLS` is process-global, and Rust runs tests on many
    /// threads. Every test that touches it takes this lock first.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Filtering `tools/list` alone was cosmetic: the names are published in
    /// this repo's docs and in the config snippet the desktop app generates,
    /// so any client could still invoke a "hidden" tool by name.
    #[test]
    fn the_allowlist_gates_dispatch_not_just_discovery() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (_g, mut server) = temp_repo();

        std::env::set_var("REPOGRAPH_MCP_TOOLS", "explore");
        let listed: Vec<String> = tools_list_result()["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(listed, vec!["repograph_explore".to_string()]);

        // Unlisted, but a caller can still name it.
        let blocked = server
            .handle_message(
                r#"{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"repograph_files","arguments":{}}}"#,
            )
            .unwrap();
        assert_eq!(blocked["error"]["code"], -32601, "{blocked}");

        // Enabling it makes the very same call succeed.
        std::env::set_var("REPOGRAPH_MCP_TOOLS", "explore,files");
        let allowed = server
            .handle_message(
                r#"{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"repograph_files","arguments":{}}}"#,
            )
            .unwrap();
        assert_eq!(allowed["result"]["isError"], false, "{allowed}");

        std::env::remove_var("REPOGRAPH_MCP_TOOLS");
    }

    #[test]
    fn default_listing_is_the_single_compound_tool() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("REPOGRAPH_MCP_TOOLS");
        let names: Vec<String> = tools_list_result()["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(names, vec!["repograph_explore".to_string()]);

        std::env::set_var("REPOGRAPH_MCP_TOOLS", "explore,search,node");
        let filtered: Vec<String> = tools_list_result()["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            filtered,
            vec![
                "repograph_node".to_string(),
                "repograph_search".to_string(),
                "repograph_explore".to_string()
            ]
        );
        std::env::remove_var("REPOGRAPH_MCP_TOOLS");
    }

    /// A `.repograph` cache built before the walker learned the secrets
    /// denylist still lists `.env` as a node, so the read boundary has to
    /// refuse it independently of how the index was built.
    #[test]
    fn read_file_refuses_credential_files_even_when_indexed() {
        let (guard, server) = temp_repo();
        std::fs::write(guard.path().join(".env"), "TOKEN=secret").unwrap();

        let err = server.read_file(".env").unwrap_err();
        assert_eq!(err.code, "secret_file_blocked");

        // Committed templates are still readable — they are documentation.
        std::fs::write(guard.path().join(".env.example"), "TOKEN=").unwrap();
        assert!(server.read_file(".env.example").is_ok());
    }

    #[test]
    fn sanitize_windows_path_removes_extended_prefixes() {
        assert_eq!(
            sanitize_windows_path("//?/C:/My-pro/syscolors"),
            PathBuf::from(format!("C:{}My-pro{}syscolors", std::path::MAIN_SEPARATOR, std::path::MAIN_SEPARATOR))
        );
        assert_eq!(
            sanitize_windows_path("\\\\?\\C:\\My-pro\\syscolors"),
            PathBuf::from(format!("C:{}My-pro{}syscolors", std::path::MAIN_SEPARATOR, std::path::MAIN_SEPARATOR))
        );
        assert_eq!(
            sanitize_windows_path("C:/My-pro/syscolors"),
            PathBuf::from(format!("C:{}My-pro{}syscolors", std::path::MAIN_SEPARATOR, std::path::MAIN_SEPARATOR))
        );
    }
}
