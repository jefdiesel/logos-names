#!/bin/bash
# The Names module's Logos Blockchain methods (chain_*) end to end over REAL Logos Core
# IPC: logosctl 0.3.2, the real blockchain_module 0.3.4 and names, against a SOLO node on
# its own chain (the node repo's standalone deployment), with no peers at all.
#
# Runs on the Mac mini, with bash (not zsh):
#   ssh minim4 "CHAIN_IPC_PORT_BASE=3340 /bin/bash /Volumes/exos162/logos/names-0.4/scripts/chain-ipc-e2e.sh"
#
#   CHAIN_IPC_DIR        work dir, wiped and recreated (default $CHAIN_IPC_AREA/run)
#   CHAIN_IPC_PORT_BASE  node ports: net = base, blend = base+1, http = base+2 (default 3340)
#   CHAIN_IPC_AREA       where the built pieces are (default /Volumes/exos162/logos/dev/chain-ipc):
#                          logosctl-0.3.2/bin/logosctl, real-module/blockchain_module, names-lgx
#   NAMES_LGX            the names lgx dir (default $CHAIN_IPC_AREA/names-lgx)
# Several copies can run at once with different CHAIN_IPC_DIR and CHAIN_IPC_PORT_BASE.
#
# Funding: the standalone deployment's genesis gives 100000 to zk key e3635f…ce07 and 100
# to 39e16b…3225; their secret keys are in the same repo's standalone-node-config.yaml.
# Both files are pinned by sha256 below. They are a PUBLIC test fixture: never use these
# keys on a real chain. Nothing is mined, and the run stays inside one epoch, so the node
# has no leader vouchers and the claim checked is the one with nothing to claim. (A claim
# with vouchers needs an epoch boundary, every 6000 slots = 100 min here; it was checked
# by hand on a solo node: one voucher per claim, see docs/wallet/PLAN.md.)
#
# Every step checks what names said against the node itself (call blockchain_module …
# directly, or the node's own log in the daemon log), prints PASS or FAIL, and the script
# exits non-zero on any FAIL. The session is always stopped (trap), and no process whose
# command line holds the work dir may be left.
#
# logosctl turns an argument that looks like a number into a JSON number, and names takes
# amounts as text, so amounts are written with a comma (3,000), which names reads as 3000.
# A bare 3000 is refused by the host before names runs ("expected string at arg2, got
# number"): an upstream logosctl behaviour, shown as a FLAG below.
set -uo pipefail

T=${CHAIN_IPC_AREA:-/Volumes/exos162/logos/dev/chain-ipc}
W=${CHAIN_IPC_DIR:-$T/run}
BASE=${CHAIN_IPC_PORT_BASE:-3340}
NAMES_LGX=${NAMES_LGX:-$T/names-lgx}
CTL=$T/logosctl-0.3.2/bin/logosctl        # the real binary, not the logos/bin symlink
MODULE=$T/real-module/blockchain_module
NET=$BASE BLEND=$((BASE + 1)) HTTP=$((BASE + 2))
DEPLOY_SHA=0929cddc3dc35a189dbdb1e8297ece5d80ca1247a9fda07c2fdfcf5330adf618
NODECFG_SHA=28b6729f2aeab5937d95192326a01102e0c9bd9594b40de1b154e7e3b5dea2cc
FIXTURES=https://raw.githubusercontent.com/logos-blockchain/logos-blockchain/0.3.4/nodes/node

say() { printf '%s\n' "$*"; }
die() { say "ABORT $*"; exit 2; }

case "$BASE" in '' | *[!0-9]*) die "CHAIN_IPC_PORT_BASE must be a number" ;; esac
for p in $NET $BLEND $HTTP; do
  case $p in 8080 | 8081 | 3000 | 3001 | 3401 | 11434) die "port $p is reserved on the Mini" ;; esac
done
for f in "$CTL" "$MODULE/manifest.json" "$NAMES_LGX"; do [ -e "$f" ] || die "missing $f"; done
# Only a dir this script made (or an empty or new one) is wiped.
if [ -d "$W" ] && [ -n "$(ls -A "$W")" ] && [ ! -e "$W/.chain-ipc-e2e" ]; then
  die "$W is not empty and was not made by this script; choose another CHAIN_IPC_DIR"
fi
if lsof -nP -i :$NET -i :$BLEND -i :$HTTP >/dev/null 2>&1; then
  lsof -nP -i :$NET -i :$BLEND -i :$HTTP; die "ports $NET-$HTTP are busy; choose another CHAIN_IPC_PORT_BASE"
fi

rm -rf "$W"
mkdir -p "$W/home" "$W/session/modules" && touch "$W/.chain-ipc-e2e" || die "cannot make $W"
cd "$W" || die "cannot enter $W"
export HOME=$W/home LOGOSCTL_CONFIG_DIR=$W/session

say "chain-ipc-e2e  $(date -u +%Y-%m-%dT%H:%M:%SZ)  host $(hostname -s)"
say "  work dir  $W"
say "  ports     net $NET (udp), blend $BLEND, http 127.0.0.1:$HTTP"
say "  logosctl  $CTL ($("$CTL" --version 2>/dev/null | head -1))"
say "  module    $MODULE ($(python3 -I -c 'import json,sys;m=json.load(open(sys.argv[1]));print(m["name"],m["version"])' "$MODULE/manifest.json"))"
say "  names     $(readlink "$NAMES_LGX" || echo "$NAMES_LGX")"
say "            lgx sha256 $(shasum -a 256 "$NAMES_LGX"/*.lgx | cut -c1-64)"

# The fixture: the spike's verified copies when present, else the node repo at 0.3.4.
for f in standalone-deployment-config.yaml standalone-node-config.yaml; do
  if [ -f "$T/spike/$f" ]; then cp "$T/spike/$f" "$f"; else curl -sfL "$FIXTURES/$f" -o "$f" || die "cannot fetch $f"; fi
done
mv standalone-deployment-config.yaml deployment.yaml
printf '%s  deployment.yaml\n%s  standalone-node-config.yaml\n' "$DEPLOY_SHA" "$NODECFG_SHA" | shasum -a 256 -c || die "fixture hash mismatch"
cp -R "$MODULE" "$W/session/modules/" || die "cannot copy the module"

STARTED=0
cleanup() {
  local rc=$?
  trap - EXIT INT TERM
  say ""
  say "== stopping the session"
  if [ -e "$W/.node-started" ] && [ ! -e "$W/.node-stopped" ]; then
    "$CTL" --json call blockchain_module stop || true
    say ""
  fi
  [ $STARTED = 1 ] && { "$CTL" daemon stop || true; }
  local left=""
  for _ in $(seq 20); do
    left=$(pgrep -fl "$W" || true)
    [ -z "$left" ] && break
    sleep 1
  done
  if [ -n "$left" ]; then
    say "FAIL session stopped — processes still hold $W:"
    say "$left"
    rc=1
  else
    say "PASS session stopped — pgrep -fl $W prints nothing"
  fi
  if [ -f "$W/.results" ]; then
    say "RESULT $(grep -c '^PASS' "$W/.results") PASS, $(grep -c '^FAIL' "$W/.results") FAIL (+ session stop above)"
  fi
  say "exit $rc"
  exit $rc
}
trap cleanup EXIT
trap 'exit 130' INT TERM

LOG_BACKEND=file LOG_DIR=$W LOG_PATH=node.log "$CTL" daemon start --detach || die "daemon start failed"
STARTED=1
say "  daemon processes holding the work dir:"
pgrep -fl "$W" | sed 's/^/    /'

export CTL W NET BLEND HTTP NAMES_LGX
python3 -I - <<'PY'
import json, os, re, subprocess, sys, time

CTL, W = os.environ["CTL"], os.environ["W"]
NET, BLEND, HTTP = (int(os.environ[k]) for k in ("NET", "BLEND", "HTTP"))
DAEMON_LOG = os.path.join(W, "session", "logs", "daemon.log")
A = "e3635f207984ae779cf76b5f20714b514373f61ff96260879fe0a6d71f2dce07"  # genesis 100000
B = "39e16b432574571a6bcd8ee36e370589641bd9f35367f6f97a972453c46c3225"  # genesis 100
BAD = "ab" * 32                       # 64 hex, but not a key the node can read
SLOTS_PER_EPOCH = 6000                # measured on this deployment; checked against get_time_info
GENESIS_MS = 1778280849000            # get_time_info.genesis_time_unix_ms of this deployment
RUNNING = "Your Logos Blockchain node isn't running. Start it in Basecamp's Blockchain app."
results = open(os.path.join(W, ".results"), "w")
fails = 0


def say(text=""):
    print(text, flush=True)


def step(name, ok, detail=""):
    global fails
    line = f"{'PASS' if ok else 'FAIL'} {name}" + (f" — {detail}" if detail else "")
    say(line)
    results.write(line + "\n")
    results.flush()
    fails += 0 if ok else 1
    return ok


def abort(why, stop_node=False):
    say(f"ABORT {why}")
    if stop_node:
        node("stop")
        open(os.path.join(W, ".node-stopped"), "w").close()
    results.write(f"FAIL {why}\n")
    results.close()
    sys.exit(2)


def loads(text):
    if isinstance(text, str):
        try:
            return json.loads(text)
        except ValueError:
            return text
    return text


def call(module, method, *args, timeout=240):
    """One logosctl call: {ok, value, error, raw}. value is parsed when it is JSON text."""
    argv = [CTL, "--json", "call", module, method, *[str(a) for a in args]]
    try:
        p = subprocess.run(argv, capture_output=True, text=True, timeout=timeout, stdin=subprocess.DEVNULL)
    except subprocess.TimeoutExpired:
        return {"ok": False, "value": None, "error": f"logosctl timed out after {timeout}s", "raw": ""}
    raw = p.stdout.strip()
    try:
        out = json.loads(raw)
    except ValueError:
        return {"ok": False, "value": None, "error": f"not JSON: {raw[:200]} {p.stderr.strip()[:200]}", "raw": raw}
    if out.get("status") != "ok":
        return {"ok": False, "value": None, "error": out.get("message") or raw, "raw": raw}
    r = loads(out.get("result"))
    if not isinstance(r, dict) or "success" not in r:
        return {"ok": True, "value": r, "error": None, "raw": raw}
    return {"ok": bool(r["success"]), "value": loads(r.get("value")), "error": r.get("error"), "raw": raw}


def node(method, *args, **kw):
    return call("blockchain_module", method, *args, **kw)


def names(method, *args, **kw):
    return call("names", method, *args, **kw)


def shown(r):
    return json.dumps(r["value"], sort_keys=True) if r["ok"] else f"ERR {r['error']}"


def show(label, r):
    say(f"    {label}: {shown(r)}")


def num(v):
    return int(str(v)) if v is not None and str(v).isdigit() else None


def balance(key):
    r = node("wallet_get_balance", key)
    return num(r["value"]) if r["ok"] else None


def ctl(*argv):
    p = subprocess.run([CTL, *argv], capture_output=True, text=True, timeout=120, stdin=subprocess.DEVNULL)
    return p.returncode, (p.stdout + p.stderr).strip()


def daemon_log():
    try:
        with open(DAEMON_LOG, errors="replace") as f:
            return f.read()
    except OSError:
        return ""


# ── The session: names installed, both modules loaded ───────────────────────
say("\n== session")
rc, out = ctl("install", "--dir", os.environ["NAMES_LGX"], "-y")
say(f"    install names: exit {rc}")
loaded = {}
for m in ("blockchain_module", "names"):
    rc, out = ctl("module", "load", m)
    say(f"    module load {m}: {out}")
    loaded[m] = loads(out) if rc == 0 else {}
ok = all(isinstance(loaded[m], dict) and loaded[m].get("status") == "ok" for m in loaded)
step("session loads blockchain_module 0.3.4 and names 0.13.0",
     ok and loaded["blockchain_module"].get("version") == "0.3.4" and loaded["names"].get("version") == "0.13.0")
if not ok:
    abort("a module did not load")

# Before the node is started, names says so, as the node does.
say("\n== refusal: node not started")
n = node("get_cryptarchia_info")
s = names("chain_status")
show("node get_cryptarchia_info", n)
show("names chain_status", s)
step("refusal: node not started", not s["ok"] and s["error"] == RUNNING and not n["ok"] and "not running" in str(n["error"]).lower())

# ── A solo node, isolated, funded at genesis ───────────────────────────────
say("\n== solo node")
gen = {"skip_ibd": True, "net_port": NET, "blend_port": BLEND, "http_addr": f"127.0.0.1:{HTTP}",
       "output": "user_config.yaml", "use_persistence_paths": True}
with open("gen-args.json", "w") as f:
    json.dump(gen, f)
say(f"    gen-args.json: {json.dumps(gen)}")
g = node("generate_user_config", "@gen-args.json")
if not g["ok"] or not str(g["value"]).endswith("user_config.yaml"):
    abort(f"generate_user_config: {shown(g)}")
CFG = str(g["value"])
KS = os.path.join(os.path.dirname(CFG), "keystore.yaml")
say(f"    config: {CFG}")
fixture = open("standalone-node-config.yaml").read()
for key, title in ((A, "GenesisStake"), (B, "GenesisSmall")):
    found = re.search(rf"^\s*{key}: !Zk\s+(\S+)", fixture, re.M)
    if not found:
        abort(f"no secret for {key[:8]} in the fixture")
    r = node("add_key", KS, "zk", found.group(1), title)
    say(f"    add_key {title} ({key[:8]}…): {'ok' if r['ok'] else 'ERR ' + str(r['error'])}")
    if not r["ok"]:
        abort("add_key failed")
# Not settable through generate_user_config; without it the node waits an hour to go Online.
with open(CFG, "a") as f:
    f.write("cryptarchia:\n  service:\n    bootstrap:\n      prolonged_bootstrap_period: 5s\n")

say("\n== ISOLATION (before anything moves)")
config = open(CFG).read()
say(f"    {CFG}, in full:")
for line in config.splitlines():
    say(f"      | {line}")
peers = [line for line in config.splitlines() if re.search(r"peer|ibd", line, re.I)]
say(f"    grep -i -E 'peer|ibd' user_config.yaml: {peers if peers else '(no match)'}")
say("    skip_ibd: true is a generate_user_config argument (gen-args.json above); the yaml it writes names no peers")
if peers:
    abort("PEERS IN CONFIG")

# An epoch boundary during the run would make leader vouchers claimable; keep the run
# inside one epoch so the claim checked is the one with nothing to claim.
slot = (int(time.time() * 1000) - GENESIS_MS) // 1000
left = SLOTS_PER_EPOCH - slot % SLOTS_PER_EPOCH
say(f"    slot {slot} (by the clock), {left} s to the next epoch boundary")
if left < 300:
    say(f"    waiting {left + 5} s for the boundary to pass")
    time.sleep(left + 5)

r = node("start", CFG, os.path.join(W, "deployment.yaml"))
say(f"    start: {shown(r)}")
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
    abort("the node did not go Online", stop_node=True)
net = node("get_network_info")
show("get_network_info", net)
nv = net["value"] if net["ok"] and isinstance(net["value"], dict) else {}
skip = [line for line in daemon_log().splitlines() if "Skipping IBD as no peers are configured" in line]
skip_line = re.sub(r".*WARN", "WARN", re.sub(r"\x1b\[[0-9;]*m", "", skip[0])) if skip else "(no Skipping IBD line)"
say(f"    node log: {skip_line}")
isolated = bool(nv) and all(nv.get(k) == 0 for k in ("n_peers", "n_connections", "n_pending_connections", "n_discovered_peers"))
if not step("ISOLATION: no peers configured, IBD skipped, zero peers and connections", isolated and bool(skip)):
    abort("PEER SEEN or isolation unproven: node stopped, nothing sent", stop_node=True)
chain_id = node("get_chain_id")
show("get_chain_id", chain_id)
t = node("get_time_info")
show("get_time_info", t)
tv = t["value"] if t["ok"] and isinstance(t["value"], dict) else {}
step("epoch arithmetic matches the node", tv.get("genesis_time_unix_ms") == GENESIS_MS
     and tv.get("current_slot", -1) // SLOTS_PER_EPOCH == tv.get("current_epoch"),
     f"slot {tv.get('current_slot')} // {SLOTS_PER_EPOCH} = epoch {tv.get('current_epoch')}")

# ── chain_status ───────────────────────────────────────────────────────────
say("\n== chain_status")
n1 = node("get_cryptarchia_info")["value"]
t1 = node("get_time_info")["value"]
s = names("chain_status")
n2 = node("get_cryptarchia_info")["value"]
t2 = node("get_time_info")["value"]
show("names chain_status", s)
say(f"    node before: {json.dumps(n1, sort_keys=True)}")
say(f"    node after:  {json.dumps(n2, sort_keys=True)}")
sv = s["value"] if s["ok"] else {}
step("chain_status matches the node", s["ok"] and sv.get("mode") == n1["mode"] == n2["mode"] == "Online"
     and n1["height"] <= sv.get("height", -1) <= n2["height"]
     and n1["slot"] <= sv.get("slot", -1) <= n2["slot"]
     and n1["lib_slot"] <= sv.get("lib_slot", -1) <= n2["lib_slot"]
     and sv.get("epoch") in (t1["current_epoch"], t2["current_epoch"]),
     f"height {n1['height']} <= {sv.get('height')} <= {n2['height']}, mode {sv.get('mode')}, epoch {sv.get('epoch')}")
extra = sorted(set(n1) - {"height", "slot", "lib_slot", "mode"})
say(f"FLAG shape: the solo node's get_cryptarchia_info has {extra} beyond the M2 record in chain.rs "
    f"(height, lib_slot, mode, slot); names reads only those four, so both shapes are accepted")

# ── chain_accounts, key by key ──────────────────────────────────────────────
say("\n== chain_accounts")
known = node("wallet_get_known_addresses")
keys = known["value"] if known["ok"] and isinstance(known["value"], list) else []
direct = {}
for k in keys:
    r = node("wallet_get_balance", k)
    say(f"    node wallet_get_balance {k[:8]}…: {shown(r)}")
    if r["ok"]:
        direct[k] = str(r["value"])
    elif "unknown wallet address" not in str(r["error"]).lower():
        direct[k] = None
s = names("chain_accounts")
show("names chain_accounts", s)
listed = {a["key"]: a["balance"] for a in s["value"]} if s["ok"] else {}
step("chain_accounts lists exactly the keys the node can spend from, with its balances",
     s["ok"] and listed == direct and direct.get(A) == "100000" and direct.get(B) == "100",
     f"{len(keys)} known to the node, {len(direct)} held, names lists {len(listed)}")

# ── chain_stake (before the send moves the aged notes) ─────────────────────
say("\n== chain_stake")
aged = node("wallet_get_leader_aged_notes", "")
vou = node("wallet_get_claimable_vouchers")
s = names("chain_stake")
show("node wallet_get_leader_aged_notes", aged)
show("node wallet_get_claimable_vouchers", vou)
show("names chain_stake", s)
av, vv, sv = aged["value"], vou["value"], (s["value"] if s["ok"] else {})
pks = sorted({n["public_key"] for n in av["notes"]})
step("chain_stake matches the node's aged notes and vouchers", s["ok"]
     and sv["staked"] == str(av["total_value"]) and sv["notes"] == len(av["notes"]) and sv["keys"] == pks
     and sv["claimable"] == str(vv["total_claimable"]) and sv["vouchers"] == len(vv["vouchers"])
     and sv["reward_per_voucher"] == str(vv["reward_amount"]),
     f"staked {sv.get('staked')} over {sv.get('notes')} notes, {sv.get('vouchers')} vouchers")

# ── chain_send: the plan ───────────────────────────────────────────────────
say("\n== chain_send plan (send=false)")
a0, b0 = balance(A), balance(B)
p = names("chain_send", A, B, "3,000", "false")
show("names chain_send plan", p)
pv = p["value"] if p["ok"] else {}
a1, b1 = balance(A), balance(B)
step("chain_send plan reads the node's balance and moves nothing", p["ok"] and pv.get("amount") == "3000"
     and pv.get("balance") == str(a0) and pv.get("from") == A and pv.get("to") == B and "hash" not in pv
     and (a1, b1) == (a0, b0), f"balance {pv.get('balance')} = node {a0}; after: A {a1}, B {b1}")

# ── chain_send, chain_tx pending → in_block, the fee ───────────────────────
say("\n== chain_send (send=true)")
before = node("wallet_get_notes", A, "")
show("node wallet_get_notes A (before)", before)
held = {n["id"]: int(n["value"]) for n in before["value"]["notes"]}
log_mark = len(daemon_log())
sent_at = time.time()
r = names("chain_send", A, B, "3,000", "true")
show("names chain_send", r)
rv = r["value"] if r["ok"] else {}
H = rv.get("hash") or ""
step("chain_send sends and returns the hash and a fee", r["ok"] and re.fullmatch(r"[0-9a-f]{64}", H) is not None
     and rv.get("fee") is not None and rv.get("amount") == "3000", f"hash {H}, fee {rv.get('fee')}, {time.time() - sent_at:.1f}s")
if not H:
    abort("no hash", stop_node=True)

say("\n== chain_tx until in_block")
# Asked at once: the solo node makes a block about every second, so the send can land
# within a second of names answering.
first = names("chain_tx", H)
first_at = time.time()               # names had answered by now
first_b = balance(B)                 # read after names answered
show(f"names chain_tx, at once (node B balance read right after: {first_b})", first)
states, landed, wrong = [first["value"].get("state") if first["ok"] else None], None, []
if states[0] == "in_block":           # the node took the block before names was asked
    landed = (first, first_b)
deadline = time.time() + 120
while states[-1] == "pending" and time.time() < deadline:
    time.sleep(0.2)
    tx = names("chain_tx", H)
    bal = balance(B)                 # read after names answered
    st = tx["value"]["state"] if tx["ok"] else f"ERR {tx['error']}"
    if states[-1] != st:
        say(f"    {time.time() - sent_at:5.1f}s  names chain_tx: {st}  node B balance read right after: {bal}")
    states.append(st)
    if st == "in_block":
        landed = (tx, bal)
    elif st != "pending":
        wrong.append(st)
pend = sum(1 for x in states if x == "pending")

# The node's own record: the block that carries the transaction, and when it took it, from its log.
blocks = []
for line in daemon_log()[log_mark:].splitlines():
    if "Received new block: " in line and H in line:
        stamp = re.match(r"\[(\d{4}-\d\d-\d\d \d\d:\d\d:\d\d)(\.\d+)\]", line)
        at = time.mktime(time.strptime(stamp.group(1), "%Y-%m-%d %H:%M:%S")) + float(stamp.group(2)) if stamp else None
        blocks.append((at, json.loads(line.split("Received new block: ", 1)[1])))
block_at = blocks[0][0] if blocks else None
say(f"    node log: block at slot {blocks[0][1]['header']['slot'] if blocks else '?'} with {H[:8]}… taken "
    f"{(block_at - sent_at) if block_at else float('nan'):.2f}s after the send began; names' first answer was back at {first_at - sent_at:.2f}s")
# The first answer must agree with the node. Pending is right if the recipient hadn't been
# paid even after names answered, or names answered before the node took the block; the
# solo node makes a block every second, so sometimes the block comes first, and then
# in_block is right only if the node's log shows the block before names answered.
if states[0] == "pending":
    step("chain_tx's first answer agrees with the node: pending before the transaction is in a block",
         first_b == b0 or (block_at is not None and first_at < block_at),
         f"node B balance {first_b} after the answer (was {b0})")
else:
    step("chain_tx's first answer agrees with the node: in_block, the block already taken (no pending to see this run)",
         states[0] == "in_block" and block_at is not None and block_at < first_at and first_b == b0 + 3000,
         f"first answer {states[0]}; block taken {(block_at - sent_at) if block_at else float('nan'):.2f}s, answer back {first_at - sent_at:.2f}s; B {b0} -> {first_b}")
if landed:
    tx, bal = landed
    step("chain_tx says in_block only once the recipient's balance moved", bal == b0 + 3000,
         f"after {pend} pending answers; node B balance {b0} -> {bal}")
else:
    tx = None
    step("chain_tx says in_block only once the recipient's balance moved", False, f"states {states[-5:]} {wrong}")
gt = node("get_transaction", H)
show("node get_transaction", gt)
inblk = [t for _, b in blocks for t in b.get("transactions", []) if t.get("id") == H]
step("the node's log shows the transaction in a block", bool(inblk) and gt["ok"]
     and inblk[0]["mantle_tx"] == gt["value"]["mantle_tx"],
     f"block at slot {blocks[0][1]['header']['slot'] if blocks else '?'} carries {H[:8]}…, same ops as get_transaction")

say("\n== chain_tx after the names module reloads")
rc1, out1 = ctl("module", "unload", "names")
rc2, out2 = ctl("module", "load", "names")
say(f"    module unload names: exit {rc1} {out1}")
say(f"    module load names: exit {rc2} {out2}")
again = names("chain_tx", H)
show("names chain_tx H after the reload", again)
step("chain_tx still follows the send after the names module reloads", rc1 == 0 and rc2 == 0 and again["ok"]
     and landed is not None and again["value"] == landed[0]["value"],
     f"state {again['value'].get('state') if again['ok'] else again['error']}, fee {again['value'].get('fee') if again['ok'] else None}")

say("\n== the fee")
ops = [op for op in gt["value"]["mantle_tx"]["ops"] if op["opcode"] == 0]
inputs = [i for op in ops for i in op["payload"]["inputs"]]
outputs = [o for op in ops for o in op["payload"]["outputs"]]
in_sum = sum(held.get(i, 0) for i in inputs)
out_sum = sum(int(o["value"]) for o in outputs)
fee = in_sum - out_sum
a2, b2 = balance(A), balance(B)
after = node("wallet_get_notes", A, "")
show("node wallet_get_notes A (after)", after)
say(f"    inputs {[i[:8] for i in inputs]} held {in_sum}; outputs {[(o['pk'][:8], o['value']) for o in outputs]} = {out_sum}; fee {fee}")
say(f"    node balances: A {a0} -> {a2} (= {a0} - 3000 - {fee}?), B {b0} -> {b2}")
change = [int(o["value"]) for o in outputs if o["pk"] == A]
step("the fee is inputs less outputs, and agrees with the node's notes and balances",
     all(i in held for i in inputs) and rv.get("fee") == str(fee) == (tx["value"].get("fee") if landed else None)
     and any(o["pk"] == B and int(o["value"]) == 3000 for o in outputs)
     and a2 == a0 - 3000 - fee and b2 == b0 + 3000
     and sorted(int(n["value"]) for n in after["value"]["notes"]) == sorted(change)
     and not set(inputs) & {n["id"] for n in after["value"]["notes"]},
     f"names fee {rv.get('fee')}, node fee {fee}")

# ── chain_claim ────────────────────────────────────────────────────────────
say("\n== chain_claim")
vou = node("wallet_get_claimable_vouchers")
show("node wallet_get_claimable_vouchers", vou)
vv = vou["value"]
c = names("chain_claim", "false")
show("names chain_claim plan", c)
step("chain_claim plan matches the node's vouchers", c["ok"] and c["value"]["claimable"] == str(vv["total_claimable"])
     and c["value"]["vouchers"] == len(vv["vouchers"]) and c["value"]["reward_per_voucher"] == str(vv["reward_amount"])
     and "hash" not in c["value"])
if len(vv["vouchers"]) == 0:
    before_claim = (balance(A), balance(B))
    log_mark = len(daemon_log())
    c = names("chain_claim", "true")
    show("names chain_claim send", c)
    time.sleep(3)
    later = [json.loads(line.split("Received new block: ", 1)[1]) for line in daemon_log()[log_mark:].splitlines()
             if "Received new block: " in line]
    carried = sum(len(b.get("transactions", [])) for b in later)
    say(f"    node: {len(later)} blocks since, carrying {carried} transactions")
    step("refusal: chain_claim with nothing to claim says so, and the node moved nothing", not c["ok"]
         and c["error"] == "There is nothing to claim: your node has no leader rewards waiting."
         and (balance(A), balance(B)) == before_claim and len(later) >= 2 and carried == 0,
         "names asks leader_claim only for vouchers; balances unchanged")
else:
    step("refusal: chain_claim with nothing to claim says so, and the node moved nothing", False,
         f"the node holds {len(vv['vouchers'])} vouchers: this run was meant to stay inside one epoch; not sent")

# ── Refusals ───────────────────────────────────────────────────────────────
say("\n== refusal: overdraft")
# The node's own refusal text can't be asked for directly: logosctl 0.3.2 can't pass the
# list argument of wallet_transfer_funds (every form tried answers success:false with no
# error), so the node's state is checked instead: nothing left B, and no block since
# carries any transaction.
hb = balance(B)
notes_b = sorted((n["id"], n["value"]) for n in node("wallet_get_notes", B, "")["value"]["notes"])
log_mark = len(daemon_log())
r = names("chain_send", B, A, f"{hb:,}", "true")
show(f"names chain_send B -> A {hb:,} (all B holds, nothing left for the fee)", r)
time.sleep(3)
later = [json.loads(line.split("Received new block: ", 1)[1]) for line in daemon_log()[log_mark:].splitlines()
         if "Received new block: " in line]
carried = sum(len(b.get("transactions", [])) for b in later)
notes_b2 = sorted((n["id"], n["value"]) for n in node("wallet_get_notes", B, "")["value"]["notes"])
say(f"    node: B {balance(B)} in {len(notes_b2)} notes (was {hb} in {len(notes_b)}); {len(later)} blocks since, carrying {carried} transactions")
step("refusal: overdraft by the fee, in plain words, and the node moved nothing",
     not r["ok"] and r["error"] == "That key doesn't hold enough for this and its fee."
     and balance(B) == hb and notes_b2 == notes_b and len(later) >= 2 and carried == 0,
     f"the amount {hb} passed names' own check (B holds {hb}), so the node refused it")
r = names("chain_send", B, A, f"{hb + 1:,}", "true")
show(f"names chain_send B -> A {hb + 1:,}", r)
step("refusal: overdraft by the amount, checked against the node's balance",
     not r["ok"] and r["error"] == f"that key holds {hb}, less than {hb + 1}" and balance(B) == hb)

say("\n== refusal: bad keys")
ha = balance(A)
cases = [
    ("malformed sender", ("nothex", A), "choose the key to send from"),
    ("malformed recipient (64 chars, not hex, not a name)", (A, "zz" * 32), "enter a Logos Blockchain key (64 hex characters) or a name"),
    ("a key the node can't read, as sender", (BAD, A), "That isn't a Logos Blockchain public key: check it and try again."),
    ("a key the node can't read, as recipient", (A, BAD), "That isn't a Logos Blockchain public key: check it and try again."),
]
d = node("wallet_get_balance", BAD)
show(f"node wallet_get_balance {BAD[:8]}…", d)
for label, (frm, to), want in cases:
    r = names("chain_send", frm, to, "1,000", "true")
    show(f"names chain_send {label}", r)
    ok = not r["ok"] and r["error"] == want
    step(f"refusal: {label}", ok and balance(A) == ha)
step("the node itself refuses that key", not d["ok"] and "invalid wallet address" in str(d["error"]).lower(),
     str(d["error"])[:60])
unheld = [k for k in keys if k not in direct]
if unheld:
    u = unheld[0]
    du = node("wallet_get_balance", u)
    r = names("chain_send", u, A, "1,000", "true")
    show(f"node wallet_get_balance {u[:8]}… (known, not held)", du)
    show("names chain_send from it", r)
    step("refusal: a key the node knows but its wallet doesn't hold", not r["ok"] and not du["ok"]
         and str(du["error"]) in str(r["error"]) and balance(A) == ha)
r = names("chain_tx", "zz")
show("names chain_tx zz", r)
step("refusal: a malformed transaction hash", not r["ok"] and r["error"] == "a transaction hash is 64 hex characters")
other = "cd" * 32
r = names("chain_tx", other)
g = node("get_transaction", other)
show("names chain_tx of a hash never sent", r)
show("node get_transaction of it", g)
step("refusal: chain_tx of a hash never sent says the node has no record of it", not r["ok"]
     and r["error"] == "Your node has no record of that transaction, and it wasn't sent from the Names app: check the hash."
     and not g["ok"] and "no transaction found" in str(g["error"]).lower())

say("\n== refusal: amounts")
ha, hb = balance(A), balance(B)
r = names("chain_send", A, B, "1,5", "false")
show("names chain_send A -> B 1,5 (a decimal comma)", r)
step("refusal: a comma that doesn't group thousands is refused, not read as 15", not r["ok"]
     and r["error"] == "1,5 isn't a whole amount: commas go only between groups of three digits, such as 1,000"
     and (balance(A), balance(B)) == (ha, hb))
r = names("chain_send", A, B, "100", "false")
q = names("chain_send", A, B, '"100"', "false")
show("names chain_send A -> B 100 (bare digits)", r)
show("names chain_send A -> B '\"100\"'", q)
say("FLAG upstream (logosctl 0.3.2): bare digits become a JSON number and the host refuses the call before names "
    f"runs ({str(r['error'])[:90]}); nothing moved: A {balance(A)} (was {ha}), B {balance(B)} (was {hb}). "
    "The Names app passes text, so only logosctl users meet it.")

# ── Node stopped ───────────────────────────────────────────────────────────
say("\n== refusal: node stopped")
r = node("stop")
show("node stop", r)
open(os.path.join(W, ".node-stopped"), "w").close()
time.sleep(1)
n = node("get_cryptarchia_info")
show("node get_cryptarchia_info", n)
down = [(m, names(m, *a)) for m, a in (("chain_status", ()), ("chain_accounts", ()), ("chain_stake", ()),
                                       ("chain_send", (A, B, "1,000", "true")), ("chain_tx", (H,)), ("chain_tx", (other,)),
                                       ("chain_claim", ("true",)), ("chain_claim", ("false",)))]
for m, x in down:
    show(f"names {m}", x)
step("refusal: node stopped, every chain method says so as the node does",
     r["ok"] and not n["ok"] and "not running" in str(n["error"]).lower()
     and all(not x["ok"] and x["error"] == RUNNING for _, x in down))

results.close()
sys.exit(1 if fails else 0)
PY
