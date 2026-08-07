//! Per-user state locations.
//!
//! These used to be four literal `C:\Users\<name>\.gemini\antigravity\...`
//! paths. That made the product unusable for anyone else and unusable on
//! macOS/Linux — and, because `active_project.json` supplies the MCP server's
//! confinement root, it put the agent's read sandbox at a fixed, predictable,
//! user-writable location.
//!
//! Resolution order (first hit wins):
//! 1. `REPO_GRAPH_STATE_DIR` — explicit override, used by the test suite.
//! 2. Platform convention: `%APPDATA%\repo-graph` on Windows,
//!    `$XDG_STATE_HOME/repo-graph` else `$HOME/.local/state/repo-graph`.

use std::path::PathBuf;

/// Pure resolution, so it can be tested without mutating process-global
/// environment state (which races across Rust's multithreaded test runner).
///
/// `explicit` is `REPO_GRAPH_STATE_DIR`; `base` is the platform's state root.
fn resolve_state_dir(explicit: Option<&std::ffi::OsStr>, base: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(e) = explicit {
        if !e.is_empty() {
            return Some(PathBuf::from(e));
        }
    }
    base.map(|b| b.join("repo-graph"))
}

fn platform_base() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA").map(PathBuf::from).or_else(|| {
            std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join("AppData/Roaming"))
        })
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
    }
}

/// Directory holding this app's cross-session state. `None` only when the
/// platform's home/config variables are all unset — callers must degrade
/// gracefully rather than falling back to a guessed absolute path.
pub fn state_dir() -> Option<PathBuf> {
    resolve_state_dir(
        std::env::var_os("REPO_GRAPH_STATE_DIR").as_deref(),
        platform_base(),
    )
}

/// Creates [`state_dir`] if needed and returns it.
pub fn ensure_state_dir() -> Option<PathBuf> {
    let dir = state_dir()?;
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// Which repo the desktop app last opened. Read by the MCP server when it is
/// launched without an explicit root — always re-validated before it is
/// trusted as a confinement boundary (see `mcp_server::resolve_mcp_root`).
pub fn active_project_registry() -> Option<PathBuf> {
    state_dir().map(|d| d.join("active_project.json"))
}

/// Recently-opened projects shown on the Project Hub dashboard.
pub fn recent_projects_config() -> Option<PathBuf> {
    state_dir().map(|d| d.join("recent_projects.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::ffi::OsStr;
    use std::path::Path;

    #[test]
    fn explicit_override_wins_over_the_platform_base() {
        let base = Some(PathBuf::from("/platform/state"));
        assert_eq!(
            resolve_state_dir(Some(OsStr::new("/tmp/custom")), base.clone()),
            Some(PathBuf::from("/tmp/custom"))
        );
        // An empty override is not an override.
        assert_eq!(
            resolve_state_dir(Some(OsStr::new("")), base.clone()),
            Some(PathBuf::from("/platform/state/repo-graph"))
        );
        assert_eq!(
            resolve_state_dir(None, base),
            Some(PathBuf::from("/platform/state/repo-graph"))
        );
    }

    /// Nothing usable rather than a guessed absolute path: falling back to a
    /// literal home directory is the defect this module exists to remove.
    #[test]
    fn no_platform_base_yields_none_not_a_guess() {
        assert_eq!(resolve_state_dir(None, None), None);
    }

    /// Both config files must sit under whatever `state_dir` resolved to —
    /// they may never be addressed independently of it.
    #[test]
    fn config_files_live_under_the_resolved_state_dir() {
        if let (Some(dir), Some(reg), Some(recent)) =
            (state_dir(), active_project_registry(), recent_projects_config())
        {
            assert_eq!(reg, dir.join("active_project.json"));
            assert_eq!(recent, dir.join("recent_projects.json"));
        }
    }

    /// The invariant this module exists to hold. The *resolved* path naturally
    /// contains the current user's name (that is what `%APPDATA%` is), so the
    /// only meaningful check is that no path is baked into the source: four
    /// literal `C:\Users\nmahe\.gemini\antigravity\...` constants made the app
    /// unusable for every other user and on every non-Windows platform, and
    /// pinned the MCP confinement root to a predictable location.
    #[test]
    fn no_source_file_hardcodes_an_author_or_foreign_tool_path() {
        // Assembled at runtime so this test does not match its own source.
        let user_dir = format!("c:{}users{}", '\\', '\\');
        let foreign = format!("{}gemini", '.');

        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut offenders = Vec::new();
        visit_rs_files(&src, &mut |file: &Path| {
            let Ok(text) = std::fs::read_to_string(file) else { return };
            for (i, line) in text.lines().enumerate() {
                let lower = line.to_lowercase();
                // Doc comments in this module name the defect on purpose.
                if lower.trim_start().starts_with("//") {
                    continue;
                }
                if lower.contains(&user_dir) || lower.contains(&foreign) {
                    offenders.push(format!("{}:{}: {}", file.display(), i + 1, line.trim()));
                }
            }
        });
        assert!(offenders.is_empty(), "hardcoded paths found:\n{}", offenders.join("\n"));
    }

    fn visit_rs_files(dir: &Path, f: &mut dyn FnMut(&Path)) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                visit_rs_files(&p, f);
            } else if p.extension().is_some_and(|e| e == "rs") {
                f(&p);
            }
        }
    }
}
