//! Tauri desktop app: hosts the React visualizer and exposes the
//! read-only `read_graph` IPC command. All indexing/serving CLI work lives
//! in the separate `mcp_server` binary (`src/bin/mcp_server.rs`).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod agent_scaffold;

use repo_graph::graph::Graph;
use repo_graph::tree::FileTreeNode;
use std::path::PathBuf;
use tauri::Manager;

/// Repo root discovery: explicit override, else walk up from the current
/// directory until a `.repograph/graph.json` cache is found (in dev the
/// cwd is `src-tauri/`, so the project root is one level up).
fn find_repo_root() -> Option<PathBuf> {
    if let Ok(root) = std::env::var("REPO_GRAPH_ROOT") {
        return Some(PathBuf::from(root));
    }
    let mut dir = std::env::current_dir().ok()?;
    loop {
        if dir.join(".repograph/graph.db").is_file() || dir.join(".repograph/graph.json").is_file() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Writes the absolute project root and synctime to the shared active_project.json registry.
fn write_active_project_registry(root: &std::path::Path) {
    let Some(registry_path) = repo_graph::paths::active_project_registry() else {
        eprintln!("[registry] no writable state directory; skipping active-project write");
        return;
    };
    if let Some(parent) = registry_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    // `\\?\C:\x` → `C:/x`: the extended-length prefix is meaningless to every
    // consumer of this file and only makes the recorded root harder to match.
    let normalized = root.to_string_lossy().replace('\\', "/");
    let clean_normalized = normalized
        .strip_prefix("//?/")
        .unwrap_or(&normalized)
        .to_string();


    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let json_block = serde_json::json!({
        "active_project_root": clean_normalized,
        "last_synced_at": now
    });
    
    if let Ok(content) = serde_json::to_string_pretty(&json_block) {
        let _ = std::fs::write(&registry_path, content);
    }
}

static RECENT_PROJECTS_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn clean_unc_path(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    let cleaned = normalized
        .trim_start_matches("//?/")
        .trim_start_matches(r"\\?\");
    cleaned.to_string()
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct ProjectStats {
    files: i64,
    symbols: i64,
    language: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct RecentProject {
    path: String,
    last_opened: String,
    stats: ProjectStats,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct RecentProjectsConfig {
    projects: Vec<RecentProject>,
}

#[tauri::command]
fn get_recent_projects() -> Result<String, String> {
    let _guard = RECENT_PROJECTS_MUTEX.lock().map_err(|e| e.to_string())?;
    let Some(path) = repo_graph::paths::recent_projects_config() else {
        return Ok("{\"projects\":[]}".to_string());
    };
    if !path.exists() {
        return Ok("{\"projects\":[]}".to_string());
    }
    std::fs::read_to_string(&path).map_err(|e| format!("failed to read recent projects: {e}"))
}

#[tauri::command]
fn add_recent_project(path: String, files: i64, symbols: i64, lang: String) -> Result<(), String> {
    let _guard = RECENT_PROJECTS_MUTEX.lock().map_err(|e| e.to_string())?;
    let config_path = repo_graph::paths::recent_projects_config()
        .ok_or_else(|| "no writable state directory for recent projects".to_string())?;

    if let Some(parent) = config_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let mut config = if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)
            .map_err(|e| format!("failed to read config: {e}"))?;
        serde_json::from_str::<RecentProjectsConfig>(&content)
            .unwrap_or(RecentProjectsConfig { projects: Vec::new() })
    } else {
        RecentProjectsConfig { projects: Vec::new() }
    };

    let normalized_path = clean_unc_path(&path);
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

    // Normalize historical entries
    for p in &mut config.projects {
        p.path = clean_unc_path(&p.path);
    }

    config.projects.retain(|p| clean_unc_path(&p.path) != normalized_path);

    config.projects.insert(0, RecentProject {
        path: normalized_path,
        last_opened: now,
        stats: ProjectStats {
            files,
            symbols,
            language: lang,
        },
    });

    config.projects.truncate(10);

    let content = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("failed to serialize config: {e}"))?;
    std::fs::write(&config_path, content)
        .map_err(|e| format!("failed to write config: {e}"))?;

    Ok(())
}

/// Does this workspace already have `.myrepograph-agent/`?
///
/// Read-only, so an unreadable or missing path is simply `false` rather than
/// an error the sidebar would have to render.
#[tauri::command]
fn check_agent_scaffold(project_root: String) -> Result<bool, String> {
    if project_root.trim().is_empty() {
        return Ok(false);
    }
    Ok(PathBuf::from(project_root)
        .join(agent_scaffold::SCAFFOLD_DIR)
        .is_dir())
}

/// Manually run the scaffolding engine for a workspace (Playbook §28).
///
/// Unlike the automatic call in `index_and_load_graph`, this one reports its
/// failure: the user clicked a button and is owed an answer. The path is
/// canonicalized and required to be an existing directory first — this command
/// writes files, and it should never act on a path that is not a real
/// workspace the user already has open.
#[tauri::command]
fn trigger_agent_scaffold(project_root: String) -> Result<(), String> {
    let root = PathBuf::from(&project_root)
        .canonicalize()
        .map_err(|e| format!("cannot resolve {project_root}: {e}"))?;
    if !root.is_dir() {
        return Err(format!("{} is not a directory", root.display()));
    }
    agent_scaffold::ensure_agent_scaffold(&root)
        .map_err(|e| format!("scaffolding failed for {}: {e}", root.display()))
}

#[tauri::command]
fn set_watch_debounce(ms: u64) -> Result<(), String> {
    std::env::set_var("CODEGRAPH_WATCH_DEBOUNCE_MS", ms.to_string());
    Ok(())
}

/// Every tool the MCP server can expose. Without this env var the server's
/// `tools/list` returns only `repograph_explore`, which reads to new users as
/// "the integration is broken" — so every generated snippet sets it.
const MCP_ALL_TOOLS: &str = "explore,files,domains,node,search,impact,callers,callees,status";

#[tauri::command]
fn get_domains(graph_json: String) -> Result<String, String> {
    let graph: repo_graph::graph::Graph = serde_json::from_str(&graph_json)
        .map_err(|e| format!("invalid graph json: {e}"))?;
    let res = repo_graph::cluster::detect_domains(&graph);
    serde_json::to_string(&res).map_err(|e| format!("failed to serialize domains: {e}"))
}

#[derive(Debug, Clone, serde::Serialize)]
struct McpConfigSnippet {
    /// Absolute path this generator resolved `mcp_server` to — shown next to
    /// the detection indicator even when `binary_exists` is false, so a user
    /// staring at "⚠️ Binary Missing" has a concrete path to go check.
    binary_path: String,
    binary_exists: bool,
    claude_desktop_json: String,
    codex_toml: String,
    vscode_json: String,
}

/// Resolve the absolute path of the sibling `mcp_server` executable.
///
/// The old generator assumed `mcp_server.exe` lives at
/// `<opened project>/src-tauri/target/debug/mcp_server.exe` — true only when
/// the "project" a user opens happens to be a checkout of Repo Graph's own
/// source. For any real target project (a `mifos_backend`, say, with no
/// `src-tauri/` of its own) that path never exists. The binary's real
/// location has nothing to do with which project is open — it lives wherever
/// *Repo Graph itself* was built or installed, so resolution is anchored to
/// this running process instead of the caller-supplied `project_root`:
///
/// 1. Beside the currently running executable (`current_exe()`'s directory).
///    True for `cargo run`/`cargo tauri dev` and for a packaged build where
///    the installer copies `mcp_server` next to the main app binary.
/// 2. Tauri's resolved resource directory — the bundling target declared in
///    `tauri.conf.json`'s `bundle.resources` for a packaged install where
///    resources land in a `resources/` subdirectory instead of beside the exe.
///
/// If neither exists, the first candidate is still returned (so the UI has a
/// real path to show) with `exists: false` rather than an empty string.
fn resolve_mcp_binary(app_handle: &tauri::AppHandle) -> (PathBuf, bool) {
    let binary_name = if cfg!(target_os = "windows") { "mcp_server.exe" } else { "mcp_server" };

    let sibling_of_exe = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(binary_name)));

    if let Some(candidate) = &sibling_of_exe {
        if candidate.is_file() {
            return (candidate.clone(), true);
        }
    }

    if let Some(resource_dir) = app_handle.path_resolver().resource_dir() {
        let candidate = resource_dir.join(binary_name);
        if candidate.is_file() {
            return (candidate, true);
        }
    }

    (
        sibling_of_exe.unwrap_or_else(|| PathBuf::from(binary_name)),
        false,
    )
}

/// Generates ready-to-paste MCP registration snippets for Claude Desktop,
/// Codex, and VS Code, using the real absolute path of the `mcp_server`
/// binary on this machine — never the caller-supplied `project_root`, whose
/// only role here is to fill the `args` array each host passes to the server.
#[tauri::command]
fn get_mcp_config_snippet(project_root: String, app_handle: tauri::AppHandle) -> Result<McpConfigSnippet, String> {
    let (binary_path, binary_exists) = resolve_mcp_binary(&app_handle);
    let path_str = binary_path.to_string_lossy().replace('\\', "/");
    let root_str = project_root.replace('\\', "/");

    let claude_desktop_json = serde_json::to_string_pretty(&serde_json::json!({
        "mcpServers": {
            "repo-graph": {
                "command": path_str,
                "args": [root_str],
                "env": { "REPOGRAPH_MCP_TOOLS": MCP_ALL_TOOLS }
            }
        }
    }))
    .map_err(|e| e.to_string())?;

    let codex_toml = format!(
        "[mcp_servers.repo-graph]\ncommand = \"{path}\"\nargs = [\"{root}\"]\n\n[mcp_servers.repo-graph.env]\nREPOGRAPH_MCP_TOOLS = \"{tools}\"\n",
        path = path_str,
        root = root_str,
        tools = MCP_ALL_TOOLS,
    );

    let vscode_json = serde_json::to_string_pretty(&serde_json::json!({
        "servers": {
            "repo-graph": {
                "type": "stdio",
                "command": path_str,
                "args": [root_str],
                "env": { "REPOGRAPH_MCP_TOOLS": MCP_ALL_TOOLS }
            }
        }
    }))
    .map_err(|e| e.to_string())?;

    Ok(McpConfigSnippet {
        binary_path: path_str,
        binary_exists,
        claude_desktop_json,
        codex_toml,
        vscode_json,
    })
}

/// Read-only IPC bridge: parse the walker's cache and hand the typed graph
/// to the frontend. No write commands are exposed over IPC.
#[tauri::command]
fn read_graph(app_handle: tauri::AppHandle) -> Result<Graph, String> {
    let root = find_repo_root().ok_or_else(|| {
        "no .repograph/graph.db or .repograph/graph.json found — run: mcp_server index <repo_root>".to_string()
    })?;
    write_active_project_registry(&root);
    repo_graph::watcher::start_watcher(app_handle, root.clone());
    
    let db_path = root.join(".repograph/graph.db");
    let graph = match repo_graph::db::init_db(&db_path).and_then(|conn| repo_graph::db::load_graph_from_db(&conn)) {
        Ok(g) => g,
        Err(db_err) => {
            let json_path = root.join(".repograph/graph.json");
            if json_path.is_file() {
                Graph::load_from_cache(&json_path).map_err(|e| format!("db error ({db_err}) and cache error ({e})"))?
            } else {
                return Err(db_err.to_string());
            }
        }
    };

    let symbol_count = graph.nodes.iter().map(|n| n.symbols.len()).sum::<usize>() as i64;
    let mut lang_counts = std::collections::HashMap::new();
    for node in &graph.nodes {
        *lang_counts.entry(node.language.clone()).or_insert(0) += 1;
    }
    let primary_lang = lang_counts
        .into_iter()
        .max_by_key(|&(_, count)| count)
        .map(|(lang, _)| lang)
        .unwrap_or_else(|| "unknown".to_string());
    let root_str = root.to_string_lossy().to_string();
    let _ = add_recent_project(root_str, graph.nodes.len() as i64, symbol_count, primary_lang);

    eprintln!(
        "[ipc] read_graph → {} nodes, {} edges (root: {})",
        graph.nodes.len(),
        graph.edges.len(),
        root.display()
    );
    Ok(graph)
}

/// Native folder picker (blocking variant is safe here: async commands run
/// off the main thread). Returns the canonical selected path, or None on
/// cancel.
#[tauri::command]
async fn open_project_dialog() -> Option<String> {
    tauri::api::dialog::blocking::FileDialogBuilder::new()
        .set_title("Open Project Folder")
        .pick_folder()
        .and_then(|p| p.canonicalize().ok())
        .map(|p| p.display().to_string())
}

/// Walk + parse + build the graph for a workspace, refresh its
/// SQLite database, and return the graph.
#[tauri::command]
async fn index_and_load_graph(root: String, app_handle: tauri::AppHandle) -> Result<Graph, String> {
    let root = PathBuf::from(&root)
        .canonicalize()
        .map_err(|e| format!("cannot open {root}: {e}"))?;
    let summary = repo_graph::indexer::index_repo(&root, Some(&app_handle)).map_err(|e| e.to_string())?;
    // Playbook §28: drop the agent scaffolding into the workspace on first
    // index. Advisory — a read-only checkout or a permissions failure must not
    // fail the index that the user actually asked for.
    if let Err(e) = agent_scaffold::ensure_agent_scaffold(&root) {
        eprintln!("[scaffold] skipped for {}: {e}", root.display());
    }
    write_active_project_registry(&root);
    repo_graph::watcher::start_watcher(app_handle, root.clone());
    eprintln!(
        "[ipc] index_and_load_graph → {} files, {} edges in {} ms ({})",
        summary.files,
        summary.edges,
        summary.duration_ms,
        root.display()
    );
    
    let db_path = root.join(".repograph/graph.db");
    let conn = repo_graph::db::init_db(&db_path).map_err(|e| e.to_string())?;
    let graph = repo_graph::db::load_graph_from_db(&conn).map_err(|e| e.to_string())?;
    
    let symbol_count = graph.nodes.iter().map(|n| n.symbols.len()).sum::<usize>() as i64;
    let mut lang_counts = std::collections::HashMap::new();
    for node in &graph.nodes {
        *lang_counts.entry(node.language.clone()).or_insert(0) += 1;
    }
    let primary_lang = lang_counts
        .into_iter()
        .max_by_key(|&(_, count)| count)
        .map(|(lang, _)| lang)
        .unwrap_or_else(|| "unknown".to_string());
    let root_str = root.to_string_lossy().to_string();
    let _ = add_recent_project(root_str, graph.nodes.len() as i64, symbol_count, primary_lang);
    
    Ok(graph)
}

/// Explorer tree for the left sidebar — read-only, noise dirs excluded,
/// recursion confined to the given root.
#[tauri::command]
async fn read_directory_tree(root: String) -> Result<Vec<FileTreeNode>, String> {
    let root = PathBuf::from(&root)
        .canonicalize()
        .map_err(|e| format!("cannot open {root}: {e}"))?;
    repo_graph::tree::read_directory_tree(&root).map_err(|e| e.to_string())
}

/// Launches VS Code with canonicalized path, joining file path with active project root.
#[tauri::command]
async fn open_in_editor(path: String, project_root: Option<String>) -> Result<(), String> {
    let clean_relative = path.trim_start_matches(['/', '\\']);
    let root_path = if let Some(ref root_str) = project_root {
        PathBuf::from(root_str)
    } else {
        find_repo_root().ok_or_else(|| "no active project root found".to_string())?
    };
    let abs_path = root_path.join(clean_relative);

    let canonical = abs_path
        .canonicalize()
        .map_err(|e| format!("failed to canonicalize path for editing: {e}"))?;

    let root_canonical = root_path
        .canonicalize()
        .map_err(|e| format!("cannot resolve root: {e}"))?;
    if !canonical.starts_with(&root_canonical) {
        return Err("access denied: path outside root".to_string());
    }

    // `canonicalize` returns the extended-length form on Windows; editors
    // generally do not accept it.
    let path_str = canonical.to_string_lossy().into_owned();
    let clean_path = path_str
        .strip_prefix(r"\\?\")
        .unwrap_or(&path_str)
        .to_string();

    eprintln!("[ipc] open_in_editor → {}", clean_path);

    #[cfg(target_os = "windows")]
    {
        // Never route this through `cmd /c`. `cmd.exe` re-parses its command
        // line with its own metacharacter rules, which Rust's argument quoting
        // does not target — so an indexed repo file named `note&calc.txt`
        // became an execution primitive the moment the user clicked it.
        // Path confinement does not help: the payload is the filename itself.
        let mut cmd = std::process::Command::new("code.cmd");
        cmd.arg(&clean_path);
        let status = cmd.status().map_err(|e| {
            format!("failed to spawn editor (is VS Code's `code` command on PATH?): {e}")
        })?;
        if !status.success() {
            return Err("failed to run code command successfully".to_string());
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let mut cmd = std::process::Command::new("code");
        cmd.arg(&clean_path);
        let status = cmd.status().map_err(|e| format!("failed to spawn editor: {e}"))?;
        if !status.success() {
            return Err("failed to run code command successfully".to_string());
        }
    }

    Ok(())
}

/// Securely reads a file's raw content, confined to the active project root if provided.
#[tauri::command]
async fn read_file(path: String, project_root: Option<String>) -> Result<String, String> {
    let clean_relative = path.trim_start_matches(['/', '\\']);
    let root_path = if let Some(ref root_str) = project_root {
        PathBuf::from(root_str)
    } else {
        find_repo_root().ok_or_else(|| "no active project root found".to_string())?
    };
    let abs_path = root_path.join(clean_relative);

    let canonical = abs_path
        .canonicalize()
        .map_err(|e| format!("failed to canonicalize path for reading: {e}"))?;

    let root_canonical = root_path
        .canonicalize()
        .map_err(|e| format!("cannot resolve root: {e}"))?;
    if !canonical.starts_with(&root_canonical) {
        return Err("access denied: path outside root".to_string());
    }

    std::fs::read_to_string(&canonical).map_err(|e| format!("failed to read file: {e}"))
}

#[tauri::command]
fn read_symbol(path: String, symbol: String, project_root: Option<String>) -> Result<String, String> {
    let clean_relative = path.trim_start_matches(['/', '\\']);
    let root_path = if let Some(ref root_str) = project_root {
        PathBuf::from(root_str)
    } else {
        find_repo_root().ok_or_else(|| "no active project root found".to_string())?
    };
    let abs_path = root_path.join(clean_relative);

    let canonical = abs_path
        .canonicalize()
        .map_err(|e| format!("failed to canonicalize path for reading: {e}"))?;

    let root_canonical = root_path
        .canonicalize()
        .map_err(|e| format!("cannot resolve root: {e}"))?;
    if !canonical.starts_with(&root_canonical) {
        return Err("access denied: path outside root".to_string());
    }

    let db_path = root_canonical.join(".repograph/graph.db");
    let conn = repo_graph::db::init_db(&db_path).map_err(|e: rusqlite::Error| e.to_string())?;

    let repo_rel = canonical
        .strip_prefix(&root_canonical)
        .map_err(|_| "failed to get relative path")?
        .to_string_lossy()
        .replace('\\', "/");

    let range = repo_graph::db::get_symbol_range(&conn, &repo_rel, &symbol)
        .map_err(|e: rusqlite::Error| e.to_string())?
        .ok_or_else(|| format!("symbol '{symbol}' not found in file '{path}'"))?;

    let content = std::fs::read_to_string(&canonical).map_err(|e| format!("failed to read file: {e}"))?;
    let lines: Vec<&str> = content.lines().collect();

    let start = range.0.max(1);
    let end = range.1.min(lines.len());
    if start > end || start > lines.len() {
        return Err("invalid symbol line range in database".to_string());
    }

    let slice = lines[start - 1..end].join("\n");
    Ok(slice)
}

#[tauri::command]
fn get_symbol_call_graph(
    path: String,
    symbol: String,
    project_root: Option<String>,
) -> Result<repo_graph::db::CallGraph, String> {
    let clean_relative = path.trim_start_matches(['/', '\\']);
    let root_path = if let Some(ref root_str) = project_root {
        PathBuf::from(root_str)
    } else {
        find_repo_root().ok_or_else(|| "no active project root found".to_string())?
    };
    let abs_path = root_path.join(clean_relative);

    let canonical = abs_path
        .canonicalize()
        .map_err(|e| format!("failed to canonicalize path: {e}"))?;

    let root_canonical = root_path
        .canonicalize()
        .map_err(|e| format!("cannot resolve root: {e}"))?;
    if !canonical.starts_with(&root_canonical) {
        return Err("access denied: path outside root".to_string());
    }

    let db_path = root_canonical.join(".repograph/graph.db");
    let conn = repo_graph::db::init_db(&db_path).map_err(|e: rusqlite::Error| e.to_string())?;

    let repo_rel = canonical
        .strip_prefix(&root_canonical)
        .map_err(|_| "failed to get relative path")?
        .to_string_lossy()
        .replace('\\', "/");

    let cg = repo_graph::db::get_call_graph(&conn, &repo_rel, &symbol)
        .map_err(|e: rusqlite::Error| e.to_string())?;

    Ok(cg)
}

/// Discovers the active project root dynamically (useful on initial startup load).
#[tauri::command]
fn get_active_project_root() -> Option<String> {
    find_repo_root().map(|p| p.display().to_string())
}

use repo_graph::db::{
    SymbolSearchResult, ExploreSymbolBlock, ExploreFilePayload, ExplorePathEdge, ExplorePayload,
};

#[tauri::command]
fn search_symbols(query: String) -> Result<Vec<SymbolSearchResult>, String> {
    use rusqlite::params;
    let root = find_repo_root().ok_or_else(|| "no active project root found".to_string())?;
    let db_path = root.join(".repograph/graph.db");
    let conn = repo_graph::db::init_db(&db_path).map_err(|e| e.to_string())?;
    
    // Shares the sub-word tokenizer with the MCP server so the in-app search
    // and the agent-facing tool resolve identically.
    let match_query = repo_graph::db::build_fts_match_query(&query);
    if match_query.trim().is_empty() {
        return Ok(Vec::new());
    }

    let mut stmt = conn
        .prepare(
            "SELECT name, file_path, content
             FROM symbols_fts
             WHERE tokens MATCH ?
             ORDER BY rank
             LIMIT 50"
        )
        .map_err(|e| e.to_string())?;
      
    let rows = stmt
        .query_map(params![match_query], |row| {
            Ok(SymbolSearchResult {
                name: row.get(0)?,
                file_path: row.get(1)?,
                content: row.get(2)?,
            })
        })
        .map_err(|e| e.to_string())?;
      
    let mut results = Vec::new();
    for res in rows.flatten() {
        results.push(res);
    }
    Ok(results)
}

#[tauri::command]
fn explore(symbols: Vec<String>) -> Result<ExplorePayload, String> {
    use rusqlite::params;
    let root = find_repo_root().ok_or_else(|| "no active project root found".to_string())?;
    let root_canonical = root.canonicalize().map_err(|e| e.to_string())?;
    let db_path = root_canonical.join(".repograph/graph.db");
    let conn = repo_graph::db::init_db(&db_path).map_err(|e| e.to_string())?;

    let mut resolved_symbols = Vec::new();
    for sym_str in &symbols {
        let (f, s) = match sym_str.split_once('#') {
            Some(pair) => (pair.0.to_string(), pair.1.to_string()),
            None => {
                let mut stmt = conn.prepare("SELECT file_path, name FROM symbols WHERE name = ? LIMIT 1").map_err(|e| e.to_string())?;
                let res: Option<(String, String)> = stmt.query_row(params![sym_str], |r| Ok((r.get(0)?, r.get(1)?))).ok();
                match res {
                    Some(pair) => pair,
                    None => continue,
                }
            }
        };
        
        let mut stmt = conn.prepare("SELECT id, kind, start_line, end_line FROM symbols WHERE file_path = ? AND name = ? LIMIT 1").map_err(|e| e.to_string())?;
        let details = stmt.query_row(params![f, s], |r| Ok((
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
        let mut callers_stmt = conn.prepare(
            "SELECT s.file_path, s.name, s.kind
             FROM edges e
             JOIN symbols s ON e.from_symbol_id = s.id
             WHERE e.to_symbol_id = ?"
        ).map_err(|e| e.to_string())?;
        let caller_rows = callers_stmt.query_map(params![id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }).map_err(|e| e.to_string())?;
        for (cf, cs) in caller_rows.flatten() {
            let from_sym = format!("{cf}#{cs}");
            let to_sym = format!("{f}#{s}");
            let edge_key = format!("{from_sym}->{to_sym}");
            if !seen_edges.contains(&edge_key) {
                seen_edges.insert(edge_key);
                paths.push(ExplorePathEdge {
                    from_symbol: from_sym,
                    to_symbol: to_sym,
                    kind: "calls".to_string(),
                });
            }
        }

        let mut callees_stmt = conn.prepare(
            "SELECT s.file_path, s.name, s.kind
             FROM edges e
             JOIN symbols s ON e.to_symbol_id = s.id
             WHERE e.from_symbol_id = ?"
        ).map_err(|e| e.to_string())?;
        let callee_rows = callees_stmt.query_map(params![id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }).map_err(|e| e.to_string())?;
        for (tf, ts) in callee_rows.flatten() {
            let from_sym = format!("{f}#{s}");
            let to_sym = format!("{tf}#{ts}");
            let edge_key = format!("{from_sym}->{to_sym}");
            if !seen_edges.contains(&edge_key) {
                seen_edges.insert(edge_key);
                paths.push(ExplorePathEdge {
                    from_symbol: from_sym,
                    to_symbol: to_sym,
                    kind: "calls".to_string(),
                });
            }
        }

        let clean_relative = f.trim_start_matches(['/', '\\']);
        let abs_path = root_canonical.join(clean_relative);
        let canonical = match abs_path.canonicalize() {
            Ok(c) => c,
            Err(_) => continue,
        };
        if !canonical.starts_with(&root_canonical) {
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
            let code = lines[final_start - 1..final_end].join("\n");
            file_map.entry(f.clone()).or_insert_with(Vec::new).push(ExploreSymbolBlock {
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
        .map(|(path, code_blocks)| ExploreFilePayload { path, code_blocks })
        .collect();

    Ok(ExplorePayload { files, paths })
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
struct GitFileStatus {
    path: String,
    status: String,
}

#[tauri::command]
fn get_git_status(root: String) -> Result<Vec<GitFileStatus>, String> {
    let clean_root = clean_unc_path(&root);
    let root_path = PathBuf::from(&clean_root);
    if !root_path.is_dir() {
        return Err(format!("Not a valid directory: {}", root));
    }

    let output = std::process::Command::new("git")
        .arg("status")
        .arg("--porcelain=v1")
        .current_dir(&root_path)
        .output();

    let output = match output {
        Ok(out) => out,
        Err(_) => return Ok(Vec::new()),
    };

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut results = Vec::new();

    for line in stdout.lines() {
        if line.len() < 4 {
            continue;
        }
        let code = &line[0..2];
        let raw_path = line[3..].trim();
        let path = if let Some((_, new_p)) = raw_path.split_once(" -> ") {
            new_p.trim_matches('"')
        } else {
            raw_path.trim_matches('"')
        };

        let normalized_path = path.replace('\\', "/");
        let status = if code.contains('M') {
            "modified"
        } else if code.contains('A') {
            "added"
        } else if code.starts_with('?') {
            "untracked"
        } else if code.contains('D') {
            "deleted"
        } else {
            "modified"
        };

        results.push(GitFileStatus {
            path: normalized_path,
            status: status.to_string(),
        });
    }

    Ok(results)
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            read_graph,
            open_project_dialog,
            index_and_load_graph,
            read_directory_tree,
            open_in_editor,
            read_file,
            get_active_project_root,
            read_symbol,
            get_symbol_call_graph,
            search_symbols,
            explore,
            get_recent_projects,
            add_recent_project,
            set_watch_debounce,
            check_agent_scaffold,
            trigger_agent_scaffold,
            get_mcp_config_snippet,
            get_domains,
            get_git_status
        ])
        .setup(|app| {
            let app_handle = app.handle();
            std::thread::spawn(move || {
                let mut last_id = 0;
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(150));
                    let root = match find_repo_root() {
                        Some(r) => r,
                        None => continue,
                    };
                    let db_path = root.join(".repograph/graph.db");
                    if !db_path.exists() {
                        continue;
                    }
                    if let Ok(conn) = repo_graph::db::init_db(&db_path) {
                        let mut stmt = match conn.prepare("SELECT id, symbol, path, action FROM agent_queries WHERE id > ? ORDER BY id ASC") {
                            Ok(s) => s,
                            Err(_) => continue,
                        };
                        let rows = stmt.query_map([last_id], |row| {
                            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?))
                        });
                        if let Ok(rows) = rows {
                            for (id, symbol, path, action) in rows.flatten() {
                                last_id = id;
                                let payload = serde_json::json!({
                                    "symbol": symbol,
                                    "path": path,
                                    "action": action,
                                });
                                let _ = app_handle.emit_all("agent_query_event", payload);
                            }
                        }
                    }
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}


#[cfg(test)]
mod scaffold_command_tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("repograph_cmd_{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn check_reports_absence_then_presence() {
        let _guard = agent_scaffold::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let root = temp_root("check");
        let root_str = root.to_string_lossy().to_string();

        assert!(!check_agent_scaffold(root_str.clone()).unwrap());
        trigger_agent_scaffold(root_str.clone()).unwrap();
        assert!(check_agent_scaffold(root_str).unwrap());

        let _ = std::fs::remove_dir_all(&root);
    }

    /// An empty or missing path must not become an error the sidebar has to
    /// render — there is simply nothing to report.
    #[test]
    fn check_is_false_for_empty_or_missing_paths() {
        assert!(!check_agent_scaffold(String::new()).unwrap());
        assert!(!check_agent_scaffold("   ".to_string()).unwrap());
        let missing = std::env::temp_dir().join("repograph_cmd_definitely_not_here");
        let _ = std::fs::remove_dir_all(&missing);
        assert!(
            !check_agent_scaffold(missing.to_string_lossy().to_string()).unwrap()
        );
    }

    /// `trigger` writes to disk, so it refuses anything that is not an existing
    /// directory rather than creating one wherever it was pointed.
    #[test]
    fn trigger_rejects_paths_that_are_not_directories() {
        let root = temp_root("reject");
        let file = root.join("a-file.txt");
        std::fs::write(&file, "x").unwrap();

        assert!(trigger_agent_scaffold(file.to_string_lossy().to_string()).is_err());
        assert!(trigger_agent_scaffold(
            root.join("no-such-dir").to_string_lossy().to_string()
        )
        .is_err());
        assert!(trigger_agent_scaffold(String::new()).is_err());

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Clicking "Setup" twice must be harmless.
    #[test]
    fn trigger_is_idempotent_and_preserves_edits() {
        let _guard = agent_scaffold::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let root = temp_root("idempotent");
        let root_str = root.to_string_lossy().to_string();

        trigger_agent_scaffold(root_str.clone()).unwrap();
        let rules = root
            .join(agent_scaffold::SCAFFOLD_DIR)
            .join("RULES.md");
        std::fs::write(&rules, "# mine\n").unwrap();

        trigger_agent_scaffold(root_str).unwrap();
        assert_eq!(std::fs::read_to_string(&rules).unwrap(), "# mine\n");

        let _ = std::fs::remove_dir_all(&root);
    }
}
