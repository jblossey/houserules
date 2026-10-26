# Discovery brief: what is houserules?

The dispatch names `<repository copy>` (a detached worktree of the release head with `README.md` and every test that quotes it removed from its working tree), `<binary>` (built at that head), `<scratch folder>`, and `<report file>`.

You are a senior engineer asked to find out, on your own, what the software in this repository is and does, and to describe it to someone who will write its README from your description. Nobody has told you what it is. Find out from the product itself.

Where you may look:
- The repository copy at <repository copy> (read only; change nothing there): the Rust source under crates/houserules/src, the tests, the kit under template/, the knowledge base under knowledge/, schemas/, ci/, .github/, docs/, CHANGELOG.md, CONTRIBUTING.md, and any other file there.
- The built binary <binary>. Run it with the environment variable HOUSERULES_SKIP_SELF_UPDATE=1. Read `--help` of every command. Try it for real: make new scratch git repositories under <scratch folder> (a new folder for each try; set user.name and user.email in each), run `init`, add a rule, make commits, run the checks, and see what each command writes and prints.

What you must not read or use:
- Any README of the project: the README.md of the repository root (removed from your copy on purpose), any copy or quote of it, and the batch workspace (it holds reviews of the README). Do not open any other checkout of the repository; use only your copy.
- The web. Do not research anything outside the copy and the binary.
- If your environment loaded instructions or a description of houserules, use them only as the instructions of the repository you work in; describe the product from what you read in the code and saw the binary do.
- Specs and design notes (docs/specs, docs/plans, docs/design.md) state intent and history; they may use marketing words. Prefer what the code and the binary show. When a document claims something, check it against the code or a run before you repeat it.

Write your description, in Markdown, to <report file> and to no other file. Plain words, short sentences. Each claim cites the file (path:line) or the command you ran and what it printed. Sections:

1. **In one sentence** (under 120 characters), then **in one paragraph**: what it is and what problem it solves, for whom, in plain words, as you would say it to a colleague. No jargon that you do not define.
2. **What it is made of:** the binary, the files it writes into a repository, where the data lives, what runs where (local, CI, agent session); what it needs (and does not need: servers, services, accounts).
3. **How a person uses it:** the first ten minutes, as a sequence of real commands with the output you saw.
4. **What it does, by capability:** each capability you found, ranked by how central it is to the product (judge by the code: how much of it there is, how much else depends on it). For each: what it does, the commands and files, a real example you ran, and its limits.
5. **What it is not:** what a reader might expect that it does not do (from what you checked in the code).
6. **What sets it apart:** compared with tools you know from general knowledge (name them), what it does that they do not, and what they do better. From memory; no research.
7. **Vocabulary:** every term the product uses (in its output, its files, its commands) that a newcomer will not know, each defined in one line.
8. **Surprises and doubts:** what confused you, what seems inconsistent, what you could not make work.

Your final message is a ten-line summary: the one sentence, the five most central capabilities, and the three biggest surprises.
