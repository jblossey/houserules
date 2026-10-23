#!/bin/sh
# SPDX-License-Identifier: MIT
# Copyright (c) 2026 Jannis Blossey
# The two calls that any workflow makes around a change. Output goes to
# .houserules/step/ below the install root, so an agent reads it with its
# file-read tool; the script prints each file's path and each exit status.
# The directory holds a .gitignore of `*`, so its output never shows in
# `git status`, whether or not the project ignores it.
# Paths and refs are relative to the install root, the parent of the directory
# that holds this script, found through the script's own symlinks.
#   houserules-step.sh before <path>...      the rules for the files to change
#   houserules-step.sh after <base> [<report>]  the audit since <base>, and the
#                                               facts and the validation of
#                                               <report>
set -u

usage() {
  echo "usage: houserules-step.sh before <path>... | after <base> [<report>]" >&2
  exit 2
}

# Prints the target of the symlink `$1`, byte for byte.
read_link() {
  if command -v readlink >/dev/null 2>&1; then
    readlink -- "$1"
  else
    # Not every supported environment ships readlink. The listing ends in
    # `<link> -> <target>`; GNU ls quotes the target unless QUOTING_STYLE is
    # literal, and honors that variable even when its output is no terminal.
    # shellcheck disable=SC2012
    listing=$(QUOTING_STYLE=literal ls -ld -- "$1")
    printf '%s\n' "${listing#* -> }"
  fi
}

# Prints the install root of this script; fails when it cannot resolve one.
resolve_install_root() {
  script_path="$0"
  hops=0
  while [ -h "$script_path" ] && [ "$hops" -lt 40 ]; do
    link_target=$(read_link "$script_path") || return 1
    case "$link_target" in
      /*) script_path="$link_target" ;;
      *) script_path="$(dirname -- "$script_path")/$link_target" ;;
    esac
    hops=$((hops + 1))
  done
  (CDPATH='' cd -- "$(dirname -- "$script_path")/.." 2>/dev/null && pwd)
}

# Creates the output directory below the install root and makes it ignore
# itself. Runs before the first output.
prepare_output_directory() {
  cd "$root" && mkdir -p .houserules/step && printf '*\n' > .houserules/step/.gitignore
}

if ! root=$(resolve_install_root); then
  echo "houserules-step.sh: cannot resolve the install root of the script" >&2
  exit 2
fi
if [ ! -f "$root/.houserules.json" ]; then
  echo "houserules-step.sh: no .houserules.json in the parent of the directory of the script" >&2
  exit 2
fi

mode=${1:-}
case "$mode" in
  before)
    [ "$#" -ge 2 ] || usage
    shift
    prepare_output_directory || exit 2
    houserules for "$@" > .houserules/step/rules.json
    status=$?
    echo "$root/.houserules/step/rules.json"
    exit "$status"
    ;;
  after)
    { [ "$#" -ge 2 ] && [ "$#" -le 3 ]; } || usage
    base=$2
    report=${3:-}
    prepare_output_directory || exit 2
    # The result of an earlier run must never reach report-facts.
    rm -f .houserules/step/audit.json
    set -- audit --base "$base" --json .houserules/step/audit.json
    [ -z "$report" ] || set -- "$@" --report "$report"
    houserules "$@" > /dev/null
    audit_status=$?
    echo "$root/.houserules/step/audit.json"
    echo "audit exit status: $audit_status"
    status=$audit_status
    if [ -n "$report" ]; then
      set -- report-facts "$report" --base "$base"
      [ ! -f .houserules/step/audit.json ] || set -- "$@" --audit .houserules/step/audit.json
      houserules "$@" > .houserules/step/report-facts.txt 2>&1
      facts_status=$?
      echo "$root/.houserules/step/report-facts.txt"
      echo "report-facts exit status: $facts_status"
      [ "$status" -ne 0 ] || status=$facts_status
      houserules validate "$report" > .houserules/step/validate.json
      validate_status=$?
      echo "$root/.houserules/step/validate.json"
      echo "validate exit status: $validate_status"
      [ "$status" -ne 0 ] || status=$validate_status
    fi
    exit "$status"
    ;;
  *)
    usage
    ;;
esac
