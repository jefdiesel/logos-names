#!/bin/bash
# LP-0022 accounts: the Names module's chain_account_* methods end to end over REAL Logos
# Core IPC (logosctl 0.3.2, the real blockchain_module 0.3.4, names) against a SOLO node
# with no peers: a new account is made in the node's keystore, survives a module reload,
# is used by the node after it restarts, receives a send from a funded key and sends back;
# labels, removal and the refusals are checked against the node itself.
#
# Runs on the Mac mini, with bash (not zsh), through the heavy-job lock (the sends prove):
#   ssh minim4 "CHAIN_IPC_PORT_BASE=3900 NAMES_LGX=<lgx dir> /Volumes/exos162/logos/dev/fix-names/mini-lock.sh \
#               /bin/bash <repo>/scripts/chain-accounts-e2e.sh"
#
#   CHAIN_IPC_DIR        work dir, wiped and recreated (default $CHAIN_IPC_AREA/accounts-run)
#   CHAIN_IPC_PORT_BASE  node ports: net = base, blend = base+1, http = base+2 (default 3900)
#   CHAIN_IPC_AREA       the built pieces (default /Volumes/exos162/logos/dev/chain-ipc):
#                          logosctl-0.3.2/bin/logosctl, real-module/blockchain_module, spike/ fixtures
#   NAMES_LGX            the names lgx dir (with chain_account_new)
#
# The funded keys are the node repo's PUBLIC standalone fixture (pinned by sha256, as in
# chain-ipc-e2e.sh): never use them on a real chain. Every step prints PASS or FAIL; the
# script exits non-zero on any FAIL, and always stops the node and the daemon.
set -uo pipefail

T=${CHAIN_IPC_AREA:-/Volumes/exos162/logos/dev/chain-ipc}
W=${CHAIN_IPC_DIR:-$T/accounts-run}
BASE=${CHAIN_IPC_PORT_BASE:-3900}
NAMES_LGX=${NAMES_LGX:?set NAMES_LGX to the names lgx dir}
CTL=$T/logosctl-0.3.2/bin/logosctl
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
if [ -d "$W" ] && [ -n "$(ls -A "$W")" ] && [ ! -e "$W/.chain-accounts-e2e" ]; then
  die "$W is not empty and was not made by this script; choose another CHAIN_IPC_DIR"
fi
if lsof -nP -i :$NET -i :$BLEND -i :$HTTP >/dev/null 2>&1; then
  lsof -nP -i :$NET -i :$BLEND -i :$HTTP; die "ports $NET-$HTTP are busy; choose another CHAIN_IPC_PORT_BASE"
fi

rm -rf "$W"
mkdir -p "$W/home" "$W/session/modules" && touch "$W/.chain-accounts-e2e" || die "cannot make $W"
cd "$W" || die "cannot enter $W"
export HOME=$W/home LOGOSCTL_CONFIG_DIR=$W/session

say "chain-accounts-e2e  $(date -u +%Y-%m-%dT%H:%M:%SZ)  host $(hostname -s)"
say "  work dir  $W"
say "  ports     net $NET (udp), blend $BLEND, http 127.0.0.1:$HTTP"
say "  logosctl  $CTL ($("$CTL" --version 2>/dev/null | head -1))"
say "  module    $MODULE ($(python3 -I -c 'import json,sys;m=json.load(open(sys.argv[1]));print(m["name"],m["version"])' "$MODULE/manifest.json"))"
say "  names     $(readlink "$NAMES_LGX" || echo "$NAMES_LGX")"
say "            lgx sha256 $(shasum -a 256 "$NAMES_LGX"/*.lgx | cut -c1-64)"

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
  "$CTL" --json call blockchain_module stop >/dev/null 2>&1 || true
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

export CTL W NET BLEND HTTP NAMES_LGX
python3 -I - <<'PY'
import json, os, re, stat, subprocess, sys, time

CTL, W = os.environ["CTL"], os.environ["W"]
NET, BLEND, HTTP = (int(os.environ[k]) for k in ("NET", "BLEND", "HTTP"))
DAEMON_LOG = os.path.join(W, "session", "logs", "daemon.log")
A = "e3635f207984ae779cf76b5f20714b514373f61ff96260879fe0a6d71f2dce07"  # genesis 100000
B = "39e16b432574571a6bcd8ee36e370589641bd9f35367f6f97a972453c46c3225"  # genesis 100
RUNNING = "Your Logos Blockchain node isn't running. Start it in Basecamp's Blockchain app."
results = open(os.path.join(W, ".results"), "w")


def say(text=""):
    print(text, flush=True)


def step(name, ok, detail=""):
    line = f"{'PASS' if ok else 'FAIL'} {name}" + (f" — {detail}" if detail else "")
    say(line)
    results.write(line + "\n")
    results.flush()
    return ok


def abort(why):
    say(f"ABORT {why}")
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
    return int(r["value"]) if r["ok"] and str(r["value"]).isdigit() else (0 if "unknown wallet address" in str(r["error"]).lower() else None)


def known():
    r = node("wallet_get_known_addresses")
    return r["value"] if r["ok"] and isinstance(r["value"], list) else None


def keystore_read():
    r = node("read_accounts", CFG)
    v = r["value"] if r["ok"] and isinstance(r["value"], dict) else {}
    return {a["title"]: a["public_key"] for a in v.get("accounts", [])}, {k["title"]: k["key_id"] for k in v.get("keystore_keys", [])}


def secret_of(title):
    found = re.search(rf"^\s*'?{re.escape(title)}'?: !Zk\s+([0-9a-f]+)", open(KS).read(), re.M)
    return found.group(1) if found else None


def wait_landed(hash_, seconds=180):
    end = time.time() + seconds
    state = None
    while time.time() < end:
        r = names("chain_tx", hash_)
        state = r["value"].get("state") if r["ok"] else f"ERR {r['error']}"
        if state != "pending":
            return state
        time.sleep(1)
    return state


def start_node(label):
    r = node("start", CFG, os.path.join(W, "deployment.yaml"))
    say(f"    start ({label}): {shown(r)}")
    info = {}
    for _ in range(40):
        time.sleep(1)
        i = node("get_cryptarchia_info")
        if i["ok"] and isinstance(i["value"], dict):
            info = i["value"]
            if info.get("mode") == "Online":
                break
    say(f"    get_cryptarchia_info: {json.dumps(info, sort_keys=True)}")
    net = node("get_network_info")
    show("get_network_info", net)
    nv = net["value"] if net["ok"] and isinstance(net["value"], dict) else {}
    isolated = info.get("mode") == "Online" and bool(nv) and all(nv.get(k) == 0 for k in ("n_peers", "n_connections", "n_pending_connections", "n_discovered_peers"))
    if not step(f"ISOLATION ({label}): Online, zero peers and connections", isolated):
        node("stop")
        abort("PEER SEEN or node not Online: node stopped, nothing sent")


# ── The session ────────────────────────────────────────────────────────────
say("\n== session")
rc, out = ctl("install", "--dir", os.environ["NAMES_LGX"], "-y")
say(f"    install names: exit {rc}")
loaded = {}
for m in ("blockchain_module", "names"):
    rc, out = ctl("module", "load", m)
    say(f"    module load {m}: {out}")
    loaded[m] = loads(out) if rc == 0 else {}
if not all(isinstance(loaded[m], dict) and loaded[m].get("status") == "ok" for m in loaded):
    abort("a module did not load")
lidl = call("names", "lidl")
methods = re.findall(r"method (chain_account_\w+)\(([^)]*)\)", str(lidl["value"]))
say(f"    names lidl: {methods}")
step("names exposes chain_account_new/label/remove", {m for m, _ in methods} >= {"chain_account_new", "chain_account_label", "chain_account_remove"})

say("\n== refusal: node not started")
s = names("chain_account_new", "Early")
show("names chain_account_new Early", s)
step("chain_account_new with no node says so, as the node does", not s["ok"] and s["error"] == RUNNING)

# ── A solo node, isolated, funded at genesis ───────────────────────────────
say("\n== solo node")
gen = {"skip_ibd": True, "net_port": NET, "blend_port": BLEND, "http_addr": f"127.0.0.1:{HTTP}",
       "output": "user_config.yaml", "use_persistence_paths": True}
json.dump(gen, open("gen-args.json", "w"))
say(f"    gen-args.json: {json.dumps(gen)}")
g = node("generate_user_config", "@gen-args.json")
if not g["ok"] or not str(g["value"]).endswith("user_config.yaml"):
    abort(f"generate_user_config: {shown(g)}")
CFG = str(g["value"])
KS = os.path.join(os.path.dirname(CFG), "keystore.yaml")
say(f"    config: {CFG}")
say(f"    keystore mode as the node wrote it: {oct(stat.S_IMODE(os.stat(KS).st_mode))}")
fixture = open("standalone-node-config.yaml").read()
for key, title in ((A, "GenesisStake"), (B, "GenesisSmall")):
    found = re.search(rf"^\s*{key}: !Zk\s+(\S+)", fixture, re.M)
    r = node("add_key", KS, "zk", found.group(1), title)
    say(f"    add_key {title} ({key[:8]}…): {'ok' if r['ok'] else 'ERR ' + str(r['error'])}")
    if not r["ok"]:
        abort("add_key failed")
with open(CFG, "a") as f:
    f.write("cryptarchia:\n  service:\n    bootstrap:\n      prolonged_bootstrap_period: 5s\n")
config = open(CFG).read()
peers = [line for line in config.splitlines() if re.search(r"peer|ibd", line, re.I)]
say(f"    grep -i -E 'peer|ibd' user_config.yaml: {peers if peers else '(no match)'}; skip_ibd: true is in gen-args.json")
if peers:
    abort("PEERS IN CONFIG")
start_node("first start")
skip = [line for line in daemon_log().splitlines() if "Skipping IBD as no peers are configured" in line]
skip_line = re.sub(r".*WARN", "WARN", re.sub("\x1b\\[[0-9;]*m", "", skip[0])) if skip else "(no Skipping IBD line)"
say(f"    node log: {skip_line}")

# ── chain_account_new ──────────────────────────────────────────────────────
say("\n== chain_account_new Savings")
known0 = known()
accounts0, titles0 = keystore_read()
say(f"    node before: known {len(known0)} keys, keystore {sorted(titles0)}")
r = names("chain_account_new", "Savings")
show("names chain_account_new Savings", r)
K = (r["value"] or {}).get("key", "") if r["ok"] else ""
accounts1, titles1 = keystore_read()
say(f"    node read_accounts after: names-1 -> {accounts1.get('names-1')}")
step("a new key is made in the node's own keystore, under a new title, the others untouched",
     r["ok"] and re.fullmatch(r"[0-9a-f]{64}", K) is not None and accounts1.get("names-1") == K
     and all(titles1.get(t) == k for t, k in titles0.items()) and len(titles1) == len(titles0) + 1,
     f"key {K[:8]}…, keystore {len(titles0)} -> {len(titles1)} keys")
mode = stat.S_IMODE(os.stat(KS).st_mode)
step("the keystore holding the new secret is readable by this user only", mode == 0o600, oct(mode))
sec = secret_of("names-1")
leaks = [p for p in (DAEMON_LOG, os.path.join(W, "node.log")) if sec and os.path.exists(p) and sec in open(p, errors="replace").read()]
cfgdir = os.path.join(os.environ["HOME"], ".config", "logos-names")
for root, _, files in os.walk(cfgdir):
    for f in files:
        if sec and sec in open(os.path.join(root, f), errors="replace").read():
            leaks.append(os.path.join(root, f))
step("the new secret is in no reply, no log and no names file", bool(sec) and sec not in r["raw"] and not leaks,
     f"secret found in {leaks}" if leaks else "searched the reply, the daemon log and the names config dir")
step("the running node doesn't use it yet, and the reply says so", K not in (known() or []) and r["value"].get("ready") is False
     and "next start" in str(r["value"].get("note")))
recs = os.path.join(cfgdir, "chain-accounts.json")
rmode = stat.S_IMODE(os.stat(recs).st_mode) if os.path.exists(recs) else None
step("the label is kept in the names config (0600), not in the keystore", rmode == 0o600 and "Savings" not in open(KS).read(),
     f"{recs} {oct(rmode) if rmode else 'missing'}")
s = names("chain_accounts")
show("names chain_accounts", s)
row = next((x for x in (s["value"] or []) if x.get("key") == K), None) if s["ok"] else None
step("chain_accounts lists it with its label, not ready, no balance yet", row == {"key": K, "balance": None, "title": "names-1", "label": "Savings", "ready": False})

say("\n== a second account, no label; a label on a genesis key")
r2 = names("chain_account_new", "")
show("names chain_account_new ''", r2)
K2 = (r2["value"] or {}).get("key", "") if r2["ok"] else ""
accounts2, _ = keystore_read()
step("the second account gets the next title", r2["ok"] and accounts2.get("names-2") == K2 and accounts2.get("names-1") == K)
l = names("chain_account_label", A, "Main")
show("names chain_account_label A Main", l)
step("a key the node lists takes a label", l["ok"] and l["value"] == {"key": A, "label": "Main"})

say("\n== a send to an account the node doesn't use yet (why the app says not to receive before the restart)")
r3 = names("chain_account_new", "Too early")
K3 = (r3["value"] or {}).get("key", "") if r3["ok"] else ""
show("names chain_account_new 'Too early'", r3)
a_e = balance(A)
r = names("chain_send", A, K3, "1,000", "true")
show("names chain_send A -> K3 1,000", r)
EARLY = (r["value"] or {}).get("hash") if r["ok"] else None
st = wait_landed(EARLY) if EARLY else None
gt = node("get_transaction", EARLY) if EARLY else {"ok": False, "value": None}
outs = [(o["pk"], int(o["value"])) for op in ((gt["value"] or {}).get("mantle_tx", {}).get("ops", []) if gt["ok"] else []) for o in op.get("payload", {}).get("outputs", [])]
say(f"    node: A {a_e} -> {balance(A)}; the transaction pays {[(k[:8], v) for k, v in outs]}; K3 balance now {node('wallet_get_balance', K3)['error'] or 'ok'}")
step("the send to K3 is in a block, paying K3 1,000", st == "in_block" and (K3, 1000) in outs)
# The block's slot, from the node's own log; the restart below waits until that block is
# final (lib_slot past it), as it is on a real node long before anyone restarts.
early_slot = None
for line in daemon_log().splitlines():
    if "Received new block: " in line and EARLY and EARLY in line:
        early_slot = json.loads(line.split("Received new block: ", 1)[1])["header"]["slot"]
        break
lib = None
for _ in range(180):
    i = node("get_cryptarchia_info")
    lib = i["value"].get("lib_slot") if i["ok"] and isinstance(i["value"], dict) else None
    if early_slot is not None and lib is not None and lib >= early_slot:
        break
    time.sleep(1)
say(f"    node: the block paying K3 is at slot {early_slot}; lib_slot now {lib}")
step("the block paying K3 is final before the node restarts", early_slot is not None and lib is not None and lib >= early_slot)

say("\n== the names module reloads")
rc1, out1 = ctl("module", "unload", "names")
rc2, out2 = ctl("module", "load", "names")
say(f"    unload: exit {rc1}; load: exit {rc2} {out2}")
s = names("chain_accounts")
show("names chain_accounts after the reload", s)
rows = {x["key"]: x for x in (s["value"] or [])} if s["ok"] else {}
step("after a module reload the accounts and labels are still there",
     rows.get(K, {}).get("label") == "Savings" and K2 in rows and rows.get(A, {}).get("label") == "Main" and rows.get(A, {}).get("balance") == str(balance(A)))

# ── The node restarts: it now uses the new keys ───────────────────────────
say("\n== the node stops and starts")
s = names("chain_accounts")
r = node("stop")
show("node stop", r)
time.sleep(2)
down = names("chain_accounts")
show("names chain_accounts, node stopped", down)
step("chain_accounts with the node stopped says so", not down["ok"] and down["error"] == RUNNING)
dn = names("chain_account_new", "While down")
show("names chain_account_new, node stopped", dn)
step("chain_account_new with the node stopped makes nothing", not dn["ok"] and dn["error"] == RUNNING and "names-4" not in open(KS).read())
start_node("restart")
kn = known() or []
say(f"    node wallet_get_known_addresses has K: {K in kn}, K2: {K2 in kn}")
s = names("chain_accounts")
show("names chain_accounts", s)
row = next((x for x in (s["value"] or []) if x.get("key") == K), None) if s["ok"] else None
step("after the restart the node uses the new keys, and chain_accounts shows them ready with balance 0",
     K in kn and K2 in kn and row is not None and row.get("ready") is True and row.get("balance") == "0" and balance(K) == 0)
k3 = node("wallet_get_balance", K3)
show("node wallet_get_balance K3 (sent 1,000 before the node used it)", k3)
step("FINDING: what reached K3 before the node used it, once final, is not seen after the restart (the node lists K3, answers no notes)",
     K3 in kn and not k3["ok"] and "unknown wallet address" in str(k3["error"]).lower(),
     "so chain_account_new's note says not to receive before the restart, and the app offers no such key to receive")

# ── It receives a send and sends back ──────────────────────────────────────
say("\n== a send to the new account from a funded key")
a0, k0 = balance(A), balance(K)
r = names("chain_send", A, K, "5,000", "true")
show("names chain_send A -> K 5,000", r)
h = (r["value"] or {}).get("hash") if r["ok"] else None
st = wait_landed(h) if h else None
a1, k1 = balance(A), balance(K)
notes = node("wallet_get_notes", K, "")
show("node wallet_get_notes K", notes)
say(f"    node balances: A {a0} -> {a1}, K {k0} -> {k1}; names fee {r['value'].get('fee') if r['ok'] else None}")
step("the new account receives: the node shows K holding 5,000", st == "in_block" and k1 == 5000 and a1 == a0 - 5000 - int(r["value"]["fee"]),
     f"state {st}")
s = names("chain_accounts")
row = next((x for x in (s["value"] or []) if x.get("key") == K), None) if s["ok"] else None
step("chain_accounts shows the new account's balance as the node does", row is not None and row.get("balance") == str(k1) and row.get("label") == "Savings", json.dumps(row))

say("\n== and back: the new account sends 1,000 to A")
a2, k2 = balance(A), balance(K)
r = names("chain_send", K, A, "1,000", "true")
show("names chain_send K -> A 1,000", r)
h = (r["value"] or {}).get("hash") if r["ok"] else None
st = wait_landed(h) if h else None
a3, k3 = balance(A), balance(K)
fee = int(r["value"]["fee"]) if r["ok"] and r["value"].get("fee") else None
say(f"    node balances: A {a2} -> {a3}, K {k2} -> {k3}; names fee {fee}")
step("the new account spends: A gains 1,000 and K pays 1,000 plus the fee", st == "in_block" and fee is not None
     and a3 == a2 + 1000 and k3 == k2 - 1000 - fee, f"state {st}")

# ── Removal ────────────────────────────────────────────────────────────────
say("\n== chain_account_remove")
before_titles = keystore_read()[1]
r = names("chain_account_remove", K)
show("names chain_account_remove K (holds funds)", r)
step("an account holding anything is not removed", not r["ok"] and f"holds {balance(K)}" in str(r["error"]) and keystore_read()[1] == before_titles)
r = names("chain_account_remove", A)
show("names chain_account_remove A (not made by Names)", r)
step("a key the Names app didn't make is not removed", not r["ok"] and "made in the Names app" in str(r["error"]) and keystore_read()[1] == before_titles)
r = names("chain_account_remove", K2)
show("names chain_account_remove K2 (empty)", r)
after_titles = keystore_read()[1]
step("an empty account made here is removed from the node's keystore, the others untouched",
     r["ok"] and "names-2" not in after_titles and all(after_titles.get(t) == k for t, k in before_titles.items() if t != "names-2"),
     f"keystore {len(before_titles)} -> {len(after_titles)} keys")
s = names("chain_accounts")
step("chain_accounts no longer lists it", s["ok"] and K2 not in [x["key"] for x in s["value"]])

say("\n== refusals: unknown keys")
other = "cd" * 32
r1 = names("chain_account_label", other, "x")
r2 = names("chain_account_remove", other)
r3 = names("chain_account_label", "nothex", "x")
for label, x in (("label of a key never seen", r1), ("remove of a key never seen", r2), ("label of a malformed key", r3)):
    show(f"names {label}", x)
step("an unknown or malformed key is refused, in plain words", not r1["ok"] and r1["error"] == "Your node doesn't hold that key."
     and not r2["ok"] and "made in the Names app" in r2["error"] and not r3["ok"] and r3["error"] == "choose the account to label")
r4 = names("chain_account_new", "x" * 41)
show("names chain_account_new with a 41-character label", r4)
step("a label over 40 characters is refused before the node is asked", not r4["ok"] and "at most 40" in r4["error"] and "names-4" not in keystore_read()[0])

results.close()
fails = sum(1 for line in open(os.path.join(W, ".results")) if line.startswith("FAIL"))
sys.exit(1 if fails else 0)
PY
