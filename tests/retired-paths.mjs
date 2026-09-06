/**
 * Formerly-`KIT_OWNED` paths the `houserules` binary's `update` deletes
 * from an install if present (`crates/houserules/src/install.rs`'s own
 * `RETIRED`, `update_deletes_retired_shell_tools_from_an_old_install`
 * pins its exact contents on the Rust side). `bin/houserules.mjs`'s own
 * `KIT_OWNED` stays frozen (phase 5 JS retirement) and still names both,
 * so this list is what `tests/dogfood.test.mjs` and `tests/init.test.mjs`
 * both need to state the retirement explicitly instead of asserting a
 * byte-for-byte pin two deleted paths can never satisfy again. One
 * export instead of a literal in each file (batch 18 T6 fix round 1,
 * issue 6: quality.principles, DRY).
 *
 * @type {readonly [string, string]}
 */
export const RETIRED = ['tools/kb.sh', 'tools/backlog.sh'];
