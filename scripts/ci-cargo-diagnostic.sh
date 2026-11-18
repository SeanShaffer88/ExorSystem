#!/usr/bin/env bash
# Temporary: expose runner diagnostics through the check API while fixing this branch.
log="$RUNNER_TEMP/exor-cargo.log"
if "$@" > "$log" 2>&1; then
    cat "$log"
else
    cat "$log"
    tail=$(tail -n 80 "$log")
    tail=${tail//%/%25}
    tail=${tail//$'\r'/%0D}
    tail=${tail//$'\n'/%0A}
    printf '::error title=Cargo diagnostics::%s\n' "$tail"
    exit 1
fi
