//! Token counting for the agent-facing payloads.
//!
//! Every savings figure this project publishes is a ratio of token counts, so
//! how those counts are produced is part of the claim. This used to be
//! `chars / 3.7` — a single constant applied to both arms of every
//! comparison. That is defensible for a ratio (the error partly cancels) and
//! indefensible for an absolute number: real code tokenizes far worse than
//! prose, because identifiers, punctuation runs and indentation all fragment.
//!
//! ## Why BPE, and which one
//!
//! Anthropic does not ship a local tokenizer, so an exact Claude count is not
//! obtainable offline — and this tool is offline-first by design (see
//! `CLAUDE.md`). `o200k_base` is used as an explicit, documented proxy: it is
//! the modern large-vocabulary BPE, it is embedded in `tiktoken-rs` so nothing
//! is fetched at runtime, and it is deterministic across machines.
//!
//! Treat the output as "tokens, measured with a real BPE" — not as a byte-
//! exact Claude count. It is materially closer to the truth than a fixed
//! chars-per-token divisor, and it is reproducible, which the estimate also
//! was not across content types.

use tiktoken_rs::{o200k_base, CoreBPE};
use std::sync::OnceLock;

/// Legacy estimator, kept so the historical figures in
/// `docs/BENCHMARK_REPORT.md` remain reproducible and comparable.
///
/// Not used for any new measurement.
pub fn estimate_tokens_legacy(chars: usize) -> usize {
    (chars as f64 / 3.7).round() as usize
}

/// The encoder is built once — constructing it parses a ~2 MB merge table,
/// which is far more expensive than encoding any manifest.
fn bpe() -> Option<&'static CoreBPE> {
    static BPE: OnceLock<Option<CoreBPE>> = OnceLock::new();
    BPE.get_or_init(|| o200k_base().ok()).as_ref()
}

/// Exact BPE token count for `text`.
///
/// Falls back to the legacy estimate only if the encoder cannot be built,
/// which would mean the embedded table failed to load — counting something is
/// better than a hard failure in a reporting path.
pub fn count_tokens(text: &str) -> usize {
    match bpe() {
        Some(enc) => enc.encode_ordinary(text).len(),
        None => estimate_tokens_legacy(text.chars().count()),
    }
}

/// Input cost in USD at `per_million` dollars per million input tokens.
pub fn cost_usd(tokens: usize, per_million: f64) -> f64 {
    tokens as f64 * per_million / 1_000_000.0
}

/// How far the old constant-divisor estimate was from a real count, as a
/// signed percentage of the true value.
///
/// Positive means the estimate was too high. Reported alongside re-measured
/// figures so a reader can see which direction the old numbers erred.
pub fn legacy_error_pct(text: &str) -> f64 {
    let actual = count_tokens(text);
    if actual == 0 {
        return 0.0;
    }
    let estimated = estimate_tokens_legacy(text.chars().count());
    (estimated as f64 - actual as f64) / actual as f64 * 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_are_deterministic_and_nonzero() {
        let text = "export const useGraphStore = create<GraphState>((set, get) => ({";
        let a = count_tokens(text);
        assert!(a > 0);
        assert_eq!(a, count_tokens(text), "encoding must be reproducible");
        assert_eq!(count_tokens(""), 0);
    }

    /// Dense code tokenizes *below* 3.7 chars/token, so the old divisor read
    /// low there. Measured at 3.38 chars/token for this sample (−9%).
    #[test]
    fn the_legacy_estimate_reads_low_on_dense_code() {
        let code = "\
pub fn collapse_endpoints(raw: Vec<(String, String, String)>, anchor_file: &str) -> Vec<Endpoint> {
    let mut by_file: BTreeMap<String, (Vec<String>, bool)> = BTreeMap::new();
}";
        assert!(
            legacy_error_pct(code) < 0.0,
            "expected the estimate to read low for dense code, got {:+.1}%",
            legacy_error_pct(code)
        );
    }

    /// And *high* on prose — English runs about 5 chars/token, so the same
    /// divisor overstates it by more than a third.
    ///
    /// This is the real indictment of a single constant: it is wrong in
    /// opposite directions depending on content, so its error cannot be
    /// reasoned about, only measured. It happens to be close on the payloads
    /// this project actually ships (see `docs/BENCHMARK_REPORT.md` §3), which
    /// is luck rather than method.
    #[test]
    fn the_legacy_estimate_reads_high_on_prose() {
        let prose = "You are looking at a compressed map. Do NOT guess file contents. \
                     Use the explore tool for symbol slices with call graphs before editing.";
        assert!(
            legacy_error_pct(prose) > 20.0,
            "expected the estimate to read high for prose, got {:+.1}%",
            legacy_error_pct(prose)
        );
    }

    /// The payload shapes this project actually emits — manifest lines and
    /// compact call-graph edges — happen to sit near 3.7 chars/token, which
    /// is why the published ratios survived the switch to exact counting.
    #[test]
    fn shipped_payload_shapes_land_close_to_the_old_divisor() {
        for sample in [
            "- /src/components/Button.tsx (Exports: Button) (Depends on: /src/hooks/useTheme.ts)",
            "src/components/nodes/CustomFileNode.tsx#CustomFileNode -calls-> src/store.ts#useGraphStore",
        ] {
            assert!(
                legacy_error_pct(sample).abs() < 10.0,
                "{sample:?} drifted {:+.1}%",
                legacy_error_pct(sample)
            );
        }
    }

    #[test]
    fn cost_scales_linearly_with_tokens() {
        assert!((cost_usd(1_000_000, 3.0) - 3.0).abs() < 1e-9);
        assert!((cost_usd(398, 3.0) - 0.001194).abs() < 1e-9);
        assert_eq!(cost_usd(0, 3.0), 0.0);
    }
}
