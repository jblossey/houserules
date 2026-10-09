---
paths:
  - "crates/houserules/src/**"
---
Generated from knowledge/ by houserules render. Do not edit.

# Cli rules

## Rules

- [houserules.cli-changes-run-full-tier] A change under crates/houserules/src/** runs the full tier: the binary's commands, flags, and output are a contract that every adopter runs.
- [quality.absence-is-designed] Render an absent value in user-facing output as a designed token; never let a raw undefined, null, or empty slot reach the user.

Detail: houserules get <id>
