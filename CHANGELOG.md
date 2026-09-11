# Changelog

## [1.0.0-alpha](https://github.com/jblossey/houserules/compare/v0.2.0-alpha...v1.0.0-alpha) (2026-09-11)


### ⚠ BREAKING CHANGES

* retire everything JS - the repository runs on cargo and the binary alone

### Features

* **cli:** the payload lives inside the binary; the flat surface completes ([467d189](https://github.com/jblossey/houserules/commit/467d1896adf6b065fb0d944b01ab5a2767b9c699))
* **dev-bins:** walkdir replaces the hand-rolled walkers (HR-079, ruled 5.47) ([50c2c38](https://github.com/jblossey/houserules/commit/50c2c38f959b4fed2129b64e2379e36f97409978))
* **houserules:** ship check-report-claims through the binary with the paste-run lint ([7a930e2](https://github.com/jblossey/houserules/commit/7a930e2461ff45cf1ff820a8954c1abd081b89f2))
* **houserules:** the report/audit gates - ephemeral paths and --sanctioned ([ced1c61](https://github.com/jblossey/houserules/commit/ced1c6157428587c6cb86c568576867721ea7680))
* **kb:** the check-side gates - dead globs, emitter round-trip, textList floor ([13ea1a4](https://github.com/jblossey/houserules/commit/13ea1a44701dedafe88c2e75ff14d72d4e1467bf))
* **release:** the cargo-dist pipeline lands, pinned and resolving ([ecac301](https://github.com/jblossey/houserules/commit/ecac30136e39ec75e4a623e1667294556f5ecad8))
* retire everything JS - the repository runs on cargo and the binary alone ([a0e4adf](https://github.com/jblossey/houserules/commit/a0e4adf2bd8ada595045fa719d1598d9633218f7))
* **rust:** phase 1 of the tier-2 port - scaffold, render, check-knowledge ([8c3cc15](https://github.com/jblossey/houserules/commit/8c3cc15da2539c07d03073047b35d51ddd55e69b))
* **rust:** phase 2 of the tier-2 port - the check runners and every read surface ([8acd08c](https://github.com/jblossey/houserules/commit/8acd08c8f118f0f14f15648eb8ecfdbbbee21d58))
* **tests:** port the JS test surface to cargo behind the T1 deletion license ([f7833f3](https://github.com/jblossey/houserules/commit/f7833f370970014a0c8d813de680d0d93db5bc9b))
* **tools:** close batch 21 - the retirement, portability, and the records ([a550830](https://github.com/jblossey/houserules/commit/a55083008081ae1b8282534f2af9ea174cd9740d))
* **tools:** close Tier 2 - the sweep, the residue gate, the evals record, the release flip ([d0cc286](https://github.com/jblossey/houserules/commit/d0cc286d4bcd3edf16beafcf4c52cea8ab49b58f))


### Bug Fixes

* **check-commit:** exempt a bot-authored commit from the body-line limit ([9a91ebd](https://github.com/jblossey/houserules/commit/9a91ebdf7688158c87f08d0840ba47f686661285))
* **rust:** collapse dot-dot components in validate's path resolution ([a49e34b](https://github.com/jblossey/houserules/commit/a49e34b4700aba2158dad3561faa65939d931327))
* **rust:** pin the CLI's bin name so Windows usage text says houserules ([81dbb34](https://github.com/jblossey/houserules/commit/81dbb34e30b4b5254806b9238f3954331a9b781e))
* **rust:** strip a Windows verbatim prefix from validate's resolved paths ([d4051b5](https://github.com/jblossey/houserules/commit/d4051b5b6bcbeae15aa83e482d7509a39c3c3798))
* **tests:** pin the frozen worktree checkout to LF on every platform ([8620ff8](https://github.com/jblossey/houserules/commit/8620ff84db511725cf1c012843a5f7f6a6c7f410))

## [0.2.0-alpha](https://github.com/jblossey/houserules/compare/houserules-v0.1.0...houserules-v0.2.0-alpha) (2026-09-03)


### Features

* **cli:** update reports the kit version drift ([ca45e79](https://github.com/jblossey/houserules/commit/ca45e79eb6e94eff349e2fd6cdd0f84e2c75ce1e))


### Bug Fixes

* **template:** the capture form and the decidable collision line ([9686872](https://github.com/jblossey/houserules/commit/9686872027446d3626fc56729df3c8cd14f50783))
