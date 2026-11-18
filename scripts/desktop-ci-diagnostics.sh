#!/usr/bin/env bash
# Temporary: surface compiler errors when the Actions log-storage endpoint cannot be reached.
set -u
export CARGO_TERM_COLOR=never
output=$(mktemp)
trap 'rm -f "$output"' EXIT
"$@" >"$output" 2>&1
status=$?
cat "$output"
if ((status != 0)); then
  excerpt=$(tail -c 11500 "$output")
  excerpt=${excerpt//%/%25}
  excerpt=${excerpt//$'\r'/%0D}
  excerpt=${excerpt//$'\n'/%0A}
  printf '::error title=Cargo diagnostics::%s\n' "$excerpt"
fi
exit "$status"
