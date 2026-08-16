use crate::graph::Graph;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Threshold below which full domain clustering is bypassed in favor of a single root domain.
pub const MIN_FILES_FOR_CLUSTERING: usize = 30;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DomainSummary {
    pub id: usize,
    pub name: String,
    pub file_count: usize,
    pub top_hub: String,
    pub cohesion_score: f64,
    pub key_exports: Vec<String>,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DomainResult {
    pub domains: Vec<DomainSummary>,
    pub file_domain_map: HashMap<String, usize>,
}

/// Identifies high-centrality shared utility files and barrel files whose edges
/// should be dampened to prevent the "giant component / utility blob" trap.
pub fn is_dampened_hub(path: &str, in_degree: u32, total_nodes: usize) -> bool {
    // 1. Files with in-degree > 15% of entire codebase are global utilities
    if total_nodes >= 20 && (in_degree as f64) > (total_nodes as f64 * 0.15) {
        return true;
    }

    let normalized = path.replace('\\', "/");
    let name = normalized.rsplit('/').next().unwrap_or(&normalized);

    // 2. Standard barrel files or universal type declarations
    matches!(
        name,
        "types.ts"
            | "types.d.ts"
            | "index.ts"
            | "index.d.ts"
            | "mod.rs"
            | "lib.rs"
            | "__init__.py"
            | "types.py"
            | "utils.py"
            | "helpers.py"
            | "constants.py"
            | "types.go"
            | "helpers.go"
            | "utils.go"
            | "constants.ts"
            | "constants.rs"
            | "errors.ts"
            | "errors.rs"
            | "utils.ts"
            | "helpers.ts"
            | "utils.rs"
            | "Util.java"
            | "Utils.java"
            | "Constants.java"
            | "Common.cs"
            | "Utils.cs"
    )
}

/// Computes deterministic Louvain community clusters across a repository graph.
pub fn detect_domains(graph: &Graph) -> DomainResult {
    let total_nodes = graph.nodes.len();
    if total_nodes == 0 {
        return DomainResult {
            domains: Vec::new(),
            file_domain_map: HashMap::new(),
        };
    }

    // Small repo threshold gating (< 30 files): Return 1 cohesive root domain
    if total_nodes < MIN_FILES_FOR_CLUSTERING {
        let mut sorted_files: Vec<String> = graph.nodes.iter().map(|n| n.path.clone()).collect();
        sorted_files.sort();

        let top_hub = graph
            .nodes
            .iter()
            .max_by(|a, b| a.rank_score.partial_cmp(&b.rank_score).unwrap())
            .map(|n| n.path.clone())
            .unwrap_or_else(|| sorted_files[0].clone());

        let mut key_exports = Vec::new();
        if let Some(hub_node) = graph.node(&top_hub) {
            for sym in &hub_node.symbols {
                if key_exports.len() < 3 {
                    key_exports.push(sym.name.clone());
                }
            }
            for exp in &hub_node.exports {
                if key_exports.len() < 3 && !key_exports.contains(exp) {
                    key_exports.push(exp.clone());
                }
            }
        }

        let mut file_domain_map = HashMap::new();
        for f in &sorted_files {
            file_domain_map.insert(f.clone(), 0);
        }

        return DomainResult {
            domains: vec![DomainSummary {
                id: 0,
                name: "Core Application".to_string(),
                file_count: total_nodes,
                top_hub,
                cohesion_score: 1.0,
                key_exports,
                files: sorted_files,
            }],
            file_domain_map,
        };
    }

    // 1. Build canonical node index with strict alphabetical ordering
    let mut sorted_nodes = graph.nodes.clone();
    sorted_nodes.sort_by(|a, b| a.path.cmp(&b.path));

    let mut path_to_idx: HashMap<String, usize> = HashMap::new();
    let mut in_degrees: Vec<u32> = vec![0; sorted_nodes.len()];
    for (idx, node) in sorted_nodes.iter().enumerate() {
        path_to_idx.insert(node.path.clone(), idx);
        in_degrees[idx] = node.in_degree;
    }

    // 2. Build weighted undirected adjacency matrix with utility dampening
    let mut adj: Vec<HashMap<usize, f64>> = vec![HashMap::new(); sorted_nodes.len()];
    for edge in &graph.edges {
        if edge.from_path == edge.to_path {
            continue;
        }
        if let (Some(&u), Some(&v)) = (
            path_to_idx.get(&edge.from_path),
            path_to_idx.get(&edge.to_path),
        ) {
            let u_is_hub = is_dampened_hub(&edge.from_path, in_degrees[u], total_nodes);
            let v_is_hub = is_dampened_hub(&edge.to_path, in_degrees[v], total_nodes);

            let weight = if u_is_hub || v_is_hub { 0.1 } else { 1.0 };

            *adj[u].entry(v).or_insert(0.0) += weight;
            *adj[v].entry(u).or_insert(0.0) += weight;
        }
    }

    // 3. Initial community assignments: each node in its own community
    let mut community_of: Vec<usize> = (0..sorted_nodes.len()).collect();

    // 4. Modularity optimization passes (fixed iteration bound for determinism)
    let max_passes = 10;
    for _ in 0..max_passes {
        let mut moved = false;
        for u in 0..sorted_nodes.len() {
            let current_comm = community_of[u];
            if adj[u].is_empty() {
                continue;
            }

            // Count neighbor community weights
            let mut comm_weights: HashMap<usize, f64> = HashMap::new();
            for (&v, &w) in &adj[u] {
                let c = community_of[v];
                *comm_weights.entry(c).or_insert(0.0) += w;
            }

            // Pick community with max weight (tie-break on lowest community ID for determinism)
            let mut best_comm = current_comm;
            let mut max_weight = comm_weights.get(&current_comm).copied().unwrap_or(0.0);

            for (&cand_comm, &w) in &comm_weights {
                if w > max_weight || ((w - max_weight).abs() < f64::EPSILON && cand_comm < best_comm) {
                    max_weight = w;
                    best_comm = cand_comm;
                }
            }

            if best_comm != current_comm {
                community_of[u] = best_comm;
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }

    // 5. Group by community
    let mut comm_members: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (u, &comm) in community_of.iter().enumerate() {
        comm_members.entry(comm).or_default().push(u);
    }

    // Sort communities by size descending, then by first node path for stability
    let mut sorted_comms: Vec<Vec<usize>> = comm_members.into_values().collect();
    sorted_comms.sort_by(|a, b| {
        b.len()
            .cmp(&a.len())
            .then_with(|| sorted_nodes[a[0]].path.cmp(&sorted_nodes[b[0]].path))
    });

    let mut domains: Vec<DomainSummary> = Vec::new();
    let mut file_domain_map: HashMap<String, usize> = HashMap::new();

    for (new_id, members) in sorted_comms.iter().enumerate() {
        let mut member_paths: Vec<String> = members
            .iter()
            .map(|&idx| sorted_nodes[idx].path.clone())
            .collect();
        member_paths.sort();

        for p in &member_paths {
            file_domain_map.insert(p.clone(), new_id);
        }

        // Find top hub by rank_score
        let mut top_hub = member_paths[0].clone();
        let mut max_rank = -1.0;
        for p in &member_paths {
            if let Some(n) = graph.node(p) {
                if n.rank_score > max_rank {
                    max_rank = n.rank_score;
                    top_hub = p.clone();
                }
            }
        }

        // Compute cohesion score
        let member_set: HashSet<usize> = members.iter().copied().collect();
        let mut internal_edges = 0;
        for &u in members {
            for (&v, _) in &adj[u] {
                if member_set.contains(&v) {
                    internal_edges += 1;
                }
            }
        }
        let actual_edges = internal_edges / 2;
        let possible_edges = (members.len() * (members.len().saturating_sub(1))) / 2;
        let cohesion_score = if possible_edges > 0 {
            (actual_edges as f64 / possible_edges as f64).min(1.0)
        } else {
            1.0
        };

        // Extract key exports from hub node and member nodes
        let mut key_exports = Vec::new();
        if let Some(hub_node) = graph.node(&top_hub) {
            for sym in &hub_node.symbols {
                if key_exports.len() < 3 && !key_exports.contains(&sym.name) {
                    key_exports.push(sym.name.clone());
                }
            }
            for exp in &hub_node.exports {
                if key_exports.len() < 3 && !key_exports.contains(exp) {
                    key_exports.push(exp.clone());
                }
            }
        }

        let name = derive_domain_name(&member_paths, &top_hub);

        domains.push(DomainSummary {
            id: new_id,
            name,
            file_count: member_paths.len(),
            top_hub,
            cohesion_score,
            key_exports,
            files: member_paths,
        });
    }

    DomainResult {
        domains,
        file_domain_map,
    }
}

/// Derives a clean architectural domain name from common path prefixes or top hub.
fn derive_domain_name(files: &[String], top_hub: &str) -> String {
    if files.len() == 1 {
        let name = files[0].rsplit('/').next().unwrap_or(&files[0]);
        return name.to_string();
    }

    // Check directory prefix frequencies
    let mut dir_counts: BTreeMap<String, usize> = BTreeMap::new();
    for f in files {
        let parts: Vec<&str> = f.split('/').collect();
        if parts.len() > 1 {
            let dir = parts[..parts.len() - 1].join("/");
            *dir_counts.entry(dir).or_insert(0) += 1;
        }
    }

    if let Some((best_dir, count)) = dir_counts.iter().max_by_key(|(_, &c)| c) {
        if *count >= (files.len() / 2).max(1) {
            let formatted = best_dir
                .replace("src/", "")
                .replace("src-tauri/src/", "")
                .replace('/', " / ");
            if !formatted.is_empty() {
                return format!("{} Domain", formatted);
            }
        }
    }

    let hub_file = top_hub.rsplit('/').next().unwrap_or(top_hub);
    let base_name = hub_file.split('.').next().unwrap_or(hub_file);
    format!("{} Domain", base_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Edge, EdgeKind, Graph, Node};

    fn make_node(path: &str, lang: &str) -> Node {
        Node {
            path: path.to_string(),
            language: lang.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn small_repo_under_threshold_returns_core_application_domain() {
        let nodes = vec![
            make_node("src/main.rs", "rust"),
            make_node("src/lib.rs", "rust"),
        ];
        let edges = vec![Edge {
            from_path: "src/main.rs".into(),
            to_path: "src/lib.rs".into(),
            kind: EdgeKind::Import,
        }];
        let graph = Graph {
            schema_version: 1,
            nodes,
            edges,
            external_dependencies: Vec::new(),
            warnings: Vec::new(),
            symbol_edges: Vec::new(),
        };
        let result = detect_domains(&graph);

        assert_eq!(result.domains.len(), 1);
        assert_eq!(result.domains[0].name, "Core Application");
        assert_eq!(result.domains[0].file_count, 2);
    }

    #[test]
    fn detects_distinct_clusters_with_utility_dampening() {
        let mut nodes = Vec::new();
        // 35 files (over threshold)
        for i in 0..15 {
            nodes.push(make_node(&format!("src/auth/file_{}.ts", i), "typescript"));
        }
        for i in 0..15 {
            nodes.push(make_node(&format!("src/engine/file_{}.rs", i), "rust"));
        }
        for i in 0..5 {
            nodes.push(make_node(&format!("src/utils/helper_{}.ts", i), "typescript"));
        }

        let mut edges = Vec::new();
        // Internal auth edges
        for i in 0..14 {
            edges.push(Edge {
                from_path: format!("src/auth/file_{}.ts", i),
                to_path: format!("src/auth/file_{}.ts", i + 1),
                kind: EdgeKind::Import,
            });
        }
        // Internal engine edges
        for i in 0..14 {
            edges.push(Edge {
                from_path: format!("src/engine/file_{}.rs", i),
                to_path: format!("src/engine/file_{}.rs", i + 1),
                kind: EdgeKind::Import,
            });
        }

        let graph = Graph {
            schema_version: 1,
            nodes,
            edges,
            external_dependencies: Vec::new(),
            warnings: Vec::new(),
            symbol_edges: Vec::new(),
        };
        let result = detect_domains(&graph);

        assert!(result.domains.len() >= 2);
        // Auth and Engine files should belong to distinct domains
        let auth_domain = result.file_domain_map.get("src/auth/file_0.ts");
        let engine_domain = result.file_domain_map.get("src/engine/file_0.rs");
        assert_ne!(auth_domain, engine_domain);
    }
}
