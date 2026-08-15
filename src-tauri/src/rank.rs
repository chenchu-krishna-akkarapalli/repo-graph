//! Relation-edge file ranking: how central is each file in the dependency
//! graph, on a normalized 0–1 scale.
//!
//! ## Why this exists
//!
//! The manifest's compression premise is "one line per file". That is cheap
//! per file but still linear in repo size, and every line competes for the
//! same context window. Ordering by directory depth — the previous scheme —
//! puts `src/App.tsx` above `src/lib/db.ts` purely because it is shallower,
//! which is not a statement about importance. Ranking lets the manifest lead
//! with the files an agent actually needs and lets `top_k` cut the tail.
//!
//! ## The algorithm: weighted, personalized PageRank
//!
//! Edges point **importer → imported**, so rank flows *toward* the thing being
//! depended on. That is already the right direction: a file imported by many
//! important files is important. No reversal is needed.
//!
//! Three deliberate departures from textbook PageRank:
//!
//! 1. **Edges are weighted by kind.** A Rust `mod` declaration is a structural
//!    statement about the module tree; a bare `use`-path reference is the
//!    noisiest edge the extractors emit. Weighting them identically would let
//!    reference noise outvote structure. `Route` edges carry zero weight — they
//!    are entry-point markers, not dependencies, and the rest of the codebase
//!    already excludes them from `dependencies_of`.
//!
//! 2. **The teleport vector is personalized, not uniform.** This is where
//!    "entry points matter more" belongs. Textbook PageRank gives every node
//!    `1/N` prior importance; an HTTP route handler is not equal prior to a
//!    random helper. Expressing the prior here rather than adding a bonus to
//!    the final score keeps the result a genuine probability distribution:
//!    the boost propagates to the handler's dependencies instead of decorating
//!    one node.
//!
//! 3. **Barrel files get a reduced prior.** A barrel (`index.ts`, `mod.rs`,
//!    `__init__.py` that only re-exports) accumulates every consumer's link
//!    and would otherwise outrank the modules it forwards to, while containing
//!    no logic to read. Lowering its *prior* — rather than damping its final
//!    score — leaves the flow through it intact, so its members still receive
//!    the rank routed via the barrel. A barrel is a routing table, not a
//!    destination.
//!
//! Scores are normalized so the top-ranked file is exactly `1.0`. That makes
//! `min_rank` a threshold with the same meaning in a 40-file repo and a
//! 40,000-file one, which a raw PageRank probability (which shrinks as `1/N`)
//! would not be.

use crate::graph::{EdgeKind, Graph, Node};
use std::collections::HashMap;

/// Standard PageRank damping. 0.85 means a walker follows a dependency edge
/// 85% of the time and restarts from the prior 15% of the time.
pub const DAMPING: f64 = 0.85;

/// Power iteration stops at whichever comes first. Convergence is on the L1
/// distance between successive score vectors.
pub const MAX_ITERATIONS: usize = 128;
pub const CONVERGENCE_L1: f64 = 1e-10;

/// Reported scores are rounded here before ordering, so the printed value and
/// the sort order can never disagree.
const SCORE_DECIMALS: u32 = 6;

const TELEPORT_BASE: f64 = 1.0;
/// Multiplier on a route handler or application entry point's prior importance.
const ENTRY_POINT_BOOST: f64 = 3.0;
/// Multiplier on a barrel file's prior importance (see module docs).
const BARREL_TELEPORT: f64 = 0.25;
/// Multiplier on a test file's prior importance to prevent tests from cluttering top ranks.
const TEST_FILE_TELEPORT: f64 = 0.25;

/// Relative trust in each edge kind as evidence of the target's importance.
pub fn edge_weight(kind: EdgeKind) -> f64 {
    match kind {
        // `contains` — a module-tree declaration, the strongest structural claim.
        EdgeKind::Mod => 1.5,
        EdgeKind::Import | EdgeKind::Require => 1.0,
        // `references` — item-path resolution, the noisiest edge we emit.
        EdgeKind::Use => 0.5,
        // Entry-point marker, not a dependency.
        EdgeKind::Route => 0.0,
    }
}

/// Detect test files so their prior doesn't pollute the top of the manifest.
pub fn is_test_file(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    let name = normalized.rsplit('/').next().unwrap_or(&normalized);
    name.contains(".test.")
        || name.contains(".spec.")
        || name.ends_with("_test.go")
        || name.ends_with("_test.py")
        || name.ends_with("_test.rs")
        || normalized.starts_with("tests/")
        || normalized.contains("/tests/")
        || normalized.starts_with("__tests__/")
        || normalized.contains("/__tests__/")
}

/// Detect app / binary root entry points that initiate application execution.
pub fn is_root_entry_point(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    matches!(
        normalized.as_str(),
        "src/App.tsx"
            | "src/App.jsx"
            | "src/main.tsx"
            | "src/main.jsx"
            | "src/main.ts"
            | "src/main.js"
            | "src/index.tsx"
            | "src/index.jsx"
            | "src/main.rs"
            | "src-tauri/src/main.rs"
            | "src-tauri/src/bin/mcp_server.rs"
            | "app/page.tsx"
            | "app/layout.tsx"
            | "pages/index.tsx"
            | "pages/_app.tsx"
    ) || normalized.starts_with("src/bin/")
        || normalized.starts_with("src-tauri/src/bin/")
}

/// A file whose whole job is re-exporting other files.
///
/// Deliberately conservative: the name must match the language's barrel
/// convention, the file must declare no symbols of its own, and it must have
/// at least two outgoing dependency edges. A real entry point named
/// `index.ts` that contains code has symbols and is not matched.
pub fn is_barrel(node: &Node, out_degree: u32) -> bool {
    if out_degree < 2 || !node.symbols.is_empty() {
        return false;
    }
    let name = node.path.rsplit('/').next().unwrap_or(node.path.as_str());
    matches!(name, "mod.rs" | "__init__.py" | "index.d.ts")
        || matches!(
            name,
            "index.ts" | "index.tsx" | "index.js" | "index.jsx" | "index.mjs" | "index.cjs"
        )
}

/// Prior importance of a node, before any link evidence.
fn teleport_weight(node: &Node, out_degree: u32) -> f64 {
    let mut w = TELEPORT_BASE;
    if !node.routes.is_empty() || is_root_entry_point(&node.path) {
        w *= ENTRY_POINT_BOOST;
    }
    if is_barrel(node, out_degree) {
        w *= BARREL_TELEPORT;
    }
    if is_test_file(&node.path) {
        w *= TEST_FILE_TELEPORT;
    }
    w
}

/// Follow barrel indirection to the files that actually hold the code.
///
/// `seen` guards against a barrel cycle (`a/index.ts` ↔ `b/index.ts`), and the
/// depth cap stops a pathological chain from fanning out combinatorially —
/// past four hops the indirection is no longer something an agent is reasoning
/// about anyway.
fn resolve_through_barrels(
    node: usize,
    raw_links: &[Vec<(usize, f64)>],
    barrel: &[bool],
    seen: &mut Vec<usize>,
    out: &mut Vec<usize>,
    depth: usize,
) {
    const MAX_DEPTH: usize = 4;
    if seen.contains(&node) {
        return;
    }
    if !barrel[node] || depth >= MAX_DEPTH || raw_links[node].is_empty() {
        if !out.contains(&node) {
            out.push(node);
        }
        return;
    }
    seen.push(node);
    for &(next, _) in &raw_links[node] {
        resolve_through_barrels(next, raw_links, barrel, seen, out, depth + 1);
    }
}

fn round_score(v: f64) -> f64 {
    let f = 10f64.powi(SCORE_DECIMALS as i32);
    (v * f).round() / f
}

/// Compute `rank_score` and `rank_order` for every node in `graph`, in place.
///
/// Idempotent, and cheap enough to run on every graph load: one pass to build
/// the weighted adjacency, then at most [`MAX_ITERATIONS`] passes over the
/// edge set. Running it on load rather than trusting the serialized values is
/// what keeps a `.repograph/graph.json` written before ranking existed from
/// silently reporting every file at rank 0.
pub fn compute_ranks(graph: &mut Graph) {
    let n = graph.nodes.len();
    if n == 0 {
        return;
    }

    let index: HashMap<&str, usize> = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(i, node)| (node.path.as_str(), i))
        .collect();

    // Raw weighted out-adjacency. Duplicate (from, to) pairs of different
    // kinds are kept separately and their weights add — two distinct kinds of
    // dependency really is more evidence than one.
    let mut raw_links: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
    let mut out_degree: Vec<u32> = vec![0; n];
    for e in &graph.edges {
        let w = edge_weight(e.kind);
        if w == 0.0 {
            continue;
        }
        let (Some(&from), Some(&to)) = (
            index.get(e.from_path.as_str()),
            index.get(e.to_path.as_str()),
        ) else {
            continue;
        };
        if from == to {
            continue; // a self-import is not evidence of anything
        }
        raw_links[from].push((to, w));
        out_degree[from] += 1;
    }

    let barrel: Vec<bool> = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(i, node)| is_barrel(node, out_degree[i]))
        .collect();

    // Contract barrels out of the flow graph: an edge into a barrel is
    // redirected to what the barrel re-exports, splitting its weight across
    // them.
    //
    // A lower prior alone does not fix barrels, and testing showed why: rank
    // is the stationary probability of *being* at a node, so a pure conduit
    // still accumulates everything routed through it and no prior small enough
    // to matter can push it below its own members. Contraction says the true
    // thing instead — `import { Button } from './ui'` is a dependency on
    // `Button.tsx`, and the barrel is an indirection artifact. Splitting the
    // weight keeps total flow identical to what would have passed through.
    let mut out_links: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
    let mut out_weight: Vec<f64> = vec![0.0; n];
    for from in 0..n {
        for &(to, w) in &raw_links[from] {
            let mut targets = Vec::new();
            let mut seen = vec![from];
            resolve_through_barrels(to, &raw_links, &barrel, &mut seen, &mut targets, 0);
            if targets.is_empty() {
                // A barrel that re-exports nothing reachable is just a file.
                targets.push(to);
            }
            let share = w / targets.len() as f64;
            for t in targets {
                out_links[from].push((t, share));
                out_weight[from] += share;
            }
        }
    }

    // Personalized teleport vector, normalized to a distribution.
    let mut teleport: Vec<f64> = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(i, node)| teleport_weight(node, out_degree[i]))
        .collect();
    let teleport_total: f64 = teleport.iter().sum();
    if teleport_total <= 0.0 {
        // Cannot happen with the weights above, but a zero vector would make
        // every score NaN — fall back to uniform rather than emit garbage.
        teleport = vec![1.0 / n as f64; n];
    } else {
        for t in &mut teleport {
            *t /= teleport_total;
        }
    }

    let mut scores = teleport.clone();
    let mut next = vec![0.0f64; n];
    for _ in 0..MAX_ITERATIONS {
        // Nodes with no outgoing dependency edges would leak probability
        // mass; redistribute it over the prior, as standard PageRank does.
        let mut dangling = 0.0;
        for i in 0..n {
            if out_links[i].is_empty() {
                dangling += scores[i];
            }
        }
        let restart = (1.0 - DAMPING) + DAMPING * dangling;
        for i in 0..n {
            next[i] = restart * teleport[i];
        }
        for from in 0..n {
            if out_links[from].is_empty() {
                continue;
            }
            let share = DAMPING * scores[from] / out_weight[from];
            for &(to, w) in &out_links[from] {
                next[to] += share * w;
            }
        }
        let delta: f64 = scores
            .iter()
            .zip(next.iter())
            .map(|(a, b)| (a - b).abs())
            .sum();
        scores.copy_from_slice(&next);
        if delta < CONVERGENCE_L1 {
            break;
        }
    }

    // Normalize so the most central file is exactly 1.0 — a scale that means
    // the same thing regardless of repo size.
    let max = scores.iter().cloned().fold(0.0f64, f64::max);
    let scale = if max > 0.0 { 1.0 / max } else { 0.0 };
    for (i, node) in graph.nodes.iter_mut().enumerate() {
        node.rank_score = round_score(scores[i] * scale);
    }

    // Dense 1-based ordering. Ties break on path so the manifest is
    // byte-reproducible across runs and machines.
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| {
        graph.nodes[b]
            .rank_score
            .partial_cmp(&graph.nodes[a].rank_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| graph.nodes[a].path.cmp(&graph.nodes[b].path))
    });
    for (position, &i) in order.iter().enumerate() {
        graph.nodes[i].rank_order = position as u32 + 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph_from(json: &str) -> Graph {
        let mut g: Graph = serde_json::from_str(json).expect("fixture parses");
        compute_ranks(&mut g);
        g
    }

    fn score(g: &Graph, path: &str) -> f64 {
        g.node(path).expect("node exists").rank_score
    }

    fn order(g: &Graph, path: &str) -> u32 {
        g.node(path).expect("node exists").rank_order
    }

    /// The headline property: a hub that many files import outranks the files
    /// importing it, and the top score is normalized to exactly 1.0.
    #[test]
    fn test_file_ranking() {
        let g = graph_from(
            r#"{"schema_version":1,
                "nodes":[
                  {"path":"src/lib/db.ts","exports":["query"]},
                  {"path":"src/a.ts"},{"path":"src/b.ts"},{"path":"src/c.ts"},
                  {"path":"src/unused.ts"}
                ],
                "edges":[
                  {"from_path":"src/a.ts","to_path":"src/lib/db.ts","kind":"imports"},
                  {"from_path":"src/b.ts","to_path":"src/lib/db.ts","kind":"imports"},
                  {"from_path":"src/c.ts","to_path":"src/lib/db.ts","kind":"imports"}
                ]}"#,
        );
        assert_eq!(order(&g, "src/lib/db.ts"), 1);
        assert_eq!(score(&g, "src/lib/db.ts"), 1.0);
        assert!(score(&g, "src/lib/db.ts") > score(&g, "src/a.ts"));
        // An unimported leaf is not zero — it still holds its prior — but it
        // must sit below anything with inbound evidence.
        assert!(score(&g, "src/unused.ts") > 0.0);
        assert!(score(&g, "src/unused.ts") < score(&g, "src/lib/db.ts"));
        // Dense, unique, 1-based ordering over every node.
        let mut orders: Vec<u32> = g.nodes.iter().map(|n| n.rank_order).collect();
        orders.sort_unstable();
        assert_eq!(orders, vec![1, 2, 3, 4, 5]);
    }

    /// Rank is transitive, which plain in-degree is not: `core` has one direct
    /// importer, `widget` has two, yet `core` is what an agent should read
    /// first because its single importer is the hub.
    #[test]
    fn rank_is_transitive_where_in_degree_is_not() {
        let g = graph_from(
            r#"{"schema_version":1,
                "nodes":[
                  {"path":"core.ts"},{"path":"hub.ts"},{"path":"widget.ts"},
                  {"path":"p1.ts"},{"path":"p2.ts"},{"path":"p3.ts"},{"path":"p4.ts"}
                ],
                "edges":[
                  {"from_path":"hub.ts","to_path":"core.ts","kind":"imports"},
                  {"from_path":"p1.ts","to_path":"hub.ts","kind":"imports"},
                  {"from_path":"p2.ts","to_path":"hub.ts","kind":"imports"},
                  {"from_path":"p3.ts","to_path":"hub.ts","kind":"imports"},
                  {"from_path":"p4.ts","to_path":"hub.ts","kind":"imports"},
                  {"from_path":"p1.ts","to_path":"widget.ts","kind":"imports"},
                  {"from_path":"p2.ts","to_path":"widget.ts","kind":"imports"}
                ]}"#,
        );
        assert!(
            score(&g, "core.ts") > score(&g, "widget.ts"),
            "core {} vs widget {}",
            score(&g, "core.ts"),
            score(&g, "widget.ts")
        );
    }

    /// Route handlers carry a higher prior, so an entry point outranks an
    /// otherwise identical plain file.
    #[test]
    fn route_handlers_carry_a_higher_prior() {
        let g = graph_from(
            r#"{"schema_version":1,
                "nodes":[
                  {"path":"api/login.ts","routes":["/api/login"]},
                  {"path":"lib/plain.ts"}
                ],
                "edges":[]}"#,
        );
        assert!(score(&g, "api/login.ts") > score(&g, "lib/plain.ts"));
        assert_eq!(order(&g, "api/login.ts"), 1);
    }

    /// A route handler's boost must reach what it depends on — that is the
    /// point of putting it in the teleport vector rather than adding it to the
    /// final score.
    ///
    /// Measured against an unrelated file, not against the handler itself: the
    /// ratio db:handler is `tp_db/tp_handler + DAMPING` and so *falls* when the
    /// handler's prior rises, no matter how large the repo. That is arithmetic
    /// about the pair, not evidence about propagation. What propagation means
    /// is that `db.ts` pulls further ahead of a comparable file nothing
    /// important depends on.
    #[test]
    fn the_entry_point_boost_propagates_to_its_dependencies() {
        let fixture = |routes: &str| {
            format!(
                r#"{{"schema_version":1,
                    "nodes":[{{"path":"api/login.ts"{routes}}},{{"path":"lib/db.ts"}},
                             {{"path":"lib/x1.ts"}},{{"path":"lib/x2.ts"}},{{"path":"lib/x3.ts"}}],
                    "edges":[{{"from_path":"api/login.ts","to_path":"lib/db.ts","kind":"imports"}}]}}"#
            )
        };
        let boosted = graph_from(&fixture(r#","routes":["/api/login"]"#));
        let plain = graph_from(&fixture(""));

        let lead = |g: &Graph| score(g, "lib/db.ts") / score(g, "lib/x1.ts");
        assert!(
            lead(&boosted) > lead(&plain) * 1.5,
            "db.ts leads an unrelated file by {:.2}x when its importer is a route \
             handler and {:.2}x when it is not — the boost did not propagate",
            lead(&boosted),
            lead(&plain)
        );
    }

    /// A pure re-export barrel must not outrank the module it forwards to,
    /// and the demotion must not starve that module of the rank routed
    /// through the barrel.
    #[test]
    fn barrels_route_rank_instead_of_hoarding_it() {
        let g = graph_from(
            r#"{"schema_version":1,
                "nodes":[
                  {"path":"src/ui/index.ts","exports":["Button","Card"]},
                  {"path":"src/ui/Button.tsx","exports":["Button"],
                   "symbols":[{"name":"Button","kind":"function","start_line":1,"end_line":9}]},
                  {"path":"src/ui/Card.tsx","exports":["Card"],
                   "symbols":[{"name":"Card","kind":"function","start_line":1,"end_line":9}]},
                  {"path":"src/a.ts"},{"path":"src/b.ts"},{"path":"src/c.ts"}
                ],
                "edges":[
                  {"from_path":"src/ui/index.ts","to_path":"src/ui/Button.tsx","kind":"imports"},
                  {"from_path":"src/ui/index.ts","to_path":"src/ui/Card.tsx","kind":"imports"},
                  {"from_path":"src/a.ts","to_path":"src/ui/index.ts","kind":"imports"},
                  {"from_path":"src/b.ts","to_path":"src/ui/index.ts","kind":"imports"},
                  {"from_path":"src/c.ts","to_path":"src/ui/index.ts","kind":"imports"}
                ]}"#,
        );
        assert!(
            is_barrel(g.node("src/ui/index.ts").unwrap(), 2),
            "the fixture's index.ts must be detected as a barrel"
        );
        assert!(
            score(&g, "src/ui/Button.tsx") > score(&g, "src/ui/index.ts"),
            "barrel {} outranked its member {}",
            score(&g, "src/ui/index.ts"),
            score(&g, "src/ui/Button.tsx")
        );
        // Rank still flows through: the members beat the leaves that import
        // the barrel. This is the half a naive "just demote barrels" fix
        // breaks — the members must inherit the barrel's inbound demand.
        assert!(
            score(&g, "src/ui/Button.tsx") > score(&g, "src/a.ts"),
            "member {} did not inherit the barrel's inbound demand over leaf {}",
            score(&g, "src/ui/Button.tsx"),
            score(&g, "src/a.ts")
        );
        // Both members share it evenly — neither is favoured by declaration
        // order in the barrel.
        assert_eq!(score(&g, "src/ui/Button.tsx"), score(&g, "src/ui/Card.tsx"));
    }

    /// Barrels chain (`src/index.ts` re-exports `src/ui/index.ts`), and they
    /// can be mutually recursive. Neither may hang the walk or leak flow.
    #[test]
    fn chained_and_cyclic_barrels_terminate() {
        let g = graph_from(
            r#"{"schema_version":1,
                "nodes":[
                  {"path":"src/index.ts"},{"path":"src/ui/index.ts"},
                  {"path":"src/ui/Button.tsx",
                   "symbols":[{"name":"Button","kind":"function","start_line":1,"end_line":9}]},
                  {"path":"src/ui/Card.tsx",
                   "symbols":[{"name":"Card","kind":"function","start_line":1,"end_line":9}]},
                  {"path":"src/app.ts"}
                ],
                "edges":[
                  {"from_path":"src/app.ts","to_path":"src/index.ts","kind":"imports"},
                  {"from_path":"src/index.ts","to_path":"src/ui/index.ts","kind":"imports"},
                  {"from_path":"src/index.ts","to_path":"src/ui/Button.tsx","kind":"imports"},
                  {"from_path":"src/ui/index.ts","to_path":"src/ui/Button.tsx","kind":"imports"},
                  {"from_path":"src/ui/index.ts","to_path":"src/ui/Card.tsx","kind":"imports"},
                  {"from_path":"src/ui/index.ts","to_path":"src/index.ts","kind":"imports"}
                ]}"#,
        );
        for n in &g.nodes {
            assert!(n.rank_score.is_finite());
        }
        assert!(score(&g, "src/ui/Button.tsx") > score(&g, "src/index.ts"));
        assert!(score(&g, "src/ui/Button.tsx") > score(&g, "src/ui/index.ts"));
    }

    /// An `index.ts` that declares its own symbols is a real module, not a
    /// barrel, and must not be demoted.
    #[test]
    fn an_index_file_with_its_own_code_is_not_a_barrel() {
        let node: Node = serde_json::from_str(
            r#"{"path":"src/index.ts",
                "symbols":[{"name":"main","kind":"function","start_line":1,"end_line":9}]}"#,
        )
        .unwrap();
        assert!(!is_barrel(&node, 5));
        let bare: Node = serde_json::from_str(r#"{"path":"src/index.ts"}"#).unwrap();
        assert!(is_barrel(&bare, 5));
        // One outgoing edge is a module, not a barrel.
        assert!(!is_barrel(&bare, 1));
    }

    /// Edge kinds are not interchangeable evidence: a `contains` edge should
    /// outweigh a `references` edge from the same source.
    #[test]
    fn edge_kinds_are_weighted_differently() {
        assert!(edge_weight(EdgeKind::Mod) > edge_weight(EdgeKind::Import));
        assert!(edge_weight(EdgeKind::Import) > edge_weight(EdgeKind::Use));
        assert_eq!(edge_weight(EdgeKind::Route), 0.0);

        let g = graph_from(
            r#"{"schema_version":1,
                "nodes":[{"path":"m.rs"},{"path":"strong.rs"},{"path":"weak.rs"}],
                "edges":[
                  {"from_path":"m.rs","to_path":"strong.rs","kind":"contains"},
                  {"from_path":"m.rs","to_path":"weak.rs","kind":"references"}
                ]}"#,
        );
        assert!(score(&g, "strong.rs") > score(&g, "weak.rs"));
    }

    /// Route edges are entry-point markers. Letting them carry flow would let
    /// a handler's own marker inflate whatever it points at.
    #[test]
    fn route_edges_carry_no_flow() {
        let with_route = graph_from(
            r#"{"schema_version":1,
                "nodes":[{"path":"a.ts"},{"path":"b.ts"}],
                "edges":[{"from_path":"a.ts","to_path":"b.ts","kind":"route"}]}"#,
        );
        let without = graph_from(
            r#"{"schema_version":1,
                "nodes":[{"path":"a.ts"},{"path":"b.ts"}],
                "edges":[]}"#,
        );
        assert_eq!(score(&with_route, "b.ts"), score(&without, "b.ts"));
    }

    /// A dependency cycle must converge rather than spin to the iteration cap
    /// with diverging scores, and must not produce NaN.
    #[test]
    fn cycles_converge_to_finite_scores() {
        let g = graph_from(
            r#"{"schema_version":1,
                "nodes":[{"path":"a.ts"},{"path":"b.ts"},{"path":"c.ts"}],
                "edges":[
                  {"from_path":"a.ts","to_path":"b.ts","kind":"imports"},
                  {"from_path":"b.ts","to_path":"c.ts","kind":"imports"},
                  {"from_path":"c.ts","to_path":"a.ts","kind":"imports"}
                ]}"#,
        );
        for n in &g.nodes {
            assert!(n.rank_score.is_finite(), "{} is not finite", n.path);
            assert!(n.rank_score > 0.0 && n.rank_score <= 1.0);
        }
        // A symmetric cycle is symmetric: all three scores equal.
        assert_eq!(score(&g, "a.ts"), score(&g, "b.ts"));
        assert_eq!(score(&g, "b.ts"), score(&g, "c.ts"));
    }

    /// Ranking runs on every graph load, so it has to be idempotent and
    /// independent of node ordering — otherwise the manifest is not
    /// reproducible.
    #[test]
    fn ranking_is_idempotent_and_order_independent() {
        let json = r#"{"schema_version":1,
            "nodes":[{"path":"a.ts"},{"path":"b.ts"},{"path":"hub.ts"}],
            "edges":[
              {"from_path":"a.ts","to_path":"hub.ts","kind":"imports"},
              {"from_path":"b.ts","to_path":"hub.ts","kind":"imports"}
            ]}"#;
        let mut g = graph_from(json);
        let first: Vec<(String, f64, u32)> = g
            .nodes
            .iter()
            .map(|n| (n.path.clone(), n.rank_score, n.rank_order))
            .collect();
        compute_ranks(&mut g);
        let second: Vec<(String, f64, u32)> = g
            .nodes
            .iter()
            .map(|n| (n.path.clone(), n.rank_score, n.rank_order))
            .collect();
        assert_eq!(first, second, "re-ranking changed the result");

        let mut reversed = graph_from(json);
        reversed.nodes.reverse();
        compute_ranks(&mut reversed);
        for (path, s, o) in &first {
            let n = reversed.node(path).unwrap();
            assert_eq!((n.rank_score, n.rank_order), (*s, *o), "{path} moved");
        }
    }

    #[test]
    fn an_empty_graph_is_not_a_panic() {
        let mut g: Graph =
            serde_json::from_str(r#"{"schema_version":1,"nodes":[],"edges":[]}"#).unwrap();
        compute_ranks(&mut g);
        assert!(g.nodes.is_empty());
    }

    #[test]
    fn test_file_prior_is_dampened_below_production_code() {
        let g = graph_from(
            r#"{"schema_version":1,
                "nodes":[
                  {"path":"src/utils.ts"},
                  {"path":"src/utils.test.ts"}
                ],
                "edges":[]}"#,
        );
        assert!(score(&g, "src/utils.ts") > score(&g, "src/utils.test.ts"));
        assert_eq!(order(&g, "src/utils.ts"), 1);
        assert_eq!(order(&g, "src/utils.test.ts"), 2);
    }

    #[test]
    fn root_entry_points_receive_teleport_boost() {
        let g = graph_from(
            r#"{"schema_version":1,
                "nodes":[
                  {"path":"src/App.tsx"},
                  {"path":"src/components/Widget.tsx"}
                ],
                "edges":[]}"#,
        );
        assert!(score(&g, "src/App.tsx") > score(&g, "src/components/Widget.tsx"));
        assert_eq!(order(&g, "src/App.tsx"), 1);
    }
}
