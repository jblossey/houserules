---
name: project-knowledge
description: Use when working on this repository as a dispatched subagent, before reading or changing any file
user-invocable: false
---
Generated from knowledge/ by tools/kb.sh render. Do not edit.

# Project knowledge

## Standing rules

- [mini.standing-example] Always run the mini fixture's own regeneration before trusting its output.

## Retrieval protocol

1. Resolve every id under `Knowledge:` in your task: `tools/kb.sh get <ids>` (JSON).
2. Before editing, run `tools/kb.sh for <every file you will change>` and `get` any rule you are unsure about.
3. Write `REPORT_FILE` as a `task-report` (schema `.claude/schemas/deliverables.json`, `self_audit: null`), then run `tools/kb.sh audit --base <BASE> --head HEAD --ids <ids, comma-separated> --report <REPORT_FILE>`. The `--ids` value is the task's `Knowledge:` list, generated from it, never typed separately. Copy the audit `summary` and its `deterministic` rows into `self_audit` — never hand-written rows; the judged rows are the reviewer's. Fix every `fail`, re-run until clean, then run `tools/kb.sh validate <REPORT_FILE>` and fix every error. List the ids you relied on in `knowledge_used`.

## Topics

mini  6  Mini fixture knowledge
