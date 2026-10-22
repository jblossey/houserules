#!/bin/sh
# The fake tool of the runner check tests. Usage:
#   fake-sarif.sh <sarif-file> <exit-status> <marker-file> [file ...]
# It appends the number of file arguments to <marker-file> (one line for each
# call, so a test counts the calls and the size of each), prints <sarif-file>
# on stdout, and exits with <exit-status>.
sarif=$1
status=$2
marker=$3
shift 3
printf '%s\n' "$#" >>"$marker"
cat "$sarif"
exit "$status"
