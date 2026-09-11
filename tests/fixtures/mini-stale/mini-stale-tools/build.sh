#!/bin/sh
# Fixture-only placeholder (HR-074): gives the `tools` area's own
# `mini-stale-tools/**` glob (knowledge/areas.json) a real file to match,
# so the dead-glob gate finds this corpus clean for the SAME reason
# `tests/fixtures/mini/mini-tools/build.sh` does -- this fixture's own
# purpose (a stale render plus a stray rule file) is unrelated to glob
# liveness.
echo "mini-stale-tools fixture placeholder"
