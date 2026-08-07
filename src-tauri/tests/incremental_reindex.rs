//! Regression tests for the incremental (watcher) indexing path.
//!
//! The full-index path was always correct; these cover what happens when a
//! *single* file is re-indexed, which is what the file watcher does on every
//! save. The pre-existing coverage used a one-file repo and so could not
//! observe cross-file damage.

use std::path::{Path, PathBuf};

fn tmp(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!("{name}-{nanos}"));
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// `a.ts` imports and calls `b.ts#target`.
fn two_file_repo() -> PathBuf {
    let root = tmp("repo-graph-incremental");
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("src/b.ts"),
        "export function target() {\n  return 1\n}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("src/a.ts"),
        "import { target } from './b'\nexport function caller() {\n  return target()\n}\n",
    )
    .unwrap();
    repo_graph::indexer::index_repo(&root, None).expect("initial index");
    root
}

fn inbound_edge_count(root: &Path) -> i64 {
    let conn = repo_graph::db::init_db(&root.join(".repograph/graph.db")).unwrap();
    conn.query_row(
        "SELECT COUNT(*) FROM edges e
         JOIN symbols f ON e.from_symbol_id = f.id
         JOIN symbols t ON e.to_symbol_id   = t.id
         WHERE t.file_path = 'src/b.ts' AND t.name = 'target'
           AND f.file_path = 'src/a.ts'",
        [],
        |r| r.get(0),
    )
    .unwrap()
}

fn reindex(root: &Path, rel: &str) {
    let abs = root.join(rel);
    let source = std::fs::read_to_string(&abs).unwrap();
    let entry = repo_graph::walker::FileEntry {
        path: rel.to_string(),
        size_bytes: source.len() as u64,
        extension: abs.extension().and_then(|s| s.to_str()).unwrap_or("").to_string(),
        language: repo_graph::walker::language_for(&abs),
    };
    let parsed = repo_graph::indexer::parse_one(root, entry);

    let conn = repo_graph::db::init_db(&root.join(".repograph/graph.db")).unwrap();
    let dependents = repo_graph::db::inbound_dependents(&conn, rel);
    let aliases = repo_graph::graph::load_tsconfig_aliases(root);
    let file_set: std::collections::BTreeSet<String> =
        ["src/a.ts".to_string(), "src/b.ts".to_string()].into_iter().collect();

    repo_graph::db::populate_file_in_db(&conn, &parsed, root, &aliases, &file_set).unwrap();

    // What the watcher now does: restore the edges the FK cascade removed.
    for dep in dependents {
        let dep_abs = root.join(&dep);
        let Ok(src) = std::fs::read_to_string(&dep_abs) else { continue };
        let dep_entry = repo_graph::walker::FileEntry {
            path: dep.clone(),
            size_bytes: src.len() as u64,
            extension: dep_abs.extension().and_then(|s| s.to_str()).unwrap_or("").to_string(),
            language: repo_graph::walker::language_for(&dep_abs),
        };
        let dep_parsed = repo_graph::indexer::parse_one(root, dep_entry);
        repo_graph::db::populate_file_in_db(&conn, &dep_parsed, root, &aliases, &file_set).unwrap();
    }
}

/// Re-indexing a file deletes its `files` row; the `ON DELETE CASCADE` chain
/// (`files` → `symbols` → `edges`) takes every edge pointing *into* it, even
/// edges owned by files that were never touched. Nothing re-parses those
/// owners on its own, so `repograph_callers` started answering "no callers"
/// after a single save.
#[test]
fn incremental_reindex_preserves_inbound_cross_file_edges() {
    let root = two_file_repo();
    let before = inbound_edge_count(&root);
    assert!(before > 0, "precondition: the initial index must build the edge");

    std::fs::write(
        root.join("src/b.ts"),
        "export function target() {\n  return 2\n}\n",
    )
    .unwrap();
    reindex(&root, "src/b.ts");

    assert_eq!(
        inbound_edge_count(&root),
        before,
        "inbound edges from src/a.ts were lost when src/b.ts was re-indexed"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// Re-indexing the same file repeatedly must converge, not accumulate or erode.
#[test]
fn repeated_incremental_reindex_is_stable() {
    let root = two_file_repo();
    let before = inbound_edge_count(&root);
    for _ in 0..4 {
        reindex(&root, "src/b.ts");
    }
    assert_eq!(inbound_edge_count(&root), before);
    let _ = std::fs::remove_dir_all(&root);
}

/// `inbound_dependents` is what makes the repair possible; if it stops
/// reporting the owner, the repair silently becomes a no-op again.
#[test]
fn inbound_dependents_reports_the_importing_file() {
    let root = two_file_repo();
    let conn = repo_graph::db::init_db(&root.join(".repograph/graph.db")).unwrap();
    let deps = repo_graph::db::inbound_dependents(&conn, "src/b.ts");
    assert!(deps.contains(&"src/a.ts".to_string()), "got {deps:?}");
    // Never reports the file itself — that would re-parse it twice per save.
    assert!(!deps.contains(&"src/b.ts".to_string()), "got {deps:?}");
    let _ = std::fs::remove_dir_all(&root);
}
