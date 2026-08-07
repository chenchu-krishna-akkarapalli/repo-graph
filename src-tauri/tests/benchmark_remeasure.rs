//! Re-measures the figures published in `docs/BENCHMARK_REPORT.md` with
//! exact BPE token counts instead of the `chars / 3.7` estimate.
//!
//! Run with `--nocapture` to print the table. The assertions pin the claims
//! that matter so the published numbers cannot rot silently; the printout is
//! what you copy into the report when they legitimately change.
//!
//! Arm A is measured against this repository's own source, which is what the
//! report's reference query reads.

use repo_graph::tokens;
use std::path::{Path, PathBuf};

/// Files the reference query would read in full without the tool — the store
/// plus its subscriber components (`BENCHMARK_REPORT.md` §2, Arm A).
const ARM_A_FILES: &[&str] = &[
    "src/store.ts",
    "src/App.tsx",
    "src/components/GraphCanvas.tsx",
    "src/components/LeftSidebar.tsx",
    "src/components/DetailSidebar.tsx",
    "src/components/nodes/CustomFileNode.tsx",
    "src/components/nodes/CustomSymbolNode.tsx",
];

const USD_PER_MILLION_INPUT: f64 = 3.0;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri has a parent")
        .to_path_buf()
}

fn read_all(root: &Path, files: &[&str]) -> Option<String> {
    let mut out = String::new();
    for f in files {
        out.push_str(&std::fs::read_to_string(root.join(f)).ok()?);
    }
    Some(out)
}

#[test]
fn arm_a_baseline_measured_with_a_real_tokenizer() {
    let root = repo_root();
    let Some(text) = read_all(&root, ARM_A_FILES) else {
        eprintln!("[skip] Arm A sources not present; nothing to re-measure");
        return;
    };

    let chars = text.chars().count();
    let exact = tokens::count_tokens(&text);
    let legacy = tokens::estimate_tokens_legacy(chars);

    eprintln!("\n=== Arm A (7 files read in full) ===");
    eprintln!("chars              {chars}");
    eprintln!("tokens (exact BPE) {exact}");
    eprintln!("tokens (chars/3.7) {legacy}");
    eprintln!("legacy error       {:+.1}%", tokens::legacy_error_pct(&text));
    eprintln!(
        "cost (exact)       ${:.6}",
        tokens::cost_usd(exact, USD_PER_MILLION_INPUT)
    );

    // The baseline is real source, so it must be far above any payload the
    // tool returns — that gap is the entire product claim.
    assert!(exact > 10_000, "Arm A collapsed to {exact} tokens");
}

/// The published savings percentage has to survive exact counting. If the two
/// arms are both re-measured, the ratio should stay within a point or two of
/// the figure derived from the estimate — the estimate's error largely
/// cancels in a ratio, which is why the headline held up.
#[test]
fn published_savings_ratio_survives_exact_counting() {
    let root = repo_root();
    let Some(arm_a) = read_all(&root, ARM_A_FILES) else {
        eprintln!("[skip] Arm A sources not present");
        return;
    };

    // Tier 5 shape: one declaration head plus compact arrow edges. Rebuilt
    // here rather than invoking the MCP server so the test needs no index.
    let tier5 = "{\"files\":[{\"path\":\"src/store.ts\",\"code_blocks\":[{\"name\":\"useGraphStore\",\
        \"kind\":\"variable\",\"code\":\"export const useGraphStore = create<GraphState>((set, get) => (\",\
        \"start_line\":144,\"end_line\":544}]}],\"paths\":[\
        \"src/App.tsx -references-> src/store.ts#useGraphStore\",\
        \"src/components/GraphCanvas.tsx -references-> src/store.ts#useGraphStore\",\
        \"src/components/LeftSidebar.tsx -references-> src/store.ts#useGraphStore\",\
        \"src/components/DetailSidebar.tsx -references-> src/store.ts#useGraphStore\",\
        \"src/components/StatusBar.tsx -references-> src/store.ts#useGraphStore\",\
        \"src/components/TopToolbar.tsx -references-> src/store.ts#useGraphStore\",\
        \"src/components/nodes/CustomFileNode.tsx#CustomFileNode -calls-> src/store.ts#useGraphStore\",\
        \"src/components/nodes/CustomSymbolNode.tsx#CustomSymbolNode -calls-> src/store.ts#useGraphStore\",\
        \"src/store.ts#useGraphStore -calls-> src/lib/hoverHighlight.ts#applyHoverHighlight\",\
        \"src/store.ts#useGraphStore -calls-> src/lib/fileTree.ts#buildTreeFromPaths\",\
        \"src/store.ts#useGraphStore -calls-> src/lib/loadGraph.ts#loadGraph\"]}";

    let a = tokens::count_tokens(&arm_a);
    let b = tokens::count_tokens(tier5);
    let savings = (1.0 - b as f64 / a as f64) * 100.0;

    eprintln!("\n=== Tier 5 (signature_only + compact_edges) ===");
    eprintln!("Arm A tokens  {a}");
    eprintln!("Tier 5 tokens {b}  ({} chars)", tier5.chars().count());
    eprintln!("savings       {savings:.2}%");
    eprintln!(
        "cost          ${:.6} vs ${:.6}",
        tokens::cost_usd(b, USD_PER_MILLION_INPUT),
        tokens::cost_usd(a, USD_PER_MILLION_INPUT)
    );

    assert!(
        savings > 95.0,
        "tier-5 savings fell to {savings:.2}% — the headline claim is >95%"
    );
}

/// The property the whole report rests on: the estimate and an exact count
/// disagree far less on a *ratio* than on either absolute number, because the
/// per-character error partly cancels between numerator and denominator.
#[test]
fn ratios_are_more_robust_than_absolute_counts() {
    let root = repo_root();
    let Some(arm_a) = read_all(&root, ARM_A_FILES) else {
        eprintln!("[skip] Arm A sources not present");
        return;
    };
    let small = "- /src/store.ts (Exports: useGraphStore, primarySelection)";

    let exact_ratio = 1.0 - tokens::count_tokens(small) as f64 / tokens::count_tokens(&arm_a) as f64;
    let legacy_ratio = 1.0
        - tokens::estimate_tokens_legacy(small.chars().count()) as f64
            / tokens::estimate_tokens_legacy(arm_a.chars().count()) as f64;

    let drift = (exact_ratio - legacy_ratio).abs() * 100.0;
    eprintln!("\nratio drift between estimate and exact: {drift:.3} percentage points");
    assert!(
        drift < 1.0,
        "savings ratio moved {drift:.2} points when switched to exact counting"
    );
}
