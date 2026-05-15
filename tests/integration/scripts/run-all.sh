#!/usr/bin/env bash
# Run every zombienet scenario in order. Stops on the first failure.
set -euo pipefail
HARNESS="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$HARNESS"

# Every scenario uses `Creds: /tmp/zn-creds.cfg` (a zombienet syntactic
# requirement). Native provider does not actually consume the file; an
# empty placeholder is enough.
CREDS="${ZN_CREDS:-/tmp/zn-creds.cfg}"
[[ -f "$CREDS" ]] || touch "$CREDS"

LOG_DIR="/tmp/zn-run-all-logs"
rm -rf "$LOG_DIR"
mkdir -p "$LOG_DIR"

SCENARIOS=(
    "scenarios/00-basic-and-finality.zndsl"
    "scenarios/01-validator-lifecycle.zndsl"
    "scenarios/02-validator-mass-offline.zndsl"
    "scenarios/03-hashrate-fluctuation.zndsl"
    "scenarios/04-network-partition.zndsl"
    "scenarios/05-validator-offline-kick.zndsl"
    "scenarios/06-max-validators-capacity.zndsl"
    "scenarios/07-equivocation-placeholder.zndsl"
    "scenarios/08-evm-tooling.zndsl"
)

fail=0
failed_list=()
for s in "${SCENARIOS[@]}"; do
    echo
    echo "=========================================================="
    echo ">>> $s"
    echo "=========================================================="
    log_file="$LOG_DIR/$(basename "$s" .zndsl).log"
    if ! zombienet -p native test "$s" 2>&1 | tee "$log_file"; then
        echo "FAIL: $s"
        fail=$((fail + 1))
        failed_list+=("$s")
    fi
done

echo
if [[ $fail -eq 0 ]]; then
    echo "All scenarios passed."
else
    echo "$fail scenario(s) failed:"
    for f in "${failed_list[@]}"; do
        echo
        echo "=========================================================="
        echo "FAIL: $f"
        echo "=========================================================="
        log_file="$LOG_DIR/$(basename "$f" .zndsl).log"
        # Show failed assertions (❌ lines) with 3 lines of context above each
        grep -n "❌\|Result:" "$log_file" | head -40 || true
        echo "--- custom script output ---"
        grep -v "^[[:space:]]*$" "$log_file" | grep -v "^┌\|^│\|^└" | tail -20 || true
    done
    echo
    echo "Full logs saved to: $LOG_DIR"
    exit 1
fi
