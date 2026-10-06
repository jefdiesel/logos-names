#!/bin/bash
# Starts a standalone LEZ v0.3.0 sequencer for local development.
#
#   LEZ_DIR    checkout of logos-execution-zone at tag v0.3.0
#   SEQ_BIN    sequencer_service built with `--features standalone`
#   SEQ_HOME   directory for the chain's state; reuse it to keep the chain
#
# The RPC has no caller authentication, so it binds to loopback only.
set -euo pipefail
: "${LEZ_DIR:?set LEZ_DIR to the logos-execution-zone checkout}"
: "${SEQ_BIN:?set SEQ_BIN to the sequencer_service binary}"
: "${SEQ_HOME:?set SEQ_HOME to the chain state directory}"
PORT=${PORT:-3040}
METRICS=${METRICS:-127.0.0.1:9140}

if curl -sf -m 3 "http://127.0.0.1:$PORT" -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"checkHealth","params":[]}' >/dev/null; then
  echo "a sequencer already answers on port $PORT"
  exit 0
fi

mkdir -p "$SEQ_HOME"
cd "$LEZ_DIR/lez/sequencer/service"
RUST_LOG=info,kameo=warn nohup "$SEQ_BIN" configs/debug/sequencer_config.json \
  --port "$PORT" --listen-address 127.0.0.1 --home "$SEQ_HOME" \
  --metrics-address "$METRICS" >"$SEQ_HOME/sequencer.log" 2>&1 </dev/null &
echo "started sequencer (pid $!), log at $SEQ_HOME/sequencer.log"
