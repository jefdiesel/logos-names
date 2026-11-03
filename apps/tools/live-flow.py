#!/usr/bin/env python3
"""The Testimonial app's flow, live: connect -> account selection -> approval -> transaction,
through the names module's Provider SDK in a logosctl session (logosctl is the host, so it
both asks, as the app "host", and approves, as the Names app would). The payload is built by
the app's own logic.mjs (live.mjs). Local rc1 chain only.

Before it: LIVE_DIR=<scratch> live-chain.sh (chain on 127.0.0.1:3220, wallet, program, $LIVE_DIR/env),
then a logosctl daemon whose HOME is a scratch folder (the names module keeps its settings
in $HOME/.config/logos-names, which must never be the person's):
  HOME=$LIVE_DIR/home logosctl --config-dir $LIVE_DIR/session daemon start &
  logosctl --config-dir $LIVE_DIR/session install -y <names lgx-portable>; ... module load names
  logosctl ... call names set_sequencer str:http://127.0.0.1:3220   (before any other call)
  logosctl ... call names set_cli str:<names CLI>; ... set_cli_wallet str:$LIVE_DIR/wallet
Run: LIVE_DIR=<scratch> NODE=node python3 live-flow.py"""
import json, os, subprocess, sys, time

HERE = os.path.dirname(os.path.abspath(__file__))
L = os.environ["LIVE_DIR"]  # the directory live-chain.sh used
LC = os.environ.get("LOGOSCTL", L + "/lc/bin/logosctl")
S = L + "/session"
LOGIC = os.path.join(HERE, "..", "testimonial", "src", "qml", "logic.mjs")
NODE = os.environ.get("NODE", "node")
env = dict(line.strip().split("=", 1) for line in open(L + "/env") if "=" in line)
F, B, TST, RPC = "Public/" + env["F"], "Public/" + env["B"], env["TST"], env["RPC"]
SUB = "lp-0021:names-wallet"
failures = 0


def call(method, *args):
    out = subprocess.run([LC, "--config-dir", S, "call", "names", method, *["str:" + a for a in args]],
                         capture_output=True, text=True, timeout=200).stdout
    reply = json.loads(out)["result"]
    if isinstance(reply, str):
        reply = json.loads(reply)
    if not reply["success"]:
        raise RuntimeError(reply["error"])
    return reply["value"]


def js(*args):
    out = subprocess.run([NODE, os.path.join(HERE, "live.mjs"), LOGIC, RPC, *args], capture_output=True, text=True, timeout=120)
    if out.returncode:
        raise RuntimeError(out.stderr)
    return json.loads(out.stdout)


def check(label, ok, detail=""):
    global failures
    failures += 0 if ok else 1
    print(("ok   " if ok else "FAIL ") + label + (("  " + detail) if detail else ""), flush=True)


def wait(request_id):
    for _ in range(120):
        v = call("request_status", request_id)
        if v["state"] not in ("pending", "running"):
            return v
        time.sleep(2)
    return v


def post(author, signer, username, text):
    """The app's steps 3-4, then the Names app's approval with `signer`."""
    made = js("call", TST, SUB, author, username, text)
    if made.get("already"):
        return made, None
    print("     payload: now=%s (chain %s) instruction %d bytes, accounts %s" % (made["now"], made["chain_time"], len(made["instruction_hex"]) // 2, made["accounts_json"][:60] + "…"))
    v = call("propose_call", made["program"], made["accounts_json"], made["instruction_hex"], made["summary"])
    pending = [r for r in call("pending_requests") if (r.get("request") or r.get("id")) == v["request"]]
    if pending:
        p = pending[0]
        print("     sheet: app=%s kind=%s choices=%s" % (p.get("app"), p.get("kind"), [c["account"][:14] for c in p.get("choices", [])]))
        print("     effects: %s" % p.get("effects"))
    started = time.time()
    try:
        approved = call("approve_request", v["request"], signer)
        print("     approve_request -> state %s (%.0fs)" % (approved.get("state"), time.time() - started))
    except RuntimeError as error:
        print("     approve_request refused: %s" % error)
    return made, wait(v["request"])


# 1. Connect.
v = call("request_access", "Post a testimonial signed by one of your public accounts")
check("request_access queues a connect request", v.get("state") == "pending", v.get("request", ""))
pending = call("pending_requests")
mine = [r for r in pending if (r.get("request") or r.get("id")) == v["request"]]
check("the Names app sees it", len(mine) == 1, "choices %d" % (len(mine[0]["choices"]) if mine else 0))
call("approve_request", v["request"], F + "," + B)
v = wait(v["request"])
check("connect approved with F and B", v["state"] == "approved" and set(v.get("accounts") or []) >= {F, B}, str(v.get("accounts")))
# 2. Account selection: the granted public accounts.
g = call("granted_accounts")
check("granted_accounts", g["connected"] and {a["account"] for a in g["accounts"]} >= {F, B}, str(g["accounts"]))
bal = call("account_balances")
print("     balances:", [(b["account"][:14], b["balance"]) for b in bal])

# 3-4. A post the person approves with the account picked in the app.
made, status = post(F, F, "ada", "Names Wallet signed this; the app never saw a key.")
check("post from F approved with F is done", status and status["state"] == "done" and bool(status.get("tx")), json.dumps(status))
listing = js("list", TST, SUB)
check("the app's list reads it back", listing.get("count") == 1 and listing["items"][0]["author"] == env["F"] and listing["items"][0]["username"] == "ada", json.dumps(listing)[:300])

# Picked B in the app, but approved with F in Names: refused before anything is sent.
before = subprocess.run(["curl", "-s", "-X", "POST", RPC, "-H", "Content-Type: application/json", "-d",
                         json.dumps({"jsonrpc": "2.0", "id": 1, "method": "getAccount", "params": [env["F"]]})], capture_output=True, text=True).stdout
made, status = post(B, F, "-", "Approved with the wrong account.")
after = subprocess.run(["curl", "-s", "-X", "POST", RPC, "-H", "Content-Type: application/json", "-d",
                        json.dumps({"jsonrpc": "2.0", "id": 1, "method": "getAccount", "params": [env["F"]]})], capture_output=True, text=True).stdout
check("a signer other than the picked account fails", status and status["state"] == "failed", json.dumps(status))
check("... and nothing was sent (F's nonce unchanged)", json.loads(before)["result"]["nonce"] == json.loads(after)["result"]["nonce"],
      "nonce %s -> %s" % (json.loads(before)["result"]["nonce"], json.loads(after)["result"]["nonce"]))

# The same post approved with B, as the app asks.
made, status = post(B, B, "-", "Second account, no username, and a line break:\nworks.")
check("post from B approved with B is done", status and status["state"] == "done", json.dumps(status))
listing = js("list", TST, SUB)
check("the list has both, newest first, per-month counts", listing.get("count") == 2 and [i["author"] for i in listing["items"]] == [env["B"], env["F"]]
      and listing["summary"]["total"] == 2, json.dumps(listing["summary"]))
print(json.dumps(listing, indent=1)[:1500])

# One per account: the app's pre-check sees F's testimonial before proposing.
made, status = post(F, F, "-", "again")
check("a second post from F is caught before proposing", made.get("already") is True and status is None)

print("ALL PASS" if failures == 0 else "%d FAILED" % failures)
