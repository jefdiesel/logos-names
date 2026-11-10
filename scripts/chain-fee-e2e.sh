#!/bin/bash
# LP-0022 gas: names' fee estimate for each chain op (fee_about in the chain_* plans and in
# `names chain send`) against the fee the node then takes, over REAL Logos Core IPC
# (logosctl 0.3.2, the real blockchain_module 0.3.4, names) on a SOLO node with no peers.
# Start-up copied from scripts/chain-ipc-e2e.sh. Runs on the Mac mini with bash:
#   CHAIN_FEE_PORT_BASE=3920 NAMES_LGX=<names lgx dir> NAMES_BIN=<names CLI> \
#     /Volumes/exos162/logos/dev/fix-names/mini-lock.sh /bin/bash scripts/chain-fee-e2e.sh
#   CHAIN_FEE_DIR  work dir, wiped and recreated (default /Volumes/exos162/logos/dev/lp22/fees/e2e-<base>)
# For each op: two plans (the node's notes must not change, nothing reserved), then the send,
# then the fee the node took, read from the node: get_transaction's inputs (valued from the
# payer's notes before) less its outputs (less a deposit's note). Exits non-zero on any FAIL.
set -uo pipefail
T=/Volumes/exos162/logos/dev/chain-ipc
BASE=${CHAIN_FEE_PORT_BASE:-3920}
W=${CHAIN_FEE_DIR:-/Volumes/exos162/logos/dev/lp22/fees/e2e-$BASE}
CTL=$T/logosctl-0.3.2/bin/logosctl
MODULE=$T/real-module/blockchain_module
NET=$BASE BLEND=$((BASE + 1)) HTTP=$((BASE + 2))
say() { printf '%s\n' "$*"; }
die() { say "ABORT $*"; exit 2; }
for p in $NET $BLEND $HTTP $((BASE + 5)); do case $p in 8080|8081|3000|3001|3401|11434) die "port $p reserved";; esac; done
[ "$BASE" -ge 3597 ] && [ "$BASE" -le 3860 ] && die "port base in logos-34's range"
[ -n "${NAMES_LGX:-}" ] && [ -e "$NAMES_LGX" ] || die "set NAMES_LGX to the names lgx dir"
if lsof -nP -i :$NET -i :$BLEND -i :$HTTP -i :$((HTTP + 3)) >/dev/null 2>&1; then die "ports $NET-$((HTTP + 3)) busy"; fi
case "$W" in /Volumes/exos162/logos/names-0.4*|/Volumes/exos162/logos/dev/chain-ipc*|*wallet-testnet*|*user-test*) die "not in $W";; esac
if [ -d "$W" ] && [ -n "$(ls -A "$W")" ] && [ ! -e "$W/.chain-fee-e2e" ]; then die "$W is not empty and not made by this script"; fi
rm -rf "$W"; mkdir -p "$W/home" "$W/session/modules" && touch "$W/.chain-fee-e2e" || die "mkdir"
cd "$W" || die cd
export HOME=$W/home LOGOSCTL_CONFIG_DIR=$W/session
say "chain-fee-e2e $(date -u +%Y-%m-%dT%H:%M:%SZ) host $(hostname -s) work dir $W"
say "  names lgx $(readlink "$NAMES_LGX" || echo "$NAMES_LGX") sha256 $(shasum -a 256 "$NAMES_LGX"/*.lgx | cut -c1-64)"
say "  names CLI ${NAMES_BIN:-(none)} $( [ -n "${NAMES_BIN:-}" ] && shasum -a 256 "$NAMES_BIN" | cut -c1-64)"
cp "$T/spike/standalone-deployment-config.yaml" deployment.yaml
cp "$T/spike/standalone-node-config.yaml" standalone-node-config.yaml
printf '%s  deployment.yaml\n%s  standalone-node-config.yaml\n' 0929cddc3dc35a189dbdb1e8297ece5d80ca1247a9fda07c2fdfcf5330adf618 28b6729f2aeab5937d95192326a01102e0c9bd9594b40de1b154e7e3b5dea2cc | shasum -a 256 -c >/dev/null || die "fixture hash"
cp -R "$MODULE" "$W/session/modules/" || die "module copy"
STARTED=0
cleanup() {
  local rc=$?
  trap - EXIT INT TERM
  say ""; say "== stopping the session"
  [ -e "$W/.node-started" ] && [ ! -e "$W/.node-stopped" ] && { "$CTL" --json call blockchain_module stop || true; echo; }
  [ $STARTED = 1 ] && { "$CTL" daemon stop || true; }
  local left=""
  for _ in $(seq 20); do left=$(pgrep -fl "$W" || true); [ -z "$left" ] && break; sleep 1; done
  if [ -n "$left" ]; then say "LEFT RUNNING: $left"; rc=1; else say "session stopped: pgrep -fl $W prints nothing"; fi
  exit $rc
}
trap cleanup EXIT
trap 'exit 130' INT TERM
LOG_BACKEND=file LOG_DIR=$W LOG_PATH=node.log "$CTL" daemon start --detach || die "daemon start"
STARTED=1
export CTL W NET BLEND HTTP NAMES_LGX
export NAMES_BIN=${NAMES_BIN:-}
python3 -I -u - <<'PY'
# Helpers and the solo node start-up (from scripts/chain-ipc-e2e.sh), then the proof.
import json, os, re, subprocess, sys, time

CTL, W = os.environ["CTL"], os.environ["W"]
NET, BLEND, HTTP = (int(os.environ[k]) for k in ("NET", "BLEND", "HTTP"))
DAEMON_LOG = os.path.join(W, "session", "logs", "daemon.log")
A = "e3635f207984ae779cf76b5f20714b514373f61ff96260879fe0a6d71f2dce07"  # genesis 100000
B = "39e16b432574571a6bcd8ee36e370589641bd9f35367f6f97a972453c46c3225"  # genesis 100
os.chdir(W)


def say(text=""):
    print(text, flush=True)


def loads(text):
    if isinstance(text, str):
        try:
            return json.loads(text)
        except ValueError:
            return text
    return text


def call(module, method, *args, timeout=240):
    argv = [CTL, "--json", "call", module, method, *[str(a) for a in args]]
    t0 = time.time()
    try:
        p = subprocess.run(argv, capture_output=True, text=True, timeout=timeout, stdin=subprocess.DEVNULL)
    except subprocess.TimeoutExpired:
        return {"ok": False, "value": None, "error": f"timeout {timeout}s", "raw": "", "s": timeout}
    raw = p.stdout.strip()
    dt = round(time.time() - t0, 2)
    try:
        out = json.loads(raw)
    except ValueError:
        return {"ok": False, "value": None, "error": f"not JSON: {raw[:300]} {p.stderr.strip()[:300]}", "raw": raw, "s": dt}
    if out.get("status") != "ok":
        return {"ok": False, "value": None, "error": out.get("message") or raw, "raw": raw, "s": dt}
    r = loads(out.get("result"))
    if not isinstance(r, dict) or "success" not in r:
        return {"ok": True, "value": r, "error": None, "raw": raw, "s": dt}
    return {"ok": bool(r["success"]), "value": loads(r.get("value")), "error": r.get("error"), "raw": raw, "s": dt}


def node(method, *args, **kw):
    return call("blockchain_module", method, *args, **kw)


def names(method, *args, **kw):
    return call("names", method, *args, **kw)


def shown(r, n=1500):
    s = json.dumps(r["value"], sort_keys=True) if r["ok"] else f"ERR {r['error']}"
    return s[:n]


def show(label, r, n=1500):
    say(f"    {label} ({r.get('s')}s): {shown(r, n)}")


def ctl(*argv):
    p = subprocess.run([CTL, *argv], capture_output=True, text=True, timeout=120, stdin=subprocess.DEVNULL)
    return p.returncode, (p.stdout + p.stderr).strip()


def daemon_log():
    try:
        with open(DAEMON_LOG, errors="replace") as f:
            return f.read()
    except OSError:
        return ""


def balance(key):
    r = node("wallet_get_balance", key)
    return int(str(r["value"])) if r["ok"] and str(r["value"]).isdigit() else None


def notes(key):
    r = node("wallet_get_notes", key, "")
    if not r["ok"]:
        return None
    return {n["id"]: int(n["value"]) for n in r["value"]["notes"]}


def blocks_since(mark):
    out = []
    for line in daemon_log()[mark:].splitlines():
        if "Received new block: " in line:
            try:
                out.append(json.loads(line.split("Received new block: ", 1)[1]))
            except ValueError:
                pass
    return out


def wait_landed(key, inputs, seconds=120):
    end = time.time() + seconds
    while time.time() < end:
        n = notes(key) or {}
        if inputs and not (set(inputs) & set(n)):
            return True
        time.sleep(0.5)
    return False


def abort(why):
    say(f"ABORT {why}")
    node("stop")
    open(os.path.join(W, ".node-stopped"), "w").close()
    sys.exit(2)


say(f"== session {time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())} work dir {W}, ports {NET}/{BLEND}/{HTTP}")
if os.environ.get("NAMES_LGX"):
    rc, out = ctl("install", "--dir", os.environ["NAMES_LGX"], "-y")
    say(f"    install names: exit {rc} {out[-200:] if rc else ''}")
mods = ["blockchain_module"] + (["names"] if os.environ.get("NAMES_LGX") else [])
for m in mods:
    rc, out = ctl("module", "load", m)
    say(f"    module load {m}: {out}")
    if rc != 0:
        sys.exit(2)

gen = {"skip_ibd": True, "net_port": NET, "blend_port": BLEND, "http_addr": f"127.0.0.1:{HTTP}",
       "output": "user_config.yaml", "use_persistence_paths": True}
json.dump(gen, open("gen-args.json", "w"))
g = node("generate_user_config", "@gen-args.json")
if not g["ok"]:
    abort(f"generate_user_config {shown(g)}")
CFG = str(g["value"])
KS = os.path.join(os.path.dirname(CFG), "keystore.yaml")
fixture = open("standalone-node-config.yaml").read()
for key, title in ((A, "GenesisStake"), (B, "GenesisSmall")):
    found = re.search(rf"^\s*{key}: !Zk\s+(\S+)", fixture, re.M)
    r = node("add_key", KS, "zk", found.group(1), title)
    if not r["ok"]:
        abort(f"add_key {shown(r)}")
with open(CFG, "a") as f:
    f.write("cryptarchia:\n  service:\n    bootstrap:\n      prolonged_bootstrap_period: 5s\n")

say("== ISOLATION")
config = open(CFG).read()
peers = [line for line in config.splitlines() if re.search(r"peer|ibd", line, re.I)]
say(f"    grep -i -E 'peer|ibd' {CFG}: {peers if peers else '(no match)'}; gen-args skip_ibd: {gen['skip_ibd']}")
if peers:
    abort("PEERS IN CONFIG")
r = node("start", CFG, os.path.join(W, "deployment.yaml"))
open(os.path.join(W, ".node-started"), "w").close()
info = {}
for _ in range(40):
    time.sleep(1)
    i = node("get_cryptarchia_info")
    if i["ok"] and isinstance(i["value"], dict):
        info = i["value"]
        if info.get("mode") == "Online":
            break
say(f"    get_cryptarchia_info: {json.dumps(info, sort_keys=True)}")
if info.get("mode") != "Online":
    abort("not Online")
net = node("get_network_info")
show("get_network_info", net)
nv = net["value"] if net["ok"] and isinstance(net["value"], dict) else {}
skip = [l for l in daemon_log().splitlines() if "Skipping IBD as no peers are configured" in l]
say(f"    node log: {'Skipping IBD as no peers are configured' if skip else '(no Skipping IBD line)'}")
isolated = bool(nv) and all(nv.get(k) == 0 for k in ("n_peers", "n_connections", "n_pending_connections", "n_discovered_peers"))
if not (isolated and skip):
    abort("PEER SEEN or isolation unproven")
say("    ISOLATED: no peers configured, IBD skipped, zero peers/connections")


# ── the proof ──
# FEES lane proof: for each chain op, names' plan estimate (fee_about) before, and the fee
# the node actually took after, read from the node itself (get_transaction + the payer's
# notes from before). Plans must move and reserve nothing: checked with the node.
import http.server, threading

results = []
NAMES_BIN = os.environ.get("NAMES_BIN", "")
RUNNING = "Your Logos Blockchain node isn't running. Start it in Basecamp's Blockchain app."


def step(name, ok, detail=""):
    line = f"{'PASS' if ok else 'FAIL'} {name}" + (f" — {detail}" if detail else "")
    say(line)
    results.append(line)
    return ok


def tx_fee(h, before, deposit=0):
    """What the node took: every input the tx spends (values from the payer's notes before),
    less every transfer output, less a deposit's whole note."""
    t = node("get_transaction", h)
    if not t["ok"]:
        return None, t
    ops = t["value"]["mantle_tx"]["ops"]
    ins = [i for op in ops for i in op["payload"].get("inputs", [])]
    outs = sum(int(o["value"]) for op in ops if op["opcode"] == 0 for o in op["payload"]["outputs"])
    if not all(i in before for i in ins):
        return None, t
    return sum(before[i] for i in ins) - outs - deposit, t


def landed(key, inputs, seconds=120):
    return wait_landed(key, inputs, seconds)


def plan_twice_then_send(label, method, args_plan, args_send, payer, deposit=0):
    """Two plans, the node's notes unchanged by them, then the real send; estimate vs actual."""
    n0 = notes(payer)
    p1 = names(method, *args_plan)
    p2 = names(method, *args_plan)
    show(f"names {method} plan", p1)
    n1 = notes(payer)
    est = p1["value"].get("fee_about") if p1["ok"] else None
    step(f"{label}: the plan carries the node's estimate, twice the same, and moves nothing",
         p1["ok"] and p2["ok"] and est is not None and est == p2["value"].get("fee_about") and n1 == n0 and p1["value"].get("hash") is None,
         f"fee_about {est}, {p2['value'].get('fee_about') if p2['ok'] else p2['error']}; payer notes unchanged {n1 == n0}")
    s = names(method, *args_send)
    show(f"names {method} send", s)
    h = s["value"].get("hash") if s["ok"] else None
    if not h:
        step(f"{label}: sent after two plans (no note reserved by them)", False, shown(s))
        return est, None
    fee, t = tx_fee(h, n0, deposit)
    lands = landed(payer, [i for op in t["value"]["mantle_tx"]["ops"] for i in op["payload"].get("inputs", [])] if t["ok"] else [])
    n2 = notes(payer)
    say(f"    node: tx {h[:12]}… spends {len(set(n0) - set(n2))} of the payer's notes; fee from the node's tx = {fee}; names reply fee {s['value'].get('fee')}")
    step(f"{label}: sent after two plans (no note reserved by them) and in a block", lands, f"hash {h[:16]}…")
    step(f"{label}: ESTIMATE {est} vs ACTUAL {fee} (node)", est is not None and fee is not None and int(est) == fee,
         f"difference {None if fee is None or est is None else int(est) - fee}")
    return est, fee


summary = []

say("\n== 1. send A -> B 3,000 (one input)")
e, f = plan_twice_then_send("send", "chain_send", (A, B, "3,000", "false"), (A, B, "3,000", "true"), A)
summary.append(("chain_send 1 input", e, f))

say("\n== 2. split A 1,234")
e, f = plan_twice_then_send("split", "chain_split", (A, "1,234", "false"), (A, "1,234", "true"), A)
summary.append(("chain_split", e, f))

say("\n== 3. send A -> B, two inputs")
nA = notes(A)
amt = max(nA.values()) + 100
e, f = plan_twice_then_send("send 2 inputs", "chain_send", (A, B, f"{amt:,}", "false"), (A, B, f"{amt:,}", "true"), A)
summary.append(("chain_send 2 inputs", e, f))

say("\n== 4. the CLI: names chain send plan over the node's HTTP API")
if NAMES_BIN:
    nB = notes(B)
    p = subprocess.run([NAMES_BIN, "chain", "send", "1000", A, "--from-key", B, "--node", f"http://127.0.0.1:{HTTP}"], capture_output=True, text=True, timeout=120)
    say("    $ names chain send 1000 <A> --from-key <B> --node http://127.0.0.1:%d\n    " % HTTP + (p.stdout + p.stderr).strip().replace("\n", "\n    "))
    pj = subprocess.run([NAMES_BIN, "chain", "send", "1000", A, "--from-key", B, "--node", f"http://127.0.0.1:{HTTP}", "--json"], capture_output=True, text=True, timeout=120)
    say("    --json: " + pj.stdout.strip())
    cli_est = (json.loads(pj.stdout) if pj.stdout.strip().startswith("{") else {}).get("fee_about")
    step("CLI: the plan prints the node's estimate and moves nothing", p.returncode == 0 and "fee: about" in p.stdout and cli_est is not None and notes(B) == nB,
         f"fee_about {cli_est}")
    s = names("chain_send", B, A, "1,000", "true")
    show("names chain_send B -> A 1,000 after the CLI plans", s)
    fee, t = tx_fee(s["value"]["hash"], nB) if s["ok"] else (None, None)
    landed(B, list(nB))
    step(f"CLI: ESTIMATE {cli_est} vs ACTUAL {fee} (node), the send after it not blocked", s["ok"] and cli_est is not None and fee == int(cli_est))
    summary.append(("names chain send (CLI plan)", cli_est, fee))
else:
    say("    NAMES_BIN not set: skipped")

say("\n== 5. bridge: mine, split 1,000,000, deposit (stand-in sequencer names channel 00..00)")
CHANNEL = "00" * 32
TO = "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk"


class Channel(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))) or b"{}")
        if body.get("method") == "getChannelId":
            out = {"jsonrpc": "2.0", "id": body.get("id"), "result": CHANNEL}
        elif body.get("method") == "getAccountView":
            out = {"jsonrpc": "2.0", "id": body.get("id"), "result": {"data": {}}}
        else:
            out = {"jsonrpc": "2.0", "id": body.get("id"), "error": {"code": -32601, "message": "not here"}}
        data = json.dumps(out).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def log_message(self, *a):
        pass


srv = http.server.ThreadingHTTPServer(("127.0.0.1", HTTP + 3), Channel)
threading.Thread(target=srv.serve_forever, daemon=True).start()
cp = os.path.join(os.environ["HOME"], ".config", "logos-names", "config.json")
os.makedirs(os.path.dirname(cp), exist_ok=True)
try:
    cfg = json.load(open(cp))
except (OSError, ValueError):
    cfg = {}
cfg["sequencer"] = f"http://127.0.0.1:{HTTP + 3}"
json.dump(cfg, open(cp, "w"))
ctl("module", "unload", "names")
say("    names reloaded: " + ctl("module", "load", "names")[1])
km = node("pow_status")["value"]["auto_claim"]["targets"][0]["public_key"]
node("pow_start_mining")
end = time.time() + 300
while time.time() < end and (balance(km) or 0) < 3_000_000:
    time.sleep(5)
node("pow_stop_mining")
say(f"    mined key {km[:12]}…: {balance(km)}")
e, f = plan_twice_then_send("bridge split", "chain_split", (km, "1,000,000", "false"), (km, "1,000,000", "true"), km)
summary.append(("chain_split (bridge, mined notes)", e, f))
end = time.time() + 120
while time.time() < end and 1_000_000 not in (notes(km) or {}).values():
    time.sleep(1)
e, f = plan_twice_then_send("bridge deposit", "chain_bridge", (km, TO, "1,000,000", "false"), (km, TO, "1,000,000", "true"), km, deposit=1_000_000)
summary.append(("chain_bridge deposit", e, f))
srv.shutdown()

say("\n== 6. claim: the plan's estimate (no comparison unless the node holds vouchers)")
vou = node("wallet_get_claimable_vouchers")
show("node wallet_get_claimable_vouchers", vou)
c = names("chain_claim", "false")
show("names chain_claim plan", c)
step("claim: the plan carries an estimate for one claim", c["ok"] and c["value"].get("fee_about") is not None, f"fee_about {c['value'].get('fee_about') if c['ok'] else c['error']}")
summary.append(("chain_claim (plan only)", c["value"].get("fee_about") if c["ok"] else None, None))

say("\n== 7. read cases: unknown key, after a module reload, node stopped")
BADK = "ab" * 32
known = node("wallet_get_known_addresses")["value"] or []
UNHELD = next((k for k in known if not node("wallet_get_balance", k)["ok"]), None)
say(f"    a key the node lists but its wallet doesn't hold: {UNHELD}")
cases = [("a key the node can't read", (BADK, B, "1,000", "false"))] + ([("a key the node doesn't hold", (UNHELD, B, "1,000", "false"))] if UNHELD and any(c in "abcdef" for c in UNHELD) else [])
nA = notes(A)
r = names("chain_send", A, B, "1,000", "false")
show(f"names chain_send plan A -> B 1,000 (A holds {nA})", r)
step("a key that can't pay the amount and its fee: the plan says so, no figure invented", r["ok"] and r["value"].get("fee_about") is None and r["value"].get("cannot_pay_fee") is True and notes(A) == nA,
     f"A holds {sum(nA.values())}; fee_about {r['value'].get('fee_about') if r['ok'] else r['error']}, cannot_pay_fee {r['value'].get('cannot_pay_fee') if r['ok'] else None}")
for label, args in cases:
    r = names("chain_send", *args)
    show(f"names chain_send plan from {label}", r)
    step(f"plan from {label}: refused in words, no estimate invented", not r["ok"] and isinstance(r["error"], str), r["error"])
ctl("module", "unload", "names")
say("    reloaded: " + ctl("module", "load", "names")[1])
r = names("chain_send", km, B, "1,000", "false")
show("names chain_send plan (mined key -> B 1,000) after the reload", r)
step("after a module reload the plan still carries the node's estimate", r["ok"] and r["value"].get("fee_about") is not None, f"fee_about {r['value'].get('fee_about') if r['ok'] else r['error']}")
node("stop")
open(os.path.join(W, ".node-stopped"), "w").close()
time.sleep(1)
down = [(m, names(m, *a)) for m, a in (("chain_send", (A, B, "1,000", "false")), ("chain_split", (A, "1,000", "false")), ("chain_claim", ("false",)),
                                       ("chain_bridge", (A, TO, "1,000", "false")))]
for m, x in down:
    show(f"names {m} plan, node stopped", x)
step("node stopped: every plan says so, as the node does (no estimate without a node)",
     all(not x["ok"] and x["error"] == RUNNING for _, x in down), str([x["error"] for _, x in down]))

say("\n== SUMMARY: estimate (names plan, before) vs actual (node, after)")
for label, e, f in summary:
    say(f"    {label:38s} estimate {str(e):>8s}   actual {str(f):>8s}")
say(f"RESULT {sum(1 for r in results if r.startswith('PASS'))} PASS, {sum(1 for r in results if r.startswith('FAIL'))} FAIL")
sys.exit(1 if any(r.startswith("FAIL") for r in results) else 0)
PY
