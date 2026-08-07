//! `mcp_server` binary.
//!
//! Usage:
//!   `mcp_server [repo_root]`        — serve the read-only MCP API (stdio)
//!   `mcp_server index <repo_root>`  — walk + parse the repo and write
//!                                     `.repograph/graph.json`
//!   `mcp_server install-rules <repo_root>` — append/update explore rules in workspace

use repo_graph::mcp_server::McpServer;
use std::path::{Path, PathBuf};

fn install_rules(root: &Path) {
    let marker = "\n# REPOGRAPH_EXPLORE_MARKER\nWhen researching or editing code in this workspace, always prioritize calling the `repograph_explore` tool/CLI. It retrieves exact symbol slices and call graphs in a single query, minimizing token ingestion and context costs.\n# END_REPOGRAPH_EXPLORE_MARKER\n";

    for rel_path in &["my-agent/RULES.md", ".agents/AGENTS.md"] {
        let file_path = root.join(rel_path);
        if file_path.is_file() {
            match std::fs::read_to_string(&file_path) {
                Ok(content) => {
                    let mut new_content = content.clone();
                    if let Some(start_idx) = content.find("# REPOGRAPH_EXPLORE_MARKER") {
                        if let Some(end_idx) = content.find("# END_REPOGRAPH_EXPLORE_MARKER") {
                            let end_pos = end_idx + "# END_REPOGRAPH_EXPLORE_MARKER".len();
                            new_content.replace_range(start_idx..end_pos, marker.trim());
                        } else {
                            new_content.push_str(marker);
                        }
                    } else {
                        new_content.push_str(marker);
                    }
                    if let Err(e) = std::fs::write(&file_path, new_content) {
                        eprintln!("Failed to write to {}: {e}", file_path.display());
                    } else {
                        eprintln!("Successfully updated rules in {}", file_path.display());
                    }
                }
                Err(e) => {
                    eprintln!("Failed to read {}: {e}", file_path.display());
                }
            }
        } else {
            eprintln!("Rules file {} does not exist (skipping)", file_path.display());
        }
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let first = args.next();

    if first.as_deref() == Some("index") {
        let root = args.next().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        match repo_graph::indexer::index_repo(&root, None) {
            Ok(s) => {
                eprintln!(
                    "indexed {} files → {} edges, {} external deps, {} warnings in {} ms",
                    s.files, s.edges, s.external_dependencies, s.warnings, s.duration_ms
                );
                eprintln!("graph cache written to {}", s.cache_path.display());
            }
            Err(e) => {
                eprintln!("repo-graph index: {e}");
                std::process::exit(1);
            }
        }
        return;
    }

    if first.as_deref() == Some("install-rules") {
        let root = args.next().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        install_rules(&root);
        return;
    }

    let root = first
        .or_else(|| std::env::var("REPO_GRAPH_ROOT").ok())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("auto"));

    let mut server = match McpServer::new(&root) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("repo-graph mcp_server: {e}");
            std::process::exit(1);
        }
    };
    if let Err(e) = server.serve_stdio() {
        eprintln!("repo-graph mcp_server: stdio transport failed: {e}");
        std::process::exit(1);
    }
}
