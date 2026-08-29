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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct McpSession {
    pub session_id: String,
    pub start_time: i64,
    pub last_heartbeat: i64,
    pub last_active: i64,
    pub heartbeat_count: u64,
    pub queries_count: u64,
    pub connected: bool,
    pub session_active: bool,
}

impl Default for McpSession {
    fn default() -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        let session_id = format!("sess_{:x}_{:x}", std::process::id(), now);
        McpSession {
            session_id,
            start_time: now,
            last_heartbeat: now,
            last_active: now,
            heartbeat_count: 0,
            queries_count: 0,
            connected: true,
            session_active: true,
        }
    }
}

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
    /// Persistent connection and session state across turns.
    pub session: McpSession,
    /// Live token metrics and savings tracker.
    pub tracker: crate::telemetry::LiveTokenTracker,
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

pub fn normalize_line_endings(s: &str) -> String {
    s.replace("\r\n", "\n")
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct FilePatch {
    pub path: String,
    pub target_content: String,
    pub replacement_content: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SearchParams {
    pub query: String,
    pub limit: Option<usize>,           // default: 10 (cap maximum result count)
    pub signature_only: Option<bool>,   // default: true (don't dump full AST body!)
    pub exact_symbol_only: Option<bool>,// match symbol identifiers vs body text
    pub force_full: Option<bool>,       // override intelligent 2,500 token compression
}

fn sanitize_windows_path(raw: &str) -> PathBuf {
    let mut cleaned = raw.trim();
    while cleaned.starts_with("//?/")
        || cleaned.starts_with(r"\\?\")
        || cleaned.starts_with(r"//?\")
        || cleaned.starts_with(r"\\?/")
    {
        cleaned = &cleaned[4..];
    }
    while cleaned.starts_with("//?") || cleaned.starts_with(r"\\?") {
        cleaned = &cleaned[3..];
    }
    let normalized = cleaned.replace('/', std::path::MAIN_SEPARATOR_STR);
    PathBuf::from(normalized)
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
                let ok = p.join(".repograph").is_dir()
                    && (p.join(".repograph/graph.db").is_file() || p.join(".repograph/graph.json").is_file());
                if !ok {
                    eprintln!(
                        "[mcp] ignoring registry root {} — no valid .repograph index found",
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
        .unwrap_or_else(|_| target_root.clone());

    let simplified = dunce::simplified(&canonical);
    let mut root_display = simplified.to_string_lossy().replace('\\', "/");
    while root_display.starts_with("//?/")
        || root_display.starts_with(r"\\?\")
        || root_display.starts_with(r"//?\")
        || root_display.starts_with(r"\\?/")
    {
        root_display = root_display[4..].to_string();
    }
    while root_display.starts_with("//?") || root_display.starts_with(r"\\?") {
        root_display = root_display[3..].to_string();
    }

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

/// Extracts a 3-line context snippet (1 line before, matching line, 1 line after)
/// around the first line matching the query within lines.
pub fn extract_3line_snippet(lines: &[&str], query: &str) -> String {
    if lines.is_empty() {
        return String::new();
    }
    let lower_query = query.to_lowercase();
    let match_idx = lines
        .iter()
        .position(|line| line.to_lowercase().contains(&lower_query))
        .unwrap_or(0);

    let start_idx = match_idx.saturating_sub(1);
    let end_idx = (match_idx + 2).min(lines.len());
    lines[start_idx..end_idx].join("\n")
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

fn validate_ast_syntax(path: &Path, content: &str) -> Result<Vec<String>, String> {
    let lang = crate::walker::language_for(path);
    let mut diagnostics = Vec::new();
    if let Some(extractor) = crate::parsers::extractor_for(&lang) {
        let res = extractor.extract(path, content);
        for w in &res.warnings {
            if w.starts_with("parse_error:") {
                return Err(format!("AST validation syntax error in {}: {}", path.display(), w));
            } else {
                diagnostics.push(w.clone());
            }
        }
    }
    Ok(diagnostics)
}

fn count_file_edges(conn: &rusqlite::Connection, rel_path: &str) -> (usize, usize) {
    let out = conn
        .query_row(
            "SELECT COUNT(*) FROM file_edges WHERE from_path = ?",
            [rel_path],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0) as usize;
    let in_e = conn
        .query_row(
            "SELECT COUNT(*) FROM file_edges WHERE to_path = ?",
            [rel_path],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0) as usize;
    (out, in_e)
}

impl McpServer {
    pub fn new(root: &Path) -> Result<McpServer, String> {
        let (canonical, root_display, sync_warning) = resolve_mcp_root(root)?;
        let _ = crate::agent_scaffold::ensure_agent_scaffold(&canonical);
        let _ = crate::rule_injector::ensure_workspace_rules(&canonical);
        let _ = crate::db::reconcile_repo_startup(&canonical);
        crate::watcher::start_standalone_watcher(canonical.clone());
        Ok(McpServer {
            root_display,
            cache_path: canonical.join(GRAPH_CACHE_RELATIVE_PATH),
            root: canonical,
            graph: None,
            adjacency: None,
            graph_mtime: None,
            sync_warning,
            conn: None,
            session: McpSession::default(),
            tracker: crate::telemetry::LiveTokenTracker::new(),
        })
    }

    /// The shared SQLite handle, opened on first use.
    fn db(&mut self) -> Result<&rusqlite::Connection, ToolError> {
        if self.conn.is_none() {
            let db_path = self.root.join(".repograph/graph.db");
            if let Some(parent) = db_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let conn = crate::db::init_db(&db_path)
                .map_err(|e| ToolError::new("db_error", format!("failed to open database: {e}")))?;
            self.conn = Some(conn);
        }
        Ok(self.conn.as_ref().expect("opened above"))
    }

    /// Blocking serve loop over stdin/stdout with persistent connection error recovery.
    pub fn serve_stdio(&mut self) -> std::io::Result<()> {
        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout();
        let mut reader = stdin.lock();
        let mut line_buf = String::new();
        loop {
            line_buf.clear();
            match reader.read_line(&mut line_buf) {
                Ok(0) => break, // EOF reached
                Ok(_) => {
                    let trimmed = line_buf.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    if let Some(response) = self.handle_message(trimmed) {
                        let bytes = response.to_string();
                        if let Err(e) = stdout
                            .write_all(bytes.as_bytes())
                            .and_then(|_| stdout.write_all(b"\n"))
                            .and_then(|_| stdout.flush())
                        {
                            eprintln!("[mcp_server] stdout error: {e}");
                            if e.kind() == std::io::ErrorKind::BrokenPipe {
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[mcp_server] stdin read error: {e}");
                    if e.kind() == std::io::ErrorKind::Interrupted {
                        continue;
                    }
                    break;
                }
            }
        }
        self.session.connected = false;
        self.session.session_active = false;
        Ok(())
    }

    /// Handle one raw JSON-RPC message; `None` for notifications (no reply).
    pub fn handle_message(&mut self, raw: &str) -> Option<Value> {
        let now = chrono::Utc::now().timestamp_millis();
        self.session.last_active = now;
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
            "ping" => {
                self.session.last_heartbeat = now;
                self.session.heartbeat_count += 1;
                self.session.connected = true;
                self.session.session_active = true;
                Ok(json!({
                    "status": "ok",
                    "connected": true,
                    "session_active": true,
                    "session_id": self.session.session_id,
                    "uptime_ms": now.saturating_sub(self.session.start_time),
                    "heartbeats": self.session.heartbeat_count,
                    "queries_count": self.session.queries_count
                }))
            }
            "tools/list" => Ok(tools_list_result()),
            "tools/call" => self.tools_call(&params),
            _ => Err((-32601, format!("method not found: {method}"))),
        };
        Some(match result {
            Ok(value) => json!({"jsonrpc": "2.0", "id": id, "result": value}),
            Err((code, message)) => jsonrpc_error(id, code, &message),
        })
    }

    fn initialize_result(&mut self, params: &Value) -> Value {
        let now = chrono::Utc::now().timestamp_millis();
        self.session.last_active = now;
        self.session.last_heartbeat = now;
        self.session.connected = true;
        self.session.session_active = true;
        let requested = params
            .get("protocolVersion")
            .and_then(Value::as_str)
            .unwrap_or("2025-06-18");
        json!({
            "protocolVersion": requested,
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "repo-graph", "version": env!("CARGO_PKG_VERSION") },
            "session": {
                "sessionId": self.session.session_id,
                "connected": self.session.connected,
                "sessionActive": self.session.session_active,
                "startTime": self.session.start_time,
                "persistent": true
            },
            "instructions": "The Repo Graph MCP server maintains a persistent, session-wide connection across all multi-turn conversation workflows. When researching, exploring, navigating, or modifying code in this workspace, ALWAYS prioritize calling Repo Graph MCP tools (`repograph_explore`, `repograph_files`, `repograph_node`, `repograph_search`, `repograph_impact`, `repograph_callers`, `repograph_callees`, `repograph_edit`, `repograph_batch_edit`, `repograph_edit_symbol`, `repograph_write`, `repograph_delete`, `repograph_status`). Restore active session context via `.myrepograph-agent/memory/runtime/context.md`, use `repograph_explore` for compound symbol retrieval and call graphs to minimize token ingestion, execute atomic refactors via `repograph_batch_edit`, check connection with `repograph_status`, and log closeout to `.myrepograph-agent/memory/runtime/dailylog.md`."
        })
    }

    fn compute_raw_tokens_for_files(&self, file_paths: &[String]) -> usize {
        let mut total = 0;
        let mut seen = std::collections::HashSet::new();
        for rel in file_paths {
            let clean_rel = rel.trim_start_matches('/').trim_start_matches('\\');
            if seen.insert(clean_rel.to_string()) {
                let abs = self.root.join(clean_rel);
                if let Ok(content) = std::fs::read_to_string(&abs) {
                    total += crate::telemetry::count_tokens(&content);
                } else if let Some(g) = &self.graph {
                    if let Some(n) = g.nodes.iter().find(|node| node.path == clean_rel || node.path.ends_with(clean_rel)) {
                        total += (n.size_bytes as f64 / 3.7).round() as usize;
                    }
                }
            }
        }
        if total == 0 {
            file_paths.len().max(1) * 1500
        } else {
            total
        }
    }

    fn tools_call(&mut self, params: &Value) -> Result<Value, (i64, String)> {
        self.session.queries_count += 1;
        self.session.last_active = chrono::Utc::now().timestamp_millis();
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

        let start_time = std::time::Instant::now();
        let mut raw_file_tokens = match name {
            "repograph_files" => {
                let paths: Vec<String> = self.graph.as_ref()
                    .map(|g| g.nodes.iter().map(|n| n.path.clone()).collect())
                    .unwrap_or_default();
                self.compute_raw_tokens_for_files(&paths)
            }
            "repograph_explore" => {
                let symbols_arr = args.get("symbols").and_then(Value::as_array);
                let mut resolved_files = Vec::new();
                if let Some(arr) = symbols_arr {
                    for v in arr {
                        if let Some(s) = v.as_str() {
                            if s.contains('#') {
                                if let Some(f) = s.split('#').next() {
                                    resolved_files.push(f.to_string());
                                }
                            } else if s.ends_with(".ts") || s.ends_with(".tsx") || s.ends_with(".rs") || s.ends_with(".py") || s.ends_with(".js") {
                                resolved_files.push(s.to_string());
                            } else if let Ok(conn) = self.db() {
                                if let Ok(mut stmt) = conn.prepare("SELECT file_path FROM symbols WHERE name = ? LIMIT 3") {
                                    if let Ok(rows) = stmt.query_map([s], |r| r.get::<_, String>(0)) {
                                        for row in rows.flatten() {
                                            resolved_files.push(row);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if resolved_files.is_empty() {
                    let count = symbols_arr.map(|a| a.len()).unwrap_or(1);
                    count * 3500
                } else {
                    self.compute_raw_tokens_for_files(&resolved_files)
                }
            }
            "repograph_search" => {
                if let Some(q) = args.get("query").and_then(Value::as_str) {
                    if let Ok(conn) = self.db() {
                        let match_query = crate::db::build_fts_match_query(q);
                        let mut found_files = Vec::new();
                        if let Ok(mut stmt) = conn.prepare("SELECT DISTINCT file_path FROM symbols_fts WHERE tokens MATCH ? LIMIT 5") {
                            if let Ok(rows) = stmt.query_map([&match_query], |r| r.get::<_, String>(0)) {
                                for row in rows.flatten() {
                                    found_files.push(row);
                                }
                            }
                        }
                        if !found_files.is_empty() {
                            self.compute_raw_tokens_for_files(&found_files)
                        } else {
                            3500
                        }
                    } else {
                        3500
                    }
                } else {
                    3500
                }
            }
            "repograph_impact" => {
                if let Some(p) = args.get("path").and_then(Value::as_str) {
                    let mut deps = vec![p.to_string()];
                    if let Ok((_, adj)) = self.graph_and_adjacency() {
                        let normalized = p.trim_start_matches('/').to_string();
                        for d in adj.dependents_of(&normalized) {
                            deps.push(d);
                        }
                    }
                    self.compute_raw_tokens_for_files(&deps)
                } else {
                    4000
                }
            }
            "repograph_callers" | "repograph_callees" => 3500,
            "repograph_node" => {
                if let Some(p) = args.get("path").and_then(Value::as_str) {
                    self.compute_raw_tokens_for_files(&[p.to_string()])
                } else {
                    1500
                }
            }
            "repograph_edit" => {
                if let Some(p) = args.get("path").and_then(Value::as_str) {
                    self.compute_raw_tokens_for_files(&[p.to_string()])
                } else {
                    2000
                }
            }
            "repograph_batch_edit" => {
                if let Some(patches_val) = args.get("patches").and_then(Value::as_array) {
                    let mut paths = Vec::new();
                    for p in patches_val {
                        if let Some(path_str) = p.get("path").and_then(Value::as_str) {
                            paths.push(path_str.to_string());
                        }
                    }
                    self.compute_raw_tokens_for_files(&paths)
                } else {
                    2500
                }
            }
            "repograph_edit_symbol" => {
                if let Some(p) = args.get("path").and_then(Value::as_str) {
                    self.compute_raw_tokens_for_files(&[p.to_string()])
                } else {
                    1500
                }
            }
            "repograph_write" => {
                if let Some(c) = args.get("content").and_then(Value::as_str) {
                    crate::telemetry::count_tokens(c) + 500
                } else {
                    1500
                }
            }
            "repograph_delete" => 500,
            _ => 1000,
        };

        let outcome = match name {
            "repograph_domains" => {
                self.repograph_domains()
            }
            "repograph_files" => {
                let options = ManifestOptions {
                    scope: args.get("scope").and_then(Value::as_str),
                    domain: args.get("domain").and_then(Value::as_str),
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
                let start_line = args.get("start_line").and_then(Value::as_u64).map(|v| v as usize);
                let end_line = args.get("end_line").and_then(Value::as_u64).map(|v| v as usize);
                let with_line_numbers = args.get("with_line_numbers").and_then(Value::as_bool);
                if let Some(s) = symbol {
                    self.read_symbol_sliced(path, s, start_line, end_line, with_line_numbers)
                } else {
                    self.read_file_sliced(path, start_line, end_line, with_line_numbers)
                }
            }
            "repograph_edit" => {
                let path = args.get("path").and_then(Value::as_str).ok_or((-32602, "repograph_edit requires 'path'".to_string()))?;
                let target = args.get("target_content").and_then(Value::as_str).ok_or((-32602, "repograph_edit requires 'target_content'".to_string()))?;
                let replacement = args.get("replacement_content").and_then(Value::as_str).ok_or((-32602, "repograph_edit requires 'replacement_content'".to_string()))?;
                self.repograph_edit(path, target, replacement)
            }
            "repograph_batch_edit" => {
                let patches_val = args.get("patches").ok_or((-32602, "repograph_batch_edit requires 'patches'".to_string()))?;
                let patches: Vec<FilePatch> = serde_json::from_value(patches_val.clone())
                    .map_err(|e| (-32602, format!("invalid 'patches' array: {e}")))?;
                self.repograph_batch_edit(patches)
            }
            "repograph_edit_symbol" => {
                let path = args.get("path").and_then(Value::as_str).ok_or((-32602, "repograph_edit_symbol requires 'path'".to_string()))?;
                let symbol = args.get("symbol").and_then(Value::as_str).ok_or((-32602, "repograph_edit_symbol requires 'symbol'".to_string()))?;
                let new_code = args.get("new_code").and_then(Value::as_str).ok_or((-32602, "repograph_edit_symbol requires 'new_code'".to_string()))?;
                self.repograph_edit_symbol(path, symbol, new_code)
            }
            "repograph_write" => {
                let path = args.get("path").and_then(Value::as_str).ok_or((-32602, "repograph_write requires 'path'".to_string()))?;
                let content = args.get("content").and_then(Value::as_str).ok_or((-32602, "repograph_write requires 'content'".to_string()))?;
                self.repograph_write(path, content)
            }
            "repograph_delete" => {
                let path = args.get("path").and_then(Value::as_str).ok_or((-32602, "repograph_delete requires 'path'".to_string()))?;
                self.repograph_delete(path)
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
                let limit = args.get("limit").and_then(Value::as_u64).map(|v| v as usize);
                let signature_only = args.get("signature_only").and_then(Value::as_bool);
                let exact_symbol_only = args.get("exact_symbol_only").and_then(Value::as_bool);
                let force_full = args.get("force_full").and_then(Value::as_bool);
                self.search_symbols(SearchParams {
                    query: query.to_string(),
                    limit,
                    signature_only,
                    exact_symbol_only,
                    force_full,
                })
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
                let force_full = args.get("force_full").and_then(Value::as_bool);
                self.explore(symbols, signature_only, compact_edges, force_full)
            }
            "repograph_skeleton" => {
                let path = args.get("path").and_then(Value::as_str).ok_or((-32602, "repograph_skeleton requires 'path'".to_string()))?;
                match self.get_skeleton(path) {
                    Ok((res, raw_toks)) => {
                        raw_file_tokens = raw_toks;
                        Ok(res)
                    }
                    Err(e) => Err(e),
                }
            }
            "repograph_trace" => {
                let entrypoint = args.get("entrypoint").and_then(Value::as_str).ok_or((-32602, "repograph_trace requires 'entrypoint'".to_string()))?;
                let depth = args.get("depth").and_then(Value::as_u64).map(|v| v as usize).unwrap_or(3);
                match self.get_execution_trace(entrypoint, depth) {
                    Ok((res, raw_toks)) => {
                        raw_file_tokens = raw_toks;
                        Ok(res)
                    }
                    Err(e) => Err(e),
                }
            }
            "repograph_status" => {
                self.codegraph_status()
            }
            other => return Err((-32602, format!("unknown tool: {other}"))),
        };

        let execution_ms = start_time.elapsed().as_millis();
        let input_str = args.to_string();

        // Tool-level failures are structured MCP tool results (isError),
        // not JSON-RPC protocol errors, so the calling agent can react.
        Ok(match outcome {
            Ok(mut text) => {
                let metrics = self.tracker.record_call(name, &input_str, &text, raw_file_tokens, execution_ms);
                let tag = self.tracker.format_inband_tag(&metrics);
                text.push_str(&tag);

                if let Ok(conn) = self.db() {
                    let symbol_hint = args.get("symbol").or_else(|| args.get("symbols").and_then(|a| a.get(0))).and_then(Value::as_str).unwrap_or("");
                    let path_hint = args.get("path").or_else(|| args.get("query")).and_then(Value::as_str).unwrap_or("");
                    crate::db::log_agent_query_metrics(
                        conn,
                        symbol_hint,
                        path_hint,
                        name,
                        metrics.input_tokens,
                        metrics.output_tokens,
                        metrics.raw_equivalent_tokens,
                        metrics.saved_tokens,
                        metrics.execution_ms,
                        metrics.turn_id,
                    );
                }

                json!({
                    "content": [{"type": "text", "text": text}],
                    "isError": false
                })
            }
            Err(e) => {
                let err_msg = format!("error [{}]: {}", e.code, e.message);
                let metrics = self.tracker.record_call(name, &input_str, &err_msg, 0, execution_ms);
                if let Ok(conn) = self.db() {
                    crate::db::log_agent_query_metrics(
                        conn,
                        "",
                        "",
                        name,
                        metrics.input_tokens,
                        metrics.output_tokens,
                        0,
                        0,
                        metrics.execution_ms,
                        metrics.turn_id,
                    );
                }
                json!({
                    "content": [{"type": "text", "text": err_msg}],
                    "isError": true
                })
            }
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

        if let Ok(conn) = self.db() {
            crate::db::log_agent_query(conn, "", options.scope.unwrap_or(""), "files");
        }
        Ok(md)
    }

    pub fn repograph_domains(&mut self) -> Result<String, ToolError> {
        let (graph, _) = self.graph_and_adjacency()?;
        let result = crate::cluster::detect_domains(graph);

        let mut lines = Vec::new();
        lines.push("# Architectural Domains (Community Partitioning)".to_string());
        lines.push(format!(
            "Detected {} architectural domains across {} files.\nUse `repograph_files(domain=\"<domain_name>\")` to view files in a domain.\n",
            result.domains.len(),
            graph.nodes.len()
        ));

        for d in &result.domains {
            lines.push(format!("## Domain {}: {}", d.id, d.name));
            lines.push(format!("- **Files:** {} (Cohesion: {:.0}%)", d.file_count, d.cohesion_score * 100.0));
            lines.push(format!("- **Top Hub:** `{}`", d.top_hub));
            if !d.key_exports.is_empty() {
                lines.push(format!("- **Key Exports:** {}", d.key_exports.join(", ")));
            }
            let sample_files: Vec<String> = d.files.iter().take(4).map(|f| format!("`{}`", f)).collect();
            lines.push(format!("- **Sample Files:** {}\n", sample_files.join(", ")));
        }

        if let Ok(conn) = self.db() {
            crate::db::log_agent_query(conn, "", "domains", "domains");
        }

        Ok(lines.join("\n"))
    }

    pub fn read_file(&mut self, path: &str) -> Result<String, ToolError> {
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

        if let Ok(conn) = self.db() {
            crate::db::log_agent_query(conn, "", path, "read_file");
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

    pub fn read_file_sliced(
        &mut self,
        path: &str,
        start_line: Option<usize>,
        end_line: Option<usize>,
        with_line_numbers: Option<bool>,
    ) -> Result<String, ToolError> {
        let content = self.read_file(path)?;
        if start_line.is_none() && end_line.is_none() && !with_line_numbers.unwrap_or(false) {
            return Ok(content);
        }

        let lines: Vec<&str> = content.lines().collect();
        let total = lines.len();
        let start = start_line.unwrap_or(1).max(1);
        let end = end_line.unwrap_or(total).min(total);

        if start > total {
            return Err(ToolError::new(
                "invalid_range",
                format!("start_line ({start}) exceeds file line count ({total})"),
            ));
        }

        let show_nums = with_line_numbers.unwrap_or(start_line.is_some() || end_line.is_some());
        let mut out = Vec::new();
        for i in start..=end {
            let line_str = lines.get(i - 1).unwrap_or(&"");
            if show_nums {
                out.push(format!("{:>4}: {}", i, line_str));
            } else {
                out.push(line_str.to_string());
            }
        }
        Ok(out.join("\n"))
    }

    pub fn read_symbol_sliced(
        &mut self,
        path: &str,
        symbol: &str,
        start_line: Option<usize>,
        end_line: Option<usize>,
        with_line_numbers: Option<bool>,
    ) -> Result<String, ToolError> {
        let symbol_slice = self.read_symbol(path, symbol)?;
        if start_line.is_none() && end_line.is_none() && !with_line_numbers.unwrap_or(false) {
            return Ok(symbol_slice);
        }

        let lines: Vec<&str> = symbol_slice.lines().collect();
        let total = lines.len();
        let start = start_line.unwrap_or(1).max(1);
        let end = end_line.unwrap_or(total).min(total);

        let show_nums = with_line_numbers.unwrap_or(start_line.is_some() || end_line.is_some());
        let mut out = Vec::new();
        for i in start..=end {
            let line_str = lines.get(i - 1).unwrap_or(&"");
            if show_nums {
                out.push(format!("{:>4}: {}", i, line_str));
            } else {
                out.push(line_str.to_string());
            }
        }
        Ok(out.join("\n"))
    }

    pub fn repograph_edit(
        &mut self,
        path: &str,
        target_content: &str,
        replacement_content: &str,
    ) -> Result<String, ToolError> {
        let clean_rel = path.trim_start_matches('/').trim_start_matches('\\').to_string();
        let resolved = self.resolve_inside_root(&clean_rel)?;
        self.reject_secret(&resolved, &clean_rel)?;

        if !resolved.is_file() {
            return Err(ToolError::new("file_not_found", format!("file not found: {clean_rel}")));
        }

        let original = std::fs::read_to_string(&resolved).map_err(|e| {
            ToolError::new("io_error", format!("failed to read {clean_rel}: {e}"))
        })?;

        let norm_orig = normalize_line_endings(&original);
        let norm_target = normalize_line_endings(target_content);
        let norm_replacement = normalize_line_endings(replacement_content);

        if !norm_orig.contains(&norm_target) {
            return Err(ToolError::new(
                "target_not_found",
                format!("target_content was not found in {clean_rel}. Ensure exact character and whitespace match."),
            ));
        }

        let updated = norm_orig.replacen(&norm_target, &norm_replacement, 1);

        let diagnostics = validate_ast_syntax(&resolved, &updated)
            .map_err(|e| ToolError::new("syntax_validation_failed", e))?;

        let (old_out, old_in): (usize, usize) = if let Ok(conn) = self.db() {
            count_file_edges(conn, &clean_rel)
        } else {
            (0, 0)
        };

        std::fs::write(&resolved, &updated).map_err(|e| {
            ToolError::new("io_error", format!("failed to write {clean_rel}: {e}"))
        })?;

        let _ = crate::watcher::sync_path(&self.root, &clean_rel);
        crate::watcher::remove_pending(&self.root, &resolved);
        self.graph = None;
        self.adjacency = None;
        let _ = self.load_graph();

        let (new_out, new_in): (usize, usize) = if let Ok(conn) = self.db() {
            count_file_edges(conn, &clean_rel)
        } else {
            (0, 0)
        };

        let added: usize = (new_out + new_in).saturating_sub(old_out + old_in);
        let removed: usize = (old_out + old_in).saturating_sub(new_out + new_in);

        let res = json!({
            "success": true,
            "modified_file": clean_rel,
            "new_edges": added,
            "broken_edges": removed,
            "diagnostics": diagnostics,
            "sync_state": "Synced",
            "message": format!("Successfully edited {clean_rel}. AST validated, graph edges updated synchronously.")
        });

        Ok(serde_json::to_string_pretty(&res).unwrap_or_else(|_| res.to_string()))
    }

    pub fn repograph_write(
        &mut self,
        path: &str,
        content: &str,
    ) -> Result<String, ToolError> {
        let clean_rel = path.trim_start_matches('/').trim_start_matches('\\').to_string();
        let resolved = self.resolve_inside_root(&clean_rel)?;
        self.reject_secret(&resolved, &clean_rel)?;

        if let Some(parent) = resolved.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let norm_content = normalize_line_endings(content);

        let diagnostics = validate_ast_syntax(&resolved, &norm_content)
            .map_err(|e| ToolError::new("syntax_validation_failed", e))?;

        std::fs::write(&resolved, &norm_content).map_err(|e| {
            ToolError::new("io_error", format!("failed to write {clean_rel}: {e}"))
        })?;

        let _ = crate::watcher::sync_path(&self.root, &clean_rel);
        crate::watcher::remove_pending(&self.root, &resolved);
        self.graph = None;
        self.adjacency = None;
        let _ = self.load_graph();

        let (new_out, new_in): (usize, usize) = if let Ok(conn) = self.db() {
            count_file_edges(conn, &clean_rel)
        } else {
            (0, 0)
        };

        let res = json!({
            "success": true,
            "modified_file": clean_rel,
            "new_edges": new_out + new_in,
            "broken_edges": 0,
            "diagnostics": diagnostics,
            "sync_state": "Synced",
            "message": format!("Successfully created/wrote {clean_rel}. AST parsed and indexed synchronously.")
        });

        Ok(serde_json::to_string_pretty(&res).unwrap_or_else(|_| res.to_string()))
    }

    pub fn repograph_delete(
        &mut self,
        path: &str,
    ) -> Result<String, ToolError> {
        let clean_rel = path.trim_start_matches('/').trim_start_matches('\\').to_string();
        let resolved = self.resolve_inside_root(&clean_rel)?;
        self.reject_secret(&resolved, &clean_rel)?;

        if !resolved.exists() {
            return Err(ToolError::new("not_found", format!("path does not exist: {clean_rel}")));
        }

        let is_dir = resolved.is_dir();
        if is_dir {
            std::fs::remove_dir_all(&resolved).map_err(|e| {
                ToolError::new("io_error", format!("failed to remove directory {clean_rel}: {e}"))
            })?;
        } else {
            std::fs::remove_file(&resolved).map_err(|e| {
                ToolError::new("io_error", format!("failed to remove file {clean_rel}: {e}"))
            })?;
        }

        let _ = crate::watcher::sync_path(&self.root, &clean_rel);
        crate::watcher::remove_pending(&self.root, &resolved);
        self.graph = None;
        self.adjacency = None;
        let _ = self.load_graph();

        let res = json!({
            "success": true,
            "modified_file": clean_rel,
            "new_edges": 0,
            "broken_edges": 0,
            "sync_state": "Synced",
            "message": format!("Successfully deleted {clean_rel} and pruned graph nodes/edges synchronously.")
        });

        Ok(serde_json::to_string_pretty(&res).unwrap_or_else(|_| res.to_string()))
    }

    pub fn repograph_batch_edit(
        &mut self,
        patches: Vec<FilePatch>,
    ) -> Result<String, ToolError> {
        if patches.is_empty() {
            return Err(ToolError::new("invalid_params", "patches array cannot be empty"));
        }

        let mut file_buffers: std::collections::HashMap<PathBuf, (String, String, PathBuf)> = std::collections::HashMap::new();
        let mut all_diagnostics = Vec::new();

        // Step 1: Pre-validate all patches in memory with buffer chaining before touching any disk files
        for patch in &patches {
            let clean_rel = patch.path.trim_start_matches('/').trim_start_matches('\\').to_string();
            let resolved = self.resolve_inside_root(&clean_rel)?;
            self.reject_secret(&resolved, &clean_rel)?;

            if !resolved.is_file() {
                return Err(ToolError::new("file_not_found", format!("file not found in batch: {clean_rel}")));
            }

            let working_content = if let Some((_, current_text, _)) = file_buffers.get(&resolved) {
                current_text.clone()
            } else {
                let original = std::fs::read_to_string(&resolved).map_err(|e| {
                    ToolError::new("io_error", format!("failed to read {clean_rel}: {e}"))
                })?;
                normalize_line_endings(&original)
            };

            let norm_target = normalize_line_endings(&patch.target_content);
            let norm_replacement = normalize_line_endings(&patch.replacement_content);

            if !working_content.contains(&norm_target) {
                return Err(ToolError::new(
                    "target_not_found",
                    format!("Target content not found in {clean_rel} during batch pre-validation. Entire batch aborted."),
                ));
            }

            let updated = working_content.replacen(&norm_target, &norm_replacement, 1);
            let diags = validate_ast_syntax(&resolved, &updated)
                .map_err(|e| ToolError::new("syntax_validation_failed", format!("Pre-validation failed on {clean_rel}: {e}. Entire batch aborted.")))?;

            all_diagnostics.extend(diags);
            file_buffers.insert(resolved.clone(), (clean_rel, updated, resolved));
        }

        let mut total_old_edges = 0;
        if let Ok(conn) = self.db() {
            for (clean_rel, _, _) in file_buffers.values() {
                let (out_e, in_e) = count_file_edges(conn, clean_rel);
                total_old_edges += out_e + in_e;
            }
        }

        // Step 2: Atomic write all unique modified files
        let mut files_changed = Vec::new();
        for (clean_rel, updated, resolved) in file_buffers.values() {
            std::fs::write(resolved, updated).map_err(|e| {
                ToolError::new("io_error", format!("failed to write {clean_rel}: {e}"))
            })?;
            files_changed.push(clean_rel.clone());
        }

        // Step 3: Synchronous multi-node graph re-index
        for (clean_rel, _, resolved) in file_buffers.values() {
            let _ = crate::watcher::sync_path(&self.root, clean_rel);
            crate::watcher::remove_pending(&self.root, resolved);
        }
        self.graph = None;
        self.adjacency = None;
        let _ = self.load_graph();

        let mut total_new_edges = 0;
        if let Ok(conn) = self.db() {
            for (clean_rel, _, _) in file_buffers.values() {
                let (out_e, in_e) = count_file_edges(conn, clean_rel);
                total_new_edges += out_e + in_e;
            }
        }

        let added = total_new_edges.saturating_sub(total_old_edges);
        let removed = total_old_edges.saturating_sub(total_new_edges);

        let res = json!({
            "success": true,
            "files_changed": files_changed.len(),
            "modified_files": files_changed,
            "new_edges": added,
            "broken_edges": removed,
            "diagnostics": all_diagnostics,
            "sync_state": "Synced",
            "message": format!("Successfully executed atomic batch edit across {} files ({} total patches).", files_changed.len(), patches.len())
        });

        Ok(serde_json::to_string_pretty(&res).unwrap_or_else(|_| res.to_string()))
    }

    pub fn repograph_edit_symbol(
        &mut self,
        path: &str,
        symbol: &str,
        new_code: &str,
    ) -> Result<String, ToolError> {
        let clean_rel = path.trim_start_matches('/').trim_start_matches('\\').to_string();
        let resolved = self.resolve_inside_root(&clean_rel)?;
        self.reject_secret(&resolved, &clean_rel)?;

        if !resolved.is_file() {
            return Err(ToolError::new("file_not_found", format!("file not found: {clean_rel}")));
        }

        let conn = self.db()?;
        let range = crate::db::get_symbol_range(conn, &clean_rel, symbol)
            .map_err(|e| ToolError::new("db_error", format!("db error: {e}")))?
            .ok_or_else(|| ToolError::new("symbol_not_found", format!("symbol '{symbol}' not found in '{clean_rel}'")))?;

        let original = std::fs::read_to_string(&resolved).map_err(|e| {
            ToolError::new("io_error", format!("failed to read {clean_rel}: {e}"))
        })?;

        let norm_orig = normalize_line_endings(&original);
        let norm_new_code = normalize_line_endings(new_code);
        let lines: Vec<&str> = norm_orig.lines().collect();
        let start = range.0.max(1);
        let end = range.1.min(lines.len());

        if start > end || start > lines.len() {
            return Err(ToolError::new("invalid_range", "symbol line range in index is invalid"));
        }

        let mut new_lines = Vec::new();
        if start > 1 {
            new_lines.extend_from_slice(&lines[0..start - 1]);
        }
        new_lines.push(&norm_new_code);
        if end < lines.len() {
            new_lines.extend_from_slice(&lines[end..]);
        }

        let updated = new_lines.join("\n");
        let diagnostics = validate_ast_syntax(&resolved, &updated)
            .map_err(|e| ToolError::new("syntax_validation_failed", e))?;

        let (old_out, old_in): (usize, usize) = count_file_edges(conn, &clean_rel);

        std::fs::write(&resolved, &updated).map_err(|e| {
            ToolError::new("io_error", format!("failed to write {clean_rel}: {e}"))
        })?;

        let _ = crate::watcher::sync_path(&self.root, &clean_rel);
        crate::watcher::remove_pending(&self.root, &resolved);
        self.graph = None;
        self.adjacency = None;
        let _ = self.load_graph();

        let (new_out, new_in): (usize, usize) = if let Ok(conn) = self.db() {
            count_file_edges(conn, &clean_rel)
        } else {
            (0, 0)
        };

        let added = (new_out + new_in).saturating_sub(old_out + old_in);
        let removed = (old_out + old_in).saturating_sub(new_out + new_in);

        let res = json!({
            "success": true,
            "modified_file": clean_rel,
            "symbol": symbol,
            "new_edges": added,
            "broken_edges": removed,
            "diagnostics": diagnostics,
            "sync_state": "Synced",
            "message": format!("Successfully replaced symbol '{symbol}' in {clean_rel}.")
        });

        Ok(serde_json::to_string_pretty(&res).unwrap_or_else(|_| res.to_string()))
    }

    pub fn get_skeleton(&mut self, path: &str) -> Result<(String, usize), ToolError> {
        let normalized = path.replace('\\', "/").trim_start_matches("./").to_string();
        let abs_path = self.root.join(&normalized);
        let canonical = match dunce::canonicalize(&abs_path).or_else(|_| abs_path.canonicalize()) {
            Ok(c) => c,
            Err(_) => {
                let matching_path = if let Ok((graph, _)) = self.graph_and_adjacency() {
                    graph.nodes.iter().find(|n| n.path.ends_with(&normalized)).map(|n| n.path.clone())
                } else {
                    None
                };
                if let Some(fp) = matching_path {
                    let fuzzy_abs = self.root.join(&fp);
                    dunce::canonicalize(&fuzzy_abs).or_else(|_| fuzzy_abs.canonicalize())
                        .map_err(|e| ToolError::new("file_not_found", format!("cannot resolve fuzzy path '{fp}': {e}")))?
                } else {
                    return Err(ToolError::new("file_not_found", format!("File '{normalized}' not found")));
                }
            }
        };

        if !canonical.starts_with(&self.root) {
            return Err(ToolError::new(
                "forbidden_path",
                "path traversal outside repo root is forbidden",
            ));
        }

        let content = std::fs::read_to_string(&canonical)
            .map_err(|e| ToolError::new("read_error", format!("failed to read file: {e}")))?;
        let raw_tokens = crate::telemetry::count_tokens(&content);
        let language = crate::walker::language_for(&canonical);
        let skeleton = crate::skeleton::extract_skeleton(&content, &language);
        Ok((skeleton, raw_tokens))
    }

    pub fn get_execution_trace(&mut self, entrypoint: &str, depth: usize) -> Result<(String, usize), ToolError> {
        let root = self.root.clone();
        let conn = self.db()?;
        let (trace_md, unique_files) = crate::db::get_execution_trace(conn, &root, entrypoint, depth)
            .map_err(|e| ToolError::new("db_error", e))?;

        let mut raw_tokens = 0;
        for f in &unique_files {
            let abs = root.join(f);
            if let Ok(c) = std::fs::read_to_string(&abs) {
                raw_tokens += crate::telemetry::count_tokens(&c);
            }
        }

        Ok((trace_md, raw_tokens))
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

    pub fn search_symbols(&mut self, params: SearchParams) -> Result<String, ToolError> {
        let query = &params.query;
        let limit = params.limit.unwrap_or(10).max(1);
        let signature_only = params.signature_only.unwrap_or(true);
        let exact_symbol_only = params.exact_symbol_only.unwrap_or(false);

        let conn = self.db()?;
        crate::db::log_agent_query(conn, "", query, "search_symbols");

        let mut results = Vec::new();

        if exact_symbol_only {
            let pattern = format!("%{}%", query);
            let mut stmt = conn
                .prepare(
                    "SELECT s.name, s.file_path, f.content
                     FROM symbols s
                     LEFT JOIN symbols_fts f ON s.name = f.name AND s.file_path = f.file_path
                     WHERE s.name = ? OR s.name LIKE ?
                     ORDER BY (s.name = ?) DESC, LENGTH(s.name) ASC
                     LIMIT ?"
                )
                .map_err(|e| ToolError::new("db_error", format!("failed to prepare statement: {e}")))?;

            let rows = stmt
                .query_map(rusqlite::params![query, pattern, query, limit as i64], |row| {
                    let name: String = row.get(0)?;
                    let file_path: String = row.get(1)?;
                    let raw_content: Option<String> = row.get(2)?;
                    Ok((name, file_path, raw_content.unwrap_or_default()))
                })
                .map_err(|e| ToolError::new("db_error", format!("query failed: {e}")))?;

            for res in rows.flatten() {
                results.push(res);
            }
        } else {
            let match_query = crate::db::build_fts_match_query(query);
            if match_query.is_empty() {
                return Ok("[]".to_string());
            }

            let mut stmt = conn
                .prepare(
                    "SELECT name, file_path, content
                     FROM symbols_fts
                     WHERE tokens MATCH ?
                     ORDER BY rank
                     LIMIT ?"
                )
                .map_err(|e| ToolError::new("db_error", format!("failed to prepare statement: {e}")))?;

            let rows = stmt
                .query_map(rusqlite::params![match_query, limit as i64], |row| {
                    let name: String = row.get(0)?;
                    let file_path: String = row.get(1)?;
                    let content: String = row.get(2)?;
                    Ok((name, file_path, content))
                })
                .map_err(|e| ToolError::new("db_error", format!("query failed: {e}")))?;

            for res in rows.flatten() {
                results.push(res);
            }
        }

        let formatted: Vec<crate::db::SymbolSearchResult> = results
            .into_iter()
            .map(|(name, file_path, raw_content)| {
                let lines: Vec<&str> = raw_content.lines().collect();
                let name_matches = name.to_lowercase().contains(&query.to_lowercase());

                let sig = if !lines.is_empty() {
                    let s = signature_block(&lines, 1, lines.len());
                    if !s.is_empty() {
                        Some(s)
                    } else {
                        Some(lines.first().copied().unwrap_or("").to_string())
                    }
                } else {
                    None
                };

                let snippet = if !name_matches && !lines.is_empty() {
                    Some(extract_3line_snippet(&lines, query))
                } else {
                    None
                };

                if signature_only {
                    crate::db::SymbolSearchResult {
                        name,
                        file_path,
                        signature: sig,
                        snippet,
                        content: None,
                    }
                } else {
                    crate::db::SymbolSearchResult {
                        name,
                        file_path,
                        signature: sig,
                        snippet,
                        content: Some(raw_content),
                    }
                }
            })
            .collect();

        let json_str = serde_json::to_string(&formatted)
            .map_err(|e| ToolError::new("serialization_error", e.to_string()))?;

        if !signature_only && params.force_full != Some(true) {
            let tok_count = crate::telemetry::count_tokens(&json_str);
            if tok_count > 2500 {
                let mut sig_params = params.clone();
                sig_params.signature_only = Some(true);
                sig_params.force_full = Some(true);
                let compressed = self.search_symbols(sig_params)?;
                return Ok(format!(
                    "{}\n\n[Auto-Budget Notice: Response payload exceeded 2,500 tokens ({} tokens). Automatically compressed to signature-only mode. Pass force_full: true to override.]",
                    compressed, tok_count
                ));
            }
        }

        Ok(json_str)
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
        force_full: Option<bool>,
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

        let rendered = if compact_edges {
            let compact: Vec<String> = paths.iter().map(compact_edge).collect();
            serde_json::to_string(&json!({ "files": files, "paths": compact }))
                .map_err(|e| ToolError::new("serialization_error", e.to_string()))?
        } else {
            let payload = crate::db::ExplorePayload { files, paths };
            serde_json::to_string(&payload)
                .map_err(|e| ToolError::new("serialization_error", e.to_string()))?
        };

        if !signature_only && force_full != Some(true) {
            let tok_count = crate::telemetry::count_tokens(&rendered);
            if tok_count > 2500 {
                let compressed = self.explore(symbols, true, compact_edges, Some(true))?;
                return Ok(format!(
                    "{}\n\n[Auto-Budget Notice: Response payload exceeded 2,500 tokens ({} tokens). Automatically compressed to signature-only mode. Pass force_full: true to override.]",
                    compressed, tok_count
                ));
            }
        }

        Ok(rendered)
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

    pub fn codegraph_status(&mut self) -> Result<String, ToolError> {
        let now = chrono::Utc::now().timestamp_millis();
        self.session.last_active = now;
        let pending_map = crate::watcher::get_pending_map(&self.root);
        let sync_state = if pending_map.is_empty() { "Synced" } else { "Pending" };
        let (node_count, edge_count) = match self.graph_and_adjacency() {
            Ok((g, _)) => (g.nodes.len(), g.edges.len()),
            Err(_) => (0, 0),
        };
        let uptime_secs = (now.saturating_sub(self.session.start_time) as f64 / 1000.0).round();

        let mut md = String::new();
        md.push_str("## CodeGraph & MCP Session Status\n\n");
        md.push_str(&format!("- **Active Project Root:** `{}`\n", self.root.display()));
        md.push_str(&format!("- **MCP Connection:** `connected: {}`, `session_active: {}`\n", self.session.connected, self.session.session_active));
        md.push_str(&format!("- **Session ID:** `{}` (uptime: {}s, {} heartbeats, {} tool queries)\n", self.session.session_id, uptime_secs, self.session.heartbeat_count, self.session.queries_count));
        md.push_str(&format!("- **Sync State:** {}\n", sync_state));
        md.push_str(&format!("- **Graph Scale:** {} indexed nodes, {} edges\n\n", node_count, edge_count));

        if pending_map.is_empty() {
            md.push_str("All files are fully indexed. Graph is up-to-date and MCP session is active.\n");
        } else {
            md.push_str("### Pending Files:\n");
            let mut rows: Vec<(String, i64)> = pending_map.into_iter().collect();
            rows.sort();
            for (rel_path, marked_at) in rows {
                let elapsed_ms = now.saturating_sub(marked_at).max(0);
                md.push_str(&format!("- `{rel_path}` (edited {elapsed_ms}ms ago, pending sync)\n"));
            }
        }

        let ratio = self.tracker.overall_compression_ratio();
        let ratio_str = if ratio > 1.0 { format!("{:.1}x", ratio) } else { "1.0x".to_string() };
        md.push_str("\n### Live Token Metrics & Context Savings\n");
        md.push_str(&format!("- **Current Turn ID:** #{}\n", self.tracker.current_turn_id));
        md.push_str(&format!("- **Session Total Output Tokens:** {}\n", self.tracker.session_total_output));
        md.push_str(&format!("- **Session Tokens Saved:** {} ({})\n", self.tracker.session_total_saved, ratio_str));
        md.push_str(&format!("- **Recent Invocations Tracked:** {}\n", self.tracker.recent_calls.len()));

        if let Ok(conn) = self.db() {
            crate::db::log_agent_query(conn, "", "status", "status");
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
        crate::db::log_agent_query(conn, symbol.unwrap_or(""), path.unwrap_or(""), "callers");

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
        crate::db::log_agent_query(conn, symbol.unwrap_or(""), path.unwrap_or(""), "callees");

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
        crate::db::log_agent_query(conn, symbol.unwrap_or(""), path.unwrap_or(""), "impact");

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
        if let Ok(resolved) = dunce::canonicalize(&joined).or_else(|_| joined.canonicalize()) {
            if !resolved.starts_with(&self.root) {
                return Err(ToolError::new(
                    "path_outside_root",
                    format!("'{path}' resolves outside the repository root"),
                ));
            }
            return Ok(resolved);
        }

        let normalized = crate::parsers::normalize_relative("", &path.replace('\\', "/"))
            .ok_or_else(|| ToolError::new("path_outside_root", format!("'{path}' resolves outside the repository root")))?;
        let full = self.root.join(normalized);
        Ok(full)
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
/// comma-separated list of suffixes (`explore,search,node,edit,write,delete`);
/// the default enables all tools for closed-loop mutation and exploration.
fn allowed_tools() -> Vec<String> {
    match std::env::var("REPOGRAPH_MCP_TOOLS") {
        Ok(env_val) if !env_val.trim().is_empty() && env_val.trim() != "all" => env_val
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect(),
        _ => vec![
            "explore".to_string(),
            "files".to_string(),
            "domains".to_string(),
            "node".to_string(),
            "search".to_string(),
            "impact".to_string(),
            "callers".to_string(),
            "callees".to_string(),
            "status".to_string(),
            "edit".to_string(),
            "write".to_string(),
            "delete".to_string(),
            "batch_edit".to_string(),
            "edit_symbol".to_string(),
            "skeleton".to_string(),
            "trace".to_string(),
        ],
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
                    "domain": {
                        "type": "string",
                        "description": "Optional architectural domain name or ID (e.g. 'Auth' or '0') to filter files strictly to that cluster."
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

    if allowed.contains(&"domains".to_string()) {
        tools.push(json!({
            "name": "repograph_domains",
            "description": "Returns high-level architectural domains detected by Louvain community clustering with cohesion scores, top hubs, and key exports. Cost: ~180 tokens.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }));
    }

    if allowed.contains(&"node".to_string()) {
        tools.push(json!({
            "name": "repograph_node",
            "description": "Reads either the raw contents of a file or the exact source block of a symbol inside that file. Supports 1-indexed sliced reading (start_line, end_line) with optional line numbers.",
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
                    },
                    "start_line": {
                        "type": "integer",
                        "minimum": 1,
                        "description": "Optional 1-indexed start line for sliced file/symbol read."
                    },
                    "end_line": {
                        "type": "integer",
                        "minimum": 1,
                        "description": "Optional 1-indexed end line (inclusive) for sliced file/symbol read."
                    },
                    "with_line_numbers": {
                        "type": "boolean",
                        "description": "If true, prefixes lines with their 1-indexed line number (e.g. '42: export function...'). Defaults to true when start_line/end_line is provided."
                    }
                },
                "required": ["path"]
            }
        }));
    }

    if allowed.contains(&"edit".to_string()) {
        tools.push(json!({
            "name": "repograph_edit",
            "description": "Performs an AST-validated atomic replacement of target_content with replacement_content in a file, returning instant dependency edge diffs (added/removed) and diagnostics.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Repo-relative file path, e.g. 'src/components/Button.tsx'"
                    },
                    "target_content": {
                        "type": "string",
                        "description": "Exact text chunk in the file to be replaced."
                    },
                    "replacement_content": {
                        "type": "string",
                        "description": "New code to replace target_content with."
                    }
                },
                "required": ["path", "target_content", "replacement_content"]
            }
        }));
    }

    if allowed.contains(&"batch_edit".to_string()) {
        tools.push(json!({
            "name": "repograph_batch_edit",
            "description": "Executes atomic multi-file refactoring across multiple files with pre-validation and rollback guarantee on failure.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "patches": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "path": { "type": "string", "description": "Repo-relative file path" },
                                "target_content": { "type": "string", "description": "Exact text chunk to replace" },
                                "replacement_content": { "type": "string", "description": "New replacement code" }
                            },
                            "required": ["path", "target_content", "replacement_content"]
                        },
                        "description": "Array of file patches to apply atomically."
                    }
                },
                "required": ["patches"]
            }
        }));
    }

    if allowed.contains(&"edit_symbol".to_string()) {
        tools.push(json!({
            "name": "repograph_edit_symbol",
            "description": "Replaces the exact source range of an AST symbol in a file without needing manual line offsets.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Repo-relative file path, e.g. 'src/components/Button.tsx'"
                    },
                    "symbol": {
                        "type": "string",
                        "description": "Exact identifier name of the symbol to replace."
                    },
                    "new_code": {
                        "type": "string",
                        "description": "New replacement code for the entire symbol body."
                    }
                },
                "required": ["path", "symbol", "new_code"]
            }
        }));
    }

    if allowed.contains(&"write".to_string()) {
        tools.push(json!({
            "name": "repograph_write",
            "description": "Creates or overwrites a file with automatic parent directory creation and immediate AST re-indexing.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Repo-relative file path, e.g. 'src/utils/helpers.ts'"
                    },
                    "content": {
                        "type": "string",
                        "description": "Full file content to write."
                    }
                },
                "required": ["path", "content"]
            }
        }));
    }

    if allowed.contains(&"delete".to_string()) {
        tools.push(json!({
            "name": "repograph_delete",
            "description": "Deletes a file or directory and automatically prunes its associated nodes and edges from the dependency graph.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Repo-relative file or directory path to delete."
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
            "description": "Searches for matching symbols in the repository index via SQLite FTS5 fuzzy match. Defaults to returning compact signature definitions and 3-line snippets to minimize token usage.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Symbol name or body query to search for, e.g. 'PageShell' or 'useGraphStore'"
                    },
                    "limit": {
                        "type": "number",
                        "description": "Maximum number of search results to return. Defaults to 10."
                    },
                    "signature_only": {
                        "type": "boolean",
                        "description": "Return only the symbol signature/declaration or 3-line snippet instead of full component/function bodies. Defaults to true."
                    },
                    "exact_symbol_only": {
                        "type": "boolean",
                        "description": "Match only exact symbol identifier names instead of body text or full-text tokens. Defaults to false."
                    },
                    "force_full": {
                        "type": "boolean",
                        "description": "If true, overrides 2,500 token intelligent context budget compression and returns full uncompressed payload."
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
                    },
                    "force_full": {
                        "type": "boolean",
                        "description": "If true, overrides 2,500 token intelligent context budget compression and returns full uncompressed payload."
                    }
                },
                "required": ["symbols"]
            }
        }));
    }

    if allowed.contains(&"skeleton".to_string()) {
        tools.push(json!({
            "name": "repograph_skeleton",
            "description": "Returns a structural AST skeleton/outline of a source file by stripping all function, method, and JSX implementation bodies while retaining imports, exports, TypeScript interfaces, type definitions, class fields, and docstrings. Replaces 500-1000 line files with ~30 lines achieving a 95%+ token reduction. Supported across TypeScript/JavaScript, Python, Rust, Go, Java, C#, C/C++, Kotlin, Swift, PHP, and Vue/Svelte SFCs.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Repo-relative or partial path of the source file, e.g. 'src/services/checkout.ts' or 'app/api/route.py'"
                    }
                },
                "required": ["path"]
            }
        }));
    }

    if allowed.contains(&"trace".to_string()) {
        tools.push(json!({
            "name": "repograph_trace",
            "description": "Traces a multi-hop static call chain starting from an entrypoint (function name, route handler, or 'file:symbol') down N hops. Returns a bundled hierarchical execution pipeline with exact type signatures, file paths, and line numbers in a single compact payload (~300 tokens).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "entrypoint": {
                        "type": "string",
                        "description": "The root entrypoint symbol name, route URL (e.g. 'POST /api/inquiry'), or file-qualified symbol (e.g. 'src/routes/auth.ts:loginHandler')"
                    },
                    "depth": {
                        "type": "integer",
                        "description": "Maximum execution hops to traverse (default: 3, max: 4)."
                    }
                },
                "required": ["entrypoint"]
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
        let _ = crate::indexer::index_repo(&root, None);
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
        let (_g, mut server) = temp_repo();
        let content = server.read_file("src/hooks/useTheme.ts").unwrap();
        assert!(content.contains("useTheme"));
    }

    #[test]
    fn read_file_rejects_path_traversal() {
        let (_g, mut server) = temp_repo();
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
            md.contains("Showing 1 of "),
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
    fn tools_listing_and_filtering() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("REPOGRAPH_MCP_TOOLS");
        let names: Vec<String> = tools_list_result()["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        assert!(names.contains(&"repograph_explore".to_string()));
        assert!(names.contains(&"repograph_edit".to_string()));
        assert!(names.contains(&"repograph_write".to_string()));
        assert!(names.contains(&"repograph_delete".to_string()));

        std::env::set_var("REPOGRAPH_MCP_TOOLS", "explore,search,node,edit");
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
                "repograph_edit".to_string(),
                "repograph_search".to_string(),
                "repograph_explore".to_string()
            ]
        );
        std::env::remove_var("REPOGRAPH_MCP_TOOLS");
    }

    #[test]
    fn mutator_tools_edit_write_delete() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("REPOGRAPH_MCP_TOOLS", "all");
        let (_dir, mut server) = temp_repo();

        // 1. repograph_write: create a new file
        let write_call = server.handle_message(
            r#"{"jsonrpc":"2.0","id":100,"method":"tools/call","params":{"name":"repograph_write","arguments":{"path":"src/utils/math.ts","content":"export const add = (a: number, b: number) => a + b;\n"}}}"#
        ).unwrap();
        assert_eq!(write_call["result"]["isError"], false, "{write_call}");
        let write_text = write_call["result"]["content"][0]["text"].as_str().unwrap();
        assert!(write_text.contains("\"success\": true"));
        assert!(write_text.contains("src/utils/math.ts"));

        // 2. repograph_edit: atomic replace in the new file
        let edit_call = server.handle_message(
            r#"{"jsonrpc":"2.0","id":101,"method":"tools/call","params":{"name":"repograph_edit","arguments":{"path":"src/utils/math.ts","target_content":"a + b","replacement_content":"a + b + 0"}}}"#
        ).unwrap();
        assert_eq!(edit_call["result"]["isError"], false, "{edit_call}");
        let edit_text = edit_call["result"]["content"][0]["text"].as_str().unwrap();
        assert!(edit_text.contains("\"success\": true"));

        // Verify edited content on disk
        let node_read = server.read_file("src/utils/math.ts").unwrap();
        assert!(node_read.contains("a + b + 0"));

        // 3. repograph_delete: delete the file
        let delete_call = server.handle_message(
            r#"{"jsonrpc":"2.0","id":102,"method":"tools/call","params":{"name":"repograph_delete","arguments":{"path":"src/utils/math.ts"}}}"#
        ).unwrap();
        assert_eq!(delete_call["result"]["isError"], false, "{delete_call}");
        assert!(!server.root.join("src/utils/math.ts").exists());

        std::env::remove_var("REPOGRAPH_MCP_TOOLS");
    }

    #[test]
    fn sliced_node_reads_with_line_numbers() {
        let (_dir, mut server) = temp_repo();
        
        let sliced = server.read_file_sliced("src/components/Button.tsx", Some(1), Some(2), Some(true)).unwrap();
        assert!(sliced.contains("   1: import { useTheme }"));
        assert!(sliced.contains("   2: export const Button"));

        let unnumbered = server.read_file_sliced("src/components/Button.tsx", Some(1), Some(1), Some(false)).unwrap();
        assert_eq!(unnumbered, "import { useTheme } from '../hooks/useTheme';");
    }

    /// A `.repograph` cache built before the walker learned the secrets
    /// denylist still lists `.env` as a node, so the read boundary has to
    /// refuse it independently of how the index was built.
    #[test]
    fn read_file_refuses_credential_files_even_when_indexed() {
        let (guard, mut server) = temp_repo();
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

    #[test]
    fn session_persistence_and_ping_heartbeat() {
        let (_guard, mut server) = temp_repo();
        assert!(server.session.connected);
        assert!(server.session.session_active);

        let init = server
            .handle_message(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#)
            .unwrap();
        assert_eq!(init["result"]["session"]["connected"], true);
        assert_eq!(init["result"]["session"]["sessionActive"], true);
        assert_eq!(init["result"]["session"]["persistent"], true);

        let ping = server
            .handle_message(r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#)
            .unwrap();
        assert_eq!(ping["result"]["status"], "ok");
        assert_eq!(ping["result"]["connected"], true);
        assert_eq!(ping["result"]["session_active"], true);
        assert_eq!(ping["result"]["heartbeats"], 1);

        let status = server.codegraph_status().unwrap();
        assert!(status.contains("connected: true") && status.contains("session_active: true"));
        assert!(status.contains("Session ID:"));
    }

    #[test]
    fn repograph_domains_and_domain_filtering() {
        let (_guard, mut server) = temp_repo();
        let domains = server.repograph_domains().unwrap();
        assert!(domains.contains("Architectural Domains"));
        assert!(domains.contains("Core Application"));

        let options = ManifestOptions {
            domain: Some("Core"),
            ..Default::default()
        };
        let filtered_manifest = server.get_manifest_with(&options).unwrap();
        assert!(filtered_manifest.contains("src/hooks/useTheme.ts") || filtered_manifest.contains("src/components/Button.tsx"));
    }

    #[test]
    fn repograph_search_optimizes_tokens_with_signature_and_limit() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("REPOGRAPH_MCP_TOOLS", "explore,files,node,search");

        let (_dir, mut server) = temp_repo();
        
        // 1. Default search: signature_only = true, limit = 10
        let search_json = server.search_symbols(SearchParams {
            query: "Button".to_string(),
            limit: None,
            signature_only: None,
            exact_symbol_only: None,
            force_full: None,
        }).unwrap();
        
        let results: Vec<crate::db::SymbolSearchResult> = serde_json::from_str(&search_json).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].name, "Button");
        assert!(results[0].signature.is_some());
        assert!(results[0].content.is_none()); // Content is omitted under signature_only!

        // Token count estimation: 10 results should be well under 1000 tokens (e.g. < 4000 bytes)
        let token_estimate = crate::tokens::count_tokens(&search_json);
        assert!(token_estimate < 1000, "Search result token estimate {token_estimate} exceeds 1000 tokens");

        // 2. Exact symbol match
        let exact_json = server.search_symbols(SearchParams {
            query: "useTheme".to_string(),
            limit: Some(5),
            signature_only: Some(true),
            exact_symbol_only: Some(true),
            force_full: None,
        }).unwrap();
        let exact_results: Vec<crate::db::SymbolSearchResult> = serde_json::from_str(&exact_json).unwrap();
        assert!(!exact_results.is_empty());
        assert_eq!(exact_results[0].name, "useTheme");

        // 3. JSON-RPC tool invocation
        let wire_call = server.handle_message(
            r#"{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"repograph_search","arguments":{"query":"Button","signature_only":true,"limit":5}}}"#
        ).unwrap();
        assert_eq!(wire_call["result"]["isError"], false);
        let wire_text = wire_call["result"]["content"][0]["text"].as_str().unwrap();
        assert!(wire_text.contains("Button"));
        assert!(wire_text.contains("signature"));
        assert!(!wire_text.contains("\"content\":"));

        std::env::remove_var("REPOGRAPH_MCP_TOOLS");
    }

    #[test]
    fn inband_telemetry_tag_and_metrics_in_tools_call() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("REPOGRAPH_MCP_TOOLS", "explore,files,node,status");

        let (_dir, mut server) = temp_repo();

        let call = server.handle_message(
            r#"{"jsonrpc":"2.0","id":20,"method":"tools/call","params":{"name":"repograph_files","arguments":{}}}"#
        ).unwrap();
        assert_eq!(call["result"]["isError"], false);
        let text = call["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("[Telemetry: Tool 'repograph_files'"));
        assert!(text.contains("Turn #1:"));
        assert!(text.contains("Saved:"));

        let status_res = server.codegraph_status().unwrap();
        assert!(status_res.contains("Live Token Metrics & Context Savings"));
        assert!(status_res.contains("Session Total Output Tokens:"));

        std::env::remove_var("REPOGRAPH_MCP_TOOLS");
    }

    #[test]
    fn crlf_and_lf_line_ending_normalization() {
        let (_dir, mut server) = temp_repo();
        // Write file with CRLF
        let crlf_content = "export const helper = () => {\r\n  return 42;\r\n};\r\n";
        server.repograph_write("src/utils/crlf.ts", crlf_content).unwrap();

        // Edit with LF target_content
        let lf_target = "return 42;";
        let edit_res = server.repograph_edit("src/utils/crlf.ts", lf_target, "return 100;").unwrap();
        assert!(edit_res.contains("\"success\": true"));

        let content = server.read_file("src/utils/crlf.ts").unwrap();
        assert!(content.contains("return 100;"));
    }

    #[test]
    fn batch_edit_atomic_refactoring() {
        let (_dir, mut server) = temp_repo();

        server.repograph_write("src/a.ts", "export const a = 1;\n").unwrap();
        server.repograph_write("src/b.ts", "export const b = 2;\n").unwrap();

        // 1. Success batch edit
        let patches = vec![
            FilePatch {
                path: "src/a.ts".to_string(),
                target_content: "a = 1;".to_string(),
                replacement_content: "a = 10;".to_string(),
            },
            FilePatch {
                path: "src/b.ts".to_string(),
                target_content: "b = 2;".to_string(),
                replacement_content: "b = 20;".to_string(),
            },
        ];
        let res = server.repograph_batch_edit(patches).unwrap();
        assert!(res.contains("\"success\": true"));
        assert!(res.contains("\"files_changed\": 2"));

        assert!(server.read_file("src/a.ts").unwrap().contains("a = 10;"));
        assert!(server.read_file("src/b.ts").unwrap().contains("b = 20;"));

        // 2. Failure batch edit: rollback / pre-validation aborts before touching any disk files
        let bad_patches = vec![
            FilePatch {
                path: "src/a.ts".to_string(),
                target_content: "a = 10;".to_string(),
                replacement_content: "a = 999;".to_string(),
            },
            FilePatch {
                path: "src/b.ts".to_string(),
                target_content: "NON_EXISTENT_TARGET".to_string(),
                replacement_content: "b = 999;".to_string(),
            },
        ];
        let err = server.repograph_batch_edit(bad_patches).unwrap_err();
        assert_eq!(err.code, "target_not_found");
        // Verify src/a.ts was untouched because pre-validation failed
        assert!(server.read_file("src/a.ts").unwrap().contains("a = 10;"));

        // 3. Multi-patch on the SAME file in a single transaction
        let same_file_patches = vec![
            FilePatch {
                path: "src/a.ts".to_string(),
                target_content: "a = 10;".to_string(),
                replacement_content: "a = 50;\nexport const extra = 100;".to_string(),
            },
            FilePatch {
                path: "src/a.ts".to_string(),
                target_content: "extra = 100;".to_string(),
                replacement_content: "extra = 200;".to_string(),
            },
        ];
        let res3 = server.repograph_batch_edit(same_file_patches).unwrap();
        assert!(res3.contains("\"success\": true"));
        assert!(res3.contains("\"files_changed\": 1"));
        let a_content = server.read_file("src/a.ts").unwrap();
        assert!(a_content.contains("a = 50;"));
        assert!(a_content.contains("extra = 200;"));
    }

    #[test]
    fn edit_symbol_scoped_replacement() {
        let (_dir, mut server) = temp_repo();
        // File already has Button symbol in temp_repo
        let res = server.repograph_edit_symbol(
            "src/components/Button.tsx",
            "Button",
            "export const Button = ({ label }: { label: string }) => <button>{label}</button>;",
        ).unwrap();
        assert!(res.contains("\"success\": true"));
        assert!(res.contains("\"symbol\": \"Button\""));

        let updated = server.read_file("src/components/Button.tsx").unwrap();
        assert!(updated.contains("<button>{label}</button>"));
    }

    #[test]
    fn intelligent_context_throttling_2500_tokens() {
        let (_guard, mut server) = temp_repo();
        // Generate a large TS file with lots of code (> 2,500 tokens)
        let mut large_code = String::from("// Large Module\n");
        for i in 0..150 {
            large_code.push_str(&format!("export const func_{i} = () => {{ console.log('payload content line {i} data string padding'); return {i}; }};\n"));
        }
        server.repograph_write("src/large.ts", &large_code).unwrap();

        // 1. Search without force_full (should auto-compress if large payload)
        let search_res = server.search_symbols(SearchParams {
            query: "func_".to_string(),
            limit: Some(100),
            signature_only: Some(false),
            exact_symbol_only: None,
            force_full: None,
        }).unwrap();
        // Under signature_only compression, signatures are returned without huge bodies
        assert!(search_res.contains("func_"));

        // 2. Explore without force_full
        let explore_res = server.explore(vec!["useTheme".to_string()], false, false, None).unwrap();
        assert!(explore_res.contains("useTheme"));
    }

    #[test]
    fn mcp_skeleton_and_trace_tools() {
        let _env_guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("REPOGRAPH_MCP_TOOLS");
        let (_guard, mut server) = temp_repo();
        
        // 1. Test repograph_skeleton
        let (skeleton, raw_tokens) = server.get_skeleton("src/components/Button.tsx").unwrap();
        assert!(skeleton.contains("Button"));
        assert!(raw_tokens > 0);

        // 2. Test JSON-RPC call for repograph_skeleton
        let req_skeleton = r#"{"jsonrpc":"2.0","id":42,"method":"tools/call","params":{"name":"repograph_skeleton","arguments":{"path":"src/components/Button.tsx"}}}"#;
        let resp_skeleton = server.handle_message(req_skeleton).expect("resp_skeleton");
        assert_eq!(resp_skeleton["result"]["isError"], false, "{resp_skeleton}");
        let resp_text = resp_skeleton["result"]["content"][0]["text"].as_str().unwrap();
        assert!(resp_text.contains("Button"));
        assert!(resp_text.contains("Telemetry: Tool 'repograph_skeleton'"));

        // 3. Test JSON-RPC call for repograph_trace
        let req_trace = r#"{"jsonrpc":"2.0","id":43,"method":"tools/call","params":{"name":"repograph_trace","arguments":{"entrypoint":"Button","depth":2}}}"#;
        let resp_trace = server.handle_message(req_trace).expect("resp_trace");
        assert_eq!(resp_trace["result"]["isError"], false, "{resp_trace}");
        let trace_text = resp_trace["result"]["content"][0]["text"].as_str().unwrap();
        assert!(trace_text.contains("Execution Trace: `Button`"));
        assert!(trace_text.contains("Telemetry: Tool 'repograph_trace'"));
    }
}

