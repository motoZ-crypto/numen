#!/usr/bin/env bash
# Build the solochain-template-node binary with the `test-runtime` feature.
# This binary MUST be used by start-network.sh; otherwise session and
# validator timing constants are too long to observe within the test budget.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT"

PROFILE="${PROFILE:-release}"

if [[ "$PROFILE" == "release" ]]; then
  cargo build --release -p solochain-template-node --features test-runtime
else
  cargo build -p solochain-template-node --features test-runtime
fi

BIN="$ROOT/target/$PROFILE/solochain-template-node"
echo "Built: $BIN"
"$BIN" --version

# Pre-generate the raw chainspec consumed by zombienet.toml. zombienet
# would otherwise rebuild a spec from --chain integration on every spawn
# and overwrite our pre-registered session keys with auto-generated ones.
SPEC="$ROOT/tests/integration/integration-raw.json"
echo "Generating raw chainspec at $SPEC"
"$BIN" build-spec --chain integration --raw --disable-default-bootnode > "$SPEC"
echo "Raw spec: $(wc -c < "$SPEC") bytes"
