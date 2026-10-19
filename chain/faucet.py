#!/usr/bin/env python3
"""Faucet for Logos Names on the public LEZ testnet.

POST /faucet {"account": "<base58 account id>"} sends DRIP base units from the
faucet account. Listens on localhost only; the Cloudflare tunnel
(lez.buggers.online/faucet) forwards to it with the caller's address in
CF-Connecting-IP. Limits: each account once a day, each address once a day,
and a daily total.
"""
import json
import os
import re
import subprocess
import threading
import time
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

HOME = os.path.expanduser("~/lez-chain")
NAMES = os.environ.get("FAUCET_NAMES", f"{HOME}/bin/names")
WALLET = os.environ.get("FAUCET_WALLET", f"{HOME}/faucet-official")
RPC = os.environ.get("FAUCET_RPC", "https://testnet.lez.logos.co")
with open(f"{WALLET}/faucet-account.txt") as f:
    FAUCET = f.read().strip().removeprefix("Public/")
# One name's 1M deposit and 1M to use: moving to private and private changes
# take no fee, so none of it goes to gas.
DRIP = int(os.environ.get("FAUCET_DRIP", 2_000_000))
# The faucet's own transfer must cover its fee reserve (the CLI gives a native
# transfer a gas limit of 1,000; about 3,000 is charged), or it is included,
# fails and is charged anyway. Kept generous.
RESERVE = 100_000
PER_ADDRESS = 1
PER_DAY = int(os.environ.get("FAUCET_PER_DAY", 50))
DAY = 86400
STATE = f"{HOME}/faucet-state.json"
ACCOUNT = re.compile(r"^[1-9A-HJ-NP-Za-km-z]{32,44}$")
ENV = dict(os.environ, LEE_WALLET_HOME_DIR=WALLET, LOGOS_NAMES_RPC=RPC)

def balance(account):
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": "getAccountBalance",
                       "params": {"account_id": account}}).encode()
    request = urllib.request.Request(RPC, body, {"Content-Type": "application/json"})
    with urllib.request.urlopen(request, timeout=20) as response:
        return int(json.load(response)["result"])


lock = threading.Lock()  # guards the state file
sending = threading.Lock()  # one transfer in flight at a time


def forget(record):
    with lock:
        state = load()
        state["sends"] = [s for s in state["sends"] if s != record]
        save(state)


def confirm(process, record, before):
    """Waits until the tokens arrive. LEZ includes failed transactions, and a
    late inclusion looks like a timeout to the wallet, so the recipient's
    balance is what counts. A send that never arrives is forgotten, so the
    account may ask again."""
    try:
        process.communicate(timeout=600)
    except subprocess.TimeoutExpired:
        process.kill()
    try:
        for _ in range(60):
            try:
                if balance(record["account"]) >= before + DRIP:
                    return
            except (OSError, ValueError, KeyError):
                pass
            time.sleep(5)
        forget(record)
        print(f"send to {record['account']} never arrived", flush=True)
    finally:
        sending.release()


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
        # Through the Cloudflare tunnel the caller's address is in CF-Connecting-IP;
        # the old gateway Worker put it in X-Client-IP.
        address = self.headers.get("CF-Connecting-IP") or self.headers.get("X-Client-IP", "local")

        # One send at a time: a signer gets one transaction per block, and the
        # faucet's balance must cover each transfer plus its fee ceiling.
        if not sending.acquire(blocking=False):
            return self.reply(409, {"error": "the faucet is busy sending; try again in a minute"})
        handed_off = False  # once the confirming thread owns `sending`, it releases it
        try:
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
                try:
                    before = balance(account)
                    funds = balance(FAUCET)
                except (OSError, ValueError, KeyError):
                    return self.reply(409, {"error": "cannot reach the chain; try again shortly"})
                if funds < DRIP + RESERVE:
                    return self.reply(409, {"error": "the faucet is empty until its next refill; "
                                            "you can bridge tokens from the Logos Blockchain instead"})
                record = {"account": account, "address": address, "at": now}
                state["sends"] = sends + [record]
                save(state)
            process = subprocess.Popen(
                [NAMES, "wallet", "auth-transfer", "send", "--from", f"Public/{FAUCET}",
                 "--to", f"Public/{account}", "--amount", str(DRIP)],
                env=ENV, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
            tx = None
            for line in process.stdout:
                found = re.search(r"Transaction hash is ([0-9a-f]{64})", line)
                if found:
                    tx = found.group(1)
                    break
            if not tx:
                process.wait()
                forget(record)
                self.log_message("send failed before submitting")
                return self.reply(422, {"error": "the transfer could not be sent; try again shortly"})
            # Inclusion on the public testnet can take minutes, longer than a
            # proxied request may stay open: answer now, confirm in the background.
            threading.Thread(target=confirm, args=(process, record, before), daemon=True).start()
            handed_off = True
        finally:
            if not handed_off:
                sending.release()
        self.reply(200, {"account": account, "amount": str(DRIP), "tx": tx})

    def do_GET(self):
        if self.path == "/faucet":
            return self.reply(200, {"amount": str(DRIP), "per_account": "once a day"})
        self.reply(404, {"error": "not found"})


if __name__ == "__main__":
    ThreadingHTTPServer(("127.0.0.1", 3041), Handler).serve_forever()
