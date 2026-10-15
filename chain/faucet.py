#!/usr/bin/env python3
"""Faucet for Logos Names on the public LEZ testnet.

POST /faucet {"account": "<base58 account id>"} sends DRIP base units from the
faucet account. Listens on localhost only; the gateway forwards to it and puts
the caller's address in X-Client-IP. Limits: each account once a day, each
address a few times a day, and a daily total.
"""
import json
import os
import re
import subprocess
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

HOME = os.path.expanduser("~/lez-chain")
NAMES = os.environ.get("FAUCET_NAMES", f"{HOME}/bin/names")
WALLET = os.environ.get("FAUCET_WALLET", f"{HOME}/faucet-official")
RPC = os.environ.get("FAUCET_RPC", "https://testnet.lez.logos.co")
with open(f"{WALLET}/faucet-account.txt") as f:
    FAUCET = f.read().strip().removeprefix("Public/")
# Enough to register a name: the sequencer admits a transaction only if the
# payer covers its fee ceiling (gas limit 2,000,000 x base fee 8 = 16M).
DRIP = int(os.environ.get("FAUCET_DRIP", 20_000_000))
PER_ADDRESS = 3
PER_DAY = 500
DAY = 86400
STATE = f"{HOME}/faucet-state.json"
ACCOUNT = re.compile(r"^[1-9A-HJ-NP-Za-km-z]{32,44}$")
ENV = dict(os.environ, LEE_WALLET_HOME_DIR=WALLET, LOGOS_NAMES_RPC=RPC)

lock = threading.Lock()  # one send at a time: one transaction per signer per block


def load():
    try:
        with open(STATE) as f:
            return json.load(f)
    except (OSError, ValueError):
        return {"sends": []}


def save(state):
    tmp = STATE + ".tmp"
    with open(tmp, "w") as f:
        json.dump(state, f)
    os.replace(tmp, STATE)


class Handler(BaseHTTPRequestHandler):
    def reply(self, code, body):
        data = json.dumps(body).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_POST(self):
        if self.path != "/faucet":
            return self.reply(404, {"error": "not found"})
        try:
            length = min(int(self.headers.get("Content-Length", 0)), 4096)
            account = json.loads(self.rfile.read(length))["account"].strip()
            account = account.removeprefix("Public/")
        except (ValueError, KeyError, TypeError, AttributeError):
            return self.reply(400, {"error": 'send {"account": "<account id>"}'})
        if not ACCOUNT.match(account):
            return self.reply(400, {"error": "that is not a public account ID"})
        address = self.headers.get("X-Client-IP", "local")

        with lock:
            now = time.time()
            state = load()
            sends = [s for s in state["sends"] if now - s["at"] < DAY]
            if any(s["account"] == account for s in sends):
                return self.reply(429, {"error": "this account was funded in the last day"})
            if sum(s["address"] == address for s in sends) >= PER_ADDRESS:
                return self.reply(429, {"error": "too many requests from your address today"})
            if len(sends) >= PER_DAY:
                return self.reply(429, {"error": "the faucet's daily limit is spent; try tomorrow"})
            run = subprocess.run(
                [NAMES, "wallet", "auth-transfer", "send", "--from", f"Public/{FAUCET}",
                 "--to", f"Public/{account}", "--amount", str(DRIP)],
                env=ENV, capture_output=True, text=True, timeout=300)
            included = re.search(r"included in block (\d+)", run.stdout)
            if run.returncode != 0 or not included:
                self.log_message("send failed: %s %s", run.stdout[-300:], run.stderr[-300:])
                return self.reply(502, {"error": "the transfer did not go through; try again shortly"})
            sends.append({"account": account, "address": address, "at": now})
            state["sends"] = sends
            save(state)
        self.reply(200, {"account": account, "amount": str(DRIP), "block": int(included.group(1))})

    def do_GET(self):
        if self.path == "/faucet":
            return self.reply(200, {"amount": str(DRIP), "per_account": "once a day"})
        self.reply(404, {"error": "not found"})


if __name__ == "__main__":
    ThreadingHTTPServer(("127.0.0.1", 3041), Handler).serve_forever()
