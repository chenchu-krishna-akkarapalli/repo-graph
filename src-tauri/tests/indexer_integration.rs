//! End-to-end pipeline test against the checked-in fixture repo:
//! walk → parse (JS/TS, Python, Rust) → build graph → serialize cache.

use repo_graph::graph::{EdgeKind, Graph};
use repo_graph::indexer::{build_graph_for, index_repo};
use std::path::{Path, PathBuf};

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mixed-repo")
}

fn has_edge(g: &Graph, from: &str, to: &str, kind: EdgeKind) -> bool {
    g.edges
        .iter()
        .any(|e| e.from_path == from && e.to_path == to && e.kind == kind)
}

#[test]
fn full_pipeline_over_mixed_fixture_repo() {
    let graph = build_graph_for(&fixture_root());

    // Walker: noise skipped, sources kept.
    assert!(graph.node("src/components/Button.tsx").is_some());
    assert!(
        graph.nodes.iter().all(|n| !n.path.contains("node_modules")),
        "node_modules must never be indexed"
    );

    // JS/TS edges + Next.js route tagging.
    assert!(has_edge(
        &graph,
        "src/components/Button.tsx",
        "src/hooks/useTheme.ts",
        EdgeKind::Import
    ));
    assert!(has_edge(
        &graph,
        "src/app/api/login/route.ts",
        "src/lib/db.ts",
        EdgeKind::Import
    ));
    let route_node = graph.node("src/app/api/login/route.ts").unwrap();
    assert_eq!(route_node.routes, vec!["/api/login"]);

    // Python: package-relative resolution + FastAPI decorator route.
    assert!(has_edge(&graph, "main.py", "api/routes.py", EdgeKind::Import));
    assert_eq!(graph.node("api/routes.py").unwrap().routes, vec!["/items"]);

    // Rust: mod + use edges inside the fixture crate.
    assert!(has_edge(
        &graph,
        "rustsvc/src/lib.rs",
        "rustsvc/src/util.rs",
        EdgeKind::Mod
    ));
    assert!(has_edge(
        &graph,
        "rustsvc/src/lib.rs",
        "rustsvc/src/util.rs",
        EdgeKind::Use
    ));

    // Go: package exports and imports
    assert!(graph.node("main.go").is_some());
    assert!(has_edge(&graph, "main.go", "helper/helper.go", EdgeKind::Import));
    assert!(graph.node("main.go").unwrap().exports.contains(&"Main".to_string()));

    // Java: package imports and classes
    assert!(graph.node("com/example/App.java").is_some());
    assert!(has_edge(&graph, "com/example/App.java", "com/example/Util.java", EdgeKind::Import));
    assert!(graph.node("com/example/App.java").unwrap().exports.contains(&"App".to_string()));

    // Kotlin: package imports
    assert!(graph.node("com/example/kotlin/App.kt").is_some());
    assert!(has_edge(&graph, "com/example/kotlin/App.kt", "com/example/kotlin/Helper.kt", EdgeKind::Import));

    // C/C++: includes
    assert!(graph.node("main.cpp").is_some());
    assert!(has_edge(&graph, "main.cpp", "helper.h", EdgeKind::Import));

    // Swift: imports
    assert!(graph.node("app.swift").is_some());

    // HTML: script/link imports and id exports
    assert!(graph.node("index.html").is_some());
    assert!(has_edge(&graph, "index.html", "src/components/Button.tsx", EdgeKind::Import));
    assert!(has_edge(&graph, "index.html", "main.py", EdgeKind::Import));
    assert!(graph.node("index.html").unwrap().exports.contains(&"id:app-root".to_string()));

    // SQL: foreign key REFERENCES
    assert!(graph.node("schema.sql").is_some());
    assert!(graph.node("schema.sql").unwrap().exports.contains(&"users".to_string()));
    assert!(graph.node("schema.sql").unwrap().exports.contains(&"posts".to_string()));

    // Prisma: relations
    assert!(graph.node("schema.prisma").is_some());
    assert!(graph.node("schema.prisma").unwrap().exports.contains(&"User".to_string()));

    // Dockerfile: base image and copy imports
    assert!(graph.node("Dockerfile").is_some());
    assert!(has_edge(&graph, "Dockerfile", "main.py", EdgeKind::Import));

    // Markdown: links
    assert!(graph.node("README.md").is_some());
    assert!(has_edge(&graph, "README.md", "main.go", EdgeKind::Import));
    assert!(has_edge(&graph, "README.md", "main.py", EdgeKind::Import));
    assert!(has_edge(&graph, "README.md", "index.html", EdgeKind::Import));

    // Externals collected, not turned into edges.
    for dep in ["react", "pg", "fastapi", "serde", "fmt", "java.util.List", "Foundation", "LocalModule", "node:18"] {
        assert!(
            graph.external_dependencies.iter().any(|d| d == dep),
            "missing external dependency {dep}"
        );
    }

    // Degrees.
    assert_eq!(graph.node("src/hooks/useTheme.ts").unwrap().in_degree, 1);
    assert_eq!(graph.node("src/components/Button.tsx").unwrap().out_degree, 1);

    // Broken file: warning, not a crash; node still present.
    assert!(graph.node("src/broken.ts").is_some());
    assert!(graph
        .warnings
        .iter()
        .any(|w| w.path == "src/broken.ts" && w.kind == "parse_error"));

    // Exports survived into nodes.
    assert_eq!(
        graph.node("src/components/Button.tsx").unwrap().exports,
        vec!["Button"]
    );
    assert_eq!(graph.node("rustsvc/src/util.rs").unwrap().exports, vec!["helper"]);
}

#[test]
fn index_repo_writes_loadable_cache() {
    // Copy the fixture to a temp dir so the checked-in fixture stays clean.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp = std::env::temp_dir().join(format!("repo-graph-index-{nanos}"));
    copy_tree(&fixture_root(), &tmp);

    let summary = index_repo(&tmp, None).expect("indexing succeeds");
    assert!(summary.files > 5);
    assert!(summary.edges >= 4);
    assert!(summary.cache_path.ends_with(Path::new(".repograph/graph.json")));

    let loaded = Graph::load_from_cache(&summary.cache_path).expect("cache round-trips");
    assert_eq!(loaded.schema_version, 1);
    assert_eq!(loaded.nodes.len(), summary.files);

    let _ = std::fs::remove_dir_all(tmp);
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}
