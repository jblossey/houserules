# Changelog

## [1.1.0](https://github.com/jblossey/houserules/compare/v1.0.0...v1.1.0) (2026-09-15)


### Features

* **cli:** update self-updates the binary; every command checks for a newer release (HR-133, HR-134) ([2b56e51](https://github.com/jblossey/houserules/commit/2b56e5118bcd230c9db1a4dcf79fe4917b6bccfa))
* **release:** move the install path to ~/.local/bin (HR-135) ([716abdc](https://github.com/jblossey/houserules/commit/716abdc0f054d56b713f33a296e65f22dcca4c1c))
* **template:** AGENTS.md is the canonical instruction file; CLAUDE.md points to it (HR-136) ([382b963](https://github.com/jblossey/houserules/commit/382b96347ac9702ed930edd21687a96770a93f60))
* **template:** seed the README-principles rule ([ea17fc6](https://github.com/jblossey/houserules/commit/ea17fc66759900a20d54f83e5a0ae3dbfd27a9b4))

## [1.0.0](https://github.com/jblossey/houserules/compare/v0.3.0...v1.0.0) (2026-09-15)


### Features

* **v1:** the v1-readiness batch - the ratified template, the gate family, the frozen surface ([1d7951d](https://github.com/jblossey/houserules/commit/1d7951d92da447bf7477f24360b5470afd01861b))


### Bug Fixes

* **release:** create-release = false stops dist colliding with release-please ([0e30f55](https://github.com/jblossey/houserules/commit/0e30f5535f8d6f07c48c6f1337cd8bf4713111d3))


### Miscellaneous Chores

* **release:** cut 1.0.0 ([3203f9e](https://github.com/jblossey/houserules/commit/3203f9e80694fb6b99e86591b1a40851c1b49a14))

## [0.3.0](https://github.com/jblossey/houserules/compare/v0.2.0-alpha...v0.3.0) (2026-09-13)


### ⚠ BREAKING CHANGES

* retire everything JS - the repository runs on cargo and the binary alone

### Features

* **archive:** the sweep - retired records leave the active set (HR-105) ([a8192a7](https://github.com/jblossey/houserules/commit/a8192a70605299a4462ce54164b6b30c5ee025ae))
* **cli:** the payload lives inside the binary; the flat surface completes ([16e06a2](https://github.com/jblossey/houserules/commit/16e06a278131b026a5273b66b8a0343cec3d93a7))
* **deliverable:** declare the task-id shape once; the two record riders (HR-103, HR-095, HR-096) ([2d3de9f](https://github.com/jblossey/houserules/commit/2d3de9f185d731e54724a300c37a34c685580a19))
* **dev-bins:** walkdir replaces the hand-rolled walkers (HR-079, ruled 5.47) ([9deb69e](https://github.com/jblossey/houserules/commit/9deb69ed9ea30c90980007d5d7320f217abfb5a6))
* **houserules:** ship check-report-claims through the binary with the paste-run lint ([85c1b41](https://github.com/jblossey/houserules/commit/85c1b41b4cf622b0e4a63ad4afd0c35538f02337))
* **houserules:** the report/audit gates - ephemeral paths and --sanctioned ([428b41e](https://github.com/jblossey/houserules/commit/428b41e66c75e25d8f568a521ff51b9c1e0bb67f))
* **install:** the downstream update model - baselines, overrides, backfill (HR-099, HR-089) ([a3197ff](https://github.com/jblossey/houserules/commit/a3197ff7cbc30bfdbcb4bd11ff02a1686f39b10d))
* **kb:** the check-side gates - dead globs, emitter round-trip, textList floor ([ad04dbe](https://github.com/jblossey/houserules/commit/ad04dbe97853d1fe2941a0a42c5fb2c2555fc857))
* **release:** the cargo-dist pipeline lands, pinned and resolving ([ecc0daa](https://github.com/jblossey/houserules/commit/ecc0daa8930f0b71ec1f0939cb5e1114d78930d1))
* **release:** the machinery - PAT trigger, plan mode, the payload-stamp attribution ([b7ef9fe](https://github.com/jblossey/houserules/commit/b7ef9fe2efff394a1bdf964f543603011d0a0798))
* retire everything JS - the repository runs on cargo and the binary alone ([2ca3036](https://github.com/jblossey/houserules/commit/2ca3036fe30fd37cf2b10dc70e3006b508ede609))
* **rust:** phase 1 of the tier-2 port - scaffold, render, check-knowledge ([0bb27a0](https://github.com/jblossey/houserules/commit/0bb27a0eda54a9dd2b8ff0a7f2c19d602c89b111))
* **rust:** phase 2 of the tier-2 port - the check runners and every read surface ([0581bbf](https://github.com/jblossey/houserules/commit/0581bbf748ec8443a0083b278c91e0916c6a4aba))
* **tests:** port the JS test surface to cargo behind the T1 deletion license ([2174a9f](https://github.com/jblossey/houserules/commit/2174a9fbf0e92402c604e5235a50758df69c4133))
* **tools:** close batch 21 - the retirement, portability, and the records ([19b75c4](https://github.com/jblossey/houserules/commit/19b75c41ef5b46903abc87bfb3c2491337078341))
* **tools:** close Tier 2 - the sweep, the residue gate, the evals record, the release flip ([b947e78](https://github.com/jblossey/houserules/commit/b947e781a6d29e0b0fda82e568e03dd86c24e3f3))


### Bug Fixes

* **check-commit:** exempt a bot-authored commit from the body-line limit ([99e88af](https://github.com/jblossey/houserules/commit/99e88af6ab13fdd4c92a53ee1d6ae8bae6232a4e))
* **release:** the release PR passes its own gates; the pre-aggregation pass ([eeb5708](https://github.com/jblossey/houserules/commit/eeb570825b32cc3d535a5a58efebe0ec24a28529))
* **rust:** collapse dot-dot components in validate's path resolution ([9d46a8c](https://github.com/jblossey/houserules/commit/9d46a8ce39a5ea10192ef2056c16d3eca7277969))
* **rust:** pin the CLI's bin name so Windows usage text says houserules ([a1e8a1f](https://github.com/jblossey/houserules/commit/a1e8a1f82c5c059af14d07f0cd11e52dd18906bd))
* **rust:** strip a Windows verbatim prefix from validate's resolved paths ([5695376](https://github.com/jblossey/houserules/commit/5695376327b9c6c0f8ac44b84fbcb38bfcfd5398))
* **tests:** pin the frozen worktree checkout to LF on every platform ([1ac5986](https://github.com/jblossey/houserules/commit/1ac5986e517cd0eefe3d233145e9dba908321aa0))


### Miscellaneous Chores

* **release:** re-aim the next release at 0.3.0 ([b35d321](https://github.com/jblossey/houserules/commit/b35d32175de19cc8251353ed908b69bd6030951f))

## [0.2.0-alpha](https://github.com/jblossey/houserules/compare/houserules-v0.1.0...houserules-v0.2.0-alpha) (2026-09-03)


### Features

* **cli:** update reports the kit version drift ([180e549](https://github.com/jblossey/houserules/commit/180e5492bfc7541a9d7d8934e7fb9fad99d5b1f4))


### Bug Fixes

* **template:** the capture form and the decidable collision line ([945e3ce](https://github.com/jblossey/houserules/commit/945e3ceb2704a3db1b09fc426be26ea22bb71acc))
