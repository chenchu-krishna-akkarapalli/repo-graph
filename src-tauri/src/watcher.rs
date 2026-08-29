use notify::{EventKind, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Manager};

static WATCHED_ROOT: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

/// Bumped whenever a new root is watched. The debounce thread reads it every
/// tick and exits when it no longer matches the generation it was spawned for,
/// so switching projects retires the old thread instead of leaving it running
/// against a stale root.
static WATCH_GENERATION: AtomicU64 = AtomicU64::new(0);

fn db_path_for(root: &Path) -> PathBuf {
    root.join(".repograph/graph.db")
}

/// Repo-relative, forward-slashed form of `path` — the key shape used
/// everywhere else in the index.
fn relative_key(root: &Path, path: &Path) -> String {
    let canonical_root = dunce::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let canonical_path = dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let rel = canonical_path
        .strip_prefix(&canonical_root)
        .or_else(|_| path.strip_prefix(root))
        .unwrap_or(path);
    rel.to_string_lossy()
        .replace('\\', "/")
}

/// Pending edits for `root`, as `(repo-relative path, epoch millis)`.
///
/// Backed by `graph.db` rather than a process-local static: the watcher runs
/// inside the Tauri app, but the freshness warning is served by the standalone
/// `mcp_server` process. While this was a `static`, that process always saw an
/// empty map and reported "Synced" unconditionally — a false assurance, which
/// is worse than no signal at all.
pub fn get_pending_map(root: &Path) -> HashMap<String, i64> {
    let db = db_path_for(root);
    if !db.exists() {
        return HashMap::new();
    }
    match crate::db::init_db(&db) {
        Ok(conn) => crate::db::get_pending(&conn).into_iter().collect(),
        Err(_) => HashMap::new(),
    }
}

pub fn mark_pending(root: &Path, path: &Path) {
    if let Ok(conn) = crate::db::init_db(&db_path_for(root)) {
        crate::db::mark_pending(&conn, &relative_key(root, path));
    }
}

pub fn remove_pending(root: &Path, path: &Path) {
    if let Ok(conn) = crate::db::init_db(&db_path_for(root)) {
        crate::db::clear_pending(&conn, &relative_key(root, path));
    }
}

pub fn start_watcher(app_handle: AppHandle, root: PathBuf) {
    let root = match root.canonicalize() {
        Ok(r) => r,
        Err(_) => return,
    };

    let generation = {
        let mut watched = WATCHED_ROOT.lock().unwrap();
        if let Some(ref path) = *watched {
            if path == &root {
                println!("[watcher] already watching {}", root.display());
                return;
            }
        }
        *watched = Some(root.clone());
        WATCH_GENERATION.fetch_add(1, Ordering::SeqCst) + 1
    };

    println!("[watcher] starting watcher for {}", root.display());

    let debounce_ms = std::env::var("CODEGRAPH_WATCH_DEBOUNCE_MS")
        .ok()
        .and_then(|val| val.parse::<u64>().ok())
        .map(|val| val.clamp(50, 60000))
        .unwrap_or(150);

    let root_clone = root.clone();
    let app_clone = app_handle.clone();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(50));
            // A newer root took over — retire rather than keep re-indexing a
            // project the user has already closed.
            if WATCH_GENERATION.load(Ordering::SeqCst) != generation {
                println!("[watcher] retiring debounce thread for {}", root_clone.display());
                return;
            }

            let now = chrono::Utc::now().timestamp_millis();
            let due: Vec<String> = get_pending_map(&root_clone)
                .into_iter()
                .filter(|&(_, marked_at)| now.saturating_sub(marked_at) >= debounce_ms as i64)
                .map(|(path, _)| path)
                .collect();

            if due.is_empty() {
                continue;
            }

            let mut any_ok = false;
            for rel in due {
                println!("[watcher] debounced change: {rel}");
                match sync_path(&root_clone, &rel) {
                    Ok(()) => any_ok = true,
                    Err(e) => eprintln!("[watcher] failed to sync {rel}: {e}"),
                }
                // Clear regardless: a file that cannot be parsed must not be
                // retried every 100 ms forever.
                if let Ok(conn) = crate::db::init_db(&db_path_for(&root_clone)) {
                    crate::db::clear_pending(&conn, &rel);
                }
            }
            if any_ok {
                if let Ok(conn) = crate::db::init_db(&db_path_for(&root_clone)) {
                    if let Ok(graph) = crate::db::load_graph_from_db(&conn) {
                        let _ = graph.save_to_cache(&root_clone);
                    }
                }
                let _ = app_clone.emit_all("graph_updated", ());
            }
        }
    });

    let filter_root = root.clone();
    let mut watcher = match notify::recommended_watcher(
        move |res: Result<notify::Event, notify::Error>| {
            let Ok(event) = res else { return };
            // Removes and renames matter as much as writes: without them a
            // deleted file kept its nodes, symbols and edges in the graph
            // forever, and agents were told about code that no longer exists.
            if !matches!(
                event.kind,
                EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
            ) {
                return;
            }
            for path in event.paths {
                if is_supported_file(&filter_root, &path) {
                    mark_pending(&filter_root, &path);
                }
            }
        },
    ) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("[watcher] failed to start: {e}");
            return;
        }
    };

    if let Err(e) = watcher.watch(&root, RecursiveMode::Recursive) {
        eprintln!("[watcher] failed to watch root: {e}");
    }

    // Held for the process lifetime: dropping it stops delivery. The
    // generation counter above is what retires superseded *threads*.
    Box::leak(Box::new(watcher));
}

/// Standalone watcher for MCP server process without a Tauri AppHandle.
pub fn start_standalone_watcher(root: PathBuf) {
    let root = match dunce::canonicalize(&root).or_else(|_| root.canonicalize()) {
        Ok(r) => r,
        Err(_) => return,
    };

    let debounce_ms = std::env::var("CODEGRAPH_WATCH_DEBOUNCE_MS")
        .ok()
        .and_then(|val| val.parse::<u64>().ok())
        .map(|val| val.clamp(50, 60000))
        .unwrap_or(150);

    let root_clone = root.clone();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(50));

            let now = chrono::Utc::now().timestamp_millis();
            let due: Vec<String> = get_pending_map(&root_clone)
                .into_iter()
                .filter(|&(_, marked_at)| now.saturating_sub(marked_at) >= debounce_ms as i64)
                .map(|(path, _)| path)
                .collect();

            if due.is_empty() {
                continue;
            }

            let mut any_ok = false;
            for rel in due {
                match sync_path(&root_clone, &rel) {
                    Ok(()) => any_ok = true,
                    Err(e) => eprintln!("[watcher] failed to sync {rel}: {e}"),
                }
                if let Ok(conn) = crate::db::init_db(&db_path_for(&root_clone)) {
                    crate::db::clear_pending(&conn, &rel);
                }
            }
            if any_ok {
                if let Ok(conn) = crate::db::init_db(&db_path_for(&root_clone)) {
                    if let Ok(graph) = crate::db::load_graph_from_db(&conn) {
                        let _ = graph.save_to_cache(&root_clone);
                    }
                }
            }
        }
    });

    let filter_root = root.clone();
    let mut watcher = match notify::recommended_watcher(
        move |res: Result<notify::Event, notify::Error>| {
            let Ok(event) = res else { return };
            if matches!(event.kind, EventKind::Access(_)) {
                return;
            }
            for path in event.paths {
                if is_supported_file(&filter_root, &path) {
                    mark_pending(&filter_root, &path);
                }
            }
        },
    ) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("[watcher] failed to start standalone watcher: {e}");
            return;
        }
    };

    if let Err(e) = watcher.watch(&root, RecursiveMode::Recursive) {
        eprintln!("[watcher] failed to watch root: {e}");
    }

    Box::leak(Box::new(watcher));
}

/// Gate watcher events through the **same** denylist the walker uses.
///
/// This previously kept a private list naming only `.git`, `node_modules` and
/// `.repograph`, so build output slipped through: a single `npm run build`
/// wrote `dist/assets/index-<hash>.js`, the watcher re-parsed that minified
/// bundle, and because minified code is a handful of enormous lines every one
/// of its thousands of symbols stored a ~190 KB source slice in `symbols_fts`.
/// That alone grew the cache to 664 MB and hung startup reconciliation.
fn is_supported_file(root: &Path, path: &Path) -> bool {
    let canonical_root = dunce::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let canonical_path = dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let relative = canonical_path
        .strip_prefix(&canonical_root)
        .or_else(|_| path.strip_prefix(root))
        .unwrap_or(path);
    for component in relative.components() {
        let name = component.as_os_str().to_string_lossy();
        if crate::walker::SKIP_DIRS.contains(&name.as_ref()) {
            return false;
        }
    }
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        if crate::walker::is_secret_file(name) {
            return false;
        }
    }
    // Only languages with a real extractor are worth re-indexing; this also
    // drops lockfiles and binaries, which resolve to `other`.
    crate::parsers::extractor_for(&crate::walker::language_for(path)).is_some()
}

/// Re-index one changed path — or purge it if it no longer exists.
pub fn sync_path(root: &Path, rel_path: &str) -> Result<(), String> {
    let abs = root.join(rel_path);
    if !abs.is_file() {
        return purge_file(root, rel_path);
    }
    reparse_file(root, rel_path)
}

/// Drop a deleted file from the index, then rebuild whatever pointed at it.
fn purge_file(root: &Path, rel_path: &str) -> Result<(), String> {
    let conn = crate::db::init_db(&db_path_for(root)).map_err(|e| e.to_string())?;
    let dependents = crate::db::inbound_dependents(&conn, rel_path);
    // Cascades to symbols → edges, and to `file_edges` in both directions.
    conn.execute("DELETE FROM files WHERE path = ?", [rel_path])
        .map_err(|e| e.to_string())?;
    reindex_dependents(root, &conn, dependents);
    Ok(())
}

fn reparse_file(root: &Path, rel_path: &str) -> Result<(), String> {
    let conn = crate::db::init_db(&db_path_for(root)).map_err(|e| e.to_string())?;

    // Must be read *before* `populate_file_in_db` deletes the file row: the
    // cascade takes every inbound edge with it, including edges owned by files
    // that were never touched, and nothing else can put them back.
    let dependents = crate::db::inbound_dependents(&conn, rel_path);

    let aliases = crate::graph::load_tsconfig_aliases(root);
    let file_set = current_file_set(&conn, rel_path);

    let parsed = parse_relative(root, rel_path)?;
    crate::db::populate_file_in_db(&conn, &parsed, root, &aliases, &file_set)
        .map_err(|e| e.to_string())?;

    reindex_dependents(root, &conn, dependents);
    Ok(())
}

/// Re-parse each dependent so the edges the cascade removed come back.
fn reindex_dependents(root: &Path, conn: &rusqlite::Connection, dependents: Vec<String>) {
    if dependents.is_empty() {
        return;
    }
    let aliases = crate::graph::load_tsconfig_aliases(root);
    let file_set = current_file_set(conn, "");
    for dep in dependents {
        match parse_relative(root, &dep) {
            Ok(parsed) => {
                if let Err(e) =
                    crate::db::populate_file_in_db(conn, &parsed, root, &aliases, &file_set)
                {
                    eprintln!("[watcher] failed to restore edges from {dep}: {e}");
                }
            }
            // A dependent that has itself been deleted simply has no edges to
            // restore.
            Err(e) => eprintln!("[watcher] skipping dependent {dep}: {e}"),
        }
    }
}

/// Every indexed path, plus `extra` if it is not there yet (a brand-new file
/// has to be in the set for its own imports to resolve).
fn current_file_set(
    conn: &rusqlite::Connection,
    extra: &str,
) -> std::collections::BTreeSet<String> {
    let mut set = std::collections::BTreeSet::new();
    if let Ok(mut stmt) = conn.prepare("SELECT path FROM files") {
        if let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(0)) {
            set.extend(rows.filter_map(|r| r.ok()));
        }
    }
    if !extra.is_empty() {
        set.insert(extra.to_string());
    }
    set
}

fn parse_relative(root: &Path, rel_path: &str) -> Result<crate::graph::ParsedFile, String> {
    let abs = root.join(rel_path);
    let source = std::fs::read_to_string(&abs).map_err(|e| e.to_string())?;
    let entry = crate::walker::FileEntry {
        path: rel_path.to_string(),
        size_bytes: source.len() as u64,
        extension: abs
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string(),
        language: crate::walker::language_for(&abs),
    };
    Ok(crate::indexer::parse_one(root, entry))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_output_is_never_re_indexed() {
        let root = Path::new("C:/repo");
        // The exact path that grew the cache to 664 MB before this filter.
        assert!(!is_supported_file(root, &root.join("dist/assets/index-8l0NhbsN.js")));
        assert!(!is_supported_file(root, &root.join("build/bundle.js")));
        assert!(!is_supported_file(root, &root.join("out/app.js")));
        assert!(!is_supported_file(root, &root.join(".next/static/chunk.js")));
        assert!(!is_supported_file(root, &root.join("coverage/lcov-report/x.js")));
        assert!(!is_supported_file(root, &root.join("node_modules/react/index.js")));
        assert!(!is_supported_file(root, &root.join("src-tauri/target/debug/x.rs")));
        assert!(!is_supported_file(root, &root.join(".repograph/graph.json")));
    }

    #[test]
    fn real_sources_are_watched_including_newly_supported_languages() {
        let root = Path::new("C:/repo");
        assert!(is_supported_file(root, &root.join("src/lib/layout.ts")));
        assert!(is_supported_file(root, &root.join("src-tauri/src/db.rs")));
        assert!(is_supported_file(root, &root.join("api/main.py")));
        // Previously unreachable: the watcher had its own 7-extension list.
        assert!(is_supported_file(root, &root.join("src/App.vue")));
        assert!(is_supported_file(root, &root.join("src/Page.svelte")));
        assert!(is_supported_file(root, &root.join("Main.java")));
        // Lockfiles/binaries resolve to `other` → no extractor → ignored.
        assert!(!is_supported_file(root, &root.join("package-lock.json")));
    }

    /// The watcher is the one component that re-reads files after the walker
    /// has already filtered them, so it needs its own secrets gate.
    #[test]
    fn secret_files_are_never_watched() {
        let root = Path::new("C:/repo");
        assert!(!is_supported_file(root, &root.join(".env")));
        assert!(!is_supported_file(root, &root.join("config/.env.production")));
    }

    #[test]
    fn standalone_watcher_reindexes_modified_file_in_realtime() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("repograph-watch-test-{nanos}"));
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("src/lib.ts"),
            "export const initialVal = 10;\n",
        )
        .unwrap();

        let _ = crate::indexer::index_repo(&root, None);
        start_standalone_watcher(root.clone());
        std::thread::sleep(Duration::from_millis(100));

        std::fs::write(
            root.join("src/lib.ts"),
            "export const updatedVal = 42;\nexport const newHelper = () => true;\n",
        )
        .unwrap();

        let db = db_path_for(&root);
        let mut updated = false;
        for _ in 0..25 {
            std::thread::sleep(Duration::from_millis(50));
            if let Ok(conn) = crate::db::init_db(&db) {
                if let Ok(all_symbols) = crate::db::get_all_symbols(&conn) {
                    let symbols = all_symbols.get("src/lib.ts").cloned().unwrap_or_default();
                    let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
                    if names.contains(&"updatedVal") || names.contains(&"newHelper") {
                        updated = true;
                        break;
                    }
                }
            }
        }
        assert!(updated, "Watcher failed to incrementally reindex modified file within timeout");

        let _ = std::fs::remove_dir_all(&root);
    }
}
