//! Autonomous agent scaffolding (Playbook §28).
//!
//! When a workspace is indexed, drop a `.myrepograph-agent/` directory at its
//! root holding context-engineering defaults — rules, duties, memory
//! scratchpads, and lifecycle hooks — so any agent that later connects to the
//! project inherits the CEPA discovery sequence instead of grepping blind.
//!
//! Three constraints, because this writes into *the user's* repository:
//!
//! 1. **Never overwrite.** Every file is created with `create_new`, so a
//!    customized `RULES.md` survives the next index. The directory-level check
//!    is the fast path; the per-file check is what makes it actually safe.
//! 2. **Never fatal.** A scaffolding failure (read-only checkout, permissions,
//!    a file where a directory should be) must not fail the index that asked
//!    for it. Callers log and continue.
//! 3. **Opt-out.** `REPOGRAPH_NO_SCAFFOLD=1` disables it entirely. Creating
//!    twenty files in someone's repository is a side effect they get to
//!    decline.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Directory name created at the workspace root.
pub const SCAFFOLD_DIR: &str = ".myrepograph-agent";

/// Set to `1`/`true` to suppress scaffolding entirely.
pub const OPT_OUT_ENV: &str = "REPOGRAPH_NO_SCAFFOLD";

/// Context architecture version stamped into the generated `agent.yaml`.
pub const ARCHITECTURE: &str = "CEPA-v1.4";

/// Directories created under `.myrepograph-agent/`, including the empty ones —
/// they are part of the contract even before anything fills them.
const DIRS: &[&str] = &[
    "agents/fact-checker",
    "compliance",
    "config",
    "examples",
    "hooks",
    "knowledge",
    "memory/runtime",
    "skills/code-review",
    "skills/codebase-design",
    "skills/improve-codebase-architecture",
    "tools",
    "workflows",
];

/// Serializes tests that mutate `REPOGRAPH_NO_SCAFFOLD`. Cargo runs a target's
/// tests in parallel threads of one process, so an env var set by one test is
/// visible to every other — without this lock the opt-out test can suppress
/// scaffolding underneath an unrelated test and flake it.
#[cfg(test)]
pub(crate) static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// True when the user has opted out via the environment.
fn opted_out() -> bool {
    matches!(
        std::env::var(OPT_OUT_ENV).unwrap_or_default().to_lowercase().as_str(),
        "1" | "true" | "yes"
    )
}

/// Create `.myrepograph-agent/` and any missing skill files/subdirectories at `root`.
///
/// Idempotent: uses `create_new(true)` for each file so user edits and existing files
/// are never overwritten, but newly added skills or missing files are automatically provisioned.
pub fn ensure_agent_scaffold(root: &Path) -> io::Result<()> {
    if opted_out() {
        return Ok(());
    }
    let base = root.join(SCAFFOLD_DIR);
    write_scaffold(&base, workspace_name(root))
}

/// Materialize the tree. Split from [`ensure_agent_scaffold`] so tests can
/// exercise the writer without depending on the existence short-circuit.
fn write_scaffold(base: &Path, workspace: &str) -> io::Result<()> {
    fs::create_dir_all(base)?;
    for dir in DIRS {
        fs::create_dir_all(base.join(dir))?;
    }
    for (relative, contents) in templates(workspace) {
        create_if_absent(&base.join(relative), &contents)?;
    }
    Ok(())
}

/// Write `contents` only when `path` does not exist.
///
/// `create_new` makes this atomic against a concurrent writer — two indexes
/// racing on the same workspace cannot truncate each other's output — and it
/// is the mechanism that protects user edits.
fn create_if_absent(path: &Path, contents: &str) -> io::Result<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    match fs::OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut file) => {
            use std::io::Write;
            file.write_all(contents.as_bytes())
        }
        // Lost the race; the other writer's content stands.
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(e),
    }
}

/// Last path component, falling back to `workspace` for odd roots (`/`, `C:\`).
fn workspace_name(root: &Path) -> &str {
    root.file_name()
        .and_then(|n| n.to_str())
        .filter(|n| !n.is_empty())
        .unwrap_or("workspace")
}

fn templates(workspace: &str) -> Vec<(PathBuf, String)> {
    vec![
        (PathBuf::from("agent.yaml"), root_agent_yaml(workspace)),
        (PathBuf::from("AGENTS.md"), agents_md().to_string()),
        (PathBuf::from("RULES.md"), rules_md().to_string()),
        (PathBuf::from("SOUL.md"), soul_md().to_string()),
        (PathBuf::from("DUTIES.md"), duties_md().to_string()),
        (
            PathBuf::from("agents/fact-checker/agent.yaml"),
            fact_checker_yaml().to_string(),
        ),
        (
            PathBuf::from("agents/fact-checker/DUTIES.md"),
            fact_checker_duties().to_string(),
        ),
        (
            PathBuf::from("agents/fact-checker/SOUL.md"),
            fact_checker_soul().to_string(),
        ),
        (PathBuf::from("hooks/bootstrap.md"), bootstrap_md().to_string()),
        (PathBuf::from("hooks/teardown.md"), teardown_md().to_string()),
        (
            PathBuf::from("memory/runtime/context.md"),
            runtime_context_md().to_string(),
        ),
        (
            PathBuf::from("memory/runtime/dailylog.md"),
            runtime_dailylog_md().to_string(),
        ),
        (
            PathBuf::from("skills/code-review/SKILL.md"),
            code_review_skill().to_string(),
        ),
        (
            PathBuf::from("skills/code-review/review.sh"),
            code_review_script().to_string(),
        ),
        (
            PathBuf::from("skills/codebase-design/SKILL.md"),
            codebase_design_skill().to_string(),
        ),
        (
            PathBuf::from("skills/improve-codebase-architecture/SKILL.md"),
            improve_codebase_architecture_skill().to_string(),
        ),
    ]
}

fn root_agent_yaml(workspace: &str) -> String {
    format!(
        "# Repo Graph agent workspace descriptor.\n\
         # Generated once; edit freely — re-indexing never overwrites this file.\n\
         name: {workspace}\n\
         version: 0.1.0\n\
         architecture: \"{ARCHITECTURE}\"\n\
         \n\
         context_stack:\n\
         \x20 instructions: RULES.md\n\
         \x20 duties: DUTIES.md\n\
         \x20 persona: SOUL.md\n\
         \x20 short_term_memory: memory/runtime/context.md\n\
         \x20 long_term_memory: knowledge/\n\
         \n\
         discovery_sequence:\n\
         \x20 - repograph_status\n\
         \x20 - repograph_files\n\
         \x20 - repograph_search\n\
         \x20 - repograph_explore  # signature_only: true\n\
         \n\
         agents:\n\
         \x20 - agents/fact-checker\n\
         skills:\n\
         \x20 - skills/code-review\n\
         \x20 - skills/codebase-design\n\
         \x20 - skills/improve-codebase-architecture\n"
    )
}

fn agents_md() -> &'static str {
    "# Agents\n\
     \n\
     Agent roles for this workspace and the context each one is allowed to load.\n\
     \n\
     ## Roles\n\
     \n\
     | Agent | Purpose | Context budget |\n\
     | :--- | :--- | :--- |\n\
     | *coordinator* (default) | Plans work, delegates, never edits blind | Manifest + signatures |\n\
     | `agents/fact-checker` | Verifies claims against the indexed graph | Signatures only |\n\
     \n\
     ## The seven-piece context stack\n\
     \n\
     Partition what you put in front of the model; when a turn goes wrong you\n\
     want to know which layer was wrong.\n\
     \n\
     1. **Instructions** — `RULES.md` (guardrails)\n\
     2. **User Input** — the task, one paragraph\n\
     3. **Retrieved Facts** — verbatim `repograph_explore` output, never paraphrased\n\
     4. **Tools** — the `repograph_*` schemas\n\
     5. **Short-term Notes** — `memory/runtime/context.md`, kept *outside* the window\n\
     6. **Long-term Memory** — `knowledge/`, loaded selectively\n\
     7. **Output Format** — the schema you require back\n\
     \n\
     Only layer 3 scales with repository size. Every token worth saving is saved\n\
     there, which is what the discovery sequence in `RULES.md` optimizes.\n"
}

fn rules_md() -> &'static str {
    "# Master Context Engineering & Behavioral Token Rules\n\
     \n\
     This directory serves as the centralized source of truth for AI coding agents operating within the Repo Graph repository.\n\
     \n\
     <!-- BEGIN REPO-GRAPH-SYNC-POLICY v1.4 -->\n\
     # MCP Continuous Sync & Strict Token Cost-Cutting Policy\n\
     \n\
     1. **Persistent Session Priority**:\n\
        - Always prioritize calling `repo-graph` MCP tools (`repograph_skeleton`, `repograph_trace`, `repograph_explore`, `repograph_files`, `repograph_node`, `repograph_impact`, `repograph_edit`, `repograph_write`, `repograph_delete`, `repograph_batch_edit`, `repograph_edit_symbol`, `repograph_status`) instead of brute-force directory scans or native shell commands.\n\
     2. **Pre-Flight Check & Context Restoration (Start of Turn)**:\n\
        - On the first turn of any task or session, call `repograph_status` to verify connection and confirm `Sync State` is `Synced`.\n\
        - Read `.myrepograph-agent/memory/runtime/context.md` to restore working context, active task goals, and previously resolved symbol references without wasting tokens re-indexing.\n\
     3. **AST Ghost Skeletons (No Full-File Dumps)**:\n\
        - Use `repograph_skeleton(path=\"...\")` for complete structural overviews (95%+ token reduction) before inspecting or reading implementations.\n\
        - For targeted blocks, use `repograph_node(path=\"...\", start_line=N, end_line=M, with_line_numbers=true)`. Prohibit reading entire files (>50 lines).\n\
     4. **Multi-Hop Execution Traces & Scoped Discovery**:\n\
        - Use `repograph_trace(entrypoint=\"...\", depth=3)` for end-to-end execution pipelines (80%+ token reduction) across routes, handlers, and databases.\n\
        - Use `repograph_files(scope=\"src/**\")` to bound file discovery.\n\
        - Ingest signatures only (`signature_only: true`) via `repograph_explore` during architecture exploration.\n\
     5. **Strict Bounded Searches**:\n\
        - Bound all `repograph_search` queries with `limit: 10` or `exact_symbol_only: true` to prevent oversized result payloads from polluting the context window.\n\
     6. **Closed-Loop MCP Mutation & Impact Analysis**:\n\
        - Before modifying central interfaces, run `repograph_impact(symbol=\"<name>\")` or `repograph_callers` to evaluate downstream ripple effects.\n\
        - Use `repograph_edit`, `repograph_batch_edit`, and `repograph_edit_symbol` for atomic refactors with instant AST re-indexing and rollback safety.\n\
     7. **Turn Completion & Zero-Token Memory Offloading (End of Turn)**:\n\
        - Update checklist and active goals in `.myrepograph-agent/memory/runtime/context.md` to offload working memory outside the model context window.\n\
        - Append a session summary entry into `.myrepograph-agent/memory/runtime/dailylog.md` detailing what changed, edge diffs, verification commands executed, and any pending items.\n\
     <!-- END REPO-GRAPH-SYNC-POLICY v1.4 -->\n\
     \n\
     # CONTEXT_ENGINEERING_PROMPT_ARCHITECTURE_MARKER\n\
     When answering architecture questions or researching dependencies, follow the Context Engineering Prompt Architecture (CEPA). Always execute the 3-step discovery sequence (Orient -> Target -> Explore Leanly) and default to `signature_only: true` on `repograph_explore` calls to minimize token ingestion.\n\
     \n\
     1. **Orient** — `repograph_status`, then `repograph_files(scope)` for the area in question.\n\
     2. **Target** — `repograph_search(query)` to isolate candidate identifiers.\n\
     3. **Explore Leanly** — `repograph_explore(symbols, signature_only: true)` for declarations plus the call graph.\n\
     \n\
     Exception — code writes: before modifying, refactoring, or debugging the behaviour of a symbol, re-call `repograph_explore` WITHOUT `signature_only` to load the implementation body. Never edit code from a signature alone.\n\
     # END_CONTEXT_ENGINEERING_PROMPT_ARCHITECTURE_MARKER\n"
}

fn soul_md() -> &'static str {
    "# SOUL.md — Operating Principles\n\
     \n\
     ## Minimal context, not minimal effort\n\
     Load the least context that can answer the question — then do the whole job.\n\
     Token frugality is never a reason to skip verification or to guess.\n\
     \n\
     ## Precision over volume\n\
     A signature and a call graph beat three files skimmed. Retrieve narrowly,\n\
     read what you retrieved, and say which parts you did not read.\n\
     \n\
     ## Static safety\n\
     Analysis is read-only: never execute project code, run build scripts, or\n\
     `eval` anything to learn what it does. Parse it or ask.\n\
     \n\
     ## Report honestly\n\
     If a check was skipped, say so. If a target was missed, give the number you\n\
     actually measured. An unverified claim costs more than an unfinished task.\n"
}

fn duties_md() -> &'static str {
    "# DUTIES.md — Execution Responsibilities\n\
     \n\
     ## Every task\n\
     1. Restate the goal in one line; note anything ambiguous before starting.\n\
     2. Orient with `repograph_files`; target with `repograph_search`.\n\
     3. Explore with `signature_only: true`; expand to bodies only to edit.\n\
     4. Record the checklist in `memory/runtime/context.md` as you go.\n\
     5. Verify — build, test, or measure — and report the real output.\n\
     6. Close out in `memory/runtime/dailylog.md`.\n\
     \n\
     ## Definition of done\n\
     - The change compiles and its tests pass, with the output shown.\n\
     - Claims are backed by a command that was actually run.\n\
     - Anything left undone is stated plainly rather than implied complete.\n"
}

fn fact_checker_yaml() -> &'static str {
    "name: fact-checker\n\
     role: verifier\n\
     architecture: \"CEPA-v1.1\"\n\
     context_budget: signatures_only\n\
     tools:\n\
     \x20 - repograph_search\n\
     \x20 - repograph_explore\n\
     \x20 - repograph_callers\n\
     \x20 - repograph_impact\n\
     writes: false\n"
}

fn fact_checker_duties() -> &'static str {
    "# DUTIES — fact-checker\n\
     \n\
     1. Take each factual claim in the work under review, one at a time.\n\
     2. Resolve it against the graph (`repograph_explore`, `repograph_callers`).\n\
     3. Mark it **confirmed**, **contradicted**, or **unverifiable from the index**.\n\
     4. For contradictions, quote the retrieved evidence and its file path.\n\
     \n\
     Never rewrite the work under review — report findings and let the author fix them.\n"
}

fn fact_checker_soul() -> &'static str {
    "# SOUL — fact-checker\n\
     \n\
     Absence of evidence is not evidence of absence: an empty `repograph_explore`\n\
     result means the query missed, not that the symbol is absent. Refine twice\n\
     before reporting \"not found\".\n\
     \n\
     Verify the claim that was made, not a weaker one that is easier to confirm.\n\
     Report \"unverifiable from the index\" when that is the truth — a confident\n\
     wrong verdict is worse than an honest gap.\n"
}

fn bootstrap_md() -> &'static str {
    "# Bootstrap Hook\n\
     \n\
     Run at the start of an agent session, before the first substantive reply.\n\
     \n\
     1. `repograph_status` — confirm the index is live and the active root matches\n\
     \x20  the directory you believe you are in. Halt on mismatch.\n\
     2. Load `RULES.md`, `DUTIES.md`, `SOUL.md` into the instruction layer.\n\
     3. Read `memory/runtime/context.md` for unfinished work from the last session.\n\
     4. `repograph_files(scope)` for the area named in the task — not the whole repo.\n\
     \n\
     Do not read source files in this phase. Bootstrap establishes *where* things\n\
     are; retrieval of *what* they contain belongs to the task itself.\n"
}

fn teardown_md() -> &'static str {
    "# Teardown Hook\n\
     \n\
     Run before ending a session.\n\
     \n\
     1. Mark completed items in `memory/runtime/context.md`; leave unfinished ones\n\
     \x20  open with a note on where they stand.\n\
     2. Append a close-out to `memory/runtime/dailylog.md`: what changed, what was\n\
     \x20  verified and how, and what is still outstanding.\n\
     3. Record surprises — a wrong assumption caught late is the highest-value\n\
     \x20  thing to write down.\n\
     4. Leave the workspace building and its tests passing, or say plainly that\n\
     \x20  you did not.\n"
}

fn runtime_context_md() -> &'static str {
    "# Short-Term Context — Active Checklists\n\
     \n\
     Working state for the current task. Kept here rather than in the context\n\
     window so long sessions do not carry their own history as ballast.\n\
     \n\
     ## Current task\n\
     \n\
     - [ ] _(no active task)_\n\
     \n\
     ## Open questions\n\
     \n\
     _none yet_\n"
}

fn runtime_dailylog_md() -> &'static str {
    "# Daily Log\n\
     \n\
     Append-only session close-outs. One entry per session: what changed, how it\n\
     was verified, and what was left undone.\n\
     \n\
     <!-- Newest entries at the bottom. -->\n"
}

fn code_review_skill() -> &'static str {
    "---\n\
     name: code-review\n\
     description: Review changed code for correctness, then verify the findings before reporting.\n\
     ---\n\
     \n\
     # Code Review\n\
     \n\
     ## 1. Scope the diff\n\
     Review what changed, plus what the change can break. Use\n\
     `repograph_impact(symbol=\"<name>\")` or `repograph_impact(path=\"<path>\")` for the blast radius.\n\
     \n\
     ## 2. Read what you are judging\n\
     Signatures are enough to map the call graph; they are not enough to review\n\
     logic. Load full bodies (`signature_only` omitted) for every symbol you\n\
     intend to comment on.\n\
     \n\
     ## 3. Verify before reporting\n\
     For each finding, construct the concrete input or state that triggers it. A\n\
     finding you cannot make fail is a hypothesis — either verify it or drop it.\n\
     \n\
     ## 4. Report\n\
     Most severe first, each with file, line, the failure scenario, and the fix.\n\
     Say explicitly when nothing was found; padding a review with style nits\n\
     buries the real defects.\n\
     \n\
     Run `./review.sh` from this directory for the mechanical checks.\n"
}

fn code_review_script() -> &'static str {
    "#!/usr/bin/env bash\n\
     # Mechanical pre-review checks. Fails loudly; the agent reviews what is left.\n\
     set -euo pipefail\n\
     \n\
     cd \"$(git rev-parse --show-toplevel 2>/dev/null || echo .)\"\n\
     \n\
     echo \"== changed files ==\"\n\
     git diff --name-only HEAD 2>/dev/null || echo \"(not a git repository)\"\n\
     \n\
     if [ -f package.json ]; then\n\
     \x20 echo \"== frontend build ==\"\n\
     \x20 npm run build\n\
     fi\n\
     \n\
     if [ -f src-tauri/Cargo.toml ]; then\n\
     \x20 echo \"== backend tests ==\"\n\
     \x20 (cd src-tauri && cargo test)\n\
     fi\n\
     \n\
     echo \"== mechanical checks passed; review the logic by hand ==\"\n"
}

fn codebase_design_skill() -> &'static str {
    "---\n\
     name: codebase-design\n\
     description: Architecture design vocabulary and principles for deepening modules, designing seams, and cutting complexity.\n\
     ---\n\
     \n\
     # Codebase Design Vocabulary & Principles\n\
     \n\
     ## Core Architecture Vocabulary\n\
     - **Module**: A cohesive unit of code hiding an implementation behind a well-defined interface. (Never call it a component or service).\n\
     - **Interface**: The public surface area exposed by a module to callers.\n\
     - **Depth (Deep Module)**: An interface that is simple and narrow while the implementation handles significant complexity.\n\
     - **Shallow Module**: An interface as complex as its implementation. Collapse or delete.\n\
     - **Seam**: A place where you can alter program behavior without changing calling code.\n\
     - **Adapter**: A concrete implementation connecting a module to a runtime dependency.\n\
     - **Leverage**: Functionality achieved per unit of interface learned.\n\
     - **Locality**: Keeping related behaviors physically close.\n\
     \n\
     ## Key Principles\n\
     - **The Deletion Test**: If deleting a module concentrates complexity, it was deep; if complexity just moves, it was shallow.\n\
     - **Seams & Adapters Law**: One adapter = hypothetical seam; two adapters = real seam.\n\
     - **The Interface is the Test Surface**: Tests target public module interfaces, not private internals.\n"
}

fn improve_codebase_architecture_skill() -> &'static str {
    "---\n\
     name: improve-codebase-architecture\n\
     description: Scan a codebase for deepening opportunities, present them as a visual HTML report, and execute architectural refactoring.\n\
     ---\n\
     \n\
     # Improve Codebase Architecture\n\
     \n\
     ## Process\n\
     1. **Explore Hotspots**: Identify high-churn areas via `git log` and `repograph_impact`.\n\
     2. **Visual HTML Report**: Generate self-contained before/after diagrams using CDN Tailwind and Mermaid in `%TEMP%`.\n\
     3. **Grilling Loop**: Review decisions, update ADRs in `docs/adr/`, and refactor via `repograph_batch_edit`.\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Isolated temp dir; removed and recreated so reruns start clean.
    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("repograph_scaffold_{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn creates_the_full_documented_tree() {
        let root = temp_root("full_tree");
        ensure_agent_scaffold(&root).unwrap();
        let base = root.join(SCAFFOLD_DIR);

        for dir in DIRS {
            assert!(base.join(dir).is_dir(), "missing directory {dir}");
        }
        for file in [
            "agent.yaml",
            "AGENTS.md",
            "RULES.md",
            "SOUL.md",
            "DUTIES.md",
            "agents/fact-checker/agent.yaml",
            "agents/fact-checker/DUTIES.md",
            "agents/fact-checker/SOUL.md",
            "hooks/bootstrap.md",
            "hooks/teardown.md",
            "memory/runtime/context.md",
            "memory/runtime/dailylog.md",
            "skills/code-review/SKILL.md",
            "skills/code-review/review.sh",
            "skills/codebase-design/SKILL.md",
            "skills/improve-codebase-architecture/SKILL.md",
        ] {
            let p = base.join(file);
            assert!(p.is_file(), "missing file {file}");
            assert!(
                fs::metadata(&p).unwrap().len() > 0,
                "{file} was created empty"
            );
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn generated_content_carries_the_context_architecture() {
        let root = temp_root("content");
        ensure_agent_scaffold(&root).unwrap();
        let base = root.join(SCAFFOLD_DIR);

        let yaml = fs::read_to_string(base.join("agent.yaml")).unwrap();
        assert!(yaml.contains("architecture: \"CEPA-v1.4\""), "got {yaml}");
        // The workspace name is derived from the directory, not hardcoded.
        assert!(
            yaml.contains("name: repograph_scaffold_content"),
            "workspace name missing from {yaml}"
        );

        let rules = fs::read_to_string(base.join("RULES.md")).unwrap();
        assert!(rules.contains("# CONTEXT_ENGINEERING_PROMPT_ARCHITECTURE_MARKER"));
        assert!(rules.contains("# END_CONTEXT_ENGINEERING_PROMPT_ARCHITECTURE_MARKER"));
        assert!(rules.contains("signature_only"));
        // The write-time exception has to travel with the marker; without it the
        // rule reads as "always use signatures", including while editing.
        assert!(
            rules.contains("Never edit code from a signature alone."),
            "marker block lost its code-write exception"
        );

        let script = fs::read_to_string(base.join("skills/code-review/review.sh")).unwrap();
        assert!(script.starts_with("#!/usr/bin/env bash"));

        let _ = fs::remove_dir_all(&root);
    }

    /// The property that matters most: a second index must not clobber the
    /// user's edits.
    #[test]
    fn is_idempotent_and_never_overwrites_user_edits() {
        let root = temp_root("idempotent");
        ensure_agent_scaffold(&root).unwrap();
        let rules = root.join(SCAFFOLD_DIR).join("RULES.md");

        fs::write(&rules, "# my own rules\n").unwrap();
        let extra = root.join(SCAFFOLD_DIR).join("knowledge/notes.md");
        fs::write(&extra, "keep me").unwrap();

        // Re-running is a no-op: the directory exists, so nothing is rewritten.
        ensure_agent_scaffold(&root).unwrap();
        assert_eq!(fs::read_to_string(&rules).unwrap(), "# my own rules\n");
        assert_eq!(fs::read_to_string(&extra).unwrap(), "keep me");

        // Even forced past the existence check, per-file `create_new` protects
        // the edit — this is what makes the guarantee real rather than incidental.
        write_scaffold(&root.join(SCAFFOLD_DIR), "x").unwrap();
        assert_eq!(fs::read_to_string(&rules).unwrap(), "# my own rules\n");
        assert_eq!(fs::read_to_string(&extra).unwrap(), "keep me");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_deleted_file_is_restored_without_touching_its_siblings() {
        let root = temp_root("restore");
        let base = root.join(SCAFFOLD_DIR);
        ensure_agent_scaffold(&root).unwrap();

        fs::write(base.join("SOUL.md"), "custom soul").unwrap();
        fs::remove_file(base.join("DUTIES.md")).unwrap();

        ensure_agent_scaffold(&root).unwrap();
        assert!(base.join("DUTIES.md").is_file(), "deleted file not restored");
        assert_eq!(fs::read_to_string(base.join("SOUL.md")).unwrap(), "custom soul");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn opt_out_env_suppresses_scaffolding() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let root = temp_root("opt_out");
        // SAFETY: the lock keeps sibling tests out of this window, and the var
        // is removed before any assertion runs.
        unsafe { std::env::set_var(OPT_OUT_ENV, "1") };
        let result = ensure_agent_scaffold(&root);
        unsafe { std::env::remove_var(OPT_OUT_ENV) };

        result.unwrap();
        assert!(
            !root.join(SCAFFOLD_DIR).exists(),
            "scaffold created despite {OPT_OUT_ENV}=1"
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// The scaffold ships an executable script into user repositories; a
    /// syntax error in generated content would only surface when someone ran
    /// it. Skipped where `bash` is unavailable rather than failing there.
    #[test]
    fn generated_review_script_is_valid_bash() {
        let root = temp_root("bash_syntax");
        ensure_agent_scaffold(&root).unwrap();
        let script = root.join(SCAFFOLD_DIR).join("skills/code-review/review.sh");

        // Probe first: on Windows `bash` may resolve to a WSL relay that spawns
        // successfully and then fails to exec, which is not a syntax verdict.
        let usable = std::process::Command::new("bash")
            .arg("-c")
            .arg("exit 0")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !usable {
            eprintln!("bash unavailable; skipping syntax check");
            let _ = fs::remove_dir_all(&root);
            return;
        }

        let out = std::process::Command::new("bash")
            .arg("-n")
            .arg(&script)
            .output()
            .expect("bash probe succeeded but -n failed to run");
        assert!(
            out.status.success(),
            "bash -n rejected the generated script:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn odd_roots_still_produce_a_valid_workspace_name() {
        assert_eq!(workspace_name(Path::new("/tmp/my-repo")), "my-repo");
        assert_eq!(workspace_name(Path::new("/")), "workspace");
    }
}
