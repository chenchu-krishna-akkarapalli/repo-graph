#!/usr/bin/env node
/**
 * Repo Graph — Context Cost & Token Savings Benchmark (Playbook §21).
 *
 * Compares the token/cost footprint of one realistic agent task —
 *   "Find where the `open_in_editor` Tauri command is declared, read its
 *    implementation, and find what calls it"
 * — resolved two ways:
 *
 *   Arm A (baseline): walk + grep + read main.rs, db.rs, store.ts in full.
 *   Arm B (Repo Graph): scoped `get_manifest()` + one `explore()` call
 *     returning only the line-sliced `open_in_editor` block + call edges.
 *
 * Token math (standard source-code estimate): tokens = round(chars / 3.7)
 * Pricing: $3.00 per million input tokens.
 *
 * Usage: node scripts/benchmark.js
 */

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const CHARS_PER_TOKEN = 3.7;
const USD_PER_TOKEN = 3.0 / 1_000_000; // $3.00 / M input tokens

const tokens = (chars) => Math.round(chars / CHARS_PER_TOKEN);
const usd = (tok) => tok * USD_PER_TOKEN;

function readRepoFile(rel) {
  const abs = path.join(ROOT, rel);
  const text = fs.readFileSync(abs, 'utf-8');
  return { rel, text, chars: text.length, lines: text.split('\n').length };
}

/**
 * Locate the `open_in_editor` function in main.rs and return its exact
 * line slice (declaration line through the brace-matched closing line) —
 * this is what `explore()` would hand the agent instead of the whole file.
 */
function sliceOpenInEditor(mainRs) {
  const lines = mainRs.text.split('\n');
  const startIdx = lines.findIndex((l) => l.includes('fn open_in_editor'));
  if (startIdx === -1) {
    throw new Error('benchmark target `fn open_in_editor` not found in main.rs');
  }
  let depth = 0;
  let seenBrace = false;
  let endIdx = startIdx;
  for (let i = startIdx; i < lines.length; i++) {
    for (const ch of lines[i]) {
      if (ch === '{') {
        depth += 1;
        seenBrace = true;
      } else if (ch === '}') {
        depth -= 1;
      }
    }
    if (seenBrace && depth <= 0) {
      endIdx = i;
      break;
    }
  }
  const slice = lines.slice(startIdx, endIdx + 1).join('\n');
  return {
    startLine: startIdx + 1,
    endLine: endIdx + 1,
    lineCount: endIdx - startIdx + 1,
    chars: slice.length,
  };
}

// ---------------------------------------------------------------------------
// Arm A — baseline crawl: walk, grep, then ingest three files in full.
// ---------------------------------------------------------------------------
const crawlTargets = ['src-tauri/src/main.rs', 'src-tauri/src/db.rs', 'src/store.ts'];
const crawled = crawlTargets.map(readRepoFile);
const baselineChars = crawled.reduce((n, f) => n + f.chars, 0);
const baselineTokens = tokens(baselineChars);
// walk + grep "open_in_editor" + grep callers + 3 full read_file calls
const baselineToolCalls = 6;

// ---------------------------------------------------------------------------
// Arm B — Repo Graph: scoped manifest + explore(open_in_editor).
// ---------------------------------------------------------------------------
const MANIFEST_CHARS = 1_500; // scoped subgraph manifest (~1 line per file)
const CALL_EDGE_METADATA_CHARS = 400; // caller/callee refs returned by explore()

const mainRs = crawled[0];
const slice = sliceOpenInEditor(mainRs);
const repoGraphChars = MANIFEST_CHARS + slice.chars + CALL_EDGE_METADATA_CHARS;
const repoGraphTokens = tokens(repoGraphChars);
const repoGraphToolCalls = 2; // get_manifest(scope) + explore(open_in_editor)

// ---------------------------------------------------------------------------
// Report
// ---------------------------------------------------------------------------
const savingsPct = 100 * (1 - repoGraphTokens / baselineTokens);
const fmt = (n) => n.toLocaleString('en-US');
const money = (n) => `$${n.toFixed(6)}`;

const report = `
# Repo Graph — Context Cost Benchmark

**Task:** locate the \`open_in_editor\` Tauri command, read its implementation, trace its callers.
**Token estimate:** 1 token ≈ ${CHARS_PER_TOKEN} chars · **Pricing:** $3.00 / 1M input tokens

## Crawled corpus (Arm A reads these in full)

| File | Lines | Characters | Tokens |
|---|---:|---:|---:|
${crawled.map((f) => `| \`${f.rel}\` | ${fmt(f.lines)} | ${fmt(f.chars)} | ${fmt(tokens(f.chars))} |`).join('\n')}

## Repo Graph payload (Arm B)

| Payload | Characters | Tokens |
|---|---:|---:|
| Scoped manifest (\`get_manifest\`) | ${fmt(MANIFEST_CHARS)} | ${fmt(tokens(MANIFEST_CHARS))} |
| \`open_in_editor\` slice (main.rs L${slice.startLine}–L${slice.endLine}, ${slice.lineCount} lines) | ${fmt(slice.chars)} | ${fmt(tokens(slice.chars))} |
| Call edge metadata (\`explore\`) | ${fmt(CALL_EDGE_METADATA_CHARS)} | ${fmt(tokens(CALL_EDGE_METADATA_CHARS))} |

## Comparison

| Metric | Arm A · Baseline crawl | Arm B · Repo Graph | Δ |
|---|---:|---:|---:|
| File reads | ${crawlTargets.length} full files | 1 line slice | −${crawlTargets.length - 1} |
| Tool calls / round-trips | ${baselineToolCalls} | ${repoGraphToolCalls} | −${baselineToolCalls - repoGraphToolCalls} |
| Ingested characters | ${fmt(baselineChars)} | ${fmt(repoGraphChars)} | −${fmt(baselineChars - repoGraphChars)} |
| Ingested tokens | ${fmt(baselineTokens)} | ${fmt(repoGraphTokens)} | −${fmt(baselineTokens - repoGraphTokens)} |
| Input cost (USD) | ${money(usd(baselineTokens))} | ${money(usd(repoGraphTokens))} | −${money(usd(baselineTokens - repoGraphTokens))} |

## Result

**Savings: ${savingsPct.toFixed(1)} %** ${savingsPct > 90 ? '✅ (target: >90 %)' : '⚠️ below the >90 % target'}
`;

console.log(report.trim());

if (savingsPct <= 90) {
  process.exitCode = 1; // CI-friendly: regressions in savings fail the run
}
