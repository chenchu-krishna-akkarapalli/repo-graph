//! `mcp_server` binary.
//!
//! Usage:
//!   `mcp_server [OPTIONS] [repo_root]` — serve the read-only MCP API (stdio)
//!   `mcp_server index <repo_root>`      — walk + parse the repo and write `.repograph/graph.json` and `.repograph/graph.db`
//!   `mcp_server install-rules <repo_root>` — provision/update agent rules in workspace
//!
//! Options:
//!   `-r, --root <PATH>`   Target project directory
//!   `-t, --tools <TOOLS>` Comma-separated list of tools or 'all'

use repo_graph::mcp_server::McpServer;
use std::path::{Path, PathBuf};

fn install_rules(root: &Path) {
    let _ = repo_graph::agent_scaffold::ensure_agent_scaffold(root);
    let _ = repo_graph::rule_injector::ensure_workspace_rules(root);
    eprintln!("Successfully ensured agent rules and scaffold in {}", root.display());
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut root_arg: Option<PathBuf> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "index" => {
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
            "install-rules" => {
                let root = args.next().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
                install_rules(&root);
                return;
            }
            "--root" | "-r" => {
                if let Some(r) = args.next() {
                    root_arg = Some(PathBuf::from(r));
                }
            }
            "--tools" | "-t" => {
                if let Some(t) = args.next() {
                    std::env::set_var("REPOGRAPH_MCP_TOOLS", t);
                }
            }
            "--help" | "-h" => {
                eprintln!("Repo Graph MCP Server v1.4");
                eprintln!("Usage: mcp_server [OPTIONS] [REPO_ROOT]\n");
                eprintln!("Commands:");
                eprintln!("  index [REPO_ROOT]          Walk and parse the repo into .repograph/graph.db");
                eprintln!("  install-rules [REPO_ROOT]  Provision agent rules into workspace\n");
                eprintln!("Options:");
                eprintln!("  -r, --root <PATH>          Set target project root");
                eprintln!("  -t, --tools <TOOLS>        Comma-separated list of enabled tools (or 'all')");
                eprintln!("  -h, --help                 Print help");
                return;
            }
            other if !other.starts_with('-') && root_arg.is_none() => {
                root_arg = Some(PathBuf::from(other));
            }
            _ => {}
        }
    }

    let root = root_arg
        .or_else(|| std::env::var("REPOGRAPH_PROJECT_ROOT").ok().map(PathBuf::from))
        .or_else(|| std::env::var("REPO_GRAPH_ROOT").ok().map(PathBuf::from))
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
