---
paths:
  - "crates/houserules/src/**"
---
Generated from knowledge/ by houserules render. Do not edit.

# Cli rules

## Rules

- [houserules.controller-gates-use-the-tree-binary] While the branch changes generated output, run controller gates and update/render through the tree binary, never the installed houserules.
- [quality.absence-is-designed] Render an absent value in user-facing output as a designed token; never let a raw undefined, null, or empty slot reach the user.

## Invariants

- [houserules.migrations-finish-after-a-stop] A multi-write migration orders its writes so that a stop between any two leaves a tree the next run finishes; the completion marker is written last.
- [houserules.writers-render-after-a-change] Every command that writes knowledge/ or .houserules.json ends with the render that init and update run, and writes a file only when its bytes change.

## Gotchas

- [houserules.diff-tree-prints-nothing-for-a-merge] git diff-tree -r prints no paths for a merge commit unless -m, -c, or --first-parent is given; a path filter over commits must read every parent diff.

Detail: houserules get <id>
