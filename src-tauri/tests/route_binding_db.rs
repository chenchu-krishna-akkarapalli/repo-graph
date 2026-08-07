//! Playbook §19 end-to-end verification: mock Hono, Echo, FastAPI,
//! Micronaut, and Axum sources are indexed; each must produce a `route`
//! symbol in SQLite with a `references` edge to its handler symbol.

use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};

fn temp_repo() -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("repo-graph-routes-{nanos}"));
    std::fs::create_dir_all(&root).unwrap();

    std::fs::write(
        root.join("hono_server.ts"),
        "export function listUsers(c) { return c.json([]); }\nconst app = { get(_p, _h) {} };\napp.get('/users', listUsers);\n",
    )
    .unwrap();

    std::fs::write(
        root.join("echo_server.go"),
        "package main\n\nfunc ListItems(c Context) error { return nil }\n\nfunc main() {\n\te := New()\n\te.GET(\"/items\", ListItems)\n}\n",
    )
    .unwrap();

    std::fs::write(
        root.join("fastapi_app.py"),
        "from fastapi import FastAPI\n\napp = FastAPI()\n\n@app.get(\"/books\")\ndef list_books():\n    return []\n",
    )
    .unwrap();

    std::fs::write(
        root.join("UserController.java"),
        "package com.example;\n\n@Controller(\"/accounts\")\npublic class UserController {\n\n    @Get(\"/{id}\")\n    public String show(Long id) { return \"\"; }\n}\n",
    )
    .unwrap();

    std::fs::write(
        root.join("axum_app.rs"),
        "async fn list_orders() -> String { String::new() }\n\nfn build() {\n    let app = Router::new().route(\"/orders\", get(list_orders));\n}\n",
    )
    .unwrap();

    root
}

fn assert_route_binding(conn: &Connection, file: &str, route: &str, handler: &str) {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*)
             FROM edges e
             JOIN symbols r ON e.from_symbol_id = r.id
             JOIN symbols h ON e.to_symbol_id = h.id
             WHERE r.kind = 'route' AND r.file_path = ?1 AND r.name = ?2
               AND h.name = ?3 AND e.kind = 'references'",
            params![file, route, handler],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        count >= 1,
        "expected route symbol {route} in {file} with a references edge to handler {handler}"
    );
}

#[test]
fn framework_routes_index_as_route_nodes_bound_to_handlers() {
    let root = temp_repo();
    let summary = repo_graph::indexer::index_repo(&root, None).expect("indexing succeeds");
    assert!(summary.files >= 5);

    let db_path = root.join(".repograph/graph.db");
    assert!(db_path.is_file(), "SQLite cache written");
    let conn = Connection::open(&db_path).unwrap();

    // One route symbol per framework, each bound to its handler.
    assert_route_binding(&conn, "hono_server.ts", "/users", "listUsers");
    assert_route_binding(&conn, "echo_server.go", "/items", "ListItems");
    assert_route_binding(&conn, "fastapi_app.py", "/books", "list_books");
    assert_route_binding(&conn, "UserController.java", "/accounts/{id}", "show");
    assert_route_binding(&conn, "axum_app.rs", "/orders", "list_orders");

    // Route symbols carry the declaration line, not a whole-file span.
    let (start, end): (i64, i64) = conn
        .query_row(
            "SELECT start_line, end_line FROM symbols WHERE kind = 'route' AND file_path = 'hono_server.ts' AND name = '/users'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((start, end), (3, 3));

    // Graph JSON view still lists the routes.
    let graph = repo_graph::graph::Graph::load_from_cache(
        Path::new(&root.join(".repograph/graph.json")),
    )
    .unwrap();
    assert_eq!(graph.node("fastapi_app.py").unwrap().routes, vec!["/books"]);
    assert_eq!(
        graph.node("UserController.java").unwrap().routes,
        vec!["/accounts/{id}"]
    );

    let _ = std::fs::remove_dir_all(root);
}
