#!/bin/sh
# Fixture-only placeholder (HR-074): gives the `tools` area's own
# `mini-tools/**` glob (knowledge/areas.json) a real file to match, so the
# dead-glob gate finds this corpus clean the way it always should have --
# `read_parity.rs`'s `for mini-tools/build.sh` golden already names this
# exact path for its own, unrelated `for`-command parity slice.
echo "mini-tools fixture placeholder"
