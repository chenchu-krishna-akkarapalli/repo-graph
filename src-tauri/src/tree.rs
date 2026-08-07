//! Directory tree for the file-explorer sidebar: recursive
//! `{name, path, is_dir, children}` nodes, sharing the walker's noise
//! denylist. Reads stay confined to the given root (recursion only
//! descends; symlinked directories are not followed).

use crate::walker::SKIP_DIRS;
use serde::Serialize;
use std::path::Path;

/// Safety valve against pathological nesting.
const MAX_DEPTH: usize = 32;

#[derive(Debug, Serialize)]
pub struct FileTreeNode {
    pub name: String,
    /// Root-relative path with forward slashes (matches graph node paths).
    pub path: String,
    pub is_dir: bool,
    pub children: Vec<FileTreeNode>,
}

pub fn read_directory_tree(root: &Path) -> std::io::Result<Vec<FileTreeNode>> {
    build_children(root, root, 0)
}

fn build_children(root: &Path, dir: &Path, depth: usize) -> std::io::Result<Vec<FileTreeNode>> {
    if depth >= MAX_DEPTH {
        return Ok(Vec::new());
    }
    let mut entries: Vec<(String, bool, std::path::PathBuf)> = Vec::new();
    for entry in std::fs::read_dir(dir)?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        // file_type() does not traverse symlinks — a symlinked dir counts
        // as non-dir here, so we never recurse outside the root.
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if is_dir && SKIP_DIRS.contains(&name.as_str()) {
            continue;
        }
        entries.push((name, is_dir, entry.path()));
    }
    // Familiar explorer ordering: folders first, then files, alphabetical.
    entries.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase())));

    let mut nodes = Vec::with_capacity(entries.len());
    for (name, is_dir, full_path) in entries {
        let rel = full_path
            .strip_prefix(root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| name.clone());
        let children = if is_dir {
            build_children(root, &full_path, depth + 1).unwrap_or_default()
        } else {
            Vec::new()
        };
        nodes.push(FileTreeNode {
            name,
            path: rel,
            is_dir,
            children,
        });
    }
    Ok(nodes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_skips_noise_and_sorts_dirs_first() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("repo-graph-tree-{nanos}"));
        std::fs::create_dir_all(root.join("src/components")).unwrap();
        std::fs::create_dir_all(root.join("node_modules/x")).unwrap();
        std::fs::write(root.join("zzz.md"), "z").unwrap();
        std::fs::write(root.join("src/app.ts"), "x").unwrap();
        std::fs::write(root.join("src/components/Button.tsx"), "x").unwrap();

        let tree = read_directory_tree(&root).unwrap();
        let top: Vec<(&str, bool)> = tree.iter().map(|n| (n.name.as_str(), n.is_dir)).collect();
        assert_eq!(top, vec![("src", true), ("zzz.md", false)]);
        let src = &tree[0];
        assert_eq!(src.children[0].name, "components");
        assert_eq!(src.children[0].children[0].path, "src/components/Button.tsx");
        assert_eq!(src.children[1].path, "src/app.ts");
        let _ = std::fs::remove_dir_all(root);
    }
}
