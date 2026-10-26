# Updating and uninstalling

## Updating

```sh
houserules update
```

Installed through the shell installer and run from where it installed, `update`
checks GitHub for a newer release, replaces the binary itself, and re-runs before
touching your repository: there is nothing else to install by hand. Run from
anywhere else (mise, a direct download, or a shell-installer copy invoked from a
different location), it prints one line that names what to do instead, and it
updates your repository on the binary that you already have.

The repository sync writes the files that houserules owns and keeps any that you
changed. It re-renders the generated files, moves each kit pack to the version of
the binary, and keeps your overrides and exclusions. Where the install runs the
reference workflow, it moves the files of the workflow too, and keeps a file that
you changed. When a pack ships the id of one of your own entries, it moves that
entry into an override (one id, one source) and removes a topic file left empty.
Your own entries keep their text: `update` changes a topic file of yours only for
that move. It also writes a seed-once file that your install lacks. It prints the version
drift that it synced, `kit <old> -> <new>`. Review the result with `git diff`.
Whenever the binary check completes, `update` also warns if another `houserules`
sits elsewhere on your `PATH`: not the copy it just checked, updated or not.

An install of houserules 1.x converts to the 2.0 layout in the same run.
[docs/upgrading-to-2.md](upgrading-to-2.md) lists what changes and each hand
step that `update` can print.

Set `HOUSERULES_SKIP_SELF_UPDATE` (to any value) to update only the repository
and skip the binary check for one run; `update` skips it automatically in CI.

On an interactive terminal, every other command also checks, at most once a day,
whether a newer release exists, and names it in one line. Set `CI` or
`HOUSERULES_NO_UPDATE_CHECK` (to any value) to turn that off.

## Uninstalling

Remove the binary through the channel you installed with:

- **Shell installer or direct download**: delete the `houserules` binary
  from where it was installed — `~/.local/bin` for the shell installer,
  wherever you placed it for a direct download. An older shell-installer
  install sits in `~/.cargo/bin`.
- **mise**: run `mise uninstall houserules` and remove the tool's entry
  from your `mise.toml`.

Everything houserules put into a project is plain files tracked by your
repository, and one notes ref when the review layer records merges. To remove
houserules from a project:

- Delete `.houserules.json` and the files that houserules wrote outside
  `knowledge/` and `backlog/`: `houserules files` lists them. Keep a file that
  holds text of your own, such as your `AGENTS.md` or `CLAUDE.md`, and delete
  the line `@AGENTS.md` from `CLAUDE.md`.
- Delete the block between `<!-- houserules:begin -->` and
  `<!-- houserules:end -->` in the root `AGENTS.md` and in the `AGENTS.md` of
  each folder of an area. Delete a file that then holds nothing.
- With `--target claude`, delete `.claude/rules/`,
  `.claude/skills/project-knowledge/`, and each hook entry of
  `.claude/settings.json` whose command names a script in `tools/`.
- Delete the line `.houserules/` from `.gitignore`, and the CI files that
  `review.ci.files` in `.houserules.json` names.
- Delete the notes ref of the change records: `git update-ref -d
  refs/notes/houserules` in a clone, and `git push origin --delete
  refs/notes/houserules` for the remote.
- With the reference workflow, undo the hook setting that you made for its
  commit-msg hook, such as `core.hooksPath`.
- Keep `knowledge/` and the `backlog/` of a JSON tracker, or delete them: they
  are readable JSON and useful without the binary.
