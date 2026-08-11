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
/// `1.2.0` — files carry graph-centrality ranking (`rank`, `rank_order`,
/// `in_degree`), the manifest is ordered by rank instead of directory depth,
/// each rendered line is prefixed `[#N]`, and `repograph_files` accepts
/// `top_k` / `min_rank` / `sort_by`. Minor rather than major: the per-line
/// format is unchanged apart from an added prefix and an added parenthetical,
/// and every new field and parameter is optional. An agent that reads the map
/// top-down gets a better map; one that parses lines keeps its parse.
///
/// `1.1.0` — `repograph_explore` can serialize call-graph edges as compact
/// arrow strings (`compact_edges`), an additive response shape.
///
/// Deliberately separate from [`crate::graph::Graph::schema_version`], which
/// is an integer identifying the **on-disk cache layout**. That layout did not
/// change, and turning it into a string would fail to deserialize every
/// existing `.repograph/graph.json` (and contradict `src/types.ts`, which
/// types it as `number`).
pub const MANIFEST_SCHEMA_VERSION: &str = "1.2.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub schema_version: String,
    pub generated_at: String,
    pub root: String,
    pub files: Vec<ManifestFile>,
    pub external_dependencies: Vec<String>,
    pub warnings: Vec<Warning>,
    /// How many files were in scope before `top_k` / `min_rank` trimmed the
    /// list. An agent must be able to tell a filtered map from a whole one —
    /// silently handing back the top 20 files of 900 and letting the agent
    /// conclude the repo has 20 files is the one way this feature can do harm.
    #[serde(default)]
    pub total_files: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestFile {
    pub path: String,
    pub exports: Vec<String>,
    pub depends_on: Vec<String>,
    pub route: Option<String>,
    /// Normalized graph centrality in `(0, 1]` — see [`crate::rank`].
    #[serde(default)]
    pub rank: f64,
    /// Dense 1-based position across the **whole graph**, not this manifest:
    /// under a scope or `top_k` the numbers stay comparable with an unfiltered
    /// map, so `[#3]` means the same file either way.
    #[serde(default)]
    pub rank_order: u32,
    /// Count of in-repo files that depend on this one (route markers excluded).
    #[serde(default)]
    pub in_degree: u32,
}

/// Presentation order for the file list. Filtering by `top_k` / `min_rank`
/// always uses rank regardless of this — "the 20 most important files, listed
/// alphabetically" is a coherent request, "the alphabetically first 20" is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortBy {
    #[default]
    Rank,
    Alphabetical,
    Depth,
}

impl SortBy {
    /// Unknown values fall back to `Rank` rather than erroring: a typo in an
    /// optional presentation hint should not fail an agent's whole query.
    pub fn parse(s: &str) -> SortBy {
        match s.trim().to_ascii_lowercase().as_str() {
            "alphabetical" | "alpha" | "path" => SortBy::Alphabetical,
            "depth" | "tree" => SortBy::Depth,
            _ => SortBy::Rank,
        }
    }
}

/// Scope and ranking filters for [`build_manifest_with`].
#[derive(Debug, Clone, Default)]
pub struct ManifestOptions<'a> {
    /// Path prefix (`src/api`, `src/api/`, or glob-ish `src/api/**`).
    pub scope: Option<&'a str>,
    /// Keep only the `k` highest-ranked in-scope files.
    pub top_k: Option<usize>,
    /// Keep only files with `rank >= min_rank`.
    pub min_rank: Option<f64>,
    pub sort_by: SortBy,
}

impl<'a> ManifestOptions<'a> {
    pub fn scoped(scope: Option<&'a str>) -> Self {
        ManifestOptions {
            scope,
            ..Default::default()
        }
    }
}

/// Build the JSON manifest from the graph, optionally scoped to a path prefix.
pub fn build_manifest(graph: &Graph, root: &str, scope: Option<&str>) -> Manifest {
    build_manifest_with(
        graph,
        &graph.adjacency(),
        root,
        &ManifestOptions::scoped(scope),
    )
}

/// Same as [`build_manifest`], but reuses a caller-owned [`Adjacency`] and
/// takes the full option set.
///
/// This used to call `graph.dependencies_of` per node — a full linear scan of
/// `edges` each time, so generating the manifest was O(N·E). The MCP server
/// caches one index per loaded graph and passes it in here.
pub fn build_manifest_with(
    graph: &Graph,
    adjacency: &Adjacency,
    root: &str,
    options: &ManifestOptions<'_>,
) -> Manifest {
    let prefix = options.scope.map(normalize_scope);
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
            rank: n.rank_score,
            rank_order: n.rank_order,
            // Derived from the same index as `depends_on`, so the two always
            // agree; `Node::in_degree` counts route markers too and can be
            // stale in a cache written by an older indexer.
            in_degree: adjacency.dependents_of(&n.path).len() as u32,
        })
        .collect();

    let total_files = files.len();

    if let Some(min) = options.min_rank {
        files.retain(|f| f.rank >= min);
    }

    // Rank order first — `top_k` must mean "top by rank" whatever `sort_by`
    // asks for afterwards.
    files.sort_by(|a, b| a.rank_order.cmp(&b.rank_order));
    if let Some(k) = options.top_k {
        files.truncate(k);
    }

    match options.sort_by {
        SortBy::Rank => {}
        SortBy::Alphabetical => files.sort_by(|a, b| a.path.cmp(&b.path)),
        // Directory depth then alphabetically, so the flat list reads like a
        // tree. This was the only ordering before 1.2.0.
        SortBy::Depth => files.sort_by(|a, b| {
            let da = a.path.matches('/').count();
            let db = b.path.matches('/').count();
            da.cmp(&db).then_with(|| a.path.cmp(&b.path))
        }),
    }

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
        total_files,
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

    // Never let a filtered map read as a complete one.
    if manifest.files.len() < manifest.total_files {
        out.push_str(&format!(
            "Showing {} of {} files, highest-ranked first. Widen with `repograph_files(top_k: ...)` or drop the filter for the full map.\n",
            manifest.files.len(),
            manifest.total_files
        ));
    }

    let file_lines: Vec<String> = manifest
        .files
        .iter()
        .enumerate()
        .map(|(i, f)| render_file_line(f, i + 1))
        .collect();

    if file_lines.len() > line_budget {
        out.push_str(&render_collapsed(manifest, line_budget));
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
         Files are listed most-central first (dependency-graph rank). `(In: k)`: k \
         files depend on this one. `[#N]`: repo-wide rank, shown only where it \
         differs from list position. Narrow with `repograph_files(top_k: 20)`.\n\
         Use `repograph_explore(symbols)` for symbol slices with call graphs, or \
         `repograph_node(path)` to read a whole file, before editing it.\n\
         For architecture questions use \
         `repograph_explore(symbols, signature_only: true)` — declarations plus \
         the call graph, without the bodies.\n"
    ));
    out
}

/// Below this, the dependent count is not worth its ~6 tokens: `In: 1` says
/// "used once", which is what a file with no marker already implies. At 2+ the
/// file is shared, and that changes how an agent edits it.
const MIN_REPORTED_IN_DEGREE: u32 = 2;

/// `position` is the file's 1-based index in the list being rendered.
///
/// The rank marker is emitted only when it disagrees with that position —
/// which is precisely when it carries information the ordering does not
/// (scoped maps, `sort_by: alphabetical`, collapsed lists). It measured at 4
/// tokens per file, a quarter of a typical manifest line, so paying it to
/// restate the line's own index would be spending the budget on nothing.
fn render_file_line(f: &ManifestFile, position: usize) -> String {
    let mut line = match f.rank_order {
        0 => format!("- /{}", f.path),
        n if n as usize == position => format!("- /{}", f.path),
        n => format!("- [#{n}] /{}", f.path),
    };
    if !f.exports.is_empty() {
        line.push_str(&format!(" (Exports: {})", f.exports.join(", ")));
    }
    if let Some(route) = &f.route {
        line.push_str(&format!(" (Route: {route})"));
    }
    if f.in_degree >= MIN_REPORTED_IN_DEGREE {
        line.push_str(&format!(" (In: {})", f.in_degree));
    }
    if !f.depends_on.is_empty() {
        let deps: Vec<String> = f.depends_on.iter().map(|d| format!("/{d}")).collect();
        line.push_str(&format!(" (Depends on: {})", deps.join(", ")));
    }
    line
}

/// Over-budget fallback: the highest-ranked `line_budget` files keep their own
/// line, everything below collapses to one summary line per directory.
///
/// Before ranking existed this used "no exports and no route" as the proxy for
/// "uninteresting", which dropped a heavily-imported barrel while keeping a
/// leaf that exports one unused constant. Rank measures the thing that proxy
/// was reaching for, so it replaces it.
fn render_collapsed(manifest: &Manifest, line_budget: usize) -> String {
    let mut out = String::new();
    let mut collapsed: BTreeMap<String, usize> = BTreeMap::new();

    // `manifest.files` follows `sort_by`, which may not be rank order, so
    // decide membership from rank explicitly.
    let mut by_rank: Vec<&ManifestFile> = manifest.files.iter().collect();
    by_rank.sort_by(|a, b| a.rank_order.cmp(&b.rank_order));
    let kept: std::collections::HashSet<&str> = by_rank
        .iter()
        .take(line_budget)
        .map(|f| f.path.as_str())
        .collect();

    let mut position = 0;
    for f in &manifest.files {
        if kept.contains(f.path.as_str()) {
            position += 1;
            out.push_str(&render_file_line(f, position));
            out.push('\n');
        } else {
            let dir = match f.path.rsplit_once('/') {
                Some((dir, _)) => dir.to_string(),
                None => String::from("."),
            };
            *collapsed.entry(dir).or_insert(0) += 1;
        }
    }
    for (dir, count) in collapsed {
        out.push_str(&format!("- /{dir}/ ({count} lower-ranked files)\n"));
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
        // `load_from_cache` ranks on load; a hand-built fixture has to do the
        // same or every file reports rank 0.
        let mut g: Graph = serde_json::from_str(
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
        .unwrap();
        crate::rank::compute_ranks(&mut g);
        g
    }

    #[test]
    fn markdown_golden_render() {
        let manifest = build_manifest(&fixture(), "/repo", None);
        let md = render_markdown(&manifest, DEFAULT_LINE_BUDGET);
        // `db.ts` leads: it is what the route handler — the highest-prior node
        // — depends on. Depth ordering used to put `Button.tsx` first purely
        // because it is shallower.
        // No `[#N]` markers: rank order equals list position here, so they
        // would restate the index. They appear under scope/`sort_by`, which
        // `scoped_rank_numbers_stay_graph_wide` covers.
        let expected = "\
## Project Architecture Map (Active Root: /repo)
- /src/lib/db.ts (Exports: db)
- /src/pages/api/login.ts (Route: /api/login) (Depends on: /src/lib/db.ts)
- /src/hooks/useTheme.ts (Exports: useTheme)
- /src/components/Button.tsx (Exports: Button) (Depends on: /src/hooks/useTheme.ts)

## Warnings (known gaps in this map)
- /src/x.ts: unresolved_dynamic

## Agent Instructions (payload format v1.2.0)
You are looking at a compressed map. Do NOT guess file contents.
Files are listed most-central first (dependency-graph rank). `(In: k)`: k files depend on this one. `[#N]`: repo-wide rank, shown only where it differs from list position. Narrow with `repograph_files(top_k: 20)`.
Use `repograph_explore(symbols)` for symbol slices with call graphs, or `repograph_node(path)` to read a whole file, before editing it.
For architecture questions use `repograph_explore(symbols, signature_only: true)` — declarations plus the call graph, without the bodies.
";
        assert_eq!(md, expected);
        // The rendered footer is the only place an agent can see the version.
        assert!(md.contains(MANIFEST_SCHEMA_VERSION));
    }

    #[test]
    fn ranking_metadata_reaches_the_json_manifest() {
        let manifest = build_manifest(&fixture(), "/repo", None);
        let db = manifest
            .files
            .iter()
            .find(|f| f.path == "src/lib/db.ts")
            .unwrap();
        assert_eq!(db.rank_order, 1);
        assert_eq!(db.rank, 1.0, "the top file normalizes to exactly 1.0");
        assert_eq!(db.in_degree, 1);
        assert_eq!(manifest.total_files, 4);
        // Descending rank, dense ordering.
        let orders: Vec<u32> = manifest.files.iter().map(|f| f.rank_order).collect();
        assert_eq!(orders, vec![1, 2, 3, 4]);
    }

    #[test]
    fn top_k_keeps_the_highest_ranked_files_and_says_it_truncated() {
        let g = fixture();
        let manifest = build_manifest_with(
            &g,
            &g.adjacency(),
            "/repo",
            &ManifestOptions {
                top_k: Some(2),
                ..Default::default()
            },
        );
        let paths: Vec<&str> = manifest.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["src/lib/db.ts", "src/pages/api/login.ts"]);
        assert_eq!(manifest.total_files, 4);
        let md = render_markdown(&manifest, DEFAULT_LINE_BUDGET);
        assert!(
            md.contains("Showing 2 of 4 files"),
            "a truncated map must not read as a complete one:\n{md}"
        );
    }

    #[test]
    fn min_rank_filters_below_the_threshold() {
        let g = fixture();
        let manifest = build_manifest_with(
            &g,
            &g.adjacency(),
            "/repo",
            &ManifestOptions {
                min_rank: Some(0.5),
                ..Default::default()
            },
        );
        assert!(manifest.files.iter().all(|f| f.rank >= 0.5));
        assert!(manifest.files.len() < manifest.total_files);
        assert!(manifest.files.iter().any(|f| f.path == "src/lib/db.ts"));
    }

    /// `sort_by` is presentation only — it must not change *which* files
    /// `top_k` selected.
    #[test]
    fn sort_by_reorders_without_changing_selection() {
        let g = fixture();
        let opts = |sort_by| ManifestOptions {
            top_k: Some(3),
            sort_by,
            ..Default::default()
        };
        let by_rank = build_manifest_with(&g, &g.adjacency(), "/repo", &opts(SortBy::Rank));
        let alpha = build_manifest_with(&g, &g.adjacency(), "/repo", &opts(SortBy::Alphabetical));
        let depth = build_manifest_with(&g, &g.adjacency(), "/repo", &opts(SortBy::Depth));

        let set = |m: &Manifest| {
            let mut v: Vec<String> = m.files.iter().map(|f| f.path.clone()).collect();
            v.sort();
            v
        };
        assert_eq!(set(&by_rank), set(&alpha));
        assert_eq!(set(&by_rank), set(&depth));

        let paths = |m: &Manifest| -> Vec<String> {
            m.files.iter().map(|f| f.path.clone()).collect()
        };
        assert_eq!(
            paths(&alpha),
            vec![
                "src/hooks/useTheme.ts".to_string(),
                "src/lib/db.ts".to_string(),
                "src/pages/api/login.ts".to_string()
            ]
        );
        assert_ne!(paths(&by_rank), paths(&alpha));
        // Depth ordering is the pre-1.2.0 behaviour, still reachable.
        assert_eq!(paths(&depth)[0], "src/hooks/useTheme.ts");
    }

    #[test]
    fn sort_by_parsing_falls_back_to_rank() {
        assert_eq!(SortBy::parse("alphabetical"), SortBy::Alphabetical);
        assert_eq!(SortBy::parse("Depth"), SortBy::Depth);
        assert_eq!(SortBy::parse("nonsense"), SortBy::Rank);
        assert_eq!(SortBy::default(), SortBy::Rank);
    }

    /// Rank is graph-wide, so a scoped map's numbers still line up with the
    /// full map — `[#2]` is the same file in both.
    #[test]
    fn scoped_rank_numbers_stay_graph_wide() {
        let g = fixture();
        let scoped = build_manifest(&g, "/repo", Some("src/pages/**"));
        assert_eq!(scoped.files.len(), 1);
        assert_eq!(scoped.files[0].rank_order, 2);
        // The marker earns its tokens here: rank 2 sits at list position 1.
        let md = render_markdown(&scoped, DEFAULT_LINE_BUDGET);
        assert!(md.contains("- [#2] /src/pages/api/login.ts"), "{md}");
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

    /// Over budget, the lowest-ranked files collapse per directory and the
    /// highest-ranked keep their own line.
    #[test]
    fn over_budget_collapses_the_lowest_ranked_files_per_directory() {
        let mut graph = fixture();
        for name in ["config/a.json", "config/b.json"] {
            graph.nodes.push(crate::graph::Node {
                path: name.into(),
                ..Default::default()
            });
        }
        crate::rank::compute_ranks(&mut graph);
        let manifest = build_manifest(&graph, "/repo", None);
        let md = render_markdown(&manifest, 3);
        assert!(
            md.contains("- /config/ (2 lower-ranked files)"),
            "expected a collapsed config directory:\n{md}"
        );
        // The top three survive; `db.ts` is #1 and must never be collapsed.
        assert!(md.contains("- /src/lib/db.ts"), "{md}");
        assert!(!md.contains("/config/a.json"));
    }

    /// The pre-1.2.0 rule collapsed by "no exports and no route", which drops
    /// a heavily-imported file that happens to export nothing. Rank keeps it.
    #[test]
    fn a_hub_with_no_exports_survives_collapsing() {
        let mut graph: Graph = serde_json::from_str(
            r#"{"schema_version":1,
                "nodes":[
                  {"path":"src/hub.ts"},
                  {"path":"src/a.ts"},{"path":"src/b.ts"},{"path":"src/c.ts"},
                  {"path":"src/leaf.ts","exports":["unused"]}
                ],
                "edges":[
                  {"from_path":"src/a.ts","to_path":"src/hub.ts","kind":"imports"},
                  {"from_path":"src/b.ts","to_path":"src/hub.ts","kind":"imports"},
                  {"from_path":"src/c.ts","to_path":"src/hub.ts","kind":"imports"}
                ]}"#,
        )
        .unwrap();
        crate::rank::compute_ranks(&mut graph);
        let md = render_markdown(&build_manifest(&graph, "/repo", None), 1);
        assert!(
            md.contains("/src/hub.ts"),
            "the most-depended-on file was collapsed:\n{md}"
        );
        assert!(!md.contains("/src/leaf.ts"));
    }

    #[test]
    fn rfc3339_epoch_and_known_date() {
        assert_eq!(rfc3339_from_unix(0), "1970-01-01T00:00:00Z");
        // 2026-07-18T00:00:00Z
        assert_eq!(rfc3339_from_unix(1_784_332_800), "2026-07-18T00:00:00Z");
    }
}
