//! Dependency graph: deserialized form of the walker's cached output
//! (`.repograph/graph.json`) plus the query operations the agent API needs.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::path::Path;

pub const GRAPH_CACHE_RELATIVE_PATH: &str = ".repograph/graph.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Graph {
    pub schema_version: u32,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    #[serde(default)]
    pub external_dependencies: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<Warning>,
    #[serde(default)]
    pub symbol_edges: Vec<crate::db::SymbolEdge>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Node {
    pub path: String,
    #[serde(default)]
    pub size_bytes: u64,
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub exports: Vec<String>,
    #[serde(default)]
    pub routes: Vec<String>,
    #[serde(default)]
    pub in_degree: u32,
    #[serde(default)]
    pub out_degree: u32,
    #[serde(default)]
    pub symbols: Vec<crate::parsers::ExtractedSymbol>,
    /// Normalized graph centrality in `(0, 1]`, `1.0` for the most central
    /// file. Computed by [`crate::rank::compute_ranks`], never hand-set.
    ///
    /// `#[serde(default)]` so a `.repograph/graph.json` written before ranking
    /// existed still deserializes — every load path recomputes, so a stale
    /// cache is refreshed rather than reported as rank 0.
    #[serde(default)]
    pub rank_score: f64,
    /// Dense 1-based position in the ranking. `0` only before ranking has run.
    #[serde(default)]
    pub rank_order: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub from_path: String,
    pub to_path: String,
    pub kind: EdgeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum EdgeKind {
    #[serde(rename = "imports", alias = "import")]
    Import,
    #[serde(rename = "require")]
    Require,
    #[serde(rename = "contains", alias = "mod")]
    Mod,
    #[serde(rename = "references", alias = "use")]
    Use,
    #[serde(rename = "route")]
    Route,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Warning {
    pub path: String,
    pub kind: String,
}

/// One file's extraction output, ready for graph construction.
#[derive(Debug)]
pub struct ParsedFile {
    pub entry: crate::walker::FileEntry,
    pub extraction: crate::parsers::ExtractionResult,
}

/// Build the graph from walker + parser output: verify import candidates
/// against the indexed file set (false negatives over false positives),
/// split externals out of the edge set, compute degrees.
pub fn load_tsconfig_aliases(root: &Path) -> HashMap<String, Vec<String>> {
    let tsconfig_path = root.join("tsconfig.json");
    if !tsconfig_path.exists() {
        return HashMap::new();
    }
    let content = match std::fs::read_to_string(&tsconfig_path) {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };
    let cleaned = clean_json_comments(&content);
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&cleaned);
    let mut aliases = HashMap::new();
    if let Ok(val) = parsed {
        if let Some(paths) = val.get("compilerOptions").and_then(|co| co.get("paths")).and_then(|p| p.as_object()) {
            for (k, v) in paths {
                if let Some(arr) = v.as_array() {
                    let mapped_paths: Vec<String> = arr.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect();
                    aliases.insert(k.clone(), mapped_paths);
                }
            }
        }
    }
    aliases
}

fn clean_json_comments(s: &str) -> String {
    let mut cleaned = String::new();
    let mut in_comment = false;
    let mut in_line_comment = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if in_line_comment {
            if c == '\n' {
                in_line_comment = false;
                cleaned.push(c);
            }
        } else if in_comment {
            if c == '*' && chars.peek() == Some(&'/') {
                chars.next();
                in_comment = false;
            }
        } else {
            if c == '/' && chars.peek() == Some(&'/') {
                chars.next();
                in_line_comment = true;
            } else if c == '/' && chars.peek() == Some(&'*') {
                chars.next();
                in_comment = true;
            } else {
                cleaned.push(c);
            }
        }
    }
    cleaned
}

pub fn resolve_tsconfig_alias(aliases: &HashMap<String, Vec<String>>, stem: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    for (pattern, targets) in aliases {
        if pattern.ends_with('*') {
            let prefix = &pattern[..pattern.len() - 1];
            if let Some(wildcard) = stem.strip_prefix(prefix) {
                for target in targets {
                    if target.ends_with('*') {
                        let target_prefix = &target[..target.len() - 1];
                        candidates.push(format!("{}{}", target_prefix, wildcard));
                    } else {
                        candidates.push(target.clone());
                    }
                }
            }
        } else if pattern == stem {
            for target in targets {
                candidates.push(target.clone());
            }
        }
    }
    candidates
}

pub fn build_graph(parsed: Vec<ParsedFile>, aliases: &HashMap<String, Vec<String>>) -> Graph {
    use std::collections::{BTreeSet, HashMap};

    let file_set: BTreeSet<String> = parsed.iter().map(|p| p.entry.path.clone()).collect();
    let mut edges: BTreeSet<(String, String, EdgeKind)> = BTreeSet::new();
    let mut externals: BTreeSet<String> = BTreeSet::new();
    let mut warnings: Vec<Warning> = Vec::new();
    let mut nodes: Vec<Node> = Vec::new();

    for p in &parsed {
        let path = &p.entry.path;
        for kind in &p.extraction.warnings {
            warnings.push(Warning {
                path: path.clone(),
                kind: kind.clone(),
            });
        }
        // `dedup` alone only collapses *consecutive* repeats, so an export
        // listed twice with anything between survived into the manifest.
        let mut exports: Vec<String> = p.extraction.exports.clone();
        exports.sort();
        exports.dedup();

        for import in &p.extraction.imports {
            let resolved = import.resolved_path.as_deref().and_then(|stem| {
                let mut stems = vec![stem.to_string()];
                let mapped = resolve_tsconfig_alias(aliases, stem);
                stems.extend(mapped);

                stems.iter().find_map(|s| {
                    resolve_candidates(&p.entry.language, s)
                        .into_iter()
                        .find(|c| file_set.contains(c))
                })
            });
            match resolved {
                Some(target) => {
                    if target != *path {
                        edges.insert((path.clone(), target, import.kind));
                    }
                }
                None if import.external => {
                    externals.insert(external_package_name(
                        &p.entry.language,
                        &import.raw_specifier,
                    ));
                }
                None => {
                    // Internal candidate that matched nothing: flag it (a
                    // known gap beats a silently missing edge), except for
                    // Rust `use` item paths where misses are routine.
                    if import.kind != EdgeKind::Use {
                        warnings.push(Warning {
                            path: path.clone(),
                            kind: format!("unresolved_import:{}", import.raw_specifier),
                        });
                    }
                }
            }
        }

        nodes.push(Node {
            path: path.clone(),
            size_bytes: p.entry.size_bytes,
            language: p.entry.language.clone(),
            exports,
            routes: {
                // Dedupe: multiple handlers can bind the same URL (e.g.
                // Remix loader + action).
                let mut rs: Vec<String> = Vec::new();
                for e in &p.extraction.entry_points {
                    if !rs.contains(&e.route) {
                        rs.push(e.route.clone());
                    }
                }
                rs
            },
            in_degree: 0,
            out_degree: 0,
            symbols: p.extraction.symbols.clone(),
            rank_score: 0.0,
            rank_order: 0,
        });
    }

    let mut out_counts: HashMap<&str, u32> = HashMap::new();
    let mut in_counts: HashMap<&str, u32> = HashMap::new();
    for (from, to, _) in &edges {
        *out_counts.entry(from.as_str()).or_insert(0) += 1;
        *in_counts.entry(to.as_str()).or_insert(0) += 1;
    }
    for node in &mut nodes {
        node.out_degree = out_counts.get(node.path.as_str()).copied().unwrap_or(0);
        node.in_degree = in_counts.get(node.path.as_str()).copied().unwrap_or(0);
    }

    let mut graph = Graph {
        // Unchanged: `rank_score`/`rank_order` are additive and defaulted, so
        // every existing cache still deserializes. Bumping this would break
        // them for a field that is recomputed on load anyway.
        schema_version: 1,
        nodes,
        edges: edges
            .into_iter()
            .map(|(from_path, to_path, kind)| Edge {
                from_path,
                to_path,
                kind,
            })
            .collect(),
        external_dependencies: externals.into_iter().collect(),
        warnings,
        symbol_edges: Vec::new(),
    };
    crate::rank::compute_ranks(&mut graph);
    graph
}

pub(crate) fn resolve_candidates(language: &str, stem: &str) -> Vec<String> {
    let mut c = vec![stem.to_string()];
    match language {
        "javascript" => {
            for ext in ["ts", "tsx", "js", "jsx", "mjs", "cjs"] {
                c.push(format!("{stem}.{ext}"));
            }
            for ext in ["ts", "tsx", "js", "jsx"] {
                c.push(format!("{stem}/index.{ext}"));
            }
        }
        "python" => {
            c.push(format!("{stem}.py"));
            c.push(format!("{stem}/__init__.py"));
        }
        "rust" => {
            c.push(format!("{stem}.rs"));
            c.push(format!("{stem}/mod.rs"));
            // `use crate::module::Item` — drop the item segment.
            if let Some((parent, _)) = stem.rsplit_once('/') {
                c.push(format!("{parent}.rs"));
                c.push(format!("{parent}/mod.rs"));
            }
        }
        "go" => {
            c.push(format!("{stem}.go"));
            if let Some((_, last)) = stem.rsplit_once('/') {
                c.push(format!("{stem}/{last}.go"));
            } else {
                c.push(format!("{stem}/{stem}.go"));
            }
        }
        "java" => {
            c.push(format!("{stem}.java"));
        }
        "kotlin" => {
            c.push(format!("{stem}.kt"));
        }
        "c" => {
            c.push(format!("{stem}.c"));
            c.push(format!("{stem}.h"));
        }
        "cpp" => {
            c.push(format!("{stem}.cpp"));
            c.push(format!("{stem}.hpp"));
            c.push(format!("{stem}.h"));
        }
        "swift" => {
            c.push(format!("{stem}.swift"));
        }
        "php" => {
            c.push(format!("{stem}.php"));
        }
        "csharp" => {
            c.push(format!("{stem}.cs"));
        }
        "html" => {
            c.push(format!("{stem}.html"));
        }
        "sql" => {
            c.push(format!("{stem}.sql"));
        }
        "prisma" => {
            c.push(format!("{stem}.prisma"));
        }
        "markdown" => {
            c.push(format!("{stem}.md"));
            c.push(format!("{stem}.markdown"));
        }
        _ => {}
    }
    c
}

/// Reduce a raw specifier to the package name recorded in
/// `external_dependencies` (`react/jsx-runtime` → `react`,
/// `@scope/pkg/sub` → `@scope/pkg`, `os.path` → `os`).
pub(crate) fn external_package_name(language: &str, raw: &str) -> String {
    match language {
        "javascript" => {
            let mut segs = raw.split('/');
            match (raw.starts_with('@'), segs.next(), segs.next()) {
                (true, Some(scope), Some(name)) => format!("{scope}/{name}"),
                (_, Some(first), _) => first.to_string(),
                _ => raw.to_string(),
            }
        }
        "python" => raw.split('.').next().unwrap_or(raw).to_string(),
        _ => raw.to_string(),
    }
}

impl Graph {
    /// Serialize to `<root>/.repograph/graph.json` for the MCP server atomically.
    pub fn save_to_cache(&self, root: &Path) -> std::io::Result<std::path::PathBuf> {
        let cache_path = root.join(GRAPH_CACHE_RELATIVE_PATH);
        if let Some(dir) = cache_path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(self).expect("graph serializes");
        let tmp_path = cache_path.with_extension("json.tmp");
        std::fs::write(&tmp_path, json)?;
        std::fs::rename(&tmp_path, &cache_path)?;
        Ok(cache_path)
    }
}

#[derive(Debug)]
pub enum GraphLoadError {
    CacheMissing(String),
    Io(std::io::Error),
    Malformed(serde_json::Error),
}

impl std::fmt::Display for GraphLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GraphLoadError::CacheMissing(p) => write!(
                f,
                "graph cache not found at {p}; run the indexer (walker) first"
            ),
            GraphLoadError::Io(e) => write!(f, "failed to read graph cache: {e}"),
            GraphLoadError::Malformed(e) => write!(f, "graph cache is not valid JSON: {e}"),
        }
    }
}

impl Graph {
    pub fn load_from_cache(cache_path: &Path) -> Result<Graph, GraphLoadError> {
        if !cache_path.is_file() {
            return Err(GraphLoadError::CacheMissing(
                cache_path.display().to_string(),
            ));
        }
        let raw = std::fs::read_to_string(cache_path).map_err(GraphLoadError::Io)?;
        // Tolerate a UTF-8 BOM — common when the cache was written by
        // Windows tooling.
        let mut graph: Graph = serde_json::from_str(raw.trim_start_matches('\u{feff}'))
            .map_err(GraphLoadError::Malformed)?;
        // Recompute rather than trust the cache: a graph.json written before
        // ranking existed carries the serde defaults, and reporting every file
        // at rank 0 is worse than the cost of one power iteration.
        crate::rank::compute_ranks(&mut graph);
        Ok(graph)
    }

    /// Node paths in descending rank order — the manifest's presentation order
    /// and the basis for `top_k`.
    pub fn ranked_paths(&self) -> Vec<&str> {
        let mut nodes: Vec<&Node> = self.nodes.iter().collect();
        nodes.sort_by_key(|n| n.rank_order);
        nodes.into_iter().map(|n| n.path.as_str()).collect()
    }

    pub fn node(&self, path: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.path == path)
    }

    /// Unique in-repo files `path` depends on (route edges are entry-point
    /// markers, not dependencies).
    ///
    /// Linear in `edges`. For anything that queries more than a couple of
    /// paths — manifest generation, blast-radius walks — build an
    /// [`Adjacency`] once instead; see [`Graph::adjacency`].
    pub fn dependencies_of(&self, path: &str) -> Vec<String> {
        let set: BTreeSet<&str> = self
            .edges
            .iter()
            .filter(|e| e.from_path == path && e.kind != EdgeKind::Route)
            .map(|e| e.to_path.as_str())
            .collect();
        set.into_iter().map(String::from).collect()
    }

    /// All files with an edge into `path` — "what breaks if I change this".
    /// Same linear-scan caveat as [`Graph::dependencies_of`].
    pub fn dependents_of(&self, path: &str) -> Vec<String> {
        let set: BTreeSet<&str> = self
            .edges
            .iter()
            .filter(|e| e.to_path == path && e.kind != EdgeKind::Route)
            .map(|e| e.from_path.as_str())
            .collect();
        set.into_iter().map(String::from).collect()
    }

    /// Build both adjacency directions in one pass over `edges`.
    ///
    /// `build_manifest` called `dependencies_of` once per node and
    /// `repograph_impact` called `dependents_of` inside a BFS, so both were
    /// O(N·E): ~5×10⁸ string comparisons on the 10k-node / 50k-edge repos this
    /// tool targets. `src/store.ts` has done exactly this since day one
    /// (`ingestGraph`, "no linear scans per query"); the agent-facing side had
    /// not caught up.
    pub fn adjacency(&self) -> Adjacency {
        let mut deps: HashMap<String, Vec<String>> = HashMap::new();
        let mut dependents: HashMap<String, Vec<String>> = HashMap::new();
        for e in &self.edges {
            if e.kind == EdgeKind::Route {
                continue;
            }
            deps.entry(e.from_path.clone()).or_default().push(e.to_path.clone());
            dependents.entry(e.to_path.clone()).or_default().push(e.from_path.clone());
        }
        // Sorted + deduped so callers see the same order the linear-scan
        // versions produced via `BTreeSet`.
        for v in deps.values_mut().chain(dependents.values_mut()) {
            v.sort();
            v.dedup();
        }
        Adjacency { deps, dependents }
    }
}

/// Precomputed both-direction adjacency for a [`Graph`]. Build once per graph
/// load, then answer every dependency query in O(1).
#[derive(Debug, Default, Clone)]
pub struct Adjacency {
    deps: HashMap<String, Vec<String>>,
    dependents: HashMap<String, Vec<String>>,
}

impl Adjacency {
    pub fn dependencies_of(&self, path: &str) -> Vec<String> {
        self.deps.get(path).cloned().unwrap_or_default()
    }

    pub fn dependents_of(&self, path: &str) -> Vec<String> {
        self.dependents.get(path).cloned().unwrap_or_default()
    }

    /// Every file transitively reachable by following dependents — the blast
    /// radius of changing `path`, excluding `path` itself.
    pub fn transitive_dependents(&self, path: &str) -> BTreeSet<String> {
        let mut affected = BTreeSet::new();
        let mut queue = vec![path.to_string()];
        while let Some(current) = queue.pop() {
            let Some(incoming) = self.dependents.get(&current) else { continue };
            for dep in incoming {
                if affected.insert(dep.clone()) {
                    queue.push(dep.clone());
                }
            }
        }
        affected.remove(path);
        affected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Graph {
        serde_json::from_str(
            r#"{
              "schema_version": 1,
              "nodes": [
                {"path": "src/components/Button.tsx", "exports": ["Button"]},
                {"path": "src/hooks/useTheme.ts", "exports": ["useTheme"]},
                {"path": "src/pages/api/login.ts", "routes": ["/api/login"]}
              ],
              "edges": [
                {"from_path": "src/components/Button.tsx", "to_path": "src/hooks/useTheme.ts", "kind": "import"},
                {"from_path": "src/pages/api/login.ts", "to_path": "src/hooks/useTheme.ts", "kind": "import"}
              ],
              "external_dependencies": ["react"]
            }"#,
        )
        .expect("fixture graph parses")
    }

    #[test]
    fn dependents_returns_all_incoming_edges() {
        let g = fixture();
        assert_eq!(
            g.dependents_of("src/hooks/useTheme.ts"),
            vec![
                "src/components/Button.tsx".to_string(),
                "src/pages/api/login.ts".to_string()
            ]
        );
        assert!(g.dependents_of("src/components/Button.tsx").is_empty());
    }

    #[test]
    fn dependencies_are_unique_and_sorted() {
        let g = fixture();
        assert_eq!(
            g.dependencies_of("src/components/Button.tsx"),
            vec!["src/hooks/useTheme.ts".to_string()]
        );
    }

    /// The index has to be a drop-in replacement for the linear scans, or
    /// swapping it into `build_manifest` silently changes the manifest.
    #[test]
    fn adjacency_matches_the_linear_scans_it_replaces() {
        let g = fixture();
        let adj = g.adjacency();
        for n in &g.nodes {
            assert_eq!(
                adj.dependencies_of(&n.path),
                g.dependencies_of(&n.path),
                "dependencies disagree for {}",
                n.path
            );
            assert_eq!(
                adj.dependents_of(&n.path),
                g.dependents_of(&n.path),
                "dependents disagree for {}",
                n.path
            );
        }
        // Route edges are entry-point markers, never dependencies.
        assert!(adj.dependencies_of("src/hooks/useTheme.ts").is_empty());
    }

    #[test]
    fn transitive_dependents_walks_the_chain_and_excludes_the_anchor() {
        let g: Graph = serde_json::from_str(
            r#"{"schema_version":1,
                "nodes":[{"path":"a.ts"},{"path":"b.ts"},{"path":"c.ts"}],
                "edges":[
                  {"from_path":"b.ts","to_path":"a.ts","kind":"import"},
                  {"from_path":"c.ts","to_path":"b.ts","kind":"import"}
                ]}"#,
        )
        .unwrap();
        let affected = g.adjacency().transitive_dependents("a.ts");
        assert_eq!(
            affected.into_iter().collect::<Vec<_>>(),
            vec!["b.ts".to_string(), "c.ts".to_string()]
        );
    }

    /// `Vec::dedup` only removes *consecutive* duplicates, so an unsorted
    /// export list kept every non-adjacent repeat.
    #[test]
    fn non_adjacent_duplicate_exports_are_removed() {
        let parsed = vec![ParsedFile {
            entry: crate::walker::FileEntry {
                path: "a.ts".into(),
                size_bytes: 0,
                extension: "ts".into(),
                language: "javascript".into(),
            },
            extraction: crate::parsers::ExtractionResult {
                exports: vec!["a".into(), "b".into(), "a".into()],
                ..Default::default()
            },
        }];
        let g = build_graph(parsed, &HashMap::new());
        assert_eq!(g.nodes[0].exports, vec!["a".to_string(), "b".to_string()]);
    }
}
