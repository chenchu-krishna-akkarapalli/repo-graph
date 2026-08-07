//! Multi-threaded file tree traversal. Skips VCS/build/dependency noise,
//! lockfiles, and binary assets; returns repo-relative `FileEntry` records
//! for the graph builder.

use ignore::{WalkBuilder, WalkState};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::mpsc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    /// Repo-relative path with forward slashes.
    pub path: String,
    pub size_bytes: u64,
    pub extension: String,
    pub language: String,
}

pub const SKIP_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    "dist",
    "build",
    "target",
    "out",
    ".next",
    ".repograph",
    "__pycache__",
    ".venv",
    "venv",
    "coverage",
    ".myrepograph-agent",
    "my-agent",
    ".agents",
];

const SKIP_FILES: &[&str] = &[
    "package-lock.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "Cargo.lock",
    "poetry.lock",
    "uv.lock",
    "Pipfile.lock",
    "composer.lock",
    "Gemfile.lock",
];

/// Files that carry credentials and must never enter the index or be served.
///
/// This is enforced twice on purpose — here, so a fresh index never contains
/// them, and again in `mcp_server::read_file`, because a `.repograph` cache
/// built before this list existed still holds these paths as nodes and would
/// otherwise keep serving them.
///
/// `.env.example` / `.sample` / `.template` / `.dist` are meant to be checked
/// in and are deliberately still indexed: they are the file an agent should
/// read to learn which variables exist.
pub fn is_secret_file(name: &str) -> bool {
    if name == ".env" || name.starts_with(".env.") {
        let suffix = name.rsplit('.').next().unwrap_or("");
        return !matches!(suffix, "example" | "sample" | "template" | "dist");
    }
    if matches!(
        name,
        ".npmrc"
            | ".pypirc"
            | ".netrc"
            | "_netrc"
            | ".pgpass"
            | ".htpasswd"
            | "credentials"
            | "id_rsa"
            | "id_dsa"
            | "id_ecdsa"
            | "id_ed25519"
            | "secrets.yml"
            | "secrets.yaml"
    ) {
        return true;
    }
    let lower = name.to_ascii_lowercase();
    [".pem", ".key", ".p12", ".pfx", ".jks", ".keystore", ".ppk"]
        .iter()
        .any(|ext| lower.ends_with(ext))
}

const BINARY_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "ico", "bmp", "svgz", "woff", "woff2", "ttf", "otf",
    "eot", "mp3", "mp4", "webm", "avi", "mov", "pdf", "zip", "gz", "tar", "7z", "rar", "exe",
    "dll", "so", "dylib", "bin", "wasm", "class", "jar", "pyc", "o", "a", "obj", "pdb", "db",
    "sqlite",
];

pub fn language_for(path: &Path) -> String {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if name == "Cargo.toml" {
        return "rust".to_string();
    }
    // `Dockerfile`, `Dockerfile.prod`, `api.dockerfile` are all Dockerfiles.
    if name == "Dockerfile" || name.starts_with("Dockerfile.") {
        return "dockerfile".to_string();
    }
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" | "mts" | "cts" => "javascript".to_string(),
        // Vue/Svelte single-file components: JS/TS inside `<script>` blocks.
        "vue" | "svelte" => "sfc".to_string(),
        "py" => "python".to_string(),
        "rs" => "rust".to_string(),
        "go" => "go".to_string(),
        "java" => "java".to_string(),
        "kt" | "kts" => "kotlin".to_string(),
        "c" | "h" => "c".to_string(),
        "cpp" | "hpp" | "cc" | "cxx" | "hh" | "hxx" => "cpp".to_string(),
        "swift" => "swift".to_string(),
        "php" => "php".to_string(),
        "cs" => "csharp".to_string(),
        "html" | "htm" => "html".to_string(),
        "sql" => "sql".to_string(),
        "prisma" => "prisma".to_string(),
        "graphql" | "gql" => "graphql".to_string(),
        "dockerfile" => "dockerfile".to_string(),
        "md" | "mdx" => "markdown".to_string(),
        _ => "other".to_string(),
    }
}


/// Walk `root` in parallel and return all indexable files, sorted by path
/// for deterministic downstream output.
pub fn walk_repo(root: &Path) -> Vec<FileEntry> {
    let (tx, rx) = mpsc::channel::<FileEntry>();
    let walker = WalkBuilder::new(root)
        .hidden(false) // we manage our own denylist; still index dotfiles like .env.example
        .git_ignore(true)
        // `ignore` only applies gitignore rules inside a real git repo by
        // default, so a checkout exported without `.git` silently indexed
        // everything its `.gitignore` excludes.
        .require_git(false)
        .follow_links(false)
        .filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            if entry.file_type().is_some_and(|t| t.is_dir()) {
                !SKIP_DIRS.contains(&name.as_ref())
            } else {
                true
            }
        })
        .build_parallel();

    let root_owned = root.to_path_buf();
    walker.run(|| {
        let tx = tx.clone();
        let root = root_owned.clone();
        Box::new(move |result| {
            let entry = match result {
                Ok(e) => e,
                Err(_) => return WalkState::Continue, // unreadable entry: skip, never crash
            };
            if !entry.file_type().is_some_and(|t| t.is_file()) {
                return WalkState::Continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if SKIP_FILES.contains(&name.as_str()) || is_secret_file(&name) {
                return WalkState::Continue;
            }
            let extension = entry
                .path()
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if BINARY_EXTENSIONS.contains(&extension.as_str()) {
                return WalkState::Continue;
            }
            let rel = match entry.path().strip_prefix(&root) {
                Ok(r) => r.to_string_lossy().replace('\\', "/"),
                Err(_) => return WalkState::Continue,
            };
            let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
            let language = language_for(entry.path());
            let _ = tx.send(FileEntry {
                path: rel,
                size_bytes,
                extension,
                language,
            });
            WalkState::Continue
        })
    });
    drop(tx);

    let mut files: Vec<FileEntry> = rx.into_iter().collect();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_tree() -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("repo-graph-walker-{nanos}"));
        for dir in ["src", "node_modules/react", ".git", "assets"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        std::fs::write(root.join("src/app.ts"), "export const a = 1;").unwrap();
        std::fs::write(root.join("src/util.py"), "import os").unwrap();
        std::fs::write(root.join("node_modules/react/index.js"), "x").unwrap();
        std::fs::write(root.join(".git/config"), "x").unwrap();
        std::fs::write(root.join("package-lock.json"), "{}").unwrap();
        std::fs::write(root.join("assets/logo.png"), [0u8, 1, 2]).unwrap();
        std::fs::write(root.join("README.md"), "# hi").unwrap();
        root
    }

    #[test]
    fn walker_skips_noise_and_keeps_sources() {
        let root = temp_tree();
        let files = walk_repo(&root);
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["README.md", "src/app.ts", "src/util.py"]);
        assert_eq!(files[1].language, "javascript");
        assert_eq!(files[2].language, "python");
        let _ = std::fs::remove_dir_all(root);
    }

    /// A `.env` reached the index, and from there `read_file` served its
    /// contents verbatim to any connected agent. Committed template files stay
    /// indexed — they document the variable names without holding the values.
    #[test]
    fn secret_files_never_enter_the_index() {
        let root = temp_tree();
        std::fs::write(root.join(".env"), "DATABASE_URL=postgres://u:pw@h/db").unwrap();
        std::fs::write(root.join(".env.local"), "TOKEN=abc").unwrap();
        std::fs::write(root.join(".env.example"), "DATABASE_URL=").unwrap();
        std::fs::write(root.join("server.pem"), "-----BEGIN PRIVATE KEY-----").unwrap();

        let paths: Vec<String> = walk_repo(&root).into_iter().map(|f| f.path).collect();
        assert!(!paths.iter().any(|p| p == ".env"), "{paths:?}");
        assert!(!paths.iter().any(|p| p == ".env.local"), "{paths:?}");
        assert!(!paths.iter().any(|p| p == "server.pem"), "{paths:?}");
        assert!(paths.iter().any(|p| p == ".env.example"), "{paths:?}");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn secret_classification_covers_keys_and_spares_templates() {
        for name in [".env", ".env.local", ".env.production", "id_rsa", "app.key", "cert.PEM"] {
            assert!(is_secret_file(name), "{name} must be treated as a secret");
        }
        for name in [".env.example", ".env.sample", ".env.template", "main.rs", "keyboard.ts"] {
            assert!(!is_secret_file(name), "{name} must stay indexable");
        }
    }

    #[test]
    fn language_dispatch_covers_cargo_toml() {
        assert_eq!(language_for(Path::new("x/Cargo.toml")), "rust");
        assert_eq!(language_for(Path::new("a/b.tsx")), "javascript");
        assert_eq!(language_for(Path::new("a/b.css")), "other");
    }
}
