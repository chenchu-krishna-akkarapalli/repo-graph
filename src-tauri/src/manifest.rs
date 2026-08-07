//! Manifest generation: turns the internal `Graph` into the compact
//! agent-facing manifest (JSON source of truth + deterministic markdown
//! rendering) defined in `docs/logic/token-optimization-logic.md`.

use crate::graph::{Adjacency, Graph, Warning};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_LINE_BUDGET: usize = 500;

/// Version of the **agent-facing payload format** (manifest + explore), semver
/// per CLAUDE.md §5: changes here are breaking for agents that parse it.
///
/// `1.1.0` — `repograph_explore` can serialize call-graph edges as compact
/// arrow strings (`compact_edges`), an additive response shape.
///
/// Deliberately separate from [`crate::graph::Graph::schema_version`], which
/// is an integer identifying the **on-disk cache layout**. That layout did not
/// change, and turning it into a string would fail to deserialize every
/// existing `.repograph/graph.json` (and contradict `src/types.ts`, which
/// types it as `number`).
pub const MANIFEST_SCHEMA_VERSION: &str = "1.1.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub schema_version: String,
    pub generated_at: String,
    pub root: String,
    pub files: Vec<ManifestFile>,
    pub external_dependencies: Vec<String>,
    pub warnings: Vec<Warning>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestFile {
    pub path: String,
    pub exports: Vec<String>,
    pub depends_on: Vec<String>,
    pub route: Option<String>,
}

/// Build the JSON manifest from the graph, optionally scoped to a path
/// prefix (`scope` accepts `src/api`, `src/api/`, or glob-ish `src/api/**`).
pub fn build_manifest(graph: &Graph, root: &str, scope: Option<&str>) -> Manifest {
    build_manifest_with(graph, &graph.adjacency(), root, scope)
}

/// Same as [`build_manifest`], but reuses a caller-owned [`Adjacency`].
///
/// This used to call `graph.dependencies_of` per node — a full linear scan of
/// `edges` each time, so generating the manifest was O(N·E). The MCP server
/// caches one index per loaded graph and passes it in here.
pub fn build_manifest_with(
    graph: &Graph,
    adjacency: &Adjacency,
    root: &str,
    scope: Option<&str>,
) -> Manifest {
    let prefix = scope.map(normalize_scope);
    let mut files: Vec<ManifestFile> = graph
        .nodes
        .iter()
        .filter(|n| match &prefix {
            Some(p) => in_scope(&n.path, p),
            None => true,
        })
        .map(|n| ManifestFile {
            path: n.path.clone(),
            exports: n.exports.clone(),
            depends_on: adjacency.dependencies_of(&n.path),
            route: n.routes.first().cloned(),
        })
        .collect();

    // Sort by directory depth then alphabetically so the flat list reads
    // like a tree.
    files.sort_by(|a, b| {
        let da = a.path.matches('/').count();
        let db = b.path.matches('/').count();
        da.cmp(&db).then_with(|| a.path.cmp(&b.path))
    });

    let warnings = match &prefix {
        Some(p) => graph
            .warnings
            .iter()
            .filter(|w| in_scope(&w.path, p))
            .cloned()
            .collect(),
        None => graph.warnings.clone(),
    };

    Manifest {
        schema_version: MANIFEST_SCHEMA_VERSION.to_string(),
        generated_at: rfc3339_now(),
        root: root.to_string(),
        files,
        external_dependencies: graph.external_dependencies.clone(),
        warnings,
    }
}

fn normalize_scope(scope: &str) -> String {
    let s = scope.trim().replace('\\', "/");
    let s = s.trim_start_matches("./");
    let s = s
        .trim_end_matches("**")
        .trim_end_matches('*')
        .trim_end_matches('/');
    s.to_string()
}

fn in_scope(path: &str, prefix: &str) -> bool {
    if prefix.is_empty() {
        return true;
    }
    path == prefix || path.starts_with(&format!("{prefix}/"))
}

/// Deterministic markdown rendering — never hand-edited, never a separate
/// source of truth. One line per file; empty fields omitted entirely.
pub fn render_markdown(manifest: &Manifest, line_budget: usize) -> String {
    let mut out = format!("## Project Architecture Map (Active Root: {})\n", manifest.root.replace('\\', "/"));

    let file_lines: Vec<String> = manifest.files.iter().map(render_file_line).collect();

    if file_lines.len() > line_budget {
        out.push_str(&render_collapsed(manifest));
    } else {
        for line in &file_lines {
            out.push_str(line);
            out.push('\n');
        }
    }

    if !manifest.warnings.is_empty() {
        out.push_str("\n## Warnings (known gaps in this map)\n");
        for w in &manifest.warnings {
            out.push_str(&format!("- /{}: {}\n", w.path, w.kind));
        }
    }

    // The version has to appear in the rendered output: `Manifest` itself is
    // never serialized to an agent, so a bump nobody can read is not a bump.
    out.push_str(&format!(
        "\n## Agent Instructions (payload format v{MANIFEST_SCHEMA_VERSION})\n\
         You are looking at a compressed map. Do NOT guess file contents.\n\
         Use `repograph_explore(symbols)` for symbol slices with call graphs, or \
         `repograph_node(path)` to read a whole file, before editing it.\n\
         For architecture questions use \
         `repograph_explore(symbols, signature_only: true)` — declarations plus \
         the call graph, without the bodies.\n"
    ));
    out
}

fn render_file_line(f: &ManifestFile) -> String {
    let mut line = format!("- /{}", f.path);
    if !f.exports.is_empty() {
        line.push_str(&format!(" (Exports: {})", f.exports.join(", ")));
    }
    if let Some(route) = &f.route {
        line.push_str(&format!(" (Route: {route})"));
    }
    if !f.depends_on.is_empty() {
        let deps: Vec<String> = f.depends_on.iter().map(|d| format!("/{d}")).collect();
        line.push_str(&format!(" (Depends on: {})", deps.join(", ")));
    }
    line
}

/// Over-budget fallback: files with no exports and no route collapse into a
/// single summary line per directory; interesting files keep their own line.
fn render_collapsed(manifest: &Manifest) -> String {
    let mut out = String::new();
    let mut collapsed: BTreeMap<String, usize> = BTreeMap::new();
    for f in &manifest.files {
        if f.exports.is_empty() && f.route.is_none() {
            let dir = match f.path.rsplit_once('/') {
                Some((dir, _)) => dir.to_string(),
                None => String::from("."),
            };
            *collapsed.entry(dir).or_insert(0) += 1;
        } else {
            out.push_str(&render_file_line(f));
            out.push('\n');
        }
    }
    for (dir, count) in collapsed {
        out.push_str(&format!(
            "- /{dir}/ ({count} files with no exports or routes)\n"
        ));
    }
    out
}

/// RFC 3339 UTC timestamp without pulling in a datetime crate.
pub fn rfc3339_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    rfc3339_from_unix(secs as i64)
}

fn rfc3339_from_unix(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

/// Howard Hinnant's `civil_from_days` algorithm (days since 1970-01-01).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Graph;

    fn fixture() -> Graph {
        serde_json::from_str(
            r#"{
              "schema_version": 1,
              "nodes": [
                {"path": "src/pages/api/login.ts", "routes": ["/api/login"]},
                {"path": "src/components/Button.tsx", "exports": ["Button"]},
                {"path": "src/hooks/useTheme.ts", "exports": ["useTheme"]},
                {"path": "src/lib/db.ts", "exports": ["db"]}
              ],
              "edges": [
                {"from_path": "src/components/Button.tsx", "to_path": "src/hooks/useTheme.ts", "kind": "import"},
                {"from_path": "src/pages/api/login.ts", "to_path": "src/lib/db.ts", "kind": "import"}
              ],
              "external_dependencies": ["react", "lodash"],
              "warnings": [{"path": "src/x.ts", "kind": "unresolved_dynamic"}]
            }"#,
        )
        .unwrap()
    }

    #[test]
    fn markdown_golden_render() {
        let manifest = build_manifest(&fixture(), "/repo", None);
        let md = render_markdown(&manifest, DEFAULT_LINE_BUDGET);
        let expected = "\
## Project Architecture Map (Active Root: /repo)
- /src/components/Button.tsx (Exports: Button) (Depends on: /src/hooks/useTheme.ts)
- /src/hooks/useTheme.ts (Exports: useTheme)
- /src/lib/db.ts (Exports: db)
- /src/pages/api/login.ts (Route: /api/login) (Depends on: /src/lib/db.ts)

## Warnings (known gaps in this map)
- /src/x.ts: unresolved_dynamic

## Agent Instructions (payload format v1.1.0)
You are looking at a compressed map. Do NOT guess file contents.
Use `repograph_explore(symbols)` for symbol slices with call graphs, or `repograph_node(path)` to read a whole file, before editing it.
For architecture questions use `repograph_explore(symbols, signature_only: true)` — declarations plus the call graph, without the bodies.
";
        assert_eq!(md, expected);
        // The rendered footer is the only place an agent can see the version.
        assert!(md.contains(MANIFEST_SCHEMA_VERSION));
    }

    #[test]
    fn scoped_manifest_filters_by_prefix() {
        let manifest = build_manifest(&fixture(), "/repo", Some("src/pages/**"));
        let paths: Vec<&str> = manifest.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["src/pages/api/login.ts"]);
        // Out-of-scope warnings are filtered too.
        assert!(manifest.warnings.is_empty());
    }

    #[test]
    fn scope_prefix_does_not_match_sibling_dirs() {
        let manifest = build_manifest(&fixture(), "/repo", Some("src/lib"));
        let paths: Vec<&str> = manifest.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["src/lib/db.ts"]);
    }

    #[test]
    fn over_budget_collapses_boring_files_per_directory() {
        let mut graph = fixture();
        graph.nodes.push(crate::graph::Node {
            path: "config/a.json".into(),
            size_bytes: 0,
            language: String::new(),
            exports: vec![],
            routes: vec![],
            in_degree: 0,
            out_degree: 0,
            symbols: vec![],
        });
        graph.nodes.push(crate::graph::Node {
            path: "config/b.json".into(),
            size_bytes: 0,
            language: String::new(),
            exports: vec![],
            routes: vec![],
            in_degree: 0,
            out_degree: 0,
            symbols: vec![],
        });
        let manifest = build_manifest(&graph, "/repo", None);
        let md = render_markdown(&manifest, 3);
        assert!(md.contains("- /config/ (2 files with no exports or routes)"));
        assert!(md.contains("- /src/components/Button.tsx (Exports: Button)"));
        assert!(!md.contains("- /config/a.json"));
    }

    #[test]
    fn rfc3339_epoch_and_known_date() {
        assert_eq!(rfc3339_from_unix(0), "1970-01-01T00:00:00Z");
        // 2026-07-18T00:00:00Z
        assert_eq!(rfc3339_from_unix(1_784_332_800), "2026-07-18T00:00:00Z");
    }
}
