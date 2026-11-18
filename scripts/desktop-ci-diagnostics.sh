#!/usr/bin/env bash
# Temporary: surface compiler output while the Actions log storage endpoint is unavailable.
set -u
export CARGO_TERM_COLOR=never
output=$(mktemp)
trap 'rm -f "$output"' EXIT
"$@" >"$output" 2>&1
status=$?
cat "$output"
if ((status != 0)); then
  tail_text=$(tail -c 6500 "$output")
  tail_text=${tail_text//%/%25}
  tail_text=${tail_text//$'\r'/%0D}
  tail_text=${tail_text//$'\n'/%0A}
  printf '::error title=Cargo diagnostics::%s\n' "$tail_text"
fi
exit "$status"
