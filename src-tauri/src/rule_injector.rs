//! Idempotent Workspace Rule Injector & Policy Merger.
//!
//! Automatically ensures that target workspace rule files (`GEMINI.md` / `AGENTS.md`)
//! contain the "Sync-First, Graph-First" MCP policy wrapped inside explicit delimited markers.
//! Preserves 100% of existing user comments and documentation without duplicate text.

use std::fs;
use std::path::{Path, PathBuf};

pub const POLICY_VERSION: &str = "v1.4";
pub const BEGIN_MARKER: &str = "<!-- BEGIN REPO-GRAPH-SYNC-POLICY v1.4 -->";
pub const END_MARKER: &str = "<!-- END REPO-GRAPH-SYNC-POLICY v1.4 -->";

pub const POLICY_BLOCK: &str = r#"<!-- BEGIN REPO-GRAPH-SYNC-POLICY v1.4 -->
# MCP Continuous Sync & Strict Token Cost-Cutting Policy

1. **Persistent Session Priority**:
   - Always prioritize calling `repo-graph` MCP tools (`repograph_skeleton`, `repograph_trace`, `repograph_explore`, `repograph_files`, `repograph_node`, `repograph_impact`, `repograph_edit`, `repograph_write`, `repograph_delete`, `repograph_batch_edit`, `repograph_edit_symbol`, `repograph_status`) instead of brute-force directory scans or native shell commands.
2. **Pre-Flight Check & Context Restoration (Start of Turn)**:
   - On the first turn of any task or session, call `repograph_status` to verify connection and confirm `Sync State` is `Synced`.
   - Read `.myrepograph-agent/memory/runtime/context.md` to restore working context, active task goals, and previously resolved symbol references without wasting tokens re-indexing.
3. **AST Ghost Skeletons (No Full-File Dumps)**:
   - Use `repograph_skeleton(path="...")` for complete structural overviews (95%+ token reduction) before inspecting or reading implementations.
   - For targeted blocks, use `repograph_node(path="...", start_line=N, end_line=M, with_line_numbers=true)`. Prohibit reading entire files (>50 lines).
4. **Multi-Hop Execution Traces & Scoped Discovery**:
   - Use `repograph_trace(entrypoint="...", depth=3)` for end-to-end execution pipelines (80%+ token reduction) across routes, handlers, and databases.
   - Use `repograph_files(scope="src/**")` to bound file discovery.
   - Ingest signatures only (`signature_only: true`) via `repograph_explore` during architecture exploration.
5. **Strict Bounded Searches**:
   - Bound all `repograph_search` queries with `limit: 10` or `exact_symbol_only: true` to prevent oversized result payloads from polluting the context window.
6. **Closed-Loop MCP Mutation & Impact Analysis**:
   - Before modifying central interfaces, run `repograph_impact(symbol="<name>")` or `repograph_callers` to evaluate downstream ripple effects.
   - Use `repograph_edit`, `repograph_batch_edit`, and `repograph_edit_symbol` for atomic refactors with instant AST re-indexing and rollback safety.
7. **Turn Completion & Zero-Token Memory Offloading (End of Turn)**:
   - Update checklist and active goals in `.myrepograph-agent/memory/runtime/context.md` to offload working memory outside the model context window.
   - Append a session summary entry into `.myrepograph-agent/memory/runtime/dailylog.md` detailing what changed, edge diffs, verification commands executed, and any pending items.
<!-- END REPO-GRAPH-SYNC-POLICY v1.4 -->"#;

pub const SUPPORTED_RULE_NAMES: &[&str] = &[
    "gemini.md",
    "claude.md",
    "agents.md",
    "chatgpt.md",
    "openai.md",
];

/// Ensures that `root` contains the Repo Graph continuous sync policy in its workspace rules.
///
/// Discovery & Provisioning precedence:
/// 1. Scans directory for any supported rule files (`GEMINI.md`, `CLAUDE.md`, `AGENTS.md`, `CHATGPT.md`,
///    `ChatGPT.md`, etc. case-insensitively).
/// 2. If any exist, updates each existing file non-destructively in place preserving exact filename.
/// 3. If none exist, auto-provisions `GEMINI.md`, `CLAUDE.md`, `AGENTS.md`, and `CHATGPT.md` with `POLICY_BLOCK`.
pub fn ensure_workspace_rules(root: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut targets = Vec::new();

    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            if let Ok(ft) = entry.file_type() {
                if ft.is_file() {
                    if let Some(name) = entry.file_name().to_str() {
                        let lower = name.to_lowercase();
                        if SUPPORTED_RULE_NAMES.contains(&lower.as_str()) {
                            targets.push(entry.path());
                        }
                    }
                }
            }
        }
    }

    if targets.is_empty() {
        targets.push(root.join("GEMINI.md"));
        targets.push(root.join("CLAUDE.md"));
        targets.push(root.join("AGENTS.md"));
        targets.push(root.join("CHATGPT.md"));
    }

    let mut modified = Vec::new();
    for target in targets {
        if update_rule_file(&target)? {
            modified.push(target);
        }
    }

    Ok(modified)
}

/// Updates a single rule file with non-destructive in-place merging.
/// Returns `Ok(true)` if the file was created or modified, `Ok(false)` if it was a NO-OP.
fn update_rule_file(path: &Path) -> Result<bool, std::io::Error> {
    if !path.exists() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, format!("{}\n", POLICY_BLOCK))?;
        return Ok(true);
    }

    let content = fs::read_to_string(path)?;

    // 1. Check if the current exact policy block is already present (Idempotent NO-OP)
    if content.contains(BEGIN_MARKER) && content.contains(END_MARKER) {
        if let (Some(start), Some(end_tag)) = (content.find(BEGIN_MARKER), content.find(END_MARKER)) {
            let end = end_tag + END_MARKER.len();
            if start < end && &content[start..end] == POLICY_BLOCK {
                return Ok(false); // Exact version and content matches, skip
            }
        }
    }

    // 2. Check if an older or variant block marker exists
    let pattern_begin = "<!-- BEGIN REPO-GRAPH-SYNC-POLICY";
    let pattern_end = "<!-- END REPO-GRAPH-SYNC-POLICY";

    if let (Some(start), Some(end_marker_pos)) = (content.find(pattern_begin), content.find(pattern_end)) {
        if let Some(tag_closing) = content[end_marker_pos..].find("-->") {
            let end = end_marker_pos + tag_closing + 3;
            if start < end {
                let mut new_content = String::with_capacity(content.len() + POLICY_BLOCK.len());
                new_content.push_str(&content[..start]);
                new_content.push_str(POLICY_BLOCK);
                new_content.push_str(&content[end..]);
                atomic_write(path, &new_content)?;
                return Ok(true);
            }
        }
    }

    // 3. Delimited block not present: append non-destructively to the end of user content
    let mut new_content = content.trim_end().to_string();
    if !new_content.is_empty() {
        new_content.push_str("\n\n");
    }
    new_content.push_str(POLICY_BLOCK);
    new_content.push('\n');

    atomic_write(path, &new_content)?;
    Ok(true)
}

/// Atomically writes content to `path` using a temporary adjacent file.
fn atomic_write(path: &Path, content: &str) -> Result<(), std::io::Error> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let temp_file = parent.join(format!(
        ".tmp_{}_{}",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("rule"),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));

    fs::write(&temp_file, content)?;
    if let Err(_) = fs::rename(&temp_file, path) {
        // Fallback to direct write if atomic rename is prohibited (e.g. cross-volume)
        let _ = fs::remove_file(&temp_file);
        fs::write(path, content)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_dir(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("repograph_rule_test_{name}_{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_1_brand_new_folder_provisions_all_standard_agent_rules() {
        let dir = create_test_dir("new_folder");
        let modified = ensure_workspace_rules(&dir).unwrap();
        assert_eq!(modified.len(), 4);
        assert!(modified.contains(&dir.join("GEMINI.md")));
        assert!(modified.contains(&dir.join("CLAUDE.md")));
        assert!(modified.contains(&dir.join("AGENTS.md")));
        assert!(modified.contains(&dir.join("CHATGPT.md")));

        for file in &["GEMINI.md", "CLAUDE.md", "AGENTS.md", "CHATGPT.md"] {
            let content = fs::read_to_string(dir.join(file)).unwrap();
            assert!(content.contains(BEGIN_MARKER));
            assert!(content.contains(END_MARKER));
            assert!(content.contains("# MCP Continuous Sync & Strict Token Cost-Cutting Policy"));
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_2_existing_claude_and_chatgpt_files_merged_cleanly() {
        let dir = create_test_dir("existing_claude_chatgpt");
        let claude_user_text = "# Anthropic Claude Code Instructions\n\nCustom prompt instructions.\n";
        let chatgpt_user_text = "# OpenAI ChatGPT Instructions\n\nCustom GPT context.\n";
        fs::write(dir.join("CLAUDE.md"), claude_user_text).unwrap();
        fs::write(dir.join("ChatGPT.md"), chatgpt_user_text).unwrap();

        let modified = ensure_workspace_rules(&dir).unwrap();
        assert_eq!(modified.len(), 2);
        assert!(modified.contains(&dir.join("CLAUDE.md")));
        assert!(modified.contains(&dir.join("ChatGPT.md")));

        let claude_res = fs::read_to_string(dir.join("CLAUDE.md")).unwrap();
        assert!(claude_res.starts_with("# Anthropic Claude Code Instructions\n\nCustom prompt instructions."));
        assert!(claude_res.contains(BEGIN_MARKER));

        let chatgpt_res = fs::read_to_string(dir.join("ChatGPT.md")).unwrap();
        assert!(chatgpt_res.starts_with("# OpenAI ChatGPT Instructions\n\nCustom GPT context."));
        assert!(chatgpt_res.contains(BEGIN_MARKER));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_3_reopening_project_is_idempotent_no_op() {
        let dir = create_test_dir("idempotent");
        let initial_user_content = "# My App Rules\n\nCustom prompt rules.\n";
        fs::write(dir.join("CLAUDE.md"), initial_user_content).unwrap();

        // 1st open
        let mod1 = ensure_workspace_rules(&dir).unwrap();
        assert_eq!(mod1.len(), 1);
        let content1 = fs::read_to_string(dir.join("CLAUDE.md")).unwrap();

        // 2nd open (re-opening project)
        let mod2 = ensure_workspace_rules(&dir).unwrap();
        assert_eq!(mod2.len(), 0); // 0 modified files (NO-OP)
        let content2 = fs::read_to_string(dir.join("CLAUDE.md")).unwrap();

        assert_eq!(content1, content2);
        assert_eq!(content2.matches(BEGIN_MARKER).count(), 1);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_4_updating_policy_version_replaces_in_place_preserving_user_content() {
        let dir = create_test_dir("version_upgrade");
        let old_content = "# Custom Header\n\n<!-- BEGIN REPO-GRAPH-SYNC-POLICY v0.9 -->\nOld Outdated Policy\n<!-- END REPO-GRAPH-SYNC-POLICY v0.9 -->\n\n# Footer Section\n";
        fs::write(dir.join("CHATGPT.md"), old_content).unwrap();

        let modified = ensure_workspace_rules(&dir).unwrap();
        assert_eq!(modified.len(), 1);

        let content = fs::read_to_string(dir.join("CHATGPT.md")).unwrap();
        assert!(content.starts_with("# Custom Header\n\n<!-- BEGIN REPO-GRAPH-SYNC-POLICY v1.4 -->"));
        assert!(content.ends_with("<!-- END REPO-GRAPH-SYNC-POLICY v1.4 -->\n\n# Footer Section\n"));
        assert!(content.contains("# MCP Continuous Sync & Strict Token Cost-Cutting Policy"));
        assert!(!content.contains("Old Outdated Policy"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_5_arbitrary_user_browsed_project_auto_provisions_all_rules_and_skills() {
        let dir = create_test_dir("user_browsed_external_project");
        // Simulate an external project with a simple package.json and src/index.ts
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(dir.join("package.json"), r#"{"name": "my-external-app"}"#).unwrap();
        fs::write(dir.join("src/index.ts"), "export const hello = () => 'world';").unwrap();

        // 1. Ensure rules and scaffold run as they do when a user opens/indexes this folder
        let _ = crate::agent_scaffold::ensure_agent_scaffold(&dir).unwrap();
        let modified_rules = ensure_workspace_rules(&dir).unwrap();
        assert_eq!(modified_rules.len(), 4); // Provisions GEMINI.md, CLAUDE.md, AGENTS.md, CHATGPT.md

        // 2. Verify all agent scaffolding and new skills exist
        let scaffold_base = dir.join(crate::agent_scaffold::SCAFFOLD_DIR);
        assert!(scaffold_base.is_dir());
        assert!(scaffold_base.join("RULES.md").is_file());
        assert!(scaffold_base.join("skills/code-review/SKILL.md").is_file());
        assert!(scaffold_base.join("skills/codebase-design/SKILL.md").is_file());
        assert!(scaffold_base.join("skills/improve-codebase-architecture/SKILL.md").is_file());
        assert!(scaffold_base.join("memory/runtime/context.md").is_file());
        assert!(scaffold_base.join("memory/runtime/dailylog.md").is_file());

        // 3. Verify RULES.md content in the scaffolded project
        let rules_content = fs::read_to_string(scaffold_base.join("RULES.md")).unwrap();
        assert!(rules_content.contains("<!-- BEGIN REPO-GRAPH-SYNC-POLICY v1.4 -->"));
        assert!(rules_content.contains("repograph_skeleton"));
        assert!(rules_content.contains("repograph_trace"));
        assert!(rules_content.contains("repograph_batch_edit"));

        // 4. Verify harness rules in the user project
        for harness in &["GEMINI.md", "CLAUDE.md", "AGENTS.md", "CHATGPT.md"] {
            let h_path = dir.join(harness);
            assert!(h_path.is_file());
            let h_content = fs::read_to_string(h_path).unwrap();
            assert!(h_content.contains("<!-- BEGIN REPO-GRAPH-SYNC-POLICY v1.4 -->"));
            assert!(h_content.contains("# MCP Continuous Sync & Strict Token Cost-Cutting Policy"));
            assert!(h_content.contains("repograph_skeleton"));
        }

        let _ = fs::remove_dir_all(&dir);
    }
}
