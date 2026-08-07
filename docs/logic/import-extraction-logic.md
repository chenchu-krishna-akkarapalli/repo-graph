# Logic: Import/Export Extraction

Owns: turning raw source text into `{exports, imports, entry_points}` per
file, per language. Every extractor implements the same trait so the graph
engine stays language-agnostic.

## Common interface

```rust
trait Extractor {
    fn extract(&self, file_path: &Path, source: &str) -> ExtractionResult;
}

struct ExtractionResult {
    exports: Vec<String>,
    imports: Vec<ImportRef>,     // { raw_specifier, resolved_path: Option<PathBuf> }
    entry_points: Vec<EntryPoint>, // e.g. route markers
}
```

## JavaScript / TypeScript

- Parser: `swc_ecma_parser` (AST-based — required because multi-line and
  aliased imports break regex).
- Extract:
  - `import { X, Y as Z } from './module'` → imports `X`, `Y` (aliased `Z`)
  - `export function foo()`, `export default`, `export const X` → exports
  - `require('module')` (CommonJS) → treated as import edge
  - Dynamic `import(...)` with a computed (non-literal) argument → flagged
    `unresolved_dynamic`, not silently dropped
- Path resolution: relative specifiers (`./`, `../`) resolve against the
  file's directory; bare specifiers (`react`, `lodash`) are external and go
  to `external_dependencies`, not the in-repo graph.
- Next.js App Router special case: files matching `app/**/page.tsx`,
  `app/**/route.ts`, `app/**/layout.tsx` are tagged as `entry_points` with
  the route derived from the directory path (`[param]` segments become
  route params in the manifest).

## Python

- Parser: start with `ast` module semantics conceptually (line-scan for PoC,
  upgrade to a real AST pass) for `import X` / `from Y import Z`.
- Extract:
  - Standard imports → import edges (resolve local package-relative imports
    against the repo; stdlib/third-party imports → external)
  - `@app.get("/route")`, `@app.post(...)`, similar decorators from
    FastAPI/Flask → entry_points with the route pattern
- Edge case: relative imports using `.` / `..` package syntax need package
  `__init__.py` awareness to resolve correctly — don't assume flat structure.

## Rust

- Parse `Cargo.toml` for workspace/crate structure and external dependency
  names (these become `external_dependencies`, not graph edges).
- Scan for `mod x;` (module declarations) and `use crate::...` /
  `use super::...` (internal edges) vs `use external_crate::...` (external).
- Exports: `pub fn`, `pub struct`, `pub enum`, `pub trait` at module level.

## Extraction quality bar

- Prefer **false negatives over false positives** — an agent missing an
  edge just means it re-checks with `read_file`; a wrong edge could send it
  to the wrong file entirely.
- Every extractor must be testable against small fixture files with a
  hand-verified expected `ExtractionResult`, checked into
  `src-tauri/tests/fixtures/<language>/`.

## Adding a new language

1. Create `parsers/<language>.rs` implementing `Extractor`.
2. Add fixtures + expected-output tests.
3. Register the extractor in the walker's language dispatch table.
4. Do not touch `graph-engine-logic.md`'s data structures — the contract is
   the `ExtractionResult` shape, nothing else should need to change.
