//! Token-budget guard for the agent-facing manifest.
//!
//! `docs/logic/token-optimization-logic.md` §Validation specifies exactly this
//! check ("fail the build if a fixture repo's manifest exceeds its expected
//! token budget by more than 20%"). It was written down but never implemented,
//! so the >90%-savings claim had nothing defending it: any change to
//! `render_markdown` could quietly inflate every agent's context.

use repo_graph::indexer::build_graph_for;
use repo_graph::manifest::{
    build_manifest, build_manifest_with, render_markdown, ManifestOptions, DEFAULT_LINE_BUDGET,
};
use std::path::{Path, PathBuf};

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mixed-repo")
}

/// Budget for the 27-file fixture repo, in **exact BPE tokens**
/// (`tokens::count_tokens`), plus the §Validation 20% headroom.
///
/// This was a `chars / 3.7` estimate. A single divisor is wrong in opposite
/// directions depending on content — it reads ~9% low on dense code and ~37%
/// high on prose — so a budget expressed in estimated tokens could drift
/// without any real change in cost. See `repo_graph::tokens`.
///
/// Raised from 620 for payload format 1.2.0, deliberately and after measuring:
/// ranking added `(In: k)` on shared files (+65 tokens over 25 files) and one
/// footer line explaining the ordering (+42, fixed). The rank marker `[#N]`
/// measured at 4 tokens per file and is now emitted only where it disagrees
/// with list position, which costs nothing on an unfiltered map.
const FIXTURE_TOKEN_BUDGET: usize = 750;

#[test]
fn fixture_manifest_stays_inside_its_token_budget() {
    let graph = build_graph_for(&fixture_root());
    let manifest = build_manifest(&graph, "/fixture", None);
    let md = render_markdown(&manifest, DEFAULT_LINE_BUDGET);
    let tokens = repo_graph::tokens::count_tokens(&md);

    assert!(
        tokens <= FIXTURE_TOKEN_BUDGET,
        "manifest grew to ~{tokens} tokens ({} chars), over the {FIXTURE_TOKEN_BUDGET}-token budget.\n\
         If this is intentional, re-measure and raise FIXTURE_TOKEN_BUDGET deliberately.\n\
         ---\n{md}",
        md.chars().count()
    );
}

/// What actually makes the manifest scale: cost per indexed file must stay
/// roughly constant, and the instructional footer must be fixed overhead
/// rather than something that grows with the repo.
///
/// A whole-repo compression *ratio* is the wrong measure here — the fixture's
/// 27 files total ~2 KB of stub source, so the fixed footer dwarfs them and
/// the ratio says nothing. On a real repo the ratio follows directly from
/// this per-file bound.
#[test]
fn manifest_cost_per_file_stays_flat() {
    let graph = build_graph_for(&fixture_root());
    let manifest = build_manifest(&graph, "/fixture", None);
    let md = render_markdown(&manifest, DEFAULT_LINE_BUDGET);

    // Everything before `## Agent Instructions` is the per-file body.
    let footer_start = md.find("\n## Agent Instructions").expect("footer present");
    let body_chars = md[..footer_start].chars().count();
    let footer_chars = md[footer_start..].chars().count();
    let files = manifest.files.len().max(1);
    let per_file = body_chars as f64 / files as f64;

    assert!(
        per_file <= 120.0,
        "manifest costs {per_file:.1} chars/file ({body_chars} chars over {files} files); \
         one line per file is the entire compression premise"
    );
    // Fixed cost, so it amortises to nothing as the repo grows.
    assert!(
        footer_chars < 700,
        "instructional footer grew to {footer_chars} chars"
    );
}

/// The over-budget path has to actually shed lines, or the line budget is
/// decorative.
#[test]
fn over_budget_rendering_collapses_uninteresting_files() {
    let graph = build_graph_for(&fixture_root());
    let manifest = build_manifest(&graph, "/fixture", None);

    let full = render_markdown(&manifest, DEFAULT_LINE_BUDGET);
    let collapsed = render_markdown(&manifest, 1);
    assert!(
        collapsed.chars().count() < full.chars().count(),
        "collapsed {} vs full {}",
        collapsed.chars().count(),
        full.chars().count()
    );
    assert!(collapsed.contains("lower-ranked files"));
}

/// `top_k` is the ranking feature's whole token argument, so it has to cut
/// cost roughly in proportion to what it drops.
///
/// Asserted on the **body**, not the total: the instructional footer is fixed
/// overhead that a 25-file fixture cannot amortise, so a whole-payload ratio
/// here would measure the footer rather than the filter. On a real repo the
/// footer is noise and the body ratio is the whole story.
#[test]
fn top_k_cuts_manifest_cost_in_proportion() {
    let graph = build_graph_for(&fixture_root());
    let adjacency = graph.adjacency();
    let body_tokens = |opts: &ManifestOptions| {
        let m = build_manifest_with(&graph, &adjacency, "/fixture", opts);
        let md = render_markdown(&m, DEFAULT_LINE_BUDGET);
        let footer = md.find("\n## Agent Instructions").expect("footer present");
        (repo_graph::tokens::count_tokens(&md[..footer]), m.files.len())
    };

    let (full, full_files) = body_tokens(&ManifestOptions::default());
    let (top5, top5_files) = body_tokens(&ManifestOptions {
        top_k: Some(5),
        ..Default::default()
    });
    assert_eq!(top5_files, 5);
    assert!(full_files > 5, "fixture too small to test truncation");

    // Includes the "Showing 5 of N" notice, which is the honest cost of not
    // letting a filtered map read as a complete one.
    let saved = 1.0 - (top5 as f64 / full as f64);
    assert!(
        saved > 0.5,
        "top_k(5) of {full_files} files saved only {:.1}% of the body \
         ({top5} vs {full} tokens)",
        saved * 100.0
    );
}

/// Scoping is the documented escape hatch for repos too large for a full
/// manifest, so it has to actually shrink the payload.
#[test]
fn scoping_shrinks_the_manifest() {
    let graph = build_graph_for(&fixture_root());
    let full = render_markdown(
        &build_manifest(&graph, "/fixture", None),
        DEFAULT_LINE_BUDGET,
    );
    let scoped = render_markdown(
        &build_manifest(&graph, "/fixture", Some("src/**")),
        DEFAULT_LINE_BUDGET,
    );
    assert!(
        scoped.len() < full.len(),
        "scoped {} vs full {}",
        scoped.len(),
        full.len()
    );
}
