---
name: houserules-step
description: Use around every change to this repository, in any workflow: read the rules for the files before you change them, then audit the change and validate its report.
---

# The houserules step

A change takes four steps. Two of them call `tools/houserules-step.sh`. The script writes its output to files below `.houserules/step/`, prints the path of each file, and prints each exit status, so you read the output with your file-read tool. Paths and refs you pass to the script are relative to the install root.

1. **Read the rules.** Run `tools/houserules-step.sh before <path>...` with every file you plan to change. The script runs `houserules for` and writes the rule package to `.houserules/step/rules.json`. Read the file. Run `houserules get <id>` for a rule you are unsure about.
2. **Implement the change.** Follow the rules you read.
3. **Audit the change.** Run `tools/houserules-step.sh after <base>` with the ref you started from. The script runs `houserules audit --base <base>` and writes the result to `.houserules/step/audit.json`. Read the file. Fix every `fail` row and every line in `references.failures`, then run the step again. A line in `references.failures` is a work-item reference that fails, the line that asks for a tracker, or the line of a tracker that cannot be reached. `summary.reference_fail` counts them.
4. **Write and validate the report.** Write the report in the report format of `.claude/schemas/deliverables.json`. Run `tools/houserules-step.sh after <base> <report>`. The script also runs `houserules validate` on the report and writes the result to `.houserules/step/validate.json`. Fix every error.

`init` adds `.houserules/step/` to `.gitignore`, because the files there are scratch output.
