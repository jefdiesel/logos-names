#!/bin/bash
# LP-0022 "inscribes on the Logos blockchain": the Names module's chain_inscribe end to end
# over REAL Logos Core IPC (logosctl 0.3.2, the real blockchain_module 0.3.4 and names)
# against a SOLO node on its own chain, with no peers at all. Start-up as in
# scripts/chain-ipc-e2e.sh (same fixture, pinned by sha256; never use its keys on a real chain).
#
# Runs on the Mac mini, with bash (not zsh):
#   ssh minim4 "INSCRIBE_E2E_PORT_BASE=3940 NAMES_LGX=<lgx dir> /bin/bash <repo>/scripts/lp22-inscribe-e2e.sh"
#
#   INSCRIBE_E2E_DIR        work dir, wiped and recreated (default $CHAIN_IPC_AREA/inscribe-run)
#   INSCRIBE_E2E_PORT_BASE  node ports: net = base, blend = base+1, http = base+2 (default 3940)
#   CHAIN_IPC_AREA          logosctl-0.3.2/bin/logosctl, real-module/blockchain_module, spike/ fixture
#   NAMES_LGX               the names lgx dir (holding the .lgx)
#
# Every step checks what names said against the node itself (blockchain_module called
# directly: channel state, transaction, balances, notes, the block the node stored),
# prints PASS or FAIL, and the script exits non-zero on any FAIL. The session is always
# stopped (trap), and no process whose command line holds the work dir may be left.
set -uo pipefail

T=${CHAIN_IPC_AREA:-/Volumes/exos162/logos/dev/chain-ipc}
W=${INSCRIBE_E2E_DIR:-$T/inscribe-run}
BASE=${INSCRIBE_E2E_PORT_BASE:-3940}
NAMES_LGX=${NAMES_LGX:?NAMES_LGX must name the names lgx dir}
CTL=$T/logosctl-0.3.2/bin/logosctl
MODULE=$T/real-module/blockchain_module
NET=$BASE BLEND=$((BASE + 1)) HTTP=$((BASE + 2))
DEPLOY_SHA=0929cddc3dc35a189dbdb1e8297ece5d80ca1247a9fda07c2fdfcf5330adf618
NODECFG_SHA=28b6729f2aeab5937d95192326a01102e0c9bd9594b40de1b154e7e3b5dea2cc
FIXTURES=https://raw.githubusercontent.com/logos-blockchain/logos-blockchain/0.3.4/nodes/node

say() { printf '%s\n' "$*"; }
die() { say "ABORT $*"; exit 2; }

case "$BASE" in '' | *[!0-9]*) die "INSCRIBE_E2E_PORT_BASE must be a number" ;; esac
for p in $NET $BLEND $HTTP; do
  case $p in 8080 | 8081 | 3000 | 3001 | 3401 | 11434) die "port $p is reserved on the Mini" ;; esac
  [ "$p" -ge 3600 ] && [ "$p" -le 3860 ] && die "port $p is another loop's"
done
for f in "$CTL" "$MODULE/manifest.json" "$NAMES_LGX"; do [ -e "$f" ] || die "missing $f"; done
if [ -d "$W" ] && [ -n "$(ls -A "$W")" ] && [ ! -e "$W/.inscribe-e2e" ]; then
  die "$W is not empty and was not made by this script; choose another INSCRIBE_E2E_DIR"
fi
if lsof -nP -i :$NET -i :$BLEND -i :$HTTP >/dev/null 2>&1; then
  lsof -nP -i :$NET -i :$BLEND -i :$HTTP; die "ports $NET-$HTTP are busy; choose another INSCRIBE_E2E_PORT_BASE"
fi

rm -rf "$W"
mkdir -p "$W/home" "$W/session/modules" && touch "$W/.inscribe-e2e" || die "cannot make $W"
cd "$W" || die "cannot enter $W"
export HOME=$W/home LOGOSCTL_CONFIG_DIR=$W/session

say "lp22-inscribe-e2e  $(date -u +%Y-%m-%dT%H:%M:%SZ)  host $(hostname -s)"
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

export CTL W NET BLEND HTTP NAMES_LGX
python3 -I - <<'PY'
import hashlib, json, os, re, stat, subprocess, sys, time

CTL, W = os.environ["CTL"], os.environ["W"]
NET, BLEND, HTTP = (int(os.environ[k]) for k in ("NET", "BLEND", "HTTP"))
DAEMON_LOG = os.path.join(W, "session", "logs", "daemon.log")
A = "e3635f207984ae779cf76b5f20714b514373f61ff96260879fe0a6d71f2dce07"  # genesis 100000
B = "39e16b432574571a6bcd8ee36e370589641bd9f35367f6f97a972453c46c3225"  # genesis 100
BAD = "ab" * 32
ZERO = "00" * 32
RUNNING = "Your Logos Blockchain node isn't running. Start it in Basecamp's Blockchain app."
KEYS = os.path.join(W, "home", ".config", "logos-names", "chain-inscribe-keys.json")
results = open(os.path.join(W, ".results"), "w")
fails = 0
replies = []   # every names reply, to look for the inscription key's secret in


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
    argv = [CTL, "--json", "call", module, method, *[str(a) for a in args]]
    try:
        p = subprocess.run(argv, capture_output=True, text=True, timeout=timeout, stdin=subprocess.DEVNULL)
    except subprocess.TimeoutExpired:
        return {"ok": False, "value": None, "error": f"logosctl timed out after {timeout}s", "raw": ""}
    raw = p.stdout.strip()
    if module == "names":
        replies.append(raw)
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


def balance(key):
    r = node("wallet_get_balance", key)
    return int(r["value"]) if r["ok"] and str(r["value"]).isdigit() else None


def notes(key):
    r = node("wallet_get_notes", key, "")
    return {n["id"]: int(n["value"]) for n in r["value"]["notes"]} if r["ok"] else None


def channel(ch):
    # Channel ids with only digits would reach the node as a number: str: keeps them text.
    return node("get_channel_state", "str:" + ch)


def ctl(*argv):
    p = subprocess.run([CTL, *argv], capture_output=True, text=True, timeout=120, stdin=subprocess.DEVNULL)
    return p.returncode, (p.stdout + p.stderr).strip()


def daemon_log():
    try:
        with open(DAEMON_LOG, errors="replace") as f:
            return f.read()
    except OSError:
        return ""


def wait_for(check, seconds, every=1.0):
    end = time.time() + seconds
    while time.time() < end:
        got = check()
        if got:
            return got
        time.sleep(every)
    return check()


def enc_op(op):
    """An op's canonical encoding (Mantle spec), for the two ops an inscription's tx holds."""
    c, p = op["opcode"], op["payload"]
    b = bytes([c])
    if c == 0x11:
        ins = bytes.fromhex(p["inscription"])
        b += bytes.fromhex(p["channel_id"]) + len(ins).to_bytes(4, "little") + ins + bytes.fromhex(p["parent"]) + bytes.fromhex(p["signer"])
    elif c == 0x00:
        b += bytes([len(p["inputs"])]) + b"".join(bytes.fromhex(i) for i in p["inputs"])
        b += bytes([len(p["outputs"])]) + b"".join(int(o["value"]).to_bytes(8, "little") + bytes.fromhex(o["pk"]) for o in p["outputs"])
    else:
        return None
    return b


def tx_hash_of(tx):
    """blake2b-256("MANTLE_TXHASH_V1" || ops), computed here, independently of names."""
    ops = tx.get("mantle_tx", {}).get("ops", [])
    parts = [enc_op(o) for o in ops]
    if any(p is None for p in parts):
        return None
    return hashlib.blake2b(b"MANTLE_TXHASH_V1" + bytes([len(ops)]) + b"".join(parts), digest_size=32).hexdigest()


def block_with(tx_hash, mark):
    """The block the node stored that carries tx_hash: its slot from the node's log, then
    the block itself from the node (get_blocks serves blocks once they are final), and in
    it the transaction whose hash, computed here from its ops, is tx_hash."""
    slot = None
    for line in daemon_log()[mark:].splitlines():
        if "Received new block: " in line and tx_hash in line:
            slot = json.loads(line.split("Received new block: ", 1)[1])["header"]["slot"]
            break
    if slot is None:
        return None, None

    def stored():
        r = node("get_blocks", slot, slot)
        blocks = r["value"] if r["ok"] and isinstance(r["value"], list) else []
        return next((t for b in blocks for t in b.get("transactions", []) if tx_hash_of(t) == tx_hash), None)
    return slot, wait_for(stored, 120)


def inscription_in(tx):
    return next((op["payload"] for op in (tx or {}).get("mantle_tx", {}).get("ops", []) if op.get("opcode") == 17), None)


# ── The session ────────────────────────────────────────────────────────────
say("\n== session")
rc, out = ctl("install", "--dir", os.environ["NAMES_LGX"], "-y")
say(f"    install names: exit {rc}")
loaded = {}
for m in ("blockchain_module", "names"):
    rc, out = ctl("module", "load", m)
    say(f"    module load {m}: {out}")
    loaded[m] = loads(out) if rc == 0 else {}
ok = all(isinstance(loaded[m], dict) and loaded[m].get("status") == "ok" for m in loaded)
step("session loads blockchain_module 0.3.4 and names", ok and loaded["blockchain_module"].get("version") == "0.3.4",
     f"names {loaded.get('names', {}).get('version')}")
if not ok:
    abort("a module did not load")

say("\n== refusal: node not started")
s = names("chain_inscribe", A, "before the node", "false")
show("names chain_inscribe plan", s)
step("refusal: node not started", not s["ok"] and s["error"] == RUNNING)

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
fixture = open("standalone-node-config.yaml").read()
for key, title in ((A, "GenesisStake"), (B, "GenesisSmall")):
    found = re.search(rf"^\s*{key}: !Zk\s+(\S+)", fixture, re.M)
    if not found:
        abort(f"no secret for {key[:8]} in the fixture")
    r = node("add_key", KS, "zk", found.group(1), title)
    if not r["ok"]:
        abort("add_key failed")
with open(CFG, "a") as f:
    f.write("cryptarchia:\n  service:\n    bootstrap:\n      prolonged_bootstrap_period: 5s\n")

say("\n== ISOLATION (before anything moves)")
config = open(CFG).read()
say(f"    {CFG}, in full:")
for line in config.splitlines():
    say(f"      | {line}")
peers = [line for line in config.splitlines() if re.search(r"peer|ibd", line, re.I)]
say(f"    grep -i -E 'peer|ibd' user_config.yaml: {peers if peers else '(no match)'}")
if peers:
    abort("PEERS IN CONFIG")
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

# ── The plan: what it will do, nothing moved ───────────────────────────────
TEXT1 = "Names puts my Logos Blockchain keys and my LEZ names in one wallet."
say("\n== chain_inscribe plan (send=false), twice")
a0, n0 = balance(A), notes(A)
p1 = names("chain_inscribe", A, TEXT1, "false")
p2 = names("chain_inscribe", A, TEXT1, "false")
show("names chain_inscribe A plan", p1)
v1 = p1["value"] if p1["ok"] else {}
CH, SIGNER = v1.get("channel", ""), v1.get("signer", "")
before = channel(CH) if CH else {"ok": True, "error": ""}
show("node get_channel_state of the plan's channel", before)
step("plan: a first inscription into the key's own channel, which the node doesn't have yet",
     p1["ok"] and v1["first"] is True and v1["parent"] == ZERO and not before["ok"] and "No channel found" in str(before["error"])
     and CH == hashlib.sha256(b"lp-0022:testimonial:" + bytes.fromhex(SIGNER)).hexdigest(),
     f"channel {CH[:16]}…, signer {SIGNER[:16]}…")
step("plan: the inscription is the testimonial's JSON, its fee as the node prices it",
     p1["ok"] and v1["text"] == TEXT1 and v1["submission"] == "lp-0022:names-wallet" and str(v1.get("fee_estimate", "")).isdigit()
     and v1["balance"] == str(a0), f"fee_estimate {v1.get('fee_estimate')}, balance {v1.get('balance')} = node {a0}")
step("plan moves nothing: a second plan of it gives the same fee, the node's balance and notes unchanged",
     p2["ok"] and p2["value"]["fee_estimate"] == v1.get("fee_estimate") and p2["value"]["channel"] == CH
     and balance(A) == a0 and notes(A) == n0, f"A {a0} -> {balance(A)}, notes {len(n0)} -> {len(notes(A) or {})}")
# A plan is priced with stand-ins of the key's notes (chain_fee.rs), so the node holds
# none of A's notes for it: another text is priced at once, a send from A can be planned,
# and the send below is funded by the node at its first asking.
other = names("chain_inscribe", A, "a different text, priced at once", "false")
show("names chain_inscribe A plan of another text, at once", other)
sendable = names("chain_send", A, B, "1,000", "false")
show("names chain_send A -> B plan, at once", sendable)
step("a plan holds none of the key's notes: another inscription and a send from A are priced at once",
     other["ok"] and str(other["value"].get("fee_estimate", "")).isdigit() and sendable["ok"] and str(sendable["value"].get("fee_about", "")).isdigit() and not sendable["value"].get("in_flight") and not sendable["value"].get("cannot_pay_fee")
     and balance(A) == a0 and notes(A) == n0,
     f"other text fee {other['value'].get('fee_estimate') if other['ok'] else other['error']}, send fee {sendable['value'].get('fee_about') if sendable['ok'] else sendable['error']}")

# ── The first inscription ──────────────────────────────────────────────────
say("\n== chain_inscribe (send=true): the first inscription")
mark = len(daemon_log())
s1 = names("chain_inscribe", A, TEXT1, "true")
show("names chain_inscribe A send", s1)
sv1 = s1["value"] if s1["ok"] else {}
H1 = sv1.get("hash") or ""
step("send: a hash, the fee it pays", s1["ok"] and re.fullmatch(r"[0-9a-f]{64}", H1) is not None and str(sv1.get("fee")).isdigit(),
     f"hash {H1}, fee {sv1.get('fee')} (plan said about {v1.get('fee_estimate')})")
step("send: funded at once after three plans, and it pays the fee its plan said",
     s1["ok"] and sv1.get("fee") == v1.get("fee_estimate"), f"paid {sv1.get('fee')}, plan {v1.get('fee_estimate')}")
if not H1:
    abort("no hash", stop_node=True)
landed = wait_for(lambda: (lambda c: c if c["ok"] and c["value"].get("tip_message") == sv1["msg_id"] else None)(channel(CH)), 120)
show("node get_channel_state after", landed or channel(CH))
step("node: the channel now exists, written only by the account's inscription key, its tip this inscription",
     bool(landed) and landed["value"]["accredited_keys"] == [SIGNER] and landed["value"]["tip_message"] == sv1["msg_id"],
     f"accredited {landed['value']['accredited_keys'] if landed else None}, tip {sv1.get('msg_id', '')[:16]}…")
gt = node("get_transaction", H1)
show("node get_transaction", gt)
ops = gt["value"]["mantle_tx"]["ops"] if gt["ok"] else []
ins = next((op["payload"] for op in ops if op["opcode"] == 17), {})
step("node: the transaction holds the inscription, the testimonial's JSON, signed by that key",
     gt["ok"] and json.loads(bytes.fromhex(ins.get("inscription", "")).decode() or "{}") == {"submission": "lp-0022:names-wallet", "text": TEXT1}
     and ins.get("signer") == SIGNER and ins.get("channel_id") == CH and ins.get("parent") == ZERO
     and list(gt["value"]["ops_proofs"][0]) == ["Ed25519Sig"])
fee_in = [i for op in ops if op["opcode"] == 0 for i in op["payload"]["inputs"]]
fee_out = sum(int(o["value"]) for op in ops if op["opcode"] == 0 for o in op["payload"]["outputs"])
a1, n1 = balance(A), notes(A)
paid = sum(n0[i] for i in fee_in if i in n0) - fee_out
step("node: the fee came from the chosen key, as names reported: inputs less change, gone from its wallet",
     all(i in n0 for i in fee_in) and str(paid) == sv1.get("fee") and a1 == a0 - paid and not set(fee_in) & set(n1),
     f"names fee {sv1.get('fee')}, node {paid}; A {a0} -> {a1}; estimate {v1.get('fee_estimate')}")
slot1, tx1 = block_with(H1, mark)
bi = inscription_in(tx1)
say(f"    node get_blocks({slot1}, {slot1}): {'a block with a transaction hashing to ' + H1[:16] + '…' if tx1 else 'nothing'}")
step("node: read back from the block it stored, the inscription says what was written",
     bi is not None and bytes.fromhex(bi["inscription"]).decode() == json.dumps({"submission": "lp-0022:names-wallet", "text": TEXT1}, separators=(",", ":")),
     f"slot {slot1}: {bytes.fromhex(bi['inscription']).decode() if bi else None}")
t = names("chain_tx", H1)
show("names chain_tx", t)
step("chain_tx follows it: in_block with the fee paid", t["ok"] and t["value"]["state"] == "in_block" and t["value"]["fee"] == sv1.get("fee"))

# ── A second inscription chains on the tip ─────────────────────────────────
TEXT2 = "Second one: it chains on the first."
say("\n== a second inscription chains on the channel's tip")
p = names("chain_inscribe", A, TEXT2, "false")
show("names chain_inscribe A plan", p)
step("plan: not first, its parent the channel's tip as the node has it",
     p["ok"] and p["value"]["first"] is False and p["value"]["parent"] == channel(CH)["value"]["tip_message"] == sv1["msg_id"])
mark = len(daemon_log())
s2 = names("chain_inscribe", A, TEXT2, "true")
show("names chain_inscribe A send", s2)
H2 = s2["value"].get("hash") if s2["ok"] else ""
landed2 = wait_for(lambda: (lambda c: c if c["ok"] and c["value"].get("tip_message") == s2["value"]["msg_id"] else None)(channel(CH)), 120) if H2 else None
_, tx2 = block_with(H2, mark) if H2 else (None, None)
b2 = inscription_in(tx2)
step("node: the second is the channel's tip, its parent the first, in a block",
     bool(landed2) and b2 is not None and b2["parent"] == sv1["msg_id"] and json.loads(bytes.fromhex(b2["inscription"]).decode())["text"] == TEXT2,
     f"tip {s2['value'].get('msg_id', '')[:16] if s2['ok'] else s2['error']}…")

# ── Another account: its own key, its own channel ──────────────────────────
say("\n== another chain account inscribes into its own channel")
r = names("chain_send", A, B, "5,000", "true")
HB = r["value"].get("hash") if r["ok"] else None
wait_for(lambda: names("chain_tx", HB)["value"].get("state") == "in_block" if HB else True, 120)
b0 = balance(B)
s3 = names("chain_inscribe", B, "From the other key.", "true")
show("names chain_inscribe B send", s3)
v3 = s3["value"] if s3["ok"] else {}
landed3 = wait_for(lambda: (lambda c: c if c["ok"] and c["value"].get("tip_message") == v3.get("msg_id") else None)(channel(v3.get("channel", "ff" * 32))), 120) if s3["ok"] else None
step("node: B's inscription is in B's own channel under B's own key, its fee from B",
     bool(landed3) and v3["channel"] != CH and v3["signer"] != SIGNER and landed3["value"]["accredited_keys"] == [v3["signer"]]
     and balance(B) == b0 - int(v3["fee"]), f"B {b0} -> {balance(B)}, fee {v3.get('fee')}")

# ── The Provider SDK: an app asks, the person picks the account and approves ─
# logosctl calls as the host, which may both ask (as an app, "host") and decide (as the
# Names app); that the asking app can't decide is checked by the module's unit tests.
TEXT3 = "Asked for by an app, approved in the Names app."
say("\n== provider: propose_inscription -> pending_inscriptions -> approve_inscription")
asked = names("propose_inscription", TEXT3)
show("names propose_inscription", asked)
RID = asked["value"].get("request") if asked["ok"] else ""
step("propose_inscription: a pending request of kind inscribe", asked["ok"] and asked["value"]["state"] == "pending"
     and asked["value"]["kind"] == "inscribe" and asked["value"]["text"] == TEXT3)
pend = names("pending_inscriptions")
show("names pending_inscriptions", pend)
mine = next((r for r in (pend["value"] or []) if r.get("id") == RID), None) if pend["ok"] else None
held = {}
for k in (node("wallet_get_known_addresses")["value"] or []):
    b = balance(k)
    if b is not None:
        held[k] = str(b)
step("pending_inscriptions: the sheet's facts (app, text, what it does) and the node's own keys as choices, with their balances",
     bool(mine) and mine["app"] == "host" and mine["says"] == "inscribes on the Logos blockchain" and mine["text"] == TEXT3
     and {c["key"]: c["balance"] for c in mine["choices"]} == held, f"{len(held)} keys held by the node")
generic = names("pending_requests")
step("pending_requests (the sheet that offers LEZ accounts) doesn't list the inscription",
     generic["ok"] and not any(r.get("id") == RID for r in (generic["value"] or [])), f"{len(generic['value'] or []) if generic['ok'] else generic['error']} listed")
est = names("chain_inscribe", A, TEXT3, "false")
a5 = balance(A)
mark = len(daemon_log())
ok_ = names("approve_inscription", RID, A)
show("names approve_inscription (account A)", ok_)
ov = ok_["value"] if ok_["ok"] else {}
H4 = ov.get("tx") or ""
landed4 = wait_for(lambda: (lambda c: c if c["ok"] and c["value"].get("tip_message") == ov.get("msg_id") else None)(channel(CH)), 120) if H4 else None
_, tx4 = block_with(H4, mark) if H4 else (None, None)
b4 = inscription_in(tx4)
step("approve_inscription: inscribed from the chosen account, in a block, the channel's tip, fee as estimated",
     ok_["ok"] and ov["state"] == "done" and ov["account"] == A and bool(landed4) and b4 is not None
     and json.loads(bytes.fromhex(b4["inscription"]).decode())["text"] == TEXT3 and balance(A) == a5 - int(ov["fee"])
     and est["ok"] and ov["fee"] == est["value"].get("fee_estimate"),
     f"tx {H4[:16]}…, fee {ov.get('fee')} (estimate {est['value'].get('fee_estimate') if est['ok'] else est['error']})")
st = names("request_status", RID)
show("names request_status", st)
step("request_status: done, with the transaction's hash", st["ok"] and st["value"]["state"] == "done" and st["value"]["tx"] == H4)
again = names("approve_inscription", RID, A)
step("approve_inscription twice is refused", not again["ok"] and "done already" in str(again["error"]), again["error"])
stray = names("propose_inscription", "y")
sid = stray["value"]["request"] if stray["ok"] else ""
no = names("approve_inscription", sid, BAD)
step("approve_inscription with a key the node doesn't hold is refused and stays pending",
     not no["ok"] and "isn't an account your node's wallet holds" in str(no["error"])
     and names("request_status", sid)["value"]["state"] == "pending", no["error"])
names("cancel_request", sid)

# ── After the names module reloads ─────────────────────────────────────────
say("\n== after the names module reloads")
rc1, _ = ctl("module", "unload", "names")
rc2, _ = ctl("module", "load", "names")
p = names("chain_inscribe", A, "after a reload", "false")
t = names("chain_tx", H1)
show("names chain_inscribe A plan", p)
show("names chain_tx H1", t)
st = names("request_status", RID)
step("reload: the approved request is still done with its hash", st["ok"] and st["value"]["state"] == "done" and st["value"]["tx"] == H4)
step("reload: the same key and channel, the parent the node's tip; chain_tx still knows the first",
     rc1 == 0 and rc2 == 0 and p["ok"] and p["value"]["signer"] == SIGNER and p["value"]["channel"] == CH
     and p["value"]["parent"] == channel(CH)["value"]["tip_message"] and t["ok"] and t["value"]["state"] == "in_block")

say("\n== the inscription keys stay private")
mode = stat.S_IMODE(os.stat(KEYS).st_mode) if os.path.exists(KEYS) else None
secrets = list(json.load(open(KEYS)).values()) if mode is not None else []
leaked = [s for s in secrets if any(s in reply for reply in replies) or s in daemon_log()]
step("keys file is 0600, holds one key per account used, and no secret is in any reply or the daemon log",
     mode == 0o600 and len(secrets) == 2 and not leaked, f"mode {oct(mode) if mode is not None else None}, {len(secrets)} keys, {len(leaked)} leaked")

# ── Refusals ───────────────────────────────────────────────────────────────
say("\n== refusals")
ha = balance(A)
cases = [
    ("a key the node can't read", (BAD, "x"), "That isn't a Logos Blockchain public key: check it and try again."),
    ("no text", (A, " "), "a testimonial needs some text"),
    ("text over 1,024 bytes", (A, "x" * 1025), "a testimonial holds at most 1024 bytes of text"),
]
for label, (frm, text), want in cases:
    r = names("chain_inscribe", frm, text, "true")
    say(f"    names chain_inscribe {label}: {('ERR ' + str(r['error'])) if not r['ok'] else shown(r)}")
    step(f"refusal: {label}", not r["ok"] and r["error"] == want and balance(A) == ha)
step("refusal: no inscription key is made for a key the node can't read", BAD not in json.load(open(KEYS)))

# ── Node stopped ───────────────────────────────────────────────────────────
say("\n== refusal: node stopped")
r = node("stop")
open(os.path.join(W, ".node-stopped"), "w").close()
time.sleep(1)
s = names("chain_inscribe", A, TEXT1, "false")
show("names chain_inscribe plan", s)
step("refusal: node stopped, chain_inscribe says so as the node does", r["ok"] and not s["ok"] and s["error"] == RUNNING)

results.close()
sys.exit(1 if fails else 0)
PY
