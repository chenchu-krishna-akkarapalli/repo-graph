//! Indexing pipeline orchestration: walk → parse (parallel) → build graph
//! → write `.repograph/graph.json`.

use crate::graph::{build_graph, Graph, ParsedFile};
use crate::parsers::{extractor_for, ExtractionResult};
use crate::walker::{walk_repo, FileEntry};
use std::path::Path;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::Manager;

/// Files above this size are indexed but never parsed (generated/minified
/// files would dominate runtime otherwise).
const MAX_PARSE_BYTES: u64 = 1_000_000;

/// Progress monitor poll interval — the ceiling on event latency, not the
/// floor: every poll reports whatever `files_processed` has reached, so a
/// burst of >100 files between polls is still caught on the next tick
/// instead of waiting for an exact multiple.
const PROGRESS_POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug)]
pub struct IndexSummary {
    pub files: usize,
    pub edges: usize,
    pub external_dependencies: usize,
    pub warnings: usize,
    pub duration_ms: u128,
    pub cache_path: std::path::PathBuf,
}

/// `index_progress` event payload. `phase` mirrors the pipeline stages in
/// `index_repo`: a UI can drive a single progress bar off this alone.
#[derive(Debug, Clone, serde::Serialize)]
pub struct IndexProgress {
    pub phase: &'static str,
    pub files_total: usize,
    pub files_processed: usize,
    pub bytes_total: u64,
    pub bytes_processed: u64,
}

/// No-op when `app_handle` is `None` — every non-desktop caller (the
/// `mcp_server` CLI, `db::reconcile_repo_startup`, and the test suite) has
/// no `tauri::AppHandle` to construct, since one only exists once
/// `tauri::Builder` has actually started an app.
fn emit_progress(
    app_handle: Option<&tauri::AppHandle>,
    phase: &'static str,
    files_processed: usize,
    files_total: usize,
    bytes_processed: u64,
    bytes_total: u64,
) {
    let Some(handle) = app_handle else { return };
    let payload = IndexProgress {
        phase,
        files_total,
        files_processed,
        bytes_total,
        bytes_processed,
    };
    let _ = handle.emit_all("index_progress", payload);
}

pub fn index_repo(root: &Path, app_handle: Option<&tauri::AppHandle>) -> Result<IndexSummary, String> {
    let started = Instant::now();
    let root = root
        .canonicalize()
        .map_err(|e| format!("cannot open repo root {}: {e}", root.display()))?;

    emit_progress(app_handle, "walking", 0, 0, 0, 0);
    let files = walk_repo(&root);
    let files_total = files.len();
    let bytes_total: u64 = files.iter().map(|f| f.size_bytes).sum();
    emit_progress(app_handle, "parsing", 0, files_total, 0, bytes_total);

    let parsed = parse_all(&root, files, app_handle, files_total, bytes_total);
    emit_progress(app_handle, "db_write", files_total, files_total, bytes_total, bytes_total);

    // Populate SQLite database
    let db_path = root.join(".repograph/graph.db");
    if let Some(parent) = db_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut db_symbols = None;
    let mut db_symbol_edges = None;
    match crate::db::init_db(&db_path) {
        Ok(mut conn) => {
            if let Err(e) = crate::db::populate_db(&mut conn, &parsed, &root) {
                eprintln!("[db] failed to populate SQLite database: {e}");
            }
            // Read back DB-derived symbols (incl. route symbols created
            // during binding) so graph.json matches what the desktop IPC
            // serves — the browser dev bridge reads the JSON directly.
            db_symbols = crate::db::get_all_symbols(&conn).ok();
            db_symbol_edges = crate::db::get_all_symbol_edges(&conn).ok();
        }
        Err(e) => {
            eprintln!("[db] failed to initialize SQLite: {e}");
        }
    }

    let aliases = crate::graph::load_tsconfig_aliases(&root);

    let mut graph = build_graph(parsed, &aliases);
    if let Some(mut symbols_map) = db_symbols {
        for node in &mut graph.nodes {
            if let Some(syms) = symbols_map.remove(&node.path) {
                node.symbols = syms;
            }
        }
    }
    if let Some(edges) = db_symbol_edges {
        graph.symbol_edges = edges;
    }
    let cache_path = graph
        .save_to_cache(&root)
        .map_err(|e| format!("failed to write graph cache: {e}"))?;

    emit_progress(app_handle, "complete", files_total, files_total, bytes_total, bytes_total);

    Ok(IndexSummary {
        files: graph.nodes.len(),
        edges: graph.edges.len(),
        external_dependencies: graph.external_dependencies.len(),
        warnings: graph.warnings.len(),
        duration_ms: started.elapsed().as_millis(),
        cache_path,
    })
}

/// Parse every parseable file across a fixed worker pool (scoped threads —
/// no async runtime needed for a CPU-bound batch). When `app_handle` is
/// `Some`, a dedicated monitor thread polls the shared atomics every
/// `PROGRESS_POLL_INTERVAL` and emits `index_progress` — workers stay
/// lock-step-free (`fetch_add` only) and never make IPC calls themselves,
/// so parse throughput does not scale down with how often the UI updates.
fn parse_all(
    root: &Path,
    files: Vec<FileEntry>,
    app_handle: Option<&tauri::AppHandle>,
    files_total: usize,
    bytes_total: u64,
) -> Vec<ParsedFile> {
    let workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let queue: Mutex<Vec<FileEntry>> = Mutex::new(files);
    let results: Mutex<Vec<ParsedFile>> = Mutex::new(Vec::new());
    let files_processed = AtomicUsize::new(0);
    let bytes_processed = AtomicU64::new(0);

    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let entry = match queue.lock().unwrap().pop() {
                    Some(e) => e,
                    None => break,
                };
                let entry_bytes = entry.size_bytes;
                let parsed = parse_one(root, entry);
                results.lock().unwrap().push(parsed);
                files_processed.fetch_add(1, Ordering::Relaxed);
                bytes_processed.fetch_add(entry_bytes, Ordering::Relaxed);
            });
        }

        if app_handle.is_some() && files_total > 0 {
            scope.spawn(|| loop {
                let processed = files_processed.load(Ordering::Relaxed);
                emit_progress(
                    app_handle,
                    "parsing",
                    processed,
                    files_total,
                    bytes_processed.load(Ordering::Relaxed),
                    bytes_total,
                );
                if processed >= files_total {
                    break;
                }
                std::thread::sleep(PROGRESS_POLL_INTERVAL);
            });
        }
    });

    let mut parsed = results.into_inner().unwrap();
    parsed.sort_by(|a, b| a.entry.path.cmp(&b.entry.path));
    parsed
}

pub fn parse_one(root: &Path, entry: FileEntry) -> ParsedFile {
    let mut extraction = ExtractionResult::default();

    if entry.size_bytes > MAX_PARSE_BYTES {
        extraction.warnings.push("skipped_large_file".to_string());
        return ParsedFile { entry, extraction };
    }
    let Some(extractor) = extractor_for(&entry.language) else {
        return ParsedFile { entry, extraction }; // indexed, not parsed
    };
    match std::fs::read_to_string(root.join(&entry.path)) {
        Ok(source) => {
            extraction = extractor.extract(Path::new(&entry.path), &source);
        }
        Err(_) => {
            // Unreadable / non-UTF-8: keep the node, flag the gap.
            extraction.warnings.push("unreadable_file".to_string());
        }
    }
    ParsedFile { entry, extraction }
}

/// Convenience for tests: index and return the graph without touching disk
/// beyond reads.
pub fn build_graph_for(root: &Path) -> Graph {
    let files = walk_repo(root);
    let aliases = crate::graph::load_tsconfig_aliases(root);
    build_graph(parse_all(root, files, None, 0, 0), &aliases)
}
