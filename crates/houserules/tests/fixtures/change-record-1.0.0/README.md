# A change record of schema 1.0.0

`note.json` is the note that `houserules record write` wrote for one range of a scratch install,
as the binary of batch 37 writes it (crate version 1.4.0, schema
`houserules/change-record@1.0.0`). The scratch install held the knowledge base of
`Install::new("", &[docs_read()], None)` in `tests/record.rs` and a commit that adds three lines to
`docs/guide.md`.

The tests of `tests/record.rs` replace these placeholders with the values of their own repository,
because a commit sha is not fixed:

| Placeholder | Value |
|---|---|
| `__BASE__` | the full sha of the base commit (it is also the merge base) |
| `__HEAD__` | the full sha of the head commit (it is also the key) |
| `__BASE7__`, `__HEAD7__` | the first seven characters of those shas |
| `__RULES_HASH__` | the hash of the rule sources of the install |

Nothing else of the note changed. It holds no `links.sessions` key: schema 1.1.0 added it.
