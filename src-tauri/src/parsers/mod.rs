//! Per-language import/export extraction. Every extractor implements the
//! same `Extractor` trait so the graph engine stays language-agnostic
//! (see `docs/logic/import-extraction-logic.md`).

pub mod javascript;
pub mod python;
pub mod ts_engine;
pub mod ts_langs;
pub mod rust_lang;
pub mod go;
pub mod java;
pub mod kotlin;
pub mod c_cpp;
pub mod swift;
pub mod html;
pub mod sql;
pub mod prisma;
pub mod dockerfile;
pub mod markdown;
pub mod php;
pub mod csharp;
pub mod sfc;
pub mod routes;
pub mod state;
pub mod schema;

use crate::graph::EdgeKind;
use std::path::Path;

pub trait Extractor {
    fn extract(&self, file_path: &Path, source: &str) -> ExtractionResult;
}

#[derive(Debug, Default, Clone)]
pub struct ExtractionResult {
    pub exports: Vec<String>,
    pub imports: Vec<ImportRef>,
    pub entry_points: Vec<EntryPoint>,
    /// Warning kinds (`parse_error`, `unresolved_dynamic`, ...) attached to
    /// this file — surfaced in the manifest, never fatal.
    pub warnings: Vec<String>,
    pub symbols: Vec<ExtractedSymbol>,
    pub references: Vec<ExtractedReference>,
    pub inheritance: Vec<InheritanceRelation>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InheritanceRelation {
    pub class_name: String,
    pub parent_name: String,
    pub is_implements: bool,
    pub line: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExtractedSymbol {
    pub name: String,
    pub kind: String,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExtractedReference {
    pub name: String,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct ImportRef {
    pub raw_specifier: String,
    /// Candidate repo-relative path *stem* (forward slashes, may lack an
    /// extension). The graph builder verifies it against the indexed file
    /// set — prefer a false negative over a wrong edge.
    pub resolved_path: Option<String>,
    pub kind: EdgeKind,
    /// Bare/third-party specifier → `external_dependencies`, not a graph edge.
    pub external: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EntryPoint {
    pub route: String,
    /// Symbol name of the handler bound to this route when the scanner
    /// could determine it (`listUsers`, `UsersController.index`, `loader`).
    pub handler: Option<String>,
    /// 1-based line of the route declaration (0 = file-level/unknown).
    pub line: usize,
}

/// Language dispatch table used by the indexer.
pub fn extractor_for(language: &str) -> Option<Box<dyn Extractor + Send + Sync>> {
    match language {
        "javascript" => Some(Box::new(javascript::JsExtractor)),
        "sfc" => Some(Box::new(sfc::SfcExtractor)),
        "python" => Some(Box::new(python::PythonExtractor)),
        "rust" => Some(Box::new(rust_lang::RustExtractor)),
        "go" => Some(Box::new(go::GoExtractor)),
        "java" => Some(Box::new(java::JavaExtractor)),
        "kotlin" => Some(Box::new(kotlin::KotlinExtractor)),
        "c" => Some(Box::new(c_cpp::CCppExtractor)),
        "cpp" => Some(Box::new(c_cpp::CCppExtractor)),
        "swift" => Some(Box::new(swift::SwiftExtractor)),
        "html" => Some(Box::new(html::HtmlExtractor)),
        "sql" => Some(Box::new(sql::SqlExtractor)),
        "prisma" => Some(Box::new(prisma::PrismaExtractor)),
        "graphql" => Some(Box::new(schema::GraphqlExtractor)),
        "dockerfile" => Some(Box::new(dockerfile::DockerfileExtractor)),
        "markdown" => Some(Box::new(markdown::MarkdownExtractor)),
        "php" => Some(Box::new(php::PhpExtractor)),
        "csharp" => Some(Box::new(csharp::CSharpExtractor)),
        _ => None,
    }
}

/// Join `base_dir` and a relative specifier, collapsing `.` / `..`
/// lexically. Returns `None` if the path escapes the repo root.
pub(crate) fn normalize_relative(base_dir: &str, specifier: &str) -> Option<String> {
    let mut parts: Vec<&str> = if base_dir.is_empty() {
        Vec::new()
    } else {
        base_dir.split('/').collect()
    };
    let specifier = specifier.replace('\\', "/");
    for seg in specifier.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            s => parts.push(s),
        }
    }
    Some(parts.join("/"))
}

/// Directory (repo-relative, forward slashes) containing `file_path`.
pub(crate) fn parent_dir(file_path: &Path) -> String {
    file_path
        .parent()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default()
}
