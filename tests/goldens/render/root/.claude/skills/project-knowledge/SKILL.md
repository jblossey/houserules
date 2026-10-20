---
name: project-knowledge
description: Use when working on this repository as a dispatched subagent, before reading or changing any file
user-invocable: false
---
Generated from knowledge/ by houserules render. Do not edit.

# Project knowledge

## Retrieval protocol

Save the output of every `houserules` command to a file, then read the file with your file-read tool.

1. Resolve every id under `Knowledge:` in your task: `houserules get <ids>` (JSON).
2. Before editing, run `houserules for <every file you will change>` and `get` any rule you are unsure about.
3. Write `REPORT_FILE` as a `task-report` (schema `.claude/schemas/deliverables.json`, `self_audit: null`), then run `houserules audit --base <BASE> --head HEAD --ids <ids, comma-separated> --report <REPORT_FILE>`. The `--ids` value is the task's `Knowledge:` list, generated from it, never typed separately. Copy the audit `summary` and its `deterministic` rows into `self_audit` — never hand-written rows; the judged rows are the reviewer's. Fix every `fail`, re-run until clean, then run `houserules validate <REPORT_FILE>` and fix every error. Declare a spec-booked interim fail once with `--sanctioned <rule>=<ref>` instead of narrating it by hand. List the ids you relied on in `knowledge_used`.

## Topics

houserules  25  Working in the houserules kit repository
knowledge-base  5  Authoring knowledge entries
process  30  How work runs: batches, dispatch, reviews, rulings
quality  6  Quality principles
security-hygiene  5  Dependency, commit, and test hygiene
writing-style  4  Writing style for docs, comments, commits, reports
