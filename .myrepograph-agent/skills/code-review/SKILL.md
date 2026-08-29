---
name: code-review
description: Review changed code for correctness, then verify the findings before reporting.
---

# Code Review

## 1. Scope the diff
Review what changed, plus what the change can break. Use
`repograph_impact(symbol="<name>")` or `repograph_impact(path="<path>")` for the blast radius.

## 2. Read what you are judging
Signatures are enough to map the call graph; they are not enough to review
logic. Load full bodies (`signature_only` omitted) for every symbol you
intend to comment on.

## 3. Verify before reporting
For each finding, construct the concrete input or state that triggers it. A
finding you cannot make fail is a hypothesis — either verify it or drop it.

## 4. Report
Most severe first, each with file, line, the failure scenario, and the fix.
Say explicitly when nothing was found; padding a review with style nits
buries the real defects.

Run `./review.sh` from this directory for the mechanical checks.
