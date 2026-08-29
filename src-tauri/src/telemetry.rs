//! Live token metrics and telemetry engine for MCP server and desktop UI.

use std::sync::OnceLock;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tiktoken_rs::{cl100k_base, CoreBPE};

fn bpe() -> Option<&'static CoreBPE> {
    static BPE: OnceLock<Option<CoreBPE>> = OnceLock::new();
    BPE.get_or_init(|| cl100k_base().ok()).as_ref()
}

pub fn count_tokens(text: &str) -> usize {
    match bpe() {
        Some(enc) => enc.encode_ordinary(text).len(),
        None => (text.chars().count() as f64 / 3.7).round() as usize,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallMetrics {
    pub timestamp: DateTime<Utc>,
    pub turn_id: usize,
    pub tool_name: String,
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub raw_equivalent_tokens: usize,
    pub saved_tokens: usize,
    pub compression_ratio: f32,
    pub execution_ms: u128,
    pub turn_total_output: usize,
    pub session_total_output: usize,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveTokenTracker {
    pub current_turn_id: usize,
    pub last_call_timestamp: Option<DateTime<Utc>>,
    pub turn_total_input: usize,
    pub turn_total_output: usize,
    pub turn_total_saved: usize,
    pub session_total_input: usize,
    pub session_total_output: usize,
    pub session_total_saved: usize,
    pub recent_calls: Vec<ToolCallMetrics>,
}

impl Default for LiveTokenTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl LiveTokenTracker {
    pub fn new() -> Self {
        Self {
            current_turn_id: 1,
            last_call_timestamp: None,
            turn_total_input: 0,
            turn_total_output: 0,
            turn_total_saved: 0,
            session_total_input: 0,
            session_total_output: 0,
            session_total_saved: 0,
            recent_calls: Vec::with_capacity(100),
        }
    }

    pub fn record_call(
        &mut self,
        tool_name: &str,
        input_json: &str,
        output_json: &str,
        raw_file_tokens: usize,
        execution_ms: u128,
    ) -> ToolCallMetrics {
        let now = Utc::now();
        if let Some(last_time) = self.last_call_timestamp {
            if (now - last_time).num_milliseconds() > 3000 {
                self.current_turn_id += 1;
                self.turn_total_input = 0;
                self.turn_total_output = 0;
                self.turn_total_saved = 0;
            }
        }
        self.last_call_timestamp = Some(now);

        let in_tokens = count_tokens(input_json);
        let out_tokens = count_tokens(output_json);
        let saved = raw_file_tokens.saturating_sub(out_tokens);
        let compression = if out_tokens > 0 && raw_file_tokens > 0 {
            (raw_file_tokens as f32 / out_tokens as f32).max(1.0)
        } else if out_tokens > 0 {
            1.0
        } else {
            1.0
        };

        self.turn_total_input += in_tokens;
        self.turn_total_output += out_tokens;
        self.turn_total_saved += saved;

        self.session_total_input += in_tokens;
        self.session_total_output += out_tokens;
        self.session_total_saved += saved;

        let warning = if out_tokens > 4000 {
            Some(format!(
                "⚠️ Large payload alert: Output is {out_tokens} tokens (>4,000 threshold). Recommend using signature_only: true or scoping with top_k / limit."
            ))
        } else {
            None
        };

        let metrics = ToolCallMetrics {
            timestamp: now,
            turn_id: self.current_turn_id,
            tool_name: tool_name.to_string(),
            input_tokens: in_tokens,
            output_tokens: out_tokens,
            raw_equivalent_tokens: raw_file_tokens,
            saved_tokens: saved,
            compression_ratio: compression,
            execution_ms,
            turn_total_output: self.turn_total_output,
            session_total_output: self.session_total_output,
            warning,
        };

        if self.recent_calls.len() >= 100 {
            self.recent_calls.remove(0);
        }
        self.recent_calls.push(metrics.clone());
        metrics
    }

    pub fn format_inband_tag(&self, metrics: &ToolCallMetrics) -> String {
        let ratio_str = if metrics.compression_ratio > 1.0 {
            format!("{:.1}x", metrics.compression_ratio)
        } else {
            "1.0x".to_string()
        };

        let mut tag = format!(
            "\n\n[Telemetry: Tool '{}' | Out: {} tokens | Saved: {} tokens ({}) | Turn #{}: {} tokens | Session: {} tokens]",
            metrics.tool_name,
            metrics.output_tokens,
            metrics.saved_tokens,
            ratio_str,
            metrics.turn_id,
            metrics.turn_total_output,
            metrics.session_total_output
        );

        if let Some(ref warn) = metrics.warning {
            tag.push_str(&format!("\n[{warn}]"));
        }

        tag
    }

    pub fn overall_compression_ratio(&self) -> f32 {
        if self.session_total_output > 0 {
            (self.session_total_saved + self.session_total_output) as f32
                / self.session_total_output as f32
        } else {
            1.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bpe_tokenizer_speed_and_accuracy() {
        // Warm up BPE merge table OnceLock
        let _ = count_tokens("warmup");

        let sample = "import { useState, useEffect } from 'react';\nexport const MyComponent = () => null;";
        let start = std::time::Instant::now();
        let tokens = count_tokens(sample);
        let elapsed = start.elapsed();
        assert!(tokens > 0);
        assert!(elapsed.as_millis() < 2, "Warm BPE encoding took too long: {elapsed:?}");
    }

    #[test]
    fn temporal_turn_clustering_and_savings_calculation() {
        let mut tracker = LiveTokenTracker::new();
        assert_eq!(tracker.current_turn_id, 1);

        // Turn 1 Call 1
        let m1 = tracker.record_call("repograph_explore", "{}", "const a = 1;", 500, 12);
        assert_eq!(m1.turn_id, 1);
        assert!(m1.saved_tokens > 400);
        assert!(m1.compression_ratio > 10.0);

        let tag1 = tracker.format_inband_tag(&m1);
        assert!(tag1.contains("[Telemetry: Tool 'repograph_explore'"));
        assert!(tag1.contains("Turn #1:"));

        // Simulate temporal jump (>3 seconds)
        tracker.last_call_timestamp = Some(Utc::now() - chrono::Duration::seconds(4));

        // Turn 2 Call 1
        let m2 = tracker.record_call("repograph_node", "{}", "const b = 2;", 100, 5);
        assert_eq!(m2.turn_id, 2);
        assert_eq!(tracker.current_turn_id, 2);

        let tag2 = tracker.format_inband_tag(&m2);
        assert!(tag2.contains("Turn #2:"));
    }

    #[test]
    fn large_payload_alert_warning() {
        let mut tracker = LiveTokenTracker::new();
        let huge_text = "word ".repeat(5000);
        let m = tracker.record_call("repograph_search", "{}", &huge_text, 10000, 20);
        assert!(m.warning.is_some());
        let tag = tracker.format_inband_tag(&m);
        assert!(tag.contains("Large payload alert"));
        assert!(tag.contains("signature_only: true"));
    }

    #[test]
    fn node_query_telemetry_accuracy() {
        let mut tracker = LiveTokenTracker::new();
        let file_body = "export function sample() {\n    console.log('Hello World');\n}\n".repeat(20);
        let raw_tokens = count_tokens(&file_body);
        let slice = "export function sample() {\n    console.log('Hello World');\n}";
        let metrics = tracker.record_call("repograph_node", "{\"path\":\"src/sample.ts\"}", slice, raw_tokens, 8);

        assert!(metrics.input_tokens > 0, "input_tokens must be > 0");
        assert!(metrics.output_tokens > 0, "output_tokens must be > 0");
        assert!(metrics.saved_tokens > 0, "saved_tokens must be > 0 for sliced node reads");
        assert!(metrics.compression_ratio > 1.0, "compression_ratio must be > 1.0x");
        let tag = tracker.format_inband_tag(&metrics);
        assert!(!tag.contains("Saved: 0 tokens (1.0x)"), "Must not calculate 0 tokens saved");
    }
}
