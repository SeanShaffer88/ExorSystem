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
  # Check annotations truncate at 4 KiB. Preserve failure details from the end, not
  # the earlier progress lines, and split longer excerpts so none are silently lost.
  tail -c 12000 "$output" | split -b 3000 - "$output.part-"
  for part in "$output".part-*; do
    excerpt=$(cat "$part")
    excerpt=${excerpt//%/%25}
    excerpt=${excerpt//$'\r'/%0D}
    excerpt=${excerpt//$'\n'/%0A}
    printf '::error title=Cargo diagnostics::%s\n' "$excerpt"
    rm -f "$part"
  done
fi
exit "$status"
